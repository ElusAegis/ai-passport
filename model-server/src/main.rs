//! Demo model server with OpenAI-compatible API.
//!
//! This server provides a mock implementation of the OpenAI chat completions API
//! for testing purposes. It can generate responses that respect word count
//! requests in prompts and max_tokens limits.
//!
//! # Configuration
//!
//! Environment variables:
//! - `MODEL_API_PORT` (optional, default: 3000): Port to bind to
//! - `MODEL_API_KEY` (optional): API key for authentication
//! - `MODEL_SERVER_TLS_CERT` (required): Path to TLS certificate
//! - `MODEL_SERVER_TLS_KEY` (required): Path to TLS private key

mod config;
mod handlers;
mod middleware;
mod response;
mod tls;
mod types;

use anyhow::Result;
use axum::http::Method;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

use config::Config;
use handlers::{chat_completions, list_models, AppState};
use middleware::require_api_key;
use tls::rustls_config_from_paths;
use types::demo_models;

#[tokio::main]
async fn main() -> Result<()> {
    // Default to debug for model_server, info for dependencies
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("debug,hyper=info,rustls=info,tower_http=info"));
    fmt().with_env_filter(env_filter).with_target(false).init();

    let config = Arc::new(Config::from_env()?);

    info!(
        addr = %config.bind_addr,
        api_key = config.api_key.is_some(),
        "starting model server"
    );

    let state = AppState {
        models: Arc::new(demo_models()),
    };

    // CORS for local dev (allows any origin; tighten later if needed)
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .allow_origin(Any);

    let public = Router::new()
        .route("/v1/models", get(list_models))
        .with_state(state.clone());

    // Chat completions route is protected by API key
    let protected = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route_layer(from_fn_with_state(config.clone(), require_api_key));

    let app = public
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let tls = rustls_config_from_paths(&config.cert_path, &config.key_path).await?;

    info!("listening on https://{}", config.bind_addr);
    axum_server::bind_rustls(config.bind_addr, tls)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

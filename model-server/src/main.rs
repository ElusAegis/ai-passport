use anyhow::Result;
use anyhow::{bail, Context};
use axum::extract::{FromRef, Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_server::tls_rustls::RustlsConfig;
use rustls::crypto::aws_lc_rs::default_provider;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{version, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::ops::Add;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use time::OffsetDateTime;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, EnvFilter};
use uuid::Uuid;

#[derive(Serialize, Clone)]
struct Model {
    id: String,
    object: &'static str,
    created: i64,
    owned_by: &'static str,
}

#[derive(Serialize)]
struct ModelList {
    object: &'static str,
    data: Vec<Model>,
}

#[derive(Deserialize, Serialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    // optional OpenAI fields allowed; ignore for now
    // temperature, top_p, stream, etc.
}

#[derive(Serialize)]
struct ChatChoice {
    index: usize,
    message: ChatMessage,
    finish_reason: &'static str,
}

#[derive(Serialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Serialize)]
struct ChatResponse {
    id: String,
    object: &'static str,
    created: i64,
    model: String,
    choices: Vec<ChatChoice>,
    usage: Usage,
}

// ---------- App State ----------
#[derive(Clone)]
struct AppState {
    models: Arc<Vec<Model>>,
    config: Arc<Config>,
}

#[derive(Clone)]
struct Config {
    bind_addr: SocketAddr,
    api_key: Option<String>,
    cert_path: String,
    key_path: String,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let bind_addr: SocketAddr = "0.0.0.0:"
            .to_string()
            .add(
                env::var("MODEL_API_PORT")
                    .unwrap_or_else(|_| "3000".into())
                    .as_str(),
            )
            .parse()?;
        let api_key = env::var("MODEL_API_KEY").ok().filter(|s| !s.is_empty());
        let cert_path = env::var("SERVER_TLS_CERT").context("SERVER_TLS_CERT must be set")?;
        let key_path = env::var("SERVER_TLS_KEY").context("SERVER_TLS_KEY must be set")?;
        Ok(Self {
            bind_addr,
            api_key,
            cert_path,
            key_path,
        })
    }
}

impl FromRef<AppState> for Arc<Config> {
    fn from_ref(state: &AppState) -> Arc<Config> {
        state.config.clone()
    }
}

fn fixed_models() -> Vec<Model> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    vec![
        Model {
            id: "demo-gpt-4o-mini".into(),
            object: "model",
            created: now,
            owned_by: "you",
        },
        Model {
            id: "demo-gpt-3.5-turbo".into(),
            object: "model",
            created: now,
            owned_by: "you",
        },
    ]
}

fn fixed_reply(req: &ChatRequest) -> String {
    let last_user = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    match req.model.as_str() {
        "demo-gpt-4o-mini" => format!("You said: \"{}\" — fixed reply.", last_user),
        "demo-gpt-3.5-turbo" => "Hello from demo-gpt-3.5-turbo (fixed).".to_string(),
        _ => "Unknown model (demo server) — generic fixed reply.".to_string(),
    }
}

// ---------- Handlers ----------

async fn list_models(State(state): State<AppState>) -> Json<ModelList> {
    Json(ModelList {
        object: "list".into(),
        data: state.models.to_vec(),
    })
}

async fn chat_completions(
    Json(req): Json<ChatRequest>,
) -> (StatusCode, HeaderMap, Json<ChatResponse>) {
    let created = OffsetDateTime::now_utc().unix_timestamp();
    let content = fixed_reply(&req);

    let resp = ChatResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion",
        created,
        model: req.model.clone(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".into(),
                content,
            },
            finish_reason: "stop",
        }],
        usage: Usage {
            prompt_tokens: 8,
            completion_tokens: 12,
            total_tokens: 20,
        },
    };

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert("OpenAI-Model", req.model.parse().unwrap());
    (StatusCode::OK, headers, Json(resp))
}

// ---------- Middleware ----------
// Optional API key: set API_KEY env to enable. Expect "Authorization: Bearer <key>"
async fn require_api_key(
    State(cfg): State<Arc<Config>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    if let Some(expected) = &cfg.api_key {
        let auth = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        // Must be "Bearer <token>"
        let Some(provided) = auth.strip_prefix("Bearer ") else {
            return Err((StatusCode::UNAUTHORIZED, "missing or invalid API key"));
        };

        // Constant-time comparison to avoid timing side-channels
        let ok: bool = provided.as_bytes().ct_eq(expected.as_bytes()).into();

        if !ok {
            return Err((StatusCode::UNAUTHORIZED, "missing or invalid API key"));
        }
    }
    Ok(next.run(req).await)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // RUST_LOG=info ./mini-openai
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,axum=info,tower_http=info"));
    fmt().with_env_filter(env_filter).with_target(false).init();

    let config = Arc::new(Config::from_env()?);

    let state = AppState {
        models: Arc::new(fixed_models()),
        config: config.clone(),
    };

    // CORS for local dev (allows any origin; tighten later if needed)
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
        .allow_origin(Any);

    let public = Router::new()
        .route("/v1/models", get(list_models))
        .with_state(state.clone());

    // Only this route is protected
    let protected = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state.clone())
        .route_layer(from_fn_with_state(config.clone(), require_api_key));

    let app = public
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let tls = rustls_config_from_paths(&config.cert_path, &config.key_path).await?;

    println!("Listening on https://{}", config.bind_addr);
    axum_server::bind_rustls(config.bind_addr, tls)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

/// Load the first private key found in `path` (PKCS#8 → PKCS#1 → SEC1).
fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    // Try PKCS#8 first
    {
        let mut r = BufReader::new(File::open(path).with_context(|| format!("open {}", path))?);
        if let Some(key) = pkcs8_private_keys(&mut r).flatten().next() {
            return Ok(PrivateKeyDer::from(key));
        }
        _ = r;
    }
    // Then PKCS#1 (RSA)
    {
        let mut r = BufReader::new(File::open(path).with_context(|| format!("open {}", path))?);
        if let Some(key) = rsa_private_keys(&mut r).flatten().next() {
            return Ok(PrivateKeyDer::from(key));
        }
        _ = r;
    }

    bail!("no private key found in {path} (tried PKCS#8, PKCS#1, SEC1)");
}

/// Load a cert chain into rustls-compatible types.
fn load_cert_chain(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let mut r = BufReader::new(File::open(path).with_context(|| format!("open {}", path))?);
    certs(&mut r)
        .collect::<Result<Vec<CertificateDer>, _>>()
        .map_err(Into::into)
}

async fn rustls_config_from_paths(cert_path: &str, key_path: &str) -> anyhow::Result<RustlsConfig> {
    // Load cert chain
    let certs = load_cert_chain(cert_path)?;
    let key = load_private_key(key_path)?;

    // Explicitly select TLS versions: TLS1.3 *and* TLS1.2
    let provider = default_provider(); // standard crypto backend
    let mut config = ServerConfig::builder_with_provider(<Arc<CryptoProvider>>::from(provider))
        .with_protocol_versions(&[&version::TLS13, &version::TLS12])?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    // Advertise ALPN for h2 and http/1.1 (helps various clients)
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(RustlsConfig::from_config(Arc::new(config)))
}

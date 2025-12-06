//! Server configuration.

use anyhow::Context;
use std::env;
use std::net::SocketAddr;
use std::ops::Add;

/// Server configuration loaded from environment variables.
#[derive(Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub api_key: Option<String>,
    pub cert_path: String,
    pub key_path: String,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Required:
    /// - `MODEL_SERVER_TLS_CERT`: Path to TLS certificate
    /// - `MODEL_SERVER_TLS_KEY`: Path to TLS private key
    ///
    /// Optional:
    /// - `MODEL_API_PORT`: Port to bind to (default: 3000)
    /// - `MODEL_API_KEY`: API key for authentication (if set, enables auth)
    pub fn from_env() -> anyhow::Result<Self> {
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
        let cert_path =
            env::var("MODEL_SERVER_TLS_CERT").context("MODEL_SERVER_TLS_CERT must be set")?;
        let key_path =
            env::var("MODEL_SERVER_TLS_KEY").context("MODEL_SERVER_TLS_KEY must be set")?;

        Ok(Self {
            bind_addr,
            api_key,
            cert_path,
            key_path,
        })
    }
}

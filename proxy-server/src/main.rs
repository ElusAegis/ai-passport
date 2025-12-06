//! Attestation proxy server binary.
//!
//! Run with:
//! ```bash
//! proxy-server --cert cert.pem --key key.pem --signing-key signing.pem
//! ```

use anyhow::Result;
use clap::Parser;
use proxy_server::{run_server, ProxyConfig};
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "proxy-server")]
#[command(about = "Attestation proxy server for AI Passport")]
struct Args {
    /// Address to listen on
    #[arg(short, long, default_value = "0.0.0.0:8443")]
    listen: SocketAddr,

    /// Path to TLS certificate file (PEM format)
    #[arg(short, long, env = "PROXY_SERVER_TLS_CERT")]
    cert: String,

    /// Path to TLS private key file (PEM format)
    #[arg(short, long, env = "PROXY_SERVER_TLS_KEY")]
    key: String,

    /// Path to ECDSA signing key file (PEM format, secp256k1)
    #[arg(short, long, env = "PROXY_SERVER_SIGNING_KEY")]
    signing_key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "proxy_server=debug,info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    info!("Starting proxy server");
    info!("  Listen: {}", args.listen);
    info!("  TLS cert: {}", args.cert);
    info!("  TLS key: {}", args.key);
    info!("  Signing key: {}", args.signing_key);

    let config = ProxyConfig {
        listen_addr: args.listen,
        cert_path: args.cert,
        key_path: args.key,
        signing_key_path: args.signing_key,
    };

    run_server(config).await
}

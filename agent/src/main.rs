#![feature(assert_matches)]
//! VeriTrade - Autonomous AI Trading Agent
//!
//! An AI-powered portfolio manager that analyzes market data and makes
//! trading decisions. Supports multiple attestation modes for verifiable
//! execution.
//!
//! # Usage
//!
//! ```bash
//! # Run with default settings (1 round, direct prover)
//! cargo run -p agent
//!
//! # Run with custom settings
//! cargo run -p agent -- --rounds 3 --round-delay 60
//!
//! # Run with TLS notarization (requires notary server)
//! cargo run -p agent -- --prover tls-single
//! ```
//!
//! # Environment Variables
//!
//! - `MODEL_API_DOMAIN`: API domain for the LLM provider (or use --api-domain)
//! - `MODEL_API_KEY`: API key for authentication
//! - `MODEL_API_PORT`: API port (default: 443)
//! - `MODEL_ID`: Model to use
//! - `AGENT_ROUNDS`: Number of trading rounds (default: 1)
//! - `AGENT_ROUND_DELAY`: Delay between rounds in seconds (default: 0)
//! - `POLYMARKET_LIMIT`: Number of markets to fetch (default: 5)
//! - `PROVER`: Prover type (direct, proxy, tls-single, tls-per-message)

mod cli;
mod core;
mod portfolio;
mod tools;
mod utils;

use crate::cli::AgentArgs;
use crate::core::input_source::AgentInputSource;
use crate::portfolio::PortfolioState;
use crate::tools::coingecko::CoinGeckoTool;
use crate::tools::polymarket::PolymarketTool;
use crate::tools::portfolio::PortfolioTool;
use crate::tools::{AttestationMode, Tool};
use crate::utils::logging::init_logging;
use ai_passport::{
    with_input_source, ApiProvider, DirectProver, NetworkSetting, NotaryConfig, NotaryMode,
    ProveConfig, Prover, ProverKind, ProxyConfig, ProxyProver, TlsPerMessageProver,
    TlsSingleShotProver,
};
use anyhow::Context;
use clap::Parser;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

const KIB: usize = 1024;

/// Hardcoded proxy config: proxy-tee.proof-of-autonomy.elusaegis.xyz:8443
fn proxy_tee_config() -> ProxyConfig {
    ProxyConfig {
        host: "proxy-tee.proof-of-autonomy.elusaegis.xyz".to_string(),
        port: 8443,
    }
}

/// Hardcoded notary config: notary.proof-of-autonomy.elusaegis.xyz:7047
fn notary_remote_config() -> NotaryConfig {
    NotaryConfig::builder()
        .domain("notary.proof-of-autonomy.elusaegis.xyz".to_string())
        .port(7047u16)
        .path_prefix("".to_string())
        .mode(NotaryMode::RemoteTLS)
        .max_total_sent(64 * KIB)
        .max_total_recv(64 * KIB)
        .max_decrypted_online(64 * KIB)
        .defer_decryption(true)
        .network_optimization(NetworkSetting::Bandwidth)
        .build()
        .expect("Failed to build NotaryConfig")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file
    let _ = dotenvy::from_filename(".env");

    // Parse CLI arguments first (this handles --help, etc.)
    let args = AgentArgs::parse();

    init_logging();

    info!("═══════════════════════════════════════════════════════════════");
    info!("  VeriTrade - Autonomous AI Trading Agent");
    info!("═══════════════════════════════════════════════════════════════");

    // Load API configuration - domain/port from CLI args, key from environment
    let api_domain = args.api_domain.clone();
    let api_port = args.api_port;
    let api_key = env::var("MODEL_API_KEY")
        .context("MODEL_API_KEY not set. Please set it to your LLM provider API key.")?;

    // Build API provider (auto-detects provider type from domain)
    let api_provider = ApiProvider::builder()
        .domain(api_domain.clone())
        .port(api_port)
        .api_key(api_key)
        .build()
        .context("Failed to build ApiProvider")?;

    // Get model ID from args or environment
    let model_id = args
        .model_id
        .or_else(|| env::var("MODEL_ID").ok())
        .unwrap_or_else(|| {
            // Default model based on provider
            if api_domain.contains("anthropic") {
                "claude-sonnet-4-20250514".to_string()
            } else {
                "gpt-4o".to_string()
            }
        });

    // Build ProveConfig for the prover
    let prove_config = ProveConfig::builder()
        .provider(api_provider)
        .model_id(model_id.clone())
        .expected_exchanges(args.rounds as u32)
        .max_request_bytes(5 * KIB as u32)
        .max_response_bytes(3 * KIB as u32)
        .build()
        .context("Failed to build ProveConfig")?;

    // Parse round delay
    let round_delay = if args.round_delay > 0 {
        Some(Duration::from_secs(args.round_delay))
    } else {
        None
    };

    info!("Configuration:");
    info!("  API Domain: {}:{}", api_domain, api_port);
    info!("  Model: {}", model_id);
    info!("  Prover: {:?}", args.prover);
    info!("  Rounds: {}", args.rounds);
    if let Some(delay) = round_delay {
        info!("  Round delay: {:?}", delay);
    }
    info!("  Polymarket markets: {}", args.polymarket_limit);
    if args.polymarket_random_page {
        info!("  Polymarket random pagination: enabled (pages 0-4)");
    }

    // Initialize portfolio with sample positions
    let portfolio = PortfolioState::sample();
    info!(
        "Initial portfolio value: ${:.2}",
        portfolio.total_value_usd()
    );

    // Create tools
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(PortfolioTool::new()),
        Arc::new(CoinGeckoTool::new()),
        Arc::new(PolymarketTool::new(args.polymarket_limit, args.polymarket_random_page)),
    ];

    info!(
        "Tools: {:?}",
        tools.iter().map(|t| t.name()).collect::<Vec<_>>()
    );

    // Create the agent input source
    let input_source = AgentInputSource::new(
        portfolio,
        tools,
        args.rounds,
        AttestationMode::Direct, // Tool attestation mode (separate from LLM prover)
        round_delay,
    );

    // Create and run the appropriate prover
    info!("Starting agent with {:?} prover...", args.prover);

    match args.prover {
        ProverKind::Direct => {
            let prover = DirectProver::new();
            with_input_source(input_source, prover.run(&prove_config)).await?;
        }
        ProverKind::Proxy => {
            let proxy_config = proxy_tee_config();
            info!(
                "Using proxy-TEE: {}:{}",
                proxy_config.host, proxy_config.port
            );
            let prover = ProxyProver::new(proxy_config);
            with_input_source(input_source, prover.run(&prove_config)).await?;
        }
        ProverKind::TlsSingleShot => {
            let notary_config = notary_remote_config();
            info!(
                "Using TLS single-shot with notary: {}:{}",
                notary_config.domain, notary_config.port
            );
            let prover = TlsSingleShotProver::new(notary_config);
            with_input_source(input_source, prover.run(&prove_config)).await?;
        }
        ProverKind::TlsPerMessage => {
            let notary_config = notary_remote_config();
            info!(
                "Using TLS per-message with notary: {}:{}",
                notary_config.domain, notary_config.port
            );
            let prover = TlsPerMessageProver::new(notary_config);
            with_input_source(input_source, prover.run(&prove_config)).await?;
        }
    }

    info!("Agent completed successfully.");

    Ok(())
}

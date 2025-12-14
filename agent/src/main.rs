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
use ai_passport::{with_input_source, ApiProvider, DirectProver, ProveConfig, Prover, ProverKind};
use anyhow::Context;
use clap::Parser;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

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
        Arc::new(PolymarketTool::new(args.polymarket_limit)),
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
            // TODO: Implement proxy prover support
            anyhow::bail!(
                "Proxy prover not yet implemented for agent. Use --prover direct for now."
            );
        }
        ProverKind::TlsSingleShot => {
            // TODO: Implement TLS single-shot prover support
            anyhow::bail!("TLS single-shot prover not yet implemented for agent. Use --prover direct for now.");
        }
        ProverKind::TlsPerMessage => {
            // TODO: Implement TLS per-message prover support
            anyhow::bail!("TLS per-message prover not yet implemented for agent. Use --prover direct for now.");
        }
    }

    info!("Agent completed successfully.");

    Ok(())
}

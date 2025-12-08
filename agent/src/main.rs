#![feature(assert_matches)]

//! AI Agent with optional attestation support.
//!
//! This agent fetches data from external sources (Polymarket, price feeds)
//! and generates investment decisions. When run with `--attested`, all
//! external API calls are routed through the attestation proxy for
//! cryptographic proof generation.
//!
//! Usage:
//!   cargo run --bin agent              # Direct mode (no attestation)
//!   cargo run --bin agent -- --attested  # Attested mode via proxy

use crate::polymarket::agent_msg::build_polymarket_context;
use crate::polymarket::fetch::Market;
use crate::portfolio::fetch::fetch_current;
use crate::portfolio::price_feed::coingeko::CoingeckoProvider;
use crate::portfolio::price_feed::context::build_portfolio_context;
use crate::portfolio::price_feed::enrich::with_prices;
use crate::utils::logging::init_logging;
use crate::utils::notary_config::gen_cfg;
use crate::utils::proxy_client::{ProxyClient, ProxyClientConfig};
use ai_passport::{with_input_source, DirectProver, Prover, VecInputSource};
use std::env;

mod decision;
mod polymarket;
mod portfolio;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    // Check for --attested flag
    let attested_mode = env::args().any(|arg| arg == "--attested");

    const KIB: usize = 1024;
    const LIMIT: usize = 14 * KIB;

    // Fetch data - either directly or via proxy for attestation
    let markets = if attested_mode {
        println!("Running in ATTESTED mode - fetching data via proxy");
        fetch_data_attested(3).await?
    } else {
        println!("Running in DIRECT mode - no attestation");
        Market::get(3).await?
    };

    let polymarket_ctx = build_polymarket_context(&markets, 12 * KIB)?;
    println!("Polymarket context size: {} bytes", polymarket_ctx.len());

    let portfolio = fetch_current().await;
    let provider = CoingeckoProvider::new();

    let priced = with_prices(&portfolio, &provider).await?;

    let portfolio_ctx = build_portfolio_context(priced, 2 * KIB);
    println!("Portfolio context size: {} bytes", portfolio_ctx.len());

    let decision_json = decision::build_decision_request(&polymarket_ctx, &portfolio_ctx, LIMIT)?;
    println!("{decision_json}");
    println!("Decision request size: {} bytes", decision_json.len());

    let src = VecInputSource::new(vec![decision_json]);
    let cfg = gen_cfg(LIMIT, LIMIT + KIB)?;
    let prover = DirectProver::new();

    if let Err(e) = with_input_source(src, prover.run(&cfg)).await {
        eprintln!("Prove failed: {}", e);
        return Err(e);
    }

    println!("Success!");

    Ok(())
}

/// Fetch all external data through the attestation proxy.
///
/// This creates a single proxy connection, fetches all required data,
/// and then requests an attestation covering all the API calls.
async fn fetch_data_attested(market_limit: usize) -> anyhow::Result<Vec<Market>> {
    // Get proxy config from environment or use defaults
    let proxy_config = ProxyClientConfig {
        host: env::var("PROXY_HOST").unwrap_or_else(|_| "localhost".to_string()),
        port: env::var("PROXY_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8443),
    };

    println!(
        "Connecting to proxy at {}:{}",
        proxy_config.host, proxy_config.port
    );

    let mut proxy = ProxyClient::new(proxy_config);
    proxy.connect().await?;

    // Fetch markets through the proxy
    let markets = Market::get_via_proxy(&mut proxy, market_limit).await?;
    println!("Fetched {} markets via proxy", markets.len());

    // Request attestation for all the API calls made
    let attestation_path = proxy.request_attestation(Market::api_domain()).await?;
    println!(
        "Data fetch attestation saved to: {}",
        attestation_path.display()
    );

    Ok(markets)
}

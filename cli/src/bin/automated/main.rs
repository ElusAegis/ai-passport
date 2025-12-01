//! Automated benchmark binary for testing provers with consistent message sizes.
//!
//! This binary generates messages of fixed sizes to enable reproducible benchmarking
//! across different provers and model providers.
//!
//! # Configuration
//!
//! Environment variables:
//! - `MODEL_API_KEY` (required): API key for the model provider
//! - `MODEL_API_DOMAIN` (required): Domain of the API endpoint
//! - `MODEL_ID` (required): Model identifier to use
//! - `MODEL_API_PORT` (optional, default: 443): API port
//! - `BENCHMARK_REQUEST_BYTES` (optional, default: 500): Target request size in bytes
//! - `BENCHMARK_RESPONSE_BYTES` (optional, default: 500): Target response size in bytes
//! - `BENCHMARK_MAX_ROUNDS` (optional, default: 5): Maximum rounds to run
//!
//! # Output
//!
//! Results are saved to `benchmarks/{provider}_{model}_{messages}_{req_bytes}_{resp_bytes}.jsonl`
//! in JSONL format, with one JSON object per benchmark run. Failed benchmarks are saved
//! with a `_failed` suffix.

mod input_source;
mod results;
mod runner;
mod stats;

use ai_passport::{AgentProver, ApiProvider, DirectProver, ProveConfig, BYTES_PER_TOKEN};
use anyhow::Context;
use dotenvy::var;
use results::BenchmarkConfig;
use runner::run_benchmark;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let _ = dotenvy::dotenv().ok();

    // Required configuration
    let api_key = var("MODEL_API_KEY").context("MODEL_API_KEY must be set")?;
    let domain = var("MODEL_API_DOMAIN").context("MODEL_API_DOMAIN must be set")?;
    let model_id = var("MODEL_ID").context("MODEL_ID must be set")?;

    // Optional configuration with defaults
    let port = var("MODEL_API_PORT")
        .map(|p| p.parse::<u16>())
        .unwrap_or(Ok(443))?;

    let target_request_bytes = var("BENCHMARK_REQUEST_BYTES")
        .map(|b| b.parse::<usize>())
        .unwrap_or(Ok(500))?;

    let target_response_bytes = var("BENCHMARK_RESPONSE_BYTES")
        .map(|t| t.parse::<u32>())
        .unwrap_or(Ok(500))?;

    let max_rounds = var("BENCHMARK_MAX_ROUNDS")
        .ok()
        .map(|r| r.parse::<usize>())
        .transpose()?
        .or(Some(5));

    info!("Benchmark configuration:");
    info!("  Domain: {}:{}", domain, port);
    info!("  Model: {}", model_id);
    info!("  Target request size: {} bytes", target_request_bytes);
    info!("  Target response size: {} bytes", target_response_bytes);
    info!(
        "  Max rounds: {}",
        max_rounds
            .map(|r| r.to_string())
            .unwrap_or_else(|| "unlimited".to_string())
    );

    // Build the benchmark config (only benchmark-specific fields)
    let benchmark_config = BenchmarkConfig {
        target_request_bytes,
        target_response_bytes,
        max_rounds,
    };

    let api_provider = ApiProvider::builder()
        .domain(domain)
        .port(port)
        .api_key(api_key)
        .build()
        .context("Failed to build ApiProvider")?;

    let prove_config = ProveConfig::builder()
        .provider(api_provider)
        .model_id(model_id)
        .max_response_tokens(target_response_bytes / BYTES_PER_TOKEN as u32)
        .build()
        .context("Failed to build ProveConfig")?;

    let prover = AgentProver::Direct(DirectProver {});

    run_benchmark(benchmark_config, prove_config, prover).await?;

    Ok(())
}
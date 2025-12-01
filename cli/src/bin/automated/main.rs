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
mod presets;
mod results;
mod runner;
mod stats;

use ai_passport::{ApiProvider, ProveConfig};
use anyhow::Context;
use dotenvy::var;
use presets::{all_notary_presets, all_prover_presets};
use results::BenchmarkConfig;
use runner::run_benchmark;
use tracing::{error, info};

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
        .map(|t| t.parse::<u32>())
        .unwrap_or(Ok(500))?;
    let target_response_bytes = var("BENCHMARK_RESPONSE_BYTES")
        .map(|t| t.parse::<u32>())
        .unwrap_or(Ok(500))?;

    let max_rounds = var("BENCHMARK_MAX_ROUNDS")
        .ok()
        .map(|r| r.parse::<usize>())
        .unwrap_or(Ok(10))?;

    let notary_max_recv_overwrite = var("NOTARY_MAX_RECV_OVERWRITE")
        .map(|v| v.parse::<usize>().ok())
        .ok()
        .flatten();
    let notary_max_send_overwrite = var("NOTARY_MAX_SEND_OVERWRITE")
        .map(|v| v.parse::<usize>().ok())
        .ok()
        .flatten();

    info!("Benchmark configuration:");
    info!("  Domain: {}:{}", domain, port);
    info!("  Model: {}", model_id);
    info!("  Target request size: {} bytes", target_request_bytes);
    info!("  Target response size: {} bytes", target_response_bytes);
    info!("  Max rounds: {}", max_rounds);

    // Build the API provider (shared across all provers)
    let api_provider = ApiProvider::builder()
        .domain(&domain)
        .port(port)
        .api_key(&api_key)
        .build()
        .context("Failed to build ApiProvider")?;

    // Get all presets
    let prover_presets = all_prover_presets();
    let notary_presets = all_notary_presets();

    info!(
        "Running {} prover(s) x {} notary preset(s)",
        prover_presets.len(),
        notary_presets.len()
    );

    let prove_config = ProveConfig::builder()
        .provider(api_provider.clone())
        .model_id(&model_id)
        .max_request_bytes(target_request_bytes)
        .max_response_bytes(target_response_bytes)
        .build()
        .context("Failed to build ProveConfig")?;

    let benchmark_config = BenchmarkConfig {
        target_request_bytes,
        target_response_bytes,
        max_rounds: Some(max_rounds),
    };

    // Track results
    let mut success_count = 0;
    let mut failure_count = 0;

    // Iterate over all prover presets
    for prover_preset in prover_presets {
        // Run with each notary preset (if not required, only run first)
        for notary_preset in &notary_presets {
            info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            info!(
                "Running: {} ({})",
                prover_preset.name,
                if prover_preset.requires_notary() {
                    notary_preset.name
                } else {
                    "no notary"
                }
            );

            let mut notary_preset = (*notary_preset).clone();
            if let Some(overwrite) = notary_max_recv_overwrite {
                notary_preset.max_recv_bytes = overwrite;
            }
            if let Some(overwrite) = notary_max_send_overwrite {
                notary_preset.max_sent_bytes = overwrite;
            }

            let prover = prover_preset.build(&notary_preset);

            match run_benchmark(&benchmark_config, &prove_config, prover).await {
                Ok(path) => {
                    info!("Completed: {}", path.display());
                    success_count += 1;
                }
                Err(e) => {
                    error!(
                        "Failed {} + {}: {}",
                        prover_preset.name, notary_preset.name, e
                    );
                    failure_count += 1;
                }
            }

            if !prover_preset.requires_notary() {
                // If the prover does not require a notary, skip further notary presets
                break;
            }
        }
    }

    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("Benchmark run complete: {success_count} succeeded, {failure_count} failed",);

    if failure_count > 0 {
        anyhow::bail!("{failure_count} benchmark(s) failed");
    }

    Ok(())
}

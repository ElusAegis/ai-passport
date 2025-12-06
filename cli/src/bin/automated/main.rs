//! Automated benchmark binary for testing provers with consistent message sizes.
//!
//! This binary generates messages of fixed sizes to enable reproducible benchmarking
//! across different provers and model providers.
//!
//! # Configuration
//!
//! Environment variables:
//!
//! ## Model Configuration (choose one approach)
//!
//! ### Option 1: Use presets (recommended)
//! - `MODEL_PRESETS` (optional): Comma-separated list of model preset names. Available:
//!   - `custom-instant`: Custom API with "instant" model
//!   - `custom-demo-gpt-4o-mini`: Custom API with "demo-gpt-4o-mini" model
//!   - `anthropic-haiku`: Anthropic API with Claude Haiku 4.5
//!   - `anthropic-sonnet`: Anthropic API with Claude Sonnet 4.5
//!   - `anthropic-opus`: Anthropic API with Claude Opus 4.5
//!   - `phala-haiku`: Phala Red Pill API with Claude Haiku 4.5
//!
//! ### Option 2: Manual configuration
//! - `MODEL_API_KEY` (required if no preset): API key for the model provider
//! - `MODEL_API_DOMAIN` (required if no preset): Domain of the API endpoint
//! - `MODEL_ID` (required if no preset): Model identifier to use
//! - `MODEL_API_PORT` (optional, default: 443): API port
//!
//! ## Benchmark Configuration
//! - `BENCHMARK_REPETITIONS` (optional, default: 1): Number of times to repeat the full benchmark suite
//! - `BENCHMARK_REQUEST_BYTES` (optional, default: 500): Target request size in bytes
//! - `BENCHMARK_RESPONSE_BYTES` (optional, default: 500): Target response size in bytes
//! - `BENCHMARK_MAX_ROUNDS` (optional, default: 10): Maximum rounds to run.
//!   Comma-separated list supported (e.g., "5,10,20"). For the largest value,
//!   only the direct prover is used.
//! - `PROVER_PRESETS` (optional): Comma-separated list of prover preset names to run
//!   (e.g., "direct,tls_single_shot"). If not set, all presets are used.
//! - `NOTARY_PRESETS` (optional): Comma-separated list of notary preset names to use
//!   (e.g., "notary-local,notary-pse"). If not set, all presets are used.
//! - `NOTARY_MAX_RECV_OVERWRITE` (optional): Override max receive bytes for notary
//! - `NOTARY_MAX_SEND_OVERWRITE` (optional): Override max send bytes for notary
//! - `NOTARY_NETWORK_OPTIMIZATION_OVERWRITE` (optional): Override network optimization
//!   ("bandwidth" or "latency")
//! - `REQUEST_TIMEOUT_SECS` (optional, default: 10): Timeout in seconds for each API request
//!
//! # Output
//!
//! Results are saved to `benchmarks/{provider}_{model}_{messages}_{req_bytes}_{resp_bytes}.jsonl`
//! in JSONL format, with one JSON object per benchmark run. Failed benchmarks are saved
//! with a `_failed` suffix.
//!
//! Logs are also saved to `benchmarks/logs/benchmark_{timestamp}.log` with trace level
//! for `ai_passport` and `automated_benchmarks` modules.

mod input_source;
mod logging;
mod presets;
mod results;
mod runner;
mod stats;

use crate::logging::setup_logging;
use ai_passport::ProveConfig;
use anyhow::Context;
use dotenvy::var;
use presets::{
    load_model_presets, load_notary_presets, load_prover_presets, parse_network_setting,
};
use results::BenchmarkConfig;
use runner::run_benchmark;
use std::time::Duration;
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv().ok();

    // Set up logging (keep guard alive for the duration of main)
    let _log_guard = setup_logging()?;

    // Benchmark configuration
    let repetitions = var("BENCHMARK_REPETITIONS")
        .ok()
        .map(|r| r.parse::<usize>())
        .unwrap_or(Ok(1))?;
    let target_request_bytes = var("BENCHMARK_REQUEST_BYTES")
        .map(|t| t.parse::<u32>())
        .unwrap_or(Ok(500))?;
    let target_response_bytes = var("BENCHMARK_RESPONSE_BYTES")
        .map(|t| t.parse::<u32>())
        .unwrap_or(Ok(500))?;

    // Parse max_rounds as comma-separated list
    let max_rounds_list: Vec<usize> = var("BENCHMARK_MAX_ROUNDS")
        .unwrap_or_else(|_| "10".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .collect();
    let max_rounds_list = if max_rounds_list.is_empty() {
        vec![10]
    } else {
        max_rounds_list
    };
    let max_rounds_largest = *max_rounds_list.iter().max().unwrap();

    // Notary overrides
    let notary_max_recv_overwrite = var("NOTARY_MAX_RECV_OVERWRITE")
        .map(|v| v.parse::<usize>().ok())
        .ok()
        .flatten();
    let notary_max_send_overwrite = var("NOTARY_MAX_SEND_OVERWRITE")
        .map(|v| v.parse::<usize>().ok())
        .ok()
        .flatten();
    let notary_network_optimization_overwrite = var("NOTARY_NETWORK_OPTIMIZATION_OVERWRITE")
        .ok()
        .and_then(|v| {
            let parsed = parse_network_setting(&v);
            if parsed.is_none() {
                warn!(
                    "Invalid NOTARY_NETWORK_OPTIMIZATION_OVERWRITE '{}', expected 'bandwidth' or 'latency'",
                    v
                );
            }
            parsed
        });

    // Request timeout (default 10 seconds for automated benchmarks)
    let request_timeout_secs: u64 = var("REQUEST_TIMEOUT_SECS")
        .map(|v| v.parse::<u64>())
        .unwrap_or(Ok(10))?;
    let request_timeout = Duration::from_secs(request_timeout_secs);

    // Load presets from environment or use all
    let model_presets = load_model_presets();
    let prover_presets = load_prover_presets();
    let notary_presets = load_notary_presets();

    info!("Benchmark configuration:");
    info!("  Repetitions: {}", repetitions);
    info!("  Target request size: {} bytes", target_request_bytes);
    info!("  Target response size: {} bytes", target_response_bytes);
    info!(
        "  Max rounds: {:?} (largest: {})",
        max_rounds_list, max_rounds_largest
    );
    info!("  Request timeout: {}s", request_timeout_secs);

    // Track results
    let mut success_count = 0;
    let mut failure_count = 0;

    // Outer loop: repetitions (run full suite N times)
    for rep in 1..=repetitions {
        if repetitions > 1 {
            info!("▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓");
            info!("Repetition {}/{}", rep, repetitions);
        }

        // Iterate over max_rounds configurations
        for &max_rounds in &max_rounds_list {
            let is_largest_rounds = max_rounds == max_rounds_largest && max_rounds_list.len() > 1;

            if max_rounds_list.len() > 1 {
                info!("▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒");
                info!(
                    "Interaction rounds: {}{}",
                    max_rounds,
                    if is_largest_rounds {
                        " (direct prover only)"
                    } else {
                        ""
                    }
                );
            }

            let benchmark_config = BenchmarkConfig {
                target_request_bytes,
                target_response_bytes,
                max_rounds: Some(max_rounds),
            };

            // Iterate over all model presets
            for model_preset in &model_presets {
                info!("══════════════════════════════════════════════════════════════");
                info!(
                    "Model: {} ({}:{})",
                    model_preset.name, model_preset.domain, model_preset.port
                );

                let api_provider = model_preset.build_api_provider();

                let prove_config = ProveConfig::builder()
                    .provider(api_provider)
                    .model_id(&model_preset.model_id)
                    .max_request_bytes(target_request_bytes)
                    .max_response_bytes(target_response_bytes)
                    .expected_exchanges(max_rounds as u32)
                    .request_timeout(request_timeout)
                    .build()
                    .context("Failed to build ProveConfig")?;

                // Iterate over all prover presets
                // For the largest max_rounds, only use direct prover
                let active_provers: Vec<_> = if is_largest_rounds {
                    prover_presets
                        .iter()
                        .filter(|p| !p.requires_notary())
                        .collect()
                } else {
                    prover_presets.iter().collect()
                };

                for prover_preset in active_provers {
                    // Run with each notary preset (if not required, only run first)
                    for notary_preset in &notary_presets {
                        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                        info!(
                            "Running: {} / {} ({}) [rounds={}]",
                            model_preset.name,
                            prover_preset.name,
                            if prover_preset.requires_notary() {
                                notary_preset.name
                            } else {
                                "no notary"
                            },
                            max_rounds
                        );

                        let mut notary_preset = (*notary_preset).clone();
                        if let Some(overwrite) = notary_max_recv_overwrite {
                            notary_preset.max_recv_bytes = overwrite;
                        }
                        if let Some(overwrite) = notary_max_send_overwrite {
                            notary_preset.max_sent_bytes = overwrite;
                        }
                        if let Some(overwrite) = notary_network_optimization_overwrite {
                            notary_preset.network_optimization = overwrite;
                        }

                        let prover = prover_preset.build(&notary_preset);

                        match run_benchmark(&benchmark_config, &prove_config, prover).await {
                            Ok(path) => {
                                info!("Completed: {}", path.display());
                                success_count += 1;
                            }
                            Err(e) => {
                                error!(
                                    "Failed {} / {} + {}: {}",
                                    model_preset.name, prover_preset.name, notary_preset.name, e
                                );
                                failure_count += 1;
                                debug!("Error details: {:?}", e.chain().collect::<Vec<_>>());
                            }
                        }

                        if !prover_preset.requires_notary() {
                            // If the prover does not require a notary, skip further notary presets
                            break;
                        }
                    }
                }
            }
        }
    }

    info!("══════════════════════════════════════════════════════════════");
    info!(
        "Benchmark run complete: {} repetition(s), {success_count} succeeded, {failure_count} failed",
        repetitions
    );

    if failure_count > 0 {
        anyhow::bail!("{failure_count} benchmark(s) failed");
    }

    Ok(())
}

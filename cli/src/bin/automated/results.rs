//! Benchmark result storage and JSONL serialization.

use super::stats::BenchmarkStats;
use ai_passport::{AgentProver, ProveConfig};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tracing::info;

/// Benchmark-specific configuration (fields not derivable from ProveConfig/AgentProver).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Target request size in bytes.
    pub target_request_bytes: u32,
    /// Target response size in bytes.
    pub target_response_bytes: u32,
    /// Maximum rounds configured (None = unlimited).
    pub max_rounds: Option<usize>,
}

/// Per-round statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundResult {
    /// Round number (1-indexed).
    pub round: usize,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Request size in bytes.
    pub request_bytes: usize,
    /// Response size in bytes.
    pub response_bytes: usize,
}

/// Results from a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    /// Number of completed rounds.
    pub completed_rounds: usize,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
    /// Setup time in milliseconds (from benchmark start to first message request).
    pub setup_time_ms: Option<u64>,
    /// Per-round breakdown.
    pub rounds: Vec<RoundResult>,
}

/// Complete benchmark run record for JSONL storage.
///
/// Contains:
/// - `benchmark`: Benchmark-specific configuration
/// - `prover`: The prover configuration (serialized, includes notary config if applicable)
/// - `provider_name` and `model_id`: Provider identification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRecord {
    /// ISO 8601 timestamp of when the benchmark completed.
    pub timestamp: DateTime<Utc>,
    /// Provider name (e.g., "anthropic", "fireworks").
    pub provider_name: String,
    /// Model identifier.
    pub model_id: String,
    /// Benchmark-specific configuration.
    pub benchmark: BenchmarkConfig,
    /// Prover configuration (includes notary config if applicable).
    pub prover: AgentProver,
    /// Results from the run.
    pub results: BenchmarkResults,
    /// Whether the benchmark completed successfully.
    pub success: bool,
    /// Error message if the benchmark failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl BenchmarkRecord {
    /// Create a successful benchmark record from stats.
    pub fn from_stats(
        benchmark_config: &BenchmarkConfig,
        prove_config: &ProveConfig,
        prover: AgentProver,
        stats: &BenchmarkStats,
    ) -> Self {
        let round_durations_ms = stats.round_durations_ms();
        let request_sizes = stats.request_sizes();
        let response_sizes = stats.response_sizes();

        let rounds: Vec<RoundResult> = round_durations_ms
            .iter()
            .enumerate()
            .map(|(i, &duration_ms)| RoundResult {
                round: i + 1,
                duration_ms,
                request_bytes: request_sizes.get(i).copied().unwrap_or(0),
                response_bytes: response_sizes.get(i).copied().unwrap_or(0),
            })
            .collect();

        let total_duration_ms = stats
            .total_duration()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let setup_time_ms = stats.setup_duration().map(|d| d.as_millis() as u64);

        Self {
            timestamp: Utc::now(),
            provider_name: prove_config.provider.provider_name().to_string(),
            model_id: prove_config.model_id.clone(),
            benchmark: benchmark_config.clone(),
            prover,
            results: BenchmarkResults {
                completed_rounds: stats.completed_rounds(),
                total_duration_ms,
                setup_time_ms,
                rounds,
            },
            success: true,
            error: None,
        }
    }

    /// Create a failed benchmark record.
    pub fn failed(
        benchmark_config: &BenchmarkConfig,
        prove_config: &ProveConfig,
        prover: AgentProver,
        stats: &BenchmarkStats,
        error: String,
    ) -> Self {
        let mut record = Self::from_stats(benchmark_config, prove_config, prover, stats);
        record.success = false;
        record.error = Some(error);
        record
    }
}

/// Generate the JSONL filename for a benchmark record.
///
/// Format: `{provider}_{model}_{messages}_{req_bytes}_{resp_bytes}.jsonl`
/// Failed benchmarks get `_failed` suffix to keep them separate.
///
/// This groups benchmark results by their unique configuration.
pub fn generate_filename(record: &BenchmarkRecord) -> String {
    // Sanitize components for filesystem safety
    let provider = record
        .provider_name
        .replace(['/', '\\', ':', '.', '-'], "_");
    let model = record.model_id.replace(['/', '\\', ':', '.', '-'], "_");
    let messages = record.results.completed_rounds;
    let req_bytes = record.benchmark.target_request_bytes;
    let resp_bytes = record.benchmark.target_response_bytes;
    let failed_suffix = if record.success { "" } else { "_failed" };

    format!("{provider}_{model}_{messages}_{req_bytes}_{resp_bytes}{failed_suffix}.jsonl",)
}

/// Get the benchmarks directory, creating it if necessary.
pub fn benchmarks_dir() -> Result<PathBuf> {
    let dir = PathBuf::from("benchmarks");
    if !dir.exists() {
        fs::create_dir_all(&dir).context("Failed to create benchmarks directory")?;
    }
    Ok(dir)
}

/// Append a benchmark record to the appropriate JSONL file.
pub fn save_record(record: &BenchmarkRecord) -> Result<PathBuf> {
    let dir = benchmarks_dir()?;
    let filename = generate_filename(record);
    let path = dir.join(&filename);

    let json_line =
        serde_json::to_string(record).context("Failed to serialize benchmark record")?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open benchmark file: {}", path.display()))?;

    writeln!(file, "{}", json_line).context("Failed to write benchmark record")?;

    info!("Benchmark results saved to: {}", path.display());
    Ok(path)
}

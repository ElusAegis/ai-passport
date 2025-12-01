//! Benchmark result storage and JSONL serialization.

use super::stats::BenchmarkStats;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::info;

/// Configuration used for a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Type of prover used (e.g., "direct", "tls_single_shot", "tls_per_message").
    pub prover_type: String,
    /// API domain.
    pub domain: String,
    /// API port.
    pub port: u16,
    /// Model identifier.
    pub model_id: String,
    /// Notary sent capacity in bytes (None for direct prover).
    pub notary_sent_capacity: Option<usize>,
    /// Notary receive capacity in bytes (None for direct prover).
    pub notary_recv_capacity: Option<usize>,
    /// Target request size in bytes.
    pub target_request_bytes: usize,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRecord {
    /// ISO 8601 timestamp of when the benchmark completed.
    pub timestamp: DateTime<Utc>,
    /// Configuration used for this run.
    pub config: BenchmarkConfig,
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
    pub fn from_stats(config: BenchmarkConfig, stats: &BenchmarkStats) -> Self {
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
            config,
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
    pub fn failed(config: BenchmarkConfig, stats: &BenchmarkStats, error: String) -> Self {
        let mut record = Self::from_stats(config, stats);
        record.success = false;
        record.error = Some(error);
        record
    }
}

/// Generate the JSONL filename for a benchmark configuration.
pub fn generate_filename(config: &BenchmarkConfig) -> String {
    // Sanitize components for filesystem safety
    let domain = config.domain.replace(['/', '\\', ':'], "_");
    let model = config.model_id.replace(['/', '\\', ':'], "_");

    let capacity_suffix = match (config.notary_sent_capacity, config.notary_recv_capacity) {
        (Some(sent), Some(recv)) => format!("_{}_{}", sent, recv),
        _ => String::new(),
    };

    format!(
        "{}_{}_{}{}",
        config.prover_type, domain, model, capacity_suffix
    )
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
    let filename = format!("{}.jsonl", generate_filename(&record.config));
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

/// Load all records from a JSONL file.
#[allow(dead_code)]
pub fn load_records(path: &Path) -> Result<Vec<BenchmarkRecord>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read benchmark file: {}", path.display()))?;

    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .with_context(|| format!("Failed to parse benchmark record: {}", line))
        })
        .collect()
}

/// List all benchmark JSONL files in the benchmarks directory.
#[allow(dead_code)]
pub fn list_benchmark_files() -> Result<Vec<PathBuf>> {
    let dir = benchmarks_dir()?;

    let files: Vec<PathBuf> = fs::read_dir(&dir)
        .context("Failed to read benchmarks directory")?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|ext| ext == "jsonl").unwrap_or(false))
        .collect();

    Ok(files)
}

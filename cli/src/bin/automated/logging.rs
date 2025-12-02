//! Logging setup for automated benchmarks.
//!
//! Configures both console and file logging with appropriate filters.

use anyhow::Result;
use chrono::Utc;
use std::fs;
use tracing::info;
use tracing_subscriber::{
    fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

/// Set up logging to both console and file.
///
/// - Console: respects `RUST_LOG` env var, defaults to `info`
/// - File: trace level for `ai_passport` and `automated_benchmarks`, info for others
///
/// Log files are saved to `benchmarks/logs/benchmark_{timestamp}.log`.
///
/// Returns a guard that must be kept alive for the duration of the program
/// to ensure all logs are flushed.
pub fn setup_logging() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    // Create logs directory
    let log_dir = std::path::Path::new("benchmarks/logs");
    fs::create_dir_all(log_dir)?;

    // Generate timestamped log filename
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let log_filename = format!("benchmark_{}.log", timestamp);

    // Create file appender
    let file_appender = tracing_appender::rolling::never(log_dir, log_filename.clone());
    let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);

    // Console filter: respect RUST_LOG env var, default to info
    let console_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // File filter: trace for ai_passport and automated_benchmarks, info for others
    let file_filter = EnvFilter::new("info")
        .add_directive("ai_passport=trace".parse()?)
        .add_directive("automated_benchmarks=trace".parse()?);

    // Console layer
    let console_layer = fmt::layer()
        .with_target(true)
        .with_filter(console_filter);

    // File layer with timestamps
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(non_blocking_file)
        .with_filter(file_filter);

    // Initialize subscriber with both layers
    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .init();

    info!(
        "Logging to file: {}",
        log_dir.join(&log_filename).display()
    );

    Ok(guard)
}
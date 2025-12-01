//! Benchmark statistics collection and analysis.

use std::time::{Duration, Instant};
use tracing::info;

/// Setup timing state.
#[derive(Debug, Clone)]
enum SetupTime {
    /// Benchmark not yet initialized.
    NotInitialized,
    /// Waiting for first request, stores init instant.
    Pending(Instant),
    /// Setup complete, stores duration.
    Complete(Duration),
}

/// Statistics collected during benchmark execution.
#[derive(Debug, Clone)]
pub struct BenchmarkStats {
    /// Setup time tracking (from init to first message request).
    setup_time: SetupTime,
    /// When the current round started.
    round_start: Option<Instant>,
    /// Duration of each completed round (request to response).
    round_durations: Vec<Duration>,
    /// Response sizes in bytes for each round.
    response_sizes: Vec<usize>,
    /// Request sizes in bytes for each round.
    request_sizes: Vec<usize>,
}

impl Default for BenchmarkStats {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkStats {
    pub fn new() -> Self {
        Self {
            setup_time: SetupTime::NotInitialized,
            round_start: None,
            round_durations: Vec::new(),
            response_sizes: Vec::new(),
            request_sizes: Vec::new(),
        }
    }

    /// Initialize the benchmark timer. Call this before starting the prover.
    pub fn init(&mut self) {
        self.setup_time = SetupTime::Pending(Instant::now());
    }

    /// Record that a round has started with the given request size.
    pub fn start_round(&mut self, request_size: usize) {
        // On first round, finalize setup duration
        if let SetupTime::Pending(init) = self.setup_time {
            self.setup_time = SetupTime::Complete(init.elapsed());
        }
        self.round_start = Some(Instant::now());
        self.request_sizes.push(request_size);
    }

    /// Record that a round has completed with the given response size.
    pub fn complete_round(&mut self, response_size: usize) {
        if let Some(start) = self.round_start.take() {
            let duration = start.elapsed();
            self.round_durations.push(duration);
            self.response_sizes.push(response_size);
        }
    }

    /// Get the setup duration (time from init to first message request).
    pub fn setup_duration(&self) -> Option<Duration> {
        match self.setup_time {
            SetupTime::Complete(d) => Some(d),
            _ => None,
        }
    }

    /// Get total benchmark duration (from init to now).
    pub fn total_duration(&self) -> Option<Duration> {
        match self.setup_time {
            SetupTime::Pending(init) => Some(init.elapsed()),
            SetupTime::Complete(setup) => {
                let rounds_total: Duration = self.round_durations.iter().sum();
                Some(setup + rounds_total)
            }
            SetupTime::NotInitialized => None,
        }
    }

    /// Get number of completed rounds.
    pub fn completed_rounds(&self) -> usize {
        self.round_durations.len()
    }

    /// Get round durations as milliseconds.
    pub fn round_durations_ms(&self) -> Vec<u64> {
        self.round_durations
            .iter()
            .map(|d| d.as_millis() as u64)
            .collect()
    }

    /// Get response sizes.
    pub fn response_sizes(&self) -> &[usize] {
        &self.response_sizes
    }

    /// Get request sizes.
    pub fn request_sizes(&self) -> &[usize] {
        &self.request_sizes
    }

    /// Print a brief summary of the benchmark run.
    pub fn print_summary(&self) {
        info!("────────────────────────────────────────");
        info!("Benchmark complete:");
        if let Some(setup) = self.setup_duration() {
            info!("  Setup time: {:.2}s", setup.as_secs_f64());
        }
        info!("  Rounds: {}", self.round_durations.len());
        if let Some(total) = self.total_duration() {
            info!("  Total time: {:.2}s", total.as_secs_f64());
        }
        info!("────────────────────────────────────────");
    }
}
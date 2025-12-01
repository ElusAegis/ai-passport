//! Benchmark runner - executes a single benchmark run.

use super::input_source::BenchmarkInputSource;
use super::results::{save_record, BenchmarkConfig, BenchmarkRecord};
use ai_passport::{
    with_input_source, AgentProver, ChannelBudget, ChatMessage, InputSource, ProveConfig, Prover,
};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Run a single benchmark and save results.
///
/// Returns the path to the saved results file.
pub async fn run_benchmark(
    benchmark_config: BenchmarkConfig,
    prove_config: ProveConfig,
    prover: AgentProver,
) -> Result<PathBuf> {
    let target_request_bytes = benchmark_config.target_request_bytes;
    let target_response_bytes = benchmark_config.target_response_bytes;
    let max_rounds = benchmark_config.max_rounds;

    // Create input source wrapped in Arc<Mutex> so we can access stats after the run
    let input_source = Arc::new(Mutex::new(BenchmarkInputSource::new(
        target_request_bytes,
        target_response_bytes,
        max_rounds,
    )));

    // Initialize stats timer before starting the prover (to measure setup time)
    {
        let mut source = input_source.lock().expect("Failed to lock input source");
        source.init_stats();
    }

    // Run the benchmark
    let input_source_clone = Arc::clone(&input_source);
    let result = with_input_source(
        InputSourceWrapper(input_source_clone),
        prover.run(&prove_config),
    )
    .await;

    // Extract stats
    let stats = {
        let source = input_source.lock().expect("Failed to lock input source");
        source.stats().clone()
    };

    // Create and save record
    let record = match &result {
        Ok(()) => BenchmarkRecord::from_stats(benchmark_config, &prove_config, prover, &stats),
        Err(e) => BenchmarkRecord::failed(
            benchmark_config,
            &prove_config,
            prover,
            &stats,
            e.to_string(),
        ),
    };

    let path = save_record(&record)?;

    // Propagate the original error if the benchmark failed
    result?;

    Ok(path)
}

/// Wrapper to implement InputSource for Arc<Mutex<BenchmarkInputSource>>.
struct InputSourceWrapper(Arc<Mutex<BenchmarkInputSource>>);

impl InputSource for InputSourceWrapper {
    fn next_message(
        &mut self,
        budget: &ChannelBudget,
        config: &ProveConfig,
        past_messages: &[ChatMessage],
    ) -> anyhow::Result<Option<ChatMessage>> {
        self.0
            .lock()
            .unwrap()
            .next_message(budget, config, past_messages)
    }
}

//! Benchmark input source implementation.

use super::stats::BenchmarkStats;
use ai_passport::{ChannelBudget, ChatMessage, InputSource, ProveConfig, BYTES_PER_TOKEN};
use tracing::{debug, info, warn};

/// Input source for benchmarking with fixed message sizes.
///
/// Generates messages of a target byte size, asking the model to respond
/// with more words than the token limit allows (ensuring max_tokens is used).
pub struct BenchmarkInputSource {
    /// Target size in bytes for each user message content.
    target_request_bytes: usize,
    /// Target response size in bytes for each assistant message.
    target_response_bytes: u32,
    /// Maximum number of rounds (safety limit). None = unlimited.
    max_rounds: Option<usize>,
    /// Current round counter.
    round: usize,
    /// Statistics collector.
    stats: BenchmarkStats,
}

impl BenchmarkInputSource {
    /// Create a new benchmark input source.
    ///
    /// # Arguments
    /// * `target_request_bytes` - Target size for each user message in bytes
    /// * `target_response_bytes` - Target size for each assistant message in bytes
    /// * `max_rounds` - Optional safety limit on number of messages
    pub fn new(
        target_request_bytes: usize,
        target_response_bytes: u32,
        max_rounds: Option<usize>,
    ) -> Self {
        Self {
            target_request_bytes,
            target_response_bytes,
            max_rounds,
            round: 0,
            stats: BenchmarkStats::new(),
        }
    }

    /// Get the collected statistics.
    pub fn stats(&self) -> &BenchmarkStats {
        &self.stats
    }

    /// Initialize the stats timer. Call this before starting the prover.
    pub fn init_stats(&mut self) {
        self.stats.init();
    }

    /// Generate a message of the target byte size.
    fn generate_message(&self) -> String {
        // Request more words than token limit allows to ensure model uses full budget.
        // Tokens ≈ words * 1.3 for English, so we ask for ~2x the token count in words.
        let words_to_request = (self.target_response_bytes as usize / BYTES_PER_TOKEN) * 2;

        // Core instruction that the model should follow
        let instruction = format!(
            "Task #{round}: Write a detailed response of at least {words} words about technology, \
             innovation, and the future of computing. Be thorough and expansive in your answer.",
            round = self.round + 1,
            words = words_to_request,
        );

        // Separator marking the end of the real prompt
        let separator = "\n\n--- PADDING BELOW (ignore, used for message size calibration) ---\n\n";

        let prefix = format!("{}{}", instruction, separator);
        let prefix_len = prefix.len();

        // Calculate padding needed to reach target size
        let padding_needed = self.target_request_bytes.saturating_sub(prefix_len);

        if padding_needed == 0 {
            warn!(
                "Generated message size {prefix_len} already exceeds target size of {} bytes",
                self.target_request_bytes
            );
            return instruction;
        }

        // Generate padding to reach exact target size
        let padding = Self::generate_padding(padding_needed);

        format!("{}{}", prefix, padding)
    }

    /// Generate comprehensible padding text of exact byte size.
    fn generate_padding(target_bytes: usize) -> String {
        const FILLER_PHRASES: &[&str] = &[
            "The digital age continues to reshape how we interact with information. ",
            "Cloud computing has revolutionized enterprise infrastructure worldwide. ",
            "Machine learning algorithms process vast amounts of data daily. ",
            "Cybersecurity remains a critical concern for organizations everywhere. ",
            "Open source software powers much of the modern internet. ",
            "Distributed systems enable scalable and resilient applications. ",
            "Programming languages evolve to meet new challenges. ",
            "Data privacy regulations affect technology development globally. ",
            "Artificial intelligence transforms industries at rapid pace. ",
            "Network protocols ensure reliable communication across systems. ",
            "Software engineering practices continue to mature and improve. ",
            "Hardware advancements enable new computational possibilities. ",
        ];

        let mut padding = String::with_capacity(target_bytes);
        let mut idx = 0;

        while padding.len() < target_bytes {
            let phrase = FILLER_PHRASES[idx % FILLER_PHRASES.len()];
            let remaining = target_bytes - padding.len();

            if phrase.len() <= remaining {
                padding.push_str(phrase);
            } else {
                // Fill remaining space - prefer word boundaries if possible
                let truncated = &phrase[..remaining];
                if let Some(last_space) = truncated.rfind(' ') {
                    padding.push_str(&truncated[..last_space]);
                    // Pad rest with spaces to reach exact size
                    let spaces_needed = remaining - last_space;
                    padding.push_str(&" ".repeat(spaces_needed));
                } else {
                    padding.push_str(truncated);
                }
            }
            idx += 1;
        }

        // Ensure exact size (should already be correct, but safety check)
        padding.truncate(target_bytes);
        padding
    }

    /// Check if we should continue generating based on budget.
    fn should_continue(&self, budget: &ChannelBudget, past_messages: &[ChatMessage]) -> bool {
        // Check max rounds limit
        if let Some(max) = self.max_rounds {
            if self.round >= max {
                debug!("Reached max rounds limit: {}", max);
                return false;
            }
        }

        // For unlimited budgets, continue (until max_rounds if set)
        if budget.is_unlimited() {
            return true;
        }

        // Check if we have enough send budget for another message
        if let Some(available_send) = budget.available_input_bytes(past_messages) {
            if available_send < self.target_request_bytes {
                debug!(
                    "Send budget exhausted: {} available, {} needed",
                    available_send, self.target_request_bytes
                );
                return false;
            }
        }

        // Check if we have enough receive budget for expected response
        if let Some(available_recv) = budget.available_recv_bytes() {
            if available_recv < self.target_response_bytes as usize {
                debug!(
                    "Receive budget exhausted: {available_recv} available, {} needed",
                    self.target_response_bytes
                );
                return false;
            }
        }

        true
    }
}

impl InputSource for BenchmarkInputSource {
    fn next_message(
        &mut self,
        budget: &ChannelBudget,
        _config: &ProveConfig,
        past_messages: &[ChatMessage],
    ) -> anyhow::Result<Option<ChatMessage>> {
        // Complete the previous round if there was one
        if let Some(last) = past_messages.last() {
            let response_size = last.content().len();
            self.stats.complete_round(response_size);

            // Print timing for the completed round
            let round_durations = self.stats.round_durations_ms();
            if let Some(&duration_ms) = round_durations.last() {
                info!(
                    "Round {}: {}ms (response: {} bytes)",
                    self.round, duration_ms, response_size
                );
            }
        }

        // Check if we should continue
        if !self.should_continue(budget, past_messages) {
            return Ok(None);
        }

        // Generate and return the next message
        let message = self.generate_message();
        let message_len = message.len();

        // Start timing for this round
        self.stats.start_round(message_len);

        // Print setup time before the first round
        if self.round == 0 {
            if let Some(setup) = self.stats.setup_duration() {
                info!("Setup: {}ms", setup.as_millis());
            }
        }

        self.round += 1;

        Ok(Some(ChatMessage::user(message)))
    }
}

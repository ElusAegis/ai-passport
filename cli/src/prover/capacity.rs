//! Dynamic capacity estimation for per-message TLS sessions.
//!
//! This module provides utilities to estimate the required notary configuration
//! for each TLS session in the per-message prover. Since each session handles
//! exactly one request-response pair, we can precisely size the channel capacity.

use crate::config::notary::NotaryConfig;
use crate::config::ProveConfig;
use crate::providers::budget::ChannelBudget;
use crate::providers::message::ChatMessage;

/// Buffer factor for safety margin (20% extra).
const BUFFER_FACTOR: f64 = 1.2;

/// Estimate the required notary configuration for a specific round.
///
/// Uses the `ChannelBudget` for accurate overhead estimates (if observed),
/// and respects the original notary config as the upper bound.
/// Additionally, it turns on `defer_decryption` and sets `max_decrypted_online` to 0,
/// since only one message is processed per session.
///
/// # Arguments
/// * `base_config` - The base notary configuration (used as upper bound and for non-capacity settings)
/// * `prove_config` - The prove configuration (contains max response/request size hints)
/// * `past_messages` - Messages already in the conversation
/// * `budget` - The channel budget (used for observed overhead values)
/// * `lookahead` - How many rounds ahead we're estimating (1 = next round, 2 = round after next)
///
/// # Returns
/// A new `NotaryConfig` with appropriately sized capacity limits, or the base config
/// if dynamic sizing is not possible (missing size hints in ProveConfig).
pub fn estimate_round_capacity(
    base_config: &NotaryConfig,
    prove_config: &ProveConfig,
    past_messages: &[ChatMessage],
    budget: &ChannelBudget,
    lookahead: usize,
) -> NotaryConfig {
    // If ProveConfig doesn't have both size hints, we can't do dynamic sizing
    let (max_request, max_response) = match (
        prove_config.max_request_bytes,
        prove_config.max_response_bytes,
    ) {
        (Some(req), Some(resp)) => (req as usize, resp as usize),
        _ => return base_config.clone(),
    };

    // Get overhead estimates from budget (uses observed values if available)
    let request_overhead = budget.request_overhead();
    let response_overhead = budget.response_overhead();

    // Calculate current conversation size when serialized
    let current_messages_size = messages_json_size(past_messages);

    // For each lookahead round beyond the first, we add:
    // - One assistant response (which will be in history for next request)
    // - One user message
    // Plus JSON structure overhead per message (~50 bytes for role/content keys)
    let json_per_message_overhead = 50;
    let growth_per_round = max_response + max_request + (2 * json_per_message_overhead);
    let total_growth = growth_per_round * lookahead.saturating_sub(1);

    // Calculate send capacity: overhead + current messages + growth + new user message
    let send_content = current_messages_size + total_growth + max_request;
    let send_capacity = request_overhead + send_content;
    let send_capacity = ((send_capacity as f64) * BUFFER_FACTOR) as usize;
    let send_capacity = send_capacity.min(base_config.max_total_sent); // Never exceed base config

    // Calculate receive capacity: overhead + expected response
    // Response size doesn't grow with conversation history
    let recv_capacity = response_overhead + max_response;
    let recv_capacity = ((recv_capacity as f64) * BUFFER_FACTOR) as usize;
    let recv_capacity = recv_capacity.min(base_config.max_total_recv); // Never exceed base config

    // Create new config with estimated capacities
    NotaryConfig::builder()
        .domain(base_config.domain.clone())
        .port(base_config.port)
        .path_prefix(base_config.path_prefix.clone())
        .mode(base_config.mode)
        .max_total_sent(send_capacity)
        .max_total_recv(recv_capacity)
        .defer_decryption(true) // We can defer decryption as we only do one message per session
        .max_decrypted_online(0) // No online decryption needed as we only do one message per session
        .network_optimization(base_config.network_optimization)
        .build()
        .expect("Failed to build NotaryConfig")
}

/// Calculate the JSON serialization size of messages.
fn messages_json_size(messages: &[ChatMessage]) -> usize {
    if messages.is_empty() {
        return 2; // "[]"
    }
    serde_json::json!(messages).to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::notary::NotaryMode;
    use crate::providers::budget::ChannelCapacity;

    fn make_base_config() -> NotaryConfig {
        NotaryConfig::builder()
            .domain("localhost".to_string())
            .port(7047u16)
            .path_prefix("".to_string())
            .mode(NotaryMode::RemoteNonTLS)
            .max_total_sent(16384)
            .max_total_recv(16384)
            .build()
            .unwrap()
    }

    fn make_prove_config_with_sizes() -> ProveConfig {
        use crate::ApiProvider;

        let provider = ApiProvider::builder()
            .domain("test.example.com")
            .api_key("test-key")
            .build()
            .unwrap();

        ProveConfig::builder()
            .provider(provider)
            .model_id("test-model")
            .max_response_bytes(2000u32)
            .max_request_bytes(500u32)
            .build()
            .unwrap()
    }

    fn make_prove_config_without_sizes() -> ProveConfig {
        use crate::ApiProvider;

        let provider = ApiProvider::builder()
            .domain("test.example.com")
            .api_key("test-key")
            .build()
            .unwrap();

        ProveConfig::builder()
            .provider(provider)
            .model_id("test-model")
            .build()
            .unwrap()
    }

    fn make_budget() -> ChannelBudget {
        ChannelBudget::with_capacity(ChannelCapacity::Limited {
            sent_capacity: 16384,
            recv_capacity: 16384,
        })
    }

    #[test]
    fn test_returns_base_config_when_sizes_missing() {
        let base = make_base_config();
        let prove = make_prove_config_without_sizes();
        let budget = make_budget();

        let config = estimate_round_capacity(&base, &prove, &[], &budget, 1);

        // Should return base config unchanged
        assert_eq!(config.max_total_sent, base.max_total_sent);
        assert_eq!(config.max_total_recv, base.max_total_recv);
    }

    #[test]
    fn test_estimate_first_round_empty_history() {
        let base = make_base_config();
        let prove = make_prove_config_with_sizes();
        let budget = make_budget();

        let config = estimate_round_capacity(&base, &prove, &[], &budget, 1);

        // First round: overhead + empty messages + new request
        // Should be smaller than base config
        assert!(config.max_total_sent < base.max_total_sent);
        assert!(config.max_total_recv < base.max_total_recv);
    }

    #[test]
    fn test_estimate_grows_with_history() {
        let base = make_base_config();
        let prove = make_prove_config_with_sizes();
        let budget = make_budget();

        let empty_config = estimate_round_capacity(&base, &prove, &[], &budget, 1);

        let messages = vec![
            ChatMessage::user("Hello, how are you?"),
            ChatMessage::assistant("I'm doing well, thank you for asking!"),
        ];

        let with_history = estimate_round_capacity(&base, &prove, &messages, &budget, 1);

        // With history, send capacity should be larger
        assert!(with_history.max_total_sent > empty_config.max_total_sent);
        // Receive capacity should be similar (response doesn't include history)
        assert_eq!(with_history.max_total_recv, empty_config.max_total_recv);
    }

    #[test]
    fn test_lookahead_increases_capacity() {
        let base = make_base_config();
        let prove = make_prove_config_with_sizes();
        let budget = make_budget();

        let lookahead_1 = estimate_round_capacity(&base, &prove, &[], &budget, 1);
        let lookahead_2 = estimate_round_capacity(&base, &prove, &[], &budget, 2);

        // More lookahead means we expect more history growth
        assert!(lookahead_2.max_total_sent > lookahead_1.max_total_sent);
    }

    #[test]
    fn test_never_exceeds_base_config() {
        let base = make_base_config();
        let budget = make_budget();

        // Create prove config with huge sizes
        let provider = crate::ApiProvider::builder()
            .domain("test.example.com")
            .api_key("test-key")
            .build()
            .unwrap();

        let prove = ProveConfig::builder()
            .provider(provider)
            .model_id("test-model")
            .max_response_bytes(100000u32) // Very large
            .max_request_bytes(100000u32)
            .build()
            .unwrap();

        let config = estimate_round_capacity(&base, &prove, &[], &budget, 1);

        // Should be capped at base config
        assert!(config.max_total_sent <= base.max_total_sent);
        assert!(config.max_total_recv <= base.max_total_recv);
    }

    #[test]
    fn test_small_sizes_produce_valid_config() {
        let base = make_base_config();
        let budget = make_budget();

        // Create prove config with tiny sizes
        let provider = crate::ApiProvider::builder()
            .domain("test.example.com")
            .api_key("test-key")
            .build()
            .unwrap();

        let prove = ProveConfig::builder()
            .provider(provider)
            .model_id("test-model")
            .max_response_bytes(10u32) // Very small
            .max_request_bytes(10u32)
            .build()
            .unwrap();

        let config = estimate_round_capacity(&base, &prove, &[], &budget, 1);

        // Should produce valid non-zero capacities
        assert!(config.max_total_sent > 0);
        assert!(config.max_total_recv > 0);
        // And still respect base config limits
        assert!(config.max_total_sent <= base.max_total_sent);
        assert!(config.max_total_recv <= base.max_total_recv);
    }
}

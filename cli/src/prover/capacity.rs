//! Dynamic capacity estimation for TLS sessions.
//!
//! This module provides utilities to estimate the required notary configuration
//! for TLS sessions. It supports two modes:
//!
//! 1. **Per-message mode** (`estimate_per_message_capacity`): Each session handles
//!    exactly one request-response pair. Capacity grows with conversation history.
//!
//! 2. **Single-shot mode** (`estimate_single_shot_capacity`): One session handles
//!    N exchanges. Capacity grows O(N²) for send due to repeated history.

use crate::config::notary::NotaryConfig;
use crate::config::ProveConfig;
use crate::providers::budget::ExpectedChannelOverhead;
use crate::providers::message::ChatMessage;
use crate::providers::message::ChatMessageRole::{Assistant, User};
use crate::providers::Provider;
use anyhow::Result;
use tracing::debug;

/// Buffer factor for safety margin (20% extra).
const BUFFER_FACTOR: f64 = 1.2;

/// Estimate the required notary configuration for a specific round in per-message mode.
///
/// In per-message mode, each TLS session handles exactly one request-response pair.
/// This function estimates capacity for a single round, accounting for conversation
/// history that gets re-sent with each request.
///
/// Uses the provided overhead estimates for capacity calculation and respects the
/// original notary config as the upper bound. Additionally, it turns on `defer_decryption`
/// and sets `max_decrypted_online` to 0, since only one message is processed per session.
///
/// # Arguments
/// * `base_config` - The base notary configuration (used as upper bound and for non-capacity settings)
/// * `prove_config` - The prove configuration (contains max response/request size hints)
/// * `past_messages` - Messages already in the conversation
/// * `overhead` - The overhead estimates (from provider or observed values)
/// * `lookahead` - How many rounds ahead we're estimating (1 = next round, 2 = round after next)
///
/// # Returns
/// A new `NotaryConfig` with appropriately sized capacity limits, or the base config
/// if dynamic sizing is not possible (missing size hints in ProveConfig).
pub fn estimate_per_message_capacity(
    base_config: &NotaryConfig,
    prove_config: &ProveConfig,
    past_messages: &[ChatMessage],
    overhead: &ExpectedChannelOverhead,
    lookahead: usize,
) -> Result<NotaryConfig> {
    // If ProveConfig doesn't have both size hints, we can't do dynamic sizing
    let (max_request, max_response) = match (
        prove_config.max_request_bytes,
        prove_config.max_response_bytes,
    ) {
        (Some(req), Some(resp)) => (req as usize, resp as usize),
        _ => return Ok(base_config.clone()),
    };

    // Get overhead estimates
    let request_overhead = overhead.request_overhead();
    let response_overhead = overhead.response_overhead();

    // Calculate current conversation size when serialized
    let current_messages_size = messages_json_size(past_messages);

    // For each lookahead round beyond the first, we add:
    // - One assistant response (which will be in history for next request)
    // - One user message
    let growth_per_round =
        max_response + max_request + ChatMessage::overhead(Assistant) + ChatMessage::overhead(User);
    let total_growth = growth_per_round * lookahead.saturating_sub(1);

    // Calculate send capacity: overhead + current messages + growth + new user message
    let send_content = current_messages_size + total_growth + max_request;
    let send_capacity = request_overhead + send_content;
    let send_capacity = ((send_capacity as f64) * BUFFER_FACTOR) as usize;
    if send_capacity > base_config.max_total_sent {
        // Ensure we don't exceed base config limits
        return Err(anyhow::anyhow!(
            "Estimated send capacity ({}) exceeds base config limit ({}). Check ProveConfig sizes
            ",
            send_capacity,
            base_config.max_total_sent
        ));
    }

    // Calculate receive capacity: overhead + expected response
    // Response size doesn't grow with conversation history
    let recv_capacity = response_overhead + max_response;
    let recv_capacity = ((recv_capacity as f64) * BUFFER_FACTOR) as usize;
    if recv_capacity > base_config.max_total_recv {
        // Ensure receive capacity does not exceed base config limits
        return Err(anyhow::anyhow!(
            "Estimated receive capacity ({}) exceeds base config limit ({}). Check ProveConfig sizes.",
            recv_capacity,
            base_config.max_total_recv
        ));
    }

    // Create new config with estimated capacities
    let new_config = NotaryConfig {
        max_total_sent: send_capacity,
        max_total_recv: recv_capacity,
        defer_decryption: true, // Only one message per session, so can defer decryption
        max_decrypted_online: 0, // Only one message, so no online decryption needed
        ..base_config.clone()
    };

    Ok(new_config)
}

/// Calculate the JSON serialization size of messages.
fn messages_json_size(messages: &[ChatMessage]) -> usize {
    if messages.is_empty() {
        return 2; // "[]"
    }
    serde_json::json!(messages).to_string().len()
}

/// Estimate the required notary configuration for single-shot mode with N exchanges.
///
/// In single-shot mode, one TLS session handles all N request-response exchanges.
/// Due to chat API semantics where conversation history is re-sent with each request,
/// the send capacity grows O(N²).
///
/// ## Formula Explanation
///
/// For N exchanges (N user messages and N assistant responses):
///
/// ### Send (Upload) - O(N²) growth:
/// Each round re-sends the entire conversation history:
/// - Round 1: Send `[user1]` → 1 user message
/// - Round 2: Send `[user1, assistant1, user2]` → 2 user + 1 assistant
/// - Round N: N user messages + (N-1) assistant messages
///
/// Summing across all rounds:
/// - User messages sent: `1 + 2 + ... + N = N*(N+1)/2`
/// - Assistant messages sent: `0 + 1 + ... + (N-1) = N*(N-1)/2`
///
/// **Total send** = `N × request_overhead + N×(N+1)/2 × max_request + N×(N-1)/2 × max_response`
///
/// ### Receive (Download) - O(N) growth:
/// Each response is independent:
/// **Total receive** = `N × (response_overhead + max_response)`
///
/// # Arguments
/// * `base_config` - The base notary configuration (used for non-capacity settings)
/// * `prove_config` - Must have `max_request_bytes`, `max_response_bytes`, and `expected_exchanges` set
///
/// # Returns
/// A new `NotaryConfig` with capacity sized for N exchanges, or an error if:
/// - Required size hints are missing from `ProveConfig`
/// - The base config doesn't have enough capacity for N exchanges
pub fn estimate_single_shot_capacity(
    base_config: &NotaryConfig,
    prove_config: &ProveConfig,
) -> Result<NotaryConfig> {
    // All three fields must be set for single-shot capacity estimation
    let (max_request, max_response, n) = match (
        prove_config.max_request_bytes,
        prove_config.max_response_bytes,
        prove_config.expected_exchanges,
    ) {
        (Some(req), Some(resp), Some(exchanges)) => {
            (req as usize, resp as usize, exchanges as usize)
        }
        _ => return Ok(base_config.clone()),
    };

    if n == 0 {
        return Err(anyhow::anyhow!(
            "expected_exchanges must be at least 1 for single-shot mode"
        ));
    }

    let n = n + 1; // Account for a safety margin of one extra exchange

    // Get overhead estimates for the provider based on expected sizes
    let expected_overhead = prove_config.provider.expected_overhead();
    let request_overhead = expected_overhead.request_overhead();
    let response_overhead = expected_overhead.response_overhead();

    let user_msg_with_overhead = max_request + ChatMessage::overhead(User);
    let assistant_msg_with_overhead = max_response + ChatMessage::overhead(Assistant);

    // Calculate total send capacity using the triangular sum formula:
    // - User messages: sent 1+2+...+N = N*(N+1)/2 times
    // - Assistant messages: sent 0+1+...+(N-1) = N*(N-1)/2 times
    // - Request overhead: N times (once per request)
    let user_msgs_total = n * (n + 1) / 2;
    let assistant_msgs_total = n * (n - 1) / 2;

    let send_capacity = (n * request_overhead)
        + (user_msgs_total * user_msg_with_overhead)
        + (assistant_msgs_total * assistant_msg_with_overhead);

    let send_capacity = ((send_capacity as f64) * BUFFER_FACTOR) as usize;
    if send_capacity > base_config.max_total_sent {
        return Err(anyhow::anyhow!(
            "Notary capacity insufficient for {} exchanges. \
             Required send: {} bytes, available: {} bytes. \
             Reduce expected_exchanges or increase notary max_total_sent.",
            n,
            send_capacity,
            base_config.max_total_sent
        ));
    }

    // Calculate total receive capacity: N independent responses
    let recv_capacity = n * (response_overhead + max_response);
    let recv_capacity = ((recv_capacity as f64) * BUFFER_FACTOR) as usize;
    let recv_capacity = recv_capacity.max(send_capacity); // Ensure recv >= send for safety
    if recv_capacity > base_config.max_total_recv {
        return Err(anyhow::anyhow!(
            "Notary capacity insufficient for {} exchanges. \
             Required receive: {} bytes, available: {} bytes. \
             Reduce expected_exchanges or increase notary max_total_recv.",
            n,
            recv_capacity,
            base_config.max_total_recv
        ));
    }

    // Create new config with estimated capacities
    debug!(
        "Estimated single-shot notary capacity for {} exchanges: send={} bytes, recv={} bytes",
        n, send_capacity, recv_capacity
    );
    let new_config = NotaryConfig {
        max_total_sent: send_capacity,
        max_total_recv: recv_capacity,
        max_decrypted_online: recv_capacity,
        ..base_config.clone()
    };

    Ok(new_config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::notary::NotaryMode;

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

    fn make_overhead() -> ExpectedChannelOverhead {
        ExpectedChannelOverhead::default()
    }

    #[test]
    fn test_returns_base_config_when_sizes_missing() {
        let base = make_base_config();
        let prove = make_prove_config_without_sizes();
        let overhead = make_overhead();

        let config = estimate_per_message_capacity(&base, &prove, &[], &overhead, 1).unwrap();

        // Should return base config unchanged
        assert_eq!(config.max_total_sent, base.max_total_sent);
        assert_eq!(config.max_total_recv, base.max_total_recv);
    }

    #[test]
    fn test_estimate_first_round_empty_history() {
        let base = make_base_config();
        let prove = make_prove_config_with_sizes();
        let overhead = make_overhead();

        let config = estimate_per_message_capacity(&base, &prove, &[], &overhead, 1).unwrap();

        // First round: overhead + empty messages + new request
        // Should be smaller than base config
        assert!(config.max_total_sent < base.max_total_sent);
        assert!(config.max_total_recv < base.max_total_recv);
    }

    #[test]
    fn test_estimate_grows_with_history() {
        let base = make_base_config();
        let prove = make_prove_config_with_sizes();
        let overhead = make_overhead();

        let empty_config = estimate_per_message_capacity(&base, &prove, &[], &overhead, 1).unwrap();

        let messages = vec![
            ChatMessage::user("Hello, how are you?"),
            ChatMessage::assistant("I'm doing well, thank you for asking!"),
        ];

        let with_history =
            estimate_per_message_capacity(&base, &prove, &messages, &overhead, 1).unwrap();

        // With history, send capacity should be larger
        assert!(with_history.max_total_sent > empty_config.max_total_sent);
        // Receive capacity should be similar (response doesn't include history)
        assert_eq!(with_history.max_total_recv, empty_config.max_total_recv);
    }

    #[test]
    fn test_lookahead_increases_capacity() {
        let base = make_base_config();
        let prove = make_prove_config_with_sizes();
        let overhead = make_overhead();

        let lookahead_1 = estimate_per_message_capacity(&base, &prove, &[], &overhead, 1).unwrap();
        let lookahead_2 = estimate_per_message_capacity(&base, &prove, &[], &overhead, 2).unwrap();

        // More lookahead means we expect more history growth
        assert!(lookahead_2.max_total_sent > lookahead_1.max_total_sent);
    }

    #[test]
    fn test_errors_when_exceeds_base_config() {
        let base = make_base_config();
        let overhead = make_overhead();

        // Create prove config with huge sizes that exceed base config
        let provider = crate::ApiProvider::builder()
            .domain("test.example.com")
            .api_key("test-key")
            .build()
            .unwrap();

        let prove = ProveConfig::builder()
            .provider(provider)
            .model_id("test-model")
            .max_response_bytes(100000u32) // Very large - exceeds base config
            .max_request_bytes(100000u32)
            .build()
            .unwrap();

        // Should return an error when sizes exceed base config limits
        let result = estimate_per_message_capacity(&base, &prove, &[], &overhead, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_small_sizes_produce_valid_config() {
        let base = make_base_config();
        let overhead = make_overhead();

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

        let config = estimate_per_message_capacity(&base, &prove, &[], &overhead, 1).unwrap();

        // Should produce valid non-zero capacities
        assert!(config.max_total_sent > 0);
        assert!(config.max_total_recv > 0);
        // And still respect base config limits
        assert!(config.max_total_sent <= base.max_total_sent);
        assert!(config.max_total_recv <= base.max_total_recv);
    }
}

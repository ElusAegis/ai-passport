//! Byte budget tracking for TLS channel usage.
//!
//! This module provides types to track and enforce byte limits on send/receive
//! channels, primarily for TLS-notarized sessions where channel capacity is limited.

use crate::providers::message::ChatMessageRole::{Assistant, User};
use crate::providers::Provider;
use crate::{ChatMessage, NotaryConfig, ProveConfig};
use anyhow::{bail, Result};
use hyper::body::Bytes;
use hyper::Request;
use tracing::{debug, warn};

/// Estimated bytes per token for response size calculation.
/// Conservative estimate accounting for UTF-8 and JSON escaping.
pub const BYTES_PER_TOKEN: u32 = 7;

/// Default request overhead (HTTP headers) for OpenAI-compatible providers.
/// This is the largest observed overhead for requests with typical headers.
pub const DEFAULT_REQUEST_OVERHEAD: usize = 350;

/// Default response overhead (JSON structure + HTTP headers) for OpenAI-compatible providers.
/// This is the largest observed overhead for typical chat completions.
pub const DEFAULT_RESPONSE_OVERHEAD: usize = 2000;

/// Threshold for warning about overhead drift (10%).
const OVERHEAD_DRIFT_THRESHOLD_PERCENT: usize = 10;

/// Expected HTTP overhead for capacity planning.
///
/// Contains expected overhead values from the provider. When fields are `None`,
/// the default constants are used. Values are updated with observed data during
/// a session, with warnings if observed differs significantly from expected.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExpectedChannelOverhead {
    /// Request overhead (HTTP headers, etc). None = use DEFAULT_REQUEST_OVERHEAD.
    request: Option<usize>,
    /// Response overhead (HTTP headers, JSON structure). None = use DEFAULT_RESPONSE_OVERHEAD.
    response: Option<usize>,
}

impl ExpectedChannelOverhead {
    /// Create a new overhead configuration with optional expected values.
    ///
    /// Use `None` for fields to use conservative defaults.
    pub fn new(request: Option<usize>, response: Option<usize>) -> Self {
        Self { request, response }
    }

    /// Get request overhead (value or default).
    pub fn request_overhead(&self) -> usize {
        self.request.unwrap_or(DEFAULT_REQUEST_OVERHEAD)
    }

    /// Get response overhead (value or default).
    pub fn response_overhead(&self) -> usize {
        self.response.unwrap_or(DEFAULT_RESPONSE_OVERHEAD)
    }

    /// Update request overhead with observed value.
    ///
    /// Logs the first observation. On subsequent observations, warns if the
    /// observed value differs significantly from the previous value.
    pub fn update_request(&mut self, total_bytes: usize, content_bytes: usize) {
        let observed = total_bytes.saturating_sub(content_bytes);

        match self.request {
            None => {
                debug!(
                    "overhead: observed request overhead = {} (total={}, content={})",
                    observed, total_bytes, content_bytes
                );
            }
            Some(expected) => {
                Self::warn_if_drifted("request", observed, expected);
            }
        }

        self.request = Some(observed);
    }

    /// Update response overhead with observed value.
    ///
    /// Logs the first observation. On subsequent observations, warns if the
    /// observed value differs significantly from the previous value.
    pub fn update_response(&mut self, total_bytes: usize, content_bytes: usize) {
        let observed = total_bytes.saturating_sub(content_bytes);

        match self.response {
            None => {
                debug!(
                    "overhead: observed response overhead = {} (total={}, content={})",
                    observed, total_bytes, content_bytes
                );
            }
            Some(expected) => {
                Self::warn_if_drifted("response", observed, expected);
            }
        }

        self.response = Some(observed);
    }

    /// Warn if observed overhead has drifted significantly from expected.
    fn warn_if_drifted(kind: &str, observed: usize, expected: usize) {
        let diff = (observed as isize - expected as isize).unsigned_abs();
        let threshold = expected / OVERHEAD_DRIFT_THRESHOLD_PERCENT;
        debug!(
            "overhead: observed {} overhead = {} (expected = {}, diff = {})",
            kind, observed, expected, diff
        );

        if diff > threshold {
            let percent = diff * 100 / expected.max(1);
            warn!(
                "observed {} overhead ({}) differs from expected ({}) by {}%",
                kind, observed, expected, percent
            );
        }
    }
}

/// Channel capacity configuration.
///
/// Defines the byte limits for send/receive channels. Use `Unlimited` for
/// passthrough modes (DirectProver, TEE) or `Limited` for TLS-notarized sessions.
#[derive(Debug, Clone, Default)]
pub enum ChannelCapacity {
    /// No limits (DirectProver, TEE).
    #[default]
    Unlimited,
    /// Limited capacity with specific byte limits.
    Limited {
        /// Maximum bytes that can be sent.
        sent_capacity: usize,
        /// Maximum bytes that can be received.
        recv_capacity: usize,
    },
}

impl From<&NotaryConfig> for ChannelCapacity {
    fn from(value: &NotaryConfig) -> Self {
        ChannelCapacity::Limited {
            sent_capacity: value.max_total_sent,
            recv_capacity: value.max_total_recv,
        }
    }
}

/// Tracks byte budget for send/receive channels.
///
/// Monitors usage against capacity, updates overhead from observed values,
/// and provides helpers for calculating remaining budget and max tokens.
#[derive(Debug, Clone, Default)]
pub struct ChannelBudget {
    /// Bytes sent over the channel.
    sent: usize,
    /// Bytes received over the channel.
    recv: usize,
    /// The capacity configuration (unlimited or limited).
    capacity: ChannelCapacity,
    /// Expected overhead (from provider, updated with observed values).
    overhead: ExpectedChannelOverhead,
}

impl ChannelBudget {
    /// Create a budget with the given capacity using default overhead estimates.
    pub fn with_capacity(capacity: ChannelCapacity) -> Self {
        ChannelBudget {
            capacity,
            ..Default::default()
        }
    }

    /// Create a budget with given capacity and expected overhead from provider.
    pub fn from_config(notary_config: &NotaryConfig, prove_config: &ProveConfig) -> Self {
        let capacity = notary_config.into();
        let overhead = prove_config.provider.expected_overhead();

        ChannelBudget {
            capacity,
            overhead,
            ..Default::default()
        }
    }

    /// Create an unlimited budget (for passthrough/TEE modes).
    pub fn unlimited() -> Self {
        ChannelBudget {
            capacity: ChannelCapacity::Unlimited,
            ..Default::default()
        }
    }

    /// Get the current overhead estimates.
    pub fn overhead(&self) -> &ExpectedChannelOverhead {
        &self.overhead
    }

    /// Reset usage counters while preserving learned overhead.
    ///
    /// Use this for per-message prover where each message gets fresh capacity
    /// but overhead learning should persist across messages.
    pub fn reset(&mut self) -> &mut Self {
        self.recv = 0;
        self.sent = 0;

        self
    }

    /// Update the capacity configuration.
    pub fn set_capacity(&mut self, capacity: ChannelCapacity) -> &mut Self {
        self.capacity = capacity;

        self
    }

    /// Check if we can send the given number of bytes.
    ///
    /// Takes the actual total bytes (headers + body) that will be sent.
    /// Returns an error with a helpful message if budget would be exceeded.
    pub fn check_request_fits(&self, total_bytes: usize) -> Result<()> {
        match self.capacity {
            ChannelCapacity::Unlimited => {}
            ChannelCapacity::Limited { sent_capacity, .. } => {
                if total_bytes + self.sent > sent_capacity {
                    let remaining = sent_capacity.saturating_sub(self.sent);
                    bail!(
                        "Insufficient send budget. Need {total_bytes} bytes but only {remaining} remaining.\n\
                         Tip: Use shorter messages or start a new session.",
                    );
                }
            }
        }

        Ok(())
    }

    /// Record bytes sent and update overhead estimate.
    ///
    /// Updates the overhead with observed values, warning if significantly different.
    pub fn record_sent(&mut self, total_bytes: usize, content_bytes: usize) {
        self.sent += total_bytes;
        self.overhead.update_request(total_bytes, content_bytes);

        if let ChannelCapacity::Limited { sent_capacity, .. } = self.capacity {
            let remaining = sent_capacity.saturating_sub(self.sent);
            debug!("budget: sent {total_bytes} bytes, remaining={remaining}");
        }

        debug!(
            "data ↑: total={} content={} (ratio={:.1}x, left={})",
            total_bytes,
            content_bytes,
            total_bytes as f64 / content_bytes.max(1) as f64,
            self.available_input_bytes(&[])
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unlimited".to_string())
        );
    }

    /// Record bytes received and update overhead estimate.
    ///
    /// Updates the overhead with observed values, warning if significantly different.
    pub fn record_recv(&mut self, total_bytes: usize, content_bytes: usize) {
        self.recv += total_bytes;
        self.overhead.update_response(total_bytes, content_bytes);

        if let ChannelCapacity::Limited { recv_capacity, .. } = self.capacity {
            let remaining = recv_capacity.saturating_sub(self.recv);
            debug!("budget: received {total_bytes} bytes, remaining={remaining}");
        }

        debug!(
            "data ↓: total={} content={} (ratio={:.1}x, left={})",
            total_bytes,
            content_bytes,
            total_bytes as f64 / content_bytes.max(1) as f64,
            self.available_recv_bytes()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unlimited".to_string())
        );
    }

    /// Calculate max_tokens based on remaining receive budget.
    ///
    /// Uses current response overhead estimate.
    /// Returns `None` for unlimited budgets, meaning no limit should be set.
    /// Returns `Some(tokens)` for limited budgets.
    pub fn max_bytes_left_for_response(&self) -> Option<u32> {
        match self.capacity {
            ChannelCapacity::Unlimited => None,
            ChannelCapacity::Limited { recv_capacity, .. } => {
                let response_overhead = self.overhead.response_overhead();
                let recv_remaining = recv_capacity.saturating_sub(self.recv);
                let usable = recv_remaining.saturating_sub(response_overhead);
                // Ensure at least some tokens if there's any budget
                Some(usable.max(1) as u32)
            }
        }
    }

    /// Get remaining input capacity for user display.
    ///
    /// Uses current request overhead estimate.
    /// Returns `None` for unlimited budgets.
    /// Returns `Some(bytes)` showing how many bytes the user can still send.
    pub fn available_input_bytes(&self, past_messages: &[ChatMessage]) -> Option<usize> {
        match self.capacity {
            ChannelCapacity::Unlimited => None,
            ChannelCapacity::Limited { sent_capacity, .. } => {
                let repeated_content_bytes: usize = serde_json::to_string(past_messages)
                    .expect("Failed to serialize messages to calculate their size")
                    .len();

                let request_overhead = self.overhead.request_overhead();
                let sent_remaining = sent_capacity.saturating_sub(self.sent);
                let user_message_remaining = sent_remaining
                    .saturating_sub(repeated_content_bytes)
                    .saturating_sub(request_overhead);
                let user_message_overhead = ChatMessage::overhead(User);
                let pure_message_remaining =
                    user_message_remaining.saturating_sub(user_message_overhead);

                Some(pure_message_remaining)
            }
        }
    }

    /// Get remaining receive capacity.
    ///
    /// Returns `None` for unlimited budgets.
    pub fn available_recv_bytes(&self) -> Option<usize> {
        match self.capacity {
            ChannelCapacity::Unlimited => None,
            ChannelCapacity::Limited { recv_capacity, .. } => {
                let recv_remaining = recv_capacity.saturating_sub(self.recv);
                let assistant_message_overhead = ChatMessage::overhead(Assistant);
                let pure_message_remaining =
                    recv_remaining.saturating_sub(assistant_message_overhead);
                Some(pure_message_remaining)
            }
        }
    }

    /// Check if this is an unlimited budget.
    pub fn is_unlimited(&self) -> bool {
        matches!(self.capacity, ChannelCapacity::Unlimited)
    }

    /// Get the current request overhead estimate.
    pub fn request_overhead(&self) -> usize {
        self.overhead.request_overhead()
    }

    /// Get the current response overhead estimate.
    pub fn response_overhead(&self) -> usize {
        self.overhead.response_overhead()
    }

    /// Calculate the actual HTTP/1.1 response size on the wire.
    pub fn calculate_response_size(headers: &hyper::HeaderMap, body: &Bytes) -> usize {
        let body_len = body.len();

        // Status line estimate: "HTTP/1.1 200 OK\r\n"
        let status_line_len = 20;

        // Headers: "Name: value\r\n" for each
        let headers_len: usize = headers
            .iter()
            .map(|(k, v)| k.as_str().len() + 2 + v.len() + 2)
            .sum();

        // Empty line separator: "\r\n"
        let separator_len = 2;

        status_line_len + headers_len + separator_len + body_len
    }

    /// Calculate the actual HTTP/1.1 request size on the wire.
    pub(crate) fn calculate_request_size(request: &Request<String>) -> usize {
        // Request line: "POST /path HTTP/1.1\r\n"
        let method_len = request.method().as_str().len();
        let uri_len = request.uri().to_string().len();
        let request_line_len = method_len + 1 + uri_len + " HTTP/1.1\r\n".len();

        // Headers: "Name: value\r\n" for each
        let headers_len: usize = request
            .headers()
            .iter()
            .map(|(k, v)| k.as_str().len() + 2 + v.len() + 2) // ": " + "\r\n"
            .sum();

        // Empty line separator: "\r\n"
        let separator_len = 2;

        // Body
        let body_len = request.body().len();

        request_line_len + headers_len + separator_len + body_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::Method;

    /// Helper to create a simple test request with a given body size.
    fn make_test_request(body: &str) -> Request<String> {
        Request::builder()
            .method(Method::POST)
            .uri("/test")
            .header("host", "example.com")
            .body(body.to_string())
            .unwrap()
    }

    /// Helper to create a limited budget for tests.
    fn make_limited_budget(sent_capacity: usize, recv_capacity: usize) -> ChannelBudget {
        ChannelBudget::with_capacity(ChannelCapacity::Limited {
            sent_capacity,
            recv_capacity,
        })
    }

    #[test]
    fn test_unlimited_budget() {
        let budget = ChannelBudget::unlimited();
        assert!(budget.is_unlimited());

        let request = make_test_request("test body");
        let request_size = ChannelBudget::calculate_request_size(&request);
        assert!(budget.check_request_fits(request_size).is_ok());
        assert!(budget.max_bytes_left_for_response().is_none());
        assert!(budget.available_input_bytes(&[]).is_none());
    }

    #[test]
    fn test_limited_budget_check_send() {
        // Small request should succeed
        let budget = make_limited_budget(1000, 2000);
        let small_request = make_test_request("small");
        let small_request_size = ChannelBudget::calculate_request_size(&small_request);
        assert!(budget.check_request_fits(small_request_size).is_ok());

        // Large request should fail
        let budget = make_limited_budget(50, 2000);
        let request = make_test_request("this body is too large for the budget");
        let request_size = ChannelBudget::calculate_request_size(&request);
        assert!(budget.check_request_fits(request_size).is_err());
    }

    #[test]
    fn test_limited_budget_record_sent() {
        let mut budget = make_limited_budget(1000, 2000);

        budget.record_sent(150, 100); // total=150, content=100

        // After sending 150 bytes, remaining should be 1000 - 150 = 850
        // available_input_bytes subtracts overhead estimate (285)
        // So: 850 - 285 = 565
        assert_eq!(budget.sent, 150);
    }

    #[test]
    fn test_max_bytes_for_response() {
        let budget = make_limited_budget(1000, 10000);

        let bytes = budget.max_bytes_left_for_response().unwrap();
        // recv_capacity=10000, recv=0, overhead_estimate=5000
        // usable = 10000 - 5000 = 5000
        assert_eq!(bytes, 5000);
    }

    #[test]
    fn test_available_input_bytes() {
        let budget = make_limited_budget(1000, 2000);

        let available = budget.available_input_bytes(&[]).unwrap();
        // sent_capacity=1000, sent=0, overhead_estimate=285, empty_messages="[]"=2
        // available = 1000 - 2 - 285 = 713
        assert_eq!(available, 713);
    }

    #[test]
    fn test_available_recv_bytes() {
        let budget = make_limited_budget(1000, 2000);

        let available = budget.available_recv_bytes().unwrap();
        // recv_capacity=2000, recv=0
        assert_eq!(available, 2000);
    }

    #[test]
    fn test_overhead_updates_via_budget() {
        let mut budget = make_limited_budget(1000, 10000);

        // Initially uses estimates (plus "[]" = 2 bytes for empty messages)
        // available_input = 1000 - 2 - 285 = 713
        assert_eq!(budget.available_input_bytes(&[]).unwrap(), 713);
        // max_bytes = 10000 - 5000 = 5000
        assert_eq!(budget.max_bytes_left_for_response().unwrap(), 5000);

        // Record sends/recvs which update overhead
        budget.record_sent(300, 100); // overhead = 200
        budget.record_recv(400, 200); // overhead = 200

        // Now uses observed values (and remaining is reduced)
        // sent_remaining = 1000 - 300 = 700
        // available_input = 700 - 2 - 200 (observed) = 498
        assert_eq!(budget.available_input_bytes(&[]).unwrap(), 498);

        // recv_remaining = 10000 - 400 = 9600
        // usable_bytes = 9600 - 200 = 9400
        assert_eq!(budget.max_bytes_left_for_response().unwrap(), 9400);
    }

    #[test]
    fn test_reset_preserves_overhead() {
        let mut budget = make_limited_budget(1000, 10000);

        // Learn some overhead
        budget.record_sent(300, 100); // overhead = 200
        budget.record_recv(400, 200); // overhead = 200

        // Reset counters but keep overhead
        budget.reset();

        assert_eq!(budget.sent, 0);
        assert_eq!(budget.recv, 0);

        // Overhead should still be observed values
        // available_input = 1000 - 2 - 200 (observed) = 798
        assert_eq!(budget.available_input_bytes(&[]).unwrap(), 798);
    }

    /// Build the expected HTTP/1.1 wire format string for a request.
    /// This is the ground truth we compare our calculation against.
    fn build_http11_wire_format(request: &Request<String>) -> String {
        let mut wire = String::new();

        // Request line: "METHOD URI HTTP/1.1\r\n"
        wire.push_str(request.method().as_str());
        wire.push(' ');
        wire.push_str(&request.uri().to_string());
        wire.push_str(" HTTP/1.1\r\n");

        // Headers: "Name: value\r\n" for each
        for (name, value) in request.headers() {
            wire.push_str(name.as_str());
            wire.push_str(": ");
            wire.push_str(value.to_str().unwrap());
            wire.push_str("\r\n");
        }

        // Empty line separator
        wire.push_str("\r\n");

        // Body
        wire.push_str(request.body());

        wire
    }

    #[test]
    fn test_calculate_request_size_simple() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/chat/completions")
            .header("host", "api.openai.com")
            .header("content-type", "application/json")
            .header("content-length", "13")
            .body(r#"{"test":true}"#.to_string())
            .unwrap();

        let wire_format = build_http11_wire_format(&request);
        let calculated = ChannelBudget::calculate_request_size(&request);

        assert_eq!(
            calculated,
            wire_format.len(),
            "Calculated size {} doesn't match wire format size {}.\nWire format:\n{}",
            calculated,
            wire_format.len(),
            wire_format
        );
    }

    #[test]
    fn test_calculate_request_size_with_auth_header() {
        let body = r#"{"model":"gpt-4","messages":[{"role":"user","content":"hello"}]}"#;
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/chat/completions")
            .header("host", "api.openai.com")
            .header("accept-encoding", "identity")
            .header("connection", "keep-alive")
            .header("content-type", "application/json")
            .header("content-length", body.len().to_string())
            .header("authorization", "Bearer sk-test-key-1234567890")
            .body(body.to_string())
            .unwrap();

        let wire_format = build_http11_wire_format(&request);
        let calculated = ChannelBudget::calculate_request_size(&request);

        assert_eq!(
            calculated,
            wire_format.len(),
            "Calculated size {} doesn't match wire format size {}.\nWire format:\n{}",
            calculated,
            wire_format.len(),
            wire_format
        );
    }

    #[test]
    fn test_calculate_request_size_empty_body() {
        let request = Request::builder()
            .method(Method::GET)
            .uri("/health")
            .header("host", "example.com")
            .body(String::new())
            .unwrap();

        let wire_format = build_http11_wire_format(&request);
        let calculated = ChannelBudget::calculate_request_size(&request);

        assert_eq!(
            calculated,
            wire_format.len(),
            "Calculated size {} doesn't match wire format size {}.\nWire format:\n{}",
            calculated,
            wire_format.len(),
            wire_format
        );
    }

    #[test]
    fn test_calculate_request_size_large_body() {
        let body = "x".repeat(10000);
        let request = Request::builder()
            .method(Method::POST)
            .uri("/upload")
            .header("host", "example.com")
            .header("content-length", body.len().to_string())
            .body(body.clone())
            .unwrap();

        let wire_format = build_http11_wire_format(&request);
        let calculated = ChannelBudget::calculate_request_size(&request);

        assert_eq!(
            calculated,
            wire_format.len(),
            "Calculated size {} doesn't match wire format size {} for large body",
            calculated,
            wire_format.len()
        );
    }
}

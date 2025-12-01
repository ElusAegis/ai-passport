//! Byte budget tracking for TLS channel usage.
//!
//! This module provides types to track and enforce byte limits on send/receive
//! channels, primarily for TLS-notarized sessions where channel capacity is limited.

use crate::config::notary::NotaryConfig;
use anyhow::{bail, Result};
use hyper::body::Bytes;
use hyper::Request;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Estimated bytes per token for response size calculation.
/// Conservative estimate accounting for UTF-8 and JSON escaping.
const BYTES_PER_TOKEN: usize = 5;

/// Initial estimate for request overhead (HTTP headers).
/// Used until we observe real values.
const REQUEST_OVERHEAD_ESTIMATE: usize = 285;

/// Initial estimate for response overhead (JSON structure + HTTP headers).
/// Used until we observe real values.
const RESPONSE_OVERHEAD_ESTIMATE: usize = 5000;

/// Shared state for observed overhead values.
/// Initially uses estimates, then switches to real observed values.
#[derive(Debug, Clone, Default)]
pub struct ObservedOverhead {
    /// Observed request overhead (total - content). None = use estimate.
    request: Option<usize>,
    /// Observed response overhead (total - content). None = use estimate.
    response: Option<usize>,
}

impl ObservedOverhead {
    /// Get request overhead (observed or estimate).
    fn request_overhead(&self) -> usize {
        self.request.unwrap_or(REQUEST_OVERHEAD_ESTIMATE)
    }

    /// Get response overhead (observed or estimate).
    fn response_overhead(&self) -> usize {
        self.response.unwrap_or(RESPONSE_OVERHEAD_ESTIMATE)
    }

    /// Update request overhead from observed values.
    /// Warns if updating after initial observation (should be constant).
    fn update_request(&mut self, total_bytes: usize, content_bytes: usize) {
        let overhead = total_bytes.saturating_sub(content_bytes);
        if let Some(prev) = self.request {
            if prev != overhead {
                warn!(
                    "request overhead changed unexpectedly: {} -> {} (expected constant)",
                    prev, overhead
                );
            }
        } else {
            debug!(
                "overhead: observed request overhead = {} (total={}, content={})",
                overhead, total_bytes, content_bytes
            );
        }
        self.request = Some(overhead);
    }

    /// Update response overhead from observed values.
    /// Warns if updating after initial observation (should be constant).
    fn update_response(&mut self, total_bytes: usize, content_bytes: usize) {
        let overhead = total_bytes.saturating_sub(content_bytes);
        if let Some(prev) = self.response {
            if prev != overhead {
                warn!(
                    "response overhead changed unexpectedly: {} -> {} (expected constant)",
                    prev, overhead
                );
            }
        } else {
            debug!(
                "overhead: observed response overhead = {} (total={}, content={})",
                overhead, total_bytes, content_bytes
            );
        }
        self.response = Some(overhead);
    }
}

/// Tracks byte budget for send/receive channels.
#[derive(Debug, Clone)]
pub enum ByteBudget {
    /// No limits (DirectProver, TEE).
    Unlimited,
    /// Limited budget with tracking.
    Limited {
        /// Bytes remaining for sending.
        sent_remaining: usize,
        /// Bytes remaining for receiving.
        recv_remaining: usize,
        /// Shared overhead state (learns from observed values).
        overhead: Arc<Mutex<ObservedOverhead>>,
    },
}

impl ByteBudget {
    /// Create a limited budget from notary configuration.
    pub fn from_notary(config: &NotaryConfig) -> Self {
        Self::Limited {
            sent_remaining: config.max_total_sent,
            recv_remaining: config.max_total_recv,
            overhead: Arc::new(Mutex::new(ObservedOverhead::default())),
        }
    }

    /// Create a limited budget with shared overhead state.
    ///
    /// Use this when creating multiple budgets (e.g., per-message prover)
    /// that should share the same observed overhead values.
    pub fn from_notary_with_shared_overhead(
        config: &NotaryConfig,
        overhead: Arc<Mutex<ObservedOverhead>>,
    ) -> Self {
        Self::Limited {
            sent_remaining: config.max_total_sent,
            recv_remaining: config.max_total_recv,
            overhead,
        }
    }

    /// Get the shared overhead state (for creating sibling budgets).
    /// Returns `None` for unlimited budgets.
    pub fn shared_overhead(&self) -> Option<Arc<Mutex<ObservedOverhead>>> {
        match self {
            Self::Unlimited => None,
            Self::Limited { overhead, .. } => Some(Arc::clone(overhead)),
        }
    }

    /// Create an unlimited budget (for passthrough/TEE modes).
    pub fn unlimited() -> Self {
        Self::Unlimited
    }

    /// Check if we can send the given number of bytes.
    ///
    /// Takes the actual total bytes (headers + body) that will be sent.
    /// Returns an error with a helpful message if budget would be exceeded.
    pub fn check_request_fits(&self, total_bytes: usize) -> Result<()> {
        match self {
            Self::Unlimited => {}
            Self::Limited { sent_remaining, .. } => {
                if total_bytes > *sent_remaining {
                    bail!(
                        "Insufficient send budget. Need {} bytes but only {} remaining.\n\
                         Tip: Use shorter messages or start a new session.",
                        total_bytes,
                        sent_remaining
                    );
                }
            }
        }

        Ok(())
    }

    /// Record bytes sent and update remaining budget.
    ///
    /// Also updates the observed overhead for future estimates.
    pub fn record_sent(&mut self, total_bytes: usize, content_bytes: usize) {
        if let Self::Limited {
            sent_remaining,
            overhead,
            ..
        } = self
        {
            *sent_remaining = sent_remaining.saturating_sub(total_bytes);
            overhead
                .lock()
                .unwrap()
                .update_request(total_bytes, content_bytes);
            debug!(
                "budget: sent {} bytes, remaining={}",
                total_bytes, sent_remaining
            );
        }

        debug!(
            "overhead ↑: total={} content={} (ratio={:.1}x)",
            total_bytes,
            content_bytes,
            total_bytes as f64 / content_bytes.max(1) as f64
        );
    }

    /// Record bytes received and update remaining budget.
    ///
    /// Also updates the observed overhead for future estimates.
    pub fn record_recv(&mut self, total_bytes: usize, content_bytes: usize) {
        if let Self::Limited {
            recv_remaining,
            overhead,
            ..
        } = self
        {
            *recv_remaining = recv_remaining.saturating_sub(total_bytes);
            overhead
                .lock()
                .unwrap()
                .update_response(total_bytes, content_bytes);
            debug!(
                "budget: recv {} bytes, remaining={}",
                total_bytes, recv_remaining
            );
        }

        debug!(
            "overhead ↓: total={} content={} (ratio={:.1}x)",
            total_bytes,
            content_bytes,
            total_bytes as f64 / content_bytes.max(1) as f64
        );
    }

    /// Calculate max_tokens based on remaining receive budget.
    ///
    /// Uses observed response overhead if available, otherwise falls back to estimate.
    /// Returns `None` for unlimited budgets, meaning no limit should be set.
    /// Returns `Some(tokens)` for limited budgets.
    pub fn max_tokens_for_response(&self) -> Option<u32> {
        match self {
            Self::Unlimited => None,
            Self::Limited {
                recv_remaining,
                overhead,
                ..
            } => {
                let response_overhead = overhead.lock().unwrap().response_overhead();
                let usable = recv_remaining.saturating_sub(response_overhead);
                let tokens = (usable / BYTES_PER_TOKEN) as u32;
                // Ensure at least some tokens if there's any budget
                Some(tokens.max(1))
            }
        }
    }

    /// Get remaining input capacity for user display.
    ///
    /// Uses observed request overhead if available, otherwise falls back to estimate.
    /// Returns `None` for unlimited budgets.
    /// Returns `Some(bytes)` showing how many bytes the user can still send.
    pub fn available_input_bytes(&self) -> Option<usize> {
        match self {
            Self::Unlimited => None,
            Self::Limited {
                sent_remaining,
                overhead,
                ..
            } => {
                let request_overhead = overhead.lock().unwrap().request_overhead();
                Some(sent_remaining.saturating_sub(request_overhead))
            }
        }
    }

    /// Get remaining receive capacity.
    ///
    /// Returns `None` for unlimited budgets.
    pub fn available_recv_bytes(&self) -> Option<usize> {
        match self {
            Self::Unlimited => None,
            Self::Limited { recv_remaining, .. } => Some(*recv_remaining),
        }
    }

    /// Check if this is an unlimited budget.
    pub fn is_unlimited(&self) -> bool {
        matches!(self, Self::Unlimited)
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

/// Tracks overhead statistics for debugging and tuning.
///
/// Collects measurements to help determine if overhead is constant
/// and could be hardcoded in the future.
#[allow(dead_code)] // For future use when we want aggregate stats
#[derive(Debug, Default)]
pub struct OverheadStats {
    /// Request header overhead per request (total - body).
    pub request_overhead: Vec<usize>,
    /// Response header overhead per response (total - body).
    pub response_overhead: Vec<usize>,
}

#[allow(dead_code)] // For future use when we want aggregate stats
impl OverheadStats {
    /// Create a new overhead stats tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record request overhead.
    pub fn record_request(&mut self, body_bytes: usize, total_bytes: usize) {
        let overhead = total_bytes.saturating_sub(body_bytes);
        self.request_overhead.push(overhead);
    }

    /// Record response overhead.
    pub fn record_response(&mut self, body_bytes: usize, total_bytes: usize) {
        let overhead = total_bytes.saturating_sub(body_bytes);
        self.response_overhead.push(overhead);
    }

    /// Log a summary of observed overhead.
    pub fn log_summary(&self) {
        if !self.request_overhead.is_empty() {
            let avg: usize =
                self.request_overhead.iter().sum::<usize>() / self.request_overhead.len();
            let min = self.request_overhead.iter().min().unwrap_or(&0);
            let max = self.request_overhead.iter().max().unwrap_or(&0);
            debug!(
                "overhead stats: request overhead avg={}, min={}, max={} (n={})",
                avg,
                min,
                max,
                self.request_overhead.len()
            );
        }

        if !self.response_overhead.is_empty() {
            let avg: usize =
                self.response_overhead.iter().sum::<usize>() / self.response_overhead.len();
            let min = self.response_overhead.iter().min().unwrap_or(&0);
            let max = self.response_overhead.iter().max().unwrap_or(&0);
            debug!(
                "overhead stats: response overhead avg={}, min={}, max={} (n={})",
                avg,
                min,
                max,
                self.response_overhead.len()
            );
        }
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
    fn make_limited_budget(sent: usize, recv: usize) -> ByteBudget {
        ByteBudget::Limited {
            sent_remaining: sent,
            recv_remaining: recv,
            overhead: Arc::new(Mutex::new(ObservedOverhead::default())),
        }
    }

    #[test]
    fn test_unlimited_budget() {
        let budget = ByteBudget::unlimited();
        assert!(budget.is_unlimited());

        let request = make_test_request("test body");
        let request_size = ByteBudget::calculate_request_size(&request);
        assert!(budget.check_request_fits(request_size).is_ok());
        assert!(budget.max_tokens_for_response().is_none());
        assert!(budget.available_input_bytes().is_none());
    }

    #[test]
    fn test_limited_budget_check_send() {
        // Small request should succeed
        let budget = make_limited_budget(1000, 2000);
        let small_request = make_test_request("small");
        let small_request_size = ByteBudget::calculate_request_size(&small_request);
        assert!(budget.check_request_fits(small_request_size).is_ok());

        // Large request should fail
        let budget = make_limited_budget(50, 2000);
        let request = make_test_request("this body is too large for the budget");
        let request_size = ByteBudget::calculate_request_size(&request);
        assert!(budget.check_request_fits(request_size).is_err());
    }

    #[test]
    fn test_limited_budget_record_sent() {
        let mut budget = make_limited_budget(1000, 2000);

        budget.record_sent(150, 100); // total=150, content=100

        match budget {
            ByteBudget::Limited { sent_remaining, .. } => {
                assert_eq!(sent_remaining, 850);
            }
            _ => panic!("Expected Limited budget"),
        }
    }

    #[test]
    fn test_max_tokens_calculation() {
        let budget = make_limited_budget(1000, 2000);

        let tokens = budget.max_tokens_for_response().unwrap();
        // (2000 - 285) / 4 = 428
        assert_eq!(tokens, 428);
    }

    #[test]
    fn test_available_input_bytes() {
        let budget = make_limited_budget(1000, 2000);

        let available = budget.available_input_bytes().unwrap();
        // 1000 - 500 = 500
        assert_eq!(available, 500);
    }

    #[test]
    fn test_overhead_updates_via_budget() {
        let mut budget = make_limited_budget(1000, 2000);

        // Initially uses estimates
        assert_eq!(budget.available_input_bytes().unwrap(), 500); // 1000 - 500 estimate
        assert_eq!(budget.max_tokens_for_response().unwrap(), 428); // (2000 - 285) / 4

        // Record sends/recvs which update overhead
        budget.record_sent(300, 100); // overhead = 200
        budget.record_recv(400, 200); // overhead = 200

        // Now uses observed values (and remaining is reduced)
        // sent_remaining = 1000 - 300 = 700
        // available_input = 700 - 200 (observed) = 500
        assert_eq!(budget.available_input_bytes().unwrap(), 500);

        // recv_remaining = 2000 - 400 = 1600
        // max_tokens = (1600 - 200) / 4 = 350
        assert_eq!(budget.max_tokens_for_response().unwrap(), 350);
    }

    #[test]
    fn test_shared_overhead_between_budgets() {
        // Create first budget
        let mut budget1 = make_limited_budget(1000, 2000);

        // Get shared overhead for sibling budget
        let shared = budget1.shared_overhead().unwrap();

        // Create second budget sharing overhead (simulates per-message prover)
        let budget2 = ByteBudget::Limited {
            sent_remaining: 1000,
            recv_remaining: 2000,
            overhead: shared,
        };

        // Initially both use estimates
        assert_eq!(budget1.available_input_bytes().unwrap(), 500);
        assert_eq!(budget2.available_input_bytes().unwrap(), 500);

        // Record on first budget (updates shared overhead)
        budget1.record_sent(300, 100); // overhead = 200

        // Second budget sees updated overhead
        assert_eq!(budget2.available_input_bytes().unwrap(), 800); // 1000 - 200 observed
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
        let calculated = ByteBudget::calculate_request_size(&request);

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
        let calculated = ByteBudget::calculate_request_size(&request);

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
        let calculated = ByteBudget::calculate_request_size(&request);

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
        let calculated = ByteBudget::calculate_request_size(&request);

        assert_eq!(
            calculated,
            wire_format.len(),
            "Calculated size {} doesn't match wire format size {} for large body",
            calculated,
            wire_format.len()
        );
    }
}

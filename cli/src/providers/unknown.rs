use super::Provider;

/// Fallback provider using OpenAI-compatible API format (the default)
#[derive(Debug, Clone, Default)]
pub struct Unknown;

impl Provider for Unknown {
    /// Response headers to censor for privacy (default: common tracking headers)
    fn response_censor_headers(&self) -> &'static [&'static str] {
        &["request-id", "cf-ray", "server-timing", "report-to"]
    }
}

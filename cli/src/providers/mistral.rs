use super::Provider;

/// Mistral AI provider - uses OpenAI-compatible API format
#[derive(Debug, Clone, Default)]
pub struct Mistral;

impl Provider for Mistral {
    // Uses default OpenAI-style endpoints, auth, body, and parsing

    fn models_headers(&self, api_key: &str) -> Vec<(&'static str, String)> {
        vec![("Authorization", format!("Bearer {}", api_key))]
    }

    fn response_censor_headers(&self) -> &'static [&'static str] {
        &[
            "request-id",
            "cf-ray",
            "server-timing",
            "report-to",
            "x-kong-request-id",
        ]
    }
}

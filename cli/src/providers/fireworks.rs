use super::Provider;

/// Fireworks AI provider - OpenAI-compatible with custom endpoint paths
#[derive(Debug, Clone, Default)]
pub struct Fireworks;

impl Provider for Fireworks {
    fn chat_endpoint(&self) -> &'static str {
        "/inference/v1/chat/completions"
    }

    fn models_endpoint(&self) -> &'static str {
        "/inference/v1/models"
    }

    fn models_headers(&self, api_key: &str) -> Vec<(&'static str, String)> {
        vec![("Authorization", format!("Bearer {}", api_key))]
    }

    fn response_censor_headers(&self) -> &'static [&'static str] {
        &[
            "request-id",
            "cf-ray",
            "server-timing",
            "report-to",
            "x-request-id",
        ]
    }
}
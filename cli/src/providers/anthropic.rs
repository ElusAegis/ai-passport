use super::Provider;
use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct Anthropic;

impl Anthropic {
    const API_VERSION: &'static str = "2023-06-01";
    const MAX_TOKENS: u32 = 1024;
}

impl Provider for Anthropic {
    fn chat_endpoint(&self) -> &'static str {
        "/v1/messages"
    }

    fn chat_headers_with_key(&self, api_key: &str) -> Vec<(&'static str, String)> {
        vec![
            ("x-api-key", api_key.to_string()),
            ("anthropic-version", Self::API_VERSION.to_string()),
        ]
    }

    fn build_chat_body(&self, model_id: &str, messages: &[Value]) -> Value {
        json!({
            "model": model_id,
            "max_tokens": Self::MAX_TOKENS,
            "messages": messages,
            "stream": false,
        })
    }

    fn parse_chat_content<'a>(&self, response: &'a Value) -> Option<&'a str> {
        response["content"][0]["text"].as_str()
    }

    fn models_headers_with_key(&self, api_key: &str) -> Vec<(&'static str, String)> {
        vec![
            ("x-api-key", api_key.to_string()),
            ("anthropic-version", Self::API_VERSION.to_string()),
        ]
    }

    fn request_censor_headers(&self) -> &'static [&'static str] {
        &["x-api-key"]
    }

    fn response_censor_headers(&self) -> &'static [&'static str] {
        &[
            // Common
            "request-id",
            "cf-ray",
            "server-timing",
            "report-to",
            // Anthropic-specific
            "anthropic-ratelimit-requests-reset",
            "anthropic-ratelimit-tokens-reset",
            "x-kong-request-id",
        ]
    }
}

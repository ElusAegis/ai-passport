use super::budget::ExpectedChannelOverhead;
use super::{ChatMessage, Provider};
use anyhow::Result;
use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct Anthropic;

impl Anthropic {
    const API_VERSION: &'static str = "2023-06-01";

    /// Maximum tokens allowed for chat completion
    pub(crate) const MAX_TOKENS: u32 = 1024 * 10;

    /// Anthropic's response overhead
    const RESPONSE_OVERHEAD: usize = 1500;

    /// Anthropic's request overhead
    const REQUEST_OVERHEAD: usize = 370;
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

    fn build_chat_body(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        max_tokens: Option<u32>,
    ) -> Value {
        json!({
            "model": model_id,
            "max_tokens": max_tokens.unwrap_or(Self::MAX_TOKENS),
            "messages": messages,
            "stream": false,
        })
    }

    fn parse_chat_reply_message(&self, response: &Value) -> Result<ChatMessage> {
        let message = response["content"][0].as_object().ok_or_else(|| {
            anyhow::anyhow!("Failed to parse assistant message from Anthropic response")
        })?;

        let content = message
            .get("text")
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content in assistant message"))?;
        Ok(ChatMessage::assistant(content))
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

    fn expected_overhead(&self) -> ExpectedChannelOverhead {
        ExpectedChannelOverhead::new(Some(Self::REQUEST_OVERHEAD), Some(Self::RESPONSE_OVERHEAD))
    }
}

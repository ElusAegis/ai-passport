mod anthropic;
pub mod budget;
mod custom;
mod fireworks;
pub mod interaction;
pub mod message;
mod mistral;
mod redpill;
mod unknown;

use ambassador::{delegatable_trait, Delegate};
pub use anthropic::Anthropic;
use anyhow::Result;
pub use budget::ExpectedChannelOverhead;
use custom::Custom;
use derive_builder::Builder;
use dialoguer::console::style;
use enum_dispatch::enum_dispatch;
pub use fireworks::Fireworks;
pub use message::ChatMessage;
pub use mistral::Mistral;
pub use redpill::Redpill;
use serde_json::{json, Value};
use strum::IntoStaticStr;
use tracing::info;
pub use unknown::Unknown;

#[delegatable_trait]
#[enum_dispatch]
pub trait Provider {
    /// Endpoint path for chat/message completions (default: OpenAI-style)
    fn chat_endpoint(&self) -> &'static str {
        "/v1/chat/completions"
    }

    /// Provider-specific headers for chat/messages endpoint (default: Bearer token)
    fn chat_headers_with_key(&self, api_key: &str) -> Vec<(&'static str, String)> {
        vec![("Authorization", format!("Bearer {}", api_key))]
    }

    /// Build the request body with an optional max_tokens limit.
    ///
    /// Default implementation calls `build_chat_body` and merges `max_tokens` if provided.
    /// Providers can override for custom behavior.
    fn build_chat_body(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        max_tokens: Option<u32>,
    ) -> Value {
        let mut body = json!({
            "model": model_id,
            "messages": messages,
        });

        if let Some(tokens) = max_tokens {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("max_tokens".to_string(), json!(tokens));
            }
        }
        body
    }

    /// Parse the assistant's content from the response (default: OpenAI-style)
    fn parse_chat_reply_message<'a>(&self, response: &'a Value) -> Result<ChatMessage> {
        let message = response["choices"][0]["message"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse assistant message from response"))?;

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content in assistant message"))?;

        Ok(ChatMessage::assistant(content))
    }

    /// Endpoint path for listing available models (default: OpenAI-style)
    fn models_endpoint(&self) -> &'static str {
        "/v1/models"
    }

    /// Provider-specific headers for models endpoint (default: Bearer token)
    fn models_headers_with_key(&self, api_key: &str) -> Vec<(&'static str, String)> {
        vec![("Authorization", format!("Bearer {}", api_key))]
    }

    /// Request headers to censor for privacy (default: authorization)
    fn request_censor_headers(&self) -> &'static [&'static str] {
        &["authorization"]
    }

    /// Response headers to censor for privacy (default: common tracking headers)
    fn response_censor_headers(&self) -> &'static [&'static str];

    /// Expected HTTP overhead for capacity planning.
    ///
    /// Returns expected overhead values. Fields set to `None` use conservative defaults.
    /// Providers with known overhead characteristics can return specific values.
    ///
    /// If observed overhead differs significantly from expected, a warning is logged.
    fn expected_overhead(&self) -> ExpectedChannelOverhead {
        ExpectedChannelOverhead::default()
    }
}

#[derive(Debug, Clone, Builder, Delegate)]
#[delegate(Provider, target = "provider")]
pub struct ApiProvider {
    /// The domain of the provider hosting the model API
    /// Also used to auto-detect the provider.
    #[builder(setter(custom))]
    pub(crate) domain: String,
    /// The port of the provider hosting the model API
    #[builder(setter(into), default = "443")]
    pub(crate) port: u16,
    /// The provider of the model API (auto-derived from domain)
    /// Use `ApiProviderBuilder::domain` to set this field automatically.
    #[builder(setter(custom))]
    provider: ApiProviderInner,
    /// The API key for authentication with the model API
    #[builder(setter(into))]
    pub(crate) api_key: String,
}

impl ApiProvider {
    pub fn builder() -> ApiProviderBuilder {
        ApiProviderBuilder::default()
    }

    pub fn chat_headers(&self) -> Vec<(&'static str, String)> {
        self.chat_headers_with_key(&self.api_key)
    }

    pub fn models_headers(&self) -> Vec<(&'static str, String)> {
        self.models_headers_with_key(&self.api_key)
    }

    /// Get the provider name (e.g., "anthropic", "fireworks", "unknown").
    pub fn provider_name(&self) -> &'static str {
        (&self.provider).into()
    }
}

impl ApiProviderBuilder {
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        let domain_str = domain.into();
        let provider = ApiProviderInner::from_domain(&domain_str);
        self.domain = Some(domain_str);
        self.provider = Some(provider);
        self
    }
}

#[enum_dispatch(Provider)]
#[derive(Debug, Clone, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum ApiProviderInner {
    Unknown,
    Custom,
    Anthropic,
    Fireworks,
    Mistral,
    Redpill,
}

impl ApiProviderInner {
    /// Detect the appropriate provider based on domain
    fn from_domain(domain: &str) -> Self {
        if domain.contains("api.proof-of-autonomy.elusaegis.xyz") {
            Custom.into()
        } else if domain.contains("anthropic") {
            Anthropic.into()
        } else if domain.contains("fireworks") {
            Fireworks.into()
        } else if domain.contains("mistral") {
            Mistral.into()
        } else if domain.contains("red-pill") {
            Redpill.into()
        } else {
            info!(target: "plain",
                "{} {} {}",
                style("⚠").yellow().bold(),
                style(format!("Unknown provider for domain '{}'", domain)).yellow(),
                style("· Using OpenAI-compatible defaults").dim()
            );
            info!(target: "plain",
                "{}",
                style("  If this fails, specify --model-chat-route and --model-list-route manually.").dim()
            );
            Unknown.into()
        }
    }
}

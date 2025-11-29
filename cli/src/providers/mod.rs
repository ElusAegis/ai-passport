mod anthropic;
mod fireworks;
pub mod interaction;
mod mistral;
mod redpill;
mod unknown;

use ambassador::{delegatable_trait, Delegate};
pub use anthropic::Anthropic;
use derive_builder::Builder;
pub use fireworks::Fireworks;
pub use mistral::Mistral;
pub use redpill::Redpill;
pub use unknown::Unknown;

use dialoguer::console::style;
use enum_dispatch::enum_dispatch;
use serde_json::{json, Value};
use tracing::info;

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

    /// Build the request body for a chat completion (default: OpenAI-style)
    fn build_chat_body(&self, model_id: &str, messages: &[Value]) -> Value {
        json!({
            "model": model_id,
            "messages": messages
        })
    }

    /// Parse the assistant's content from the response (default: OpenAI-style)
    fn parse_chat_content<'a>(&self, response: &'a Value) -> Option<&'a str> {
        response["choices"][0]["message"]["content"].as_str()
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
#[derive(Debug, Clone)]
enum ApiProviderInner {
    Unknown,
    Anthropic,
    Fireworks,
    Mistral,
    Redpill,
}

impl ApiProviderInner {
    /// Detect the appropriate provider based on domain
    fn from_domain(domain: &str) -> Self {
        if domain.contains("anthropic") {
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

use crate::remote::attribution::config::model_selection::select_model_id;
use anyhow::{Context, Result};
use derive_builder::Builder;
use load_api_key::load_api_key;

mod load_api_key;
mod model_selection;

/// Configuration for API settings, including server endpoints and the API key
/// The API is expected to follow the OpenAI API specification
#[derive(Builder)]
pub(crate) struct ModelApiSettings {
    /// The domain of the server hosting the model API
    #[builder(setter(into), default = "String::from(\"api.red-pill.ai\")")]
    pub(crate) server_domain: String,
    /// The route for inference requests
    #[builder(setter(into), default = "String::from(\"/v1/chat/completions\")")]
    pub(crate) inference_route: String,
    /// The route for listing available models
    #[builder(setter(into), default = "String::from(\"/v1/models\")")]
    pub(crate) model_list_route: String,
    /// The API key for authentication with the model API
    pub(crate) api_key: String,
}

impl ModelApiSettings {
    /// Creates a new builder for `ModelApiSettings`
    pub(crate) fn builder() -> ModelApiSettingsBuilder {
        ModelApiSettingsBuilder::default()
    }
}

/// Privacy settings including topics to censor in requests and responses
#[allow(dead_code)]
pub(crate) struct PrivacySettings {
    pub(crate) request_topics_to_censor: &'static [&'static str],
    pub(crate) response_topics_to_censor: &'static [&'static str],
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            request_topics_to_censor: &["authorization"],
            response_topics_to_censor: &[
                "anthropic-ratelimit-requests-reset",
                "anthropic-ratelimit-tokens-reset",
                "request-id",
                "x-kong-request-id",
                "cf-ray",
                "server-timing",
                "report-to",
            ],
        }
    }
}

/// Model settings including API settings, model ID, and setup prompt
pub(crate) struct ModelSettings {
    pub api_settings: ModelApiSettings,
    pub id: String,
}

impl ModelSettings {
    fn new(model_id: String, api_settings: ModelApiSettings) -> Self {
        Self {
            api_settings,
            id: model_id,
        }
    }
}

/// Complete application configuration including model, privacy, and notary settings
#[derive(Builder)]
#[builder(pattern = "owned")]
pub(crate) struct ApplicationConfig {
    pub(crate) model_settings: ModelSettings,
    #[builder(default)]
    #[allow(dead_code)]
    pub(crate) privacy_settings: PrivacySettings,
}

impl ApplicationConfig {
    /// Creates a new builder for `ApplicationConfig`
    pub(crate) fn builder() -> ApplicationConfigBuilder {
        ApplicationConfigBuilder::default()
    }

    /// Setup configuration by loading API key, selecting a model, and returning Config
    pub(crate) async fn setup() -> Result<Self> {
        let api_key = load_api_key().context("Failed to load API key")?;
        let api_settings: ModelApiSettings = ModelApiSettings::builder()
            .api_key(api_key)
            .build()
            .context("Failed to build API settings")?;

        let model_id = select_model_id(&api_settings)
            .await
            .context("Failed to select model")?;

        let model_settings = ModelSettings::new(model_id, api_settings);

        Ok(Self::builder().model_settings(model_settings).build()?)
    }
}

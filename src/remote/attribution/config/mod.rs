use crate::remote::attribution::config::model_selection::select_model_id;
use anyhow::{Context, Result};
use load_api_key::load_api_key;
use std::sync::LazyLock;

mod load_api_key;
mod model_selection;

static SETUP_PROMPT: LazyLock<&str> =
    LazyLock::new(|| "Model Prompt: YOU ARE GOING TO BE ACTING AS A HELPFUL ASSISTANT");

/// Configuration for API settings, including server endpoints and the API key
#[derive(Debug, Default)]
pub struct ModelApiSettings {
    pub server_domain: &'static str,
    pub inference_route: &'static str,
    pub model_list_route: &'static str,
    pub api_key: String,
}

impl ModelApiSettings {
    fn new(api_key: String) -> Self {
        Self {
            server_domain: "api.red-pill.ai",
            inference_route: "/v1/chat/completions",
            model_list_route: "/v1/models",
            api_key,
        }
    }
}

#[derive(Debug)]
pub struct NotarySettings {
    pub host: &'static str,
    pub port: u16,
    pub path: &'static str,
}

/// Configuration for Notary settings, defining host, port, and path
impl Default for NotarySettings {
    fn default() -> Self {
        NotarySettings {
            host: "notary.pse.dev",
            port: 443,
            path: "nightly",
        }
    }
}

/// Privacy settings including topics to censor in requests and responses
#[derive(Debug, Default)]
pub struct PrivacySettings {
    pub request_topics_to_censor: &'static [&'static str],
    pub response_topics_to_censor: &'static [&'static str],
}

impl PrivacySettings {
    fn new() -> Self {
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
#[derive(Debug)]
pub struct ModelSettings {
    pub api_settings: ModelApiSettings,
    pub id: String,
    pub setup_prompt: &'static str,
}

impl ModelSettings {
    fn new(model_id: String, api_settings: ModelApiSettings) -> Self {
        Self {
            api_settings,
            id: model_id,
            setup_prompt: *SETUP_PROMPT,
        }
    }
}

/// Complete application configuration including model, privacy, and notary settings
#[derive(Debug)]
pub struct Config {
    pub model_settings: ModelSettings,
    pub privacy_settings: PrivacySettings,
    pub notary_settings: NotarySettings,
}

impl Config {
    fn new(model_settings: ModelSettings) -> Self {
        Self {
            model_settings,
            privacy_settings: PrivacySettings::new(),
            notary_settings: NotarySettings::default(),
        }
    }
}

/// Setup configuration by loading API key, selecting a model, and returning Config
pub(super) async fn setup_config() -> Result<Config> {
    let api_key = load_api_key().context("Failed to load API key")?;
    let api_settings = ModelApiSettings::new(api_key.clone());

    let model_id = select_model_id(&api_settings)
        .await
        .context("Failed to select model")?;

    let model_settings = ModelSettings::new(model_id, api_settings);

    Ok(Config::new(model_settings))
}

use crate::args::{ProveArgs, SessionMode, VerifyArgs};
use crate::config::load_api_domain::load_api_domain;
use crate::config::load_api_key::load_api_key;
use crate::config::load_api_port::load_api_port;
use crate::config::select_model::select_model_id;
use crate::config::select_proof_path::select_proof_path;
use anyhow::{Context, Result};
use derive_builder::Builder;
use dialoguer::console::Term;
use std::path::PathBuf;
use tlsn_common::config::NetworkSetting;

mod load_api_domain;
mod load_api_key;
mod load_api_port;
mod select_model;
mod select_proof_path;

/// Privacy settings including topics to censor in requests and responses
#[derive(Builder, Clone)]
pub struct PrivacyConfig {
    pub(crate) request_topics_to_censor: &'static [&'static str],
    pub(crate) response_topics_to_censor: &'static [&'static str],
}

impl Default for PrivacyConfig {
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

#[derive(Builder, Clone)]
pub struct ModelConfig {
    /// The domain of the server hosting the model API
    pub(crate) domain: String,
    /// The port of the server hosting the model API
    #[builder(setter(into))]
    pub(crate) port: u16,
    /// The route for inference requests
    #[builder(setter(into), default = "String::from(\"/v1/chat/completions\")")]
    pub(crate) inference_route: String,
    /// The route for listing available models
    #[builder(setter(into), default = "String::from(\"/v1/models\")")]
    pub(crate) model_list_route: String,
    /// The API key for authentication with the model API
    #[builder(setter(into))]
    pub(crate) api_key: String,
    /// The ID of the model
    #[builder(setter(into))]
    pub model_id: String,
}

impl ModelConfig {
    pub fn builder() -> ModelConfigBuilder {
        ModelConfigBuilder::default()
    }
}

#[derive(Builder, Clone, Copy)]
pub struct NotaryConfig {
    /// Maximum expected number of requests to send
    pub(crate) max_req_num_sent: usize,
    /// Maximum number of bytes in user prompt
    pub(crate) max_single_request_size: usize,
    /// Maximum number of bytes in the response
    pub(crate) max_single_response_size: usize,
    /// Network optimization strategy
    #[builder(default)]
    pub(crate) network_optimization: NetworkSetting,
    /// Two modes:
    /// - **One‑shot**: we spin up a fresh protocol instance per request/response pair,
    ///   so we can size the send/recv budgets exactly from the *current* message sizes.
    /// - **Multi‑round (default)**: the model API is stateless; every new request must
    ///   re‑send the whole conversation so far. That makes request sizes grow roughly
    ///   quadratically with the number of rounds.
    #[builder(default)]
    pub(crate) is_one_shot_mode: bool,
}

impl NotaryConfig {
    pub fn builder() -> NotaryConfigBuilder {
        NotaryConfigBuilder::default()
    }
}

#[derive(Builder, Clone)]
#[builder(pattern = "owned")]
pub struct ProveConfig {
    pub(crate) model_config: ModelConfig,
    #[builder(default)]
    pub(crate) privacy_config: PrivacyConfig,
    pub(crate) notary_config: NotaryConfig,
}

impl ProveConfig {
    pub fn builder() -> ProveConfigBuilder {
        ProveConfigBuilder::default()
    }

    pub(crate) async fn setup(args: ProveArgs) -> Result<ProveConfig> {
        let _ = dotenvy::from_filename(args.env_file);

        let api_domain = load_api_domain().context("Failed to load API domain")?;
        let api_key = load_api_key().context("Failed to load API key")?;
        let api_port = load_api_port().context("Failed to load API port")?;

        let mut model_config_builder = ModelConfig::builder()
            .api_key(api_key)
            .domain(api_domain)
            .port(api_port)
            .clone();

        let model_id = match args.model_id {
            Some(id) => id,
            None => select_model_id(&model_config_builder.model_id("tmp").build()?)
                .await
                .context("Failed to select model")?,
        };

        let model_config = model_config_builder
            .model_id(model_id)
            .build()
            .context("Failed to build model")?;

        let notary_config = NotaryConfig::builder()
            .max_req_num_sent(args.max_req_num_sent)
            .max_single_request_size(args.max_single_request_size)
            .max_single_response_size(args.max_single_response_size)
            .is_one_shot_mode(matches!(args.session_mode, SessionMode::OneShot))
            .network_optimization(args.network_optimization)
            .build()?;

        let term = Term::stderr();

        let summary = format!(
            "{} {} {}\n",
            dialoguer::console::style("✔").cyan(),
            dialoguer::console::style("Configuration setup complete").bold(),
            dialoguer::console::style("✔").cyan(),
        );

        term.write_line(&summary)?;

        Self::builder()
            .model_config(model_config)
            .notary_config(notary_config)
            .build()
            .map_err(Into::into)
    }
}

#[derive(Builder)]
pub struct VerifyConfig {
    pub(crate) proof_path: PathBuf,
}

impl VerifyConfig {
    pub(crate) fn builder() -> VerifyConfigBuilder {
        VerifyConfigBuilder::default()
    }

    pub(crate) fn setup(args: VerifyArgs) -> Result<VerifyConfig> {
        let raw_path = match args.proof_path {
            Some(path) => path,
            None => select_proof_path()?,
        };

        // Prefer a canonical absolute path if possible
        let path = PathBuf::from(raw_path);
        let path = std::fs::canonicalize(&path).unwrap_or(path);

        Self::builder().proof_path(path).build().map_err(Into::into)
    }
}

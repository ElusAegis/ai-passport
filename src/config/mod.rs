use crate::config::load_api_domain::load_api_domain;
use crate::config::load_api_key::load_api_key;
use crate::config::select_model::select_model_id;
use crate::config::select_proof_path::select_proof_path;
use anyhow::{Context, Result};
use clap::ValueHint;
use clap::{Args, Subcommand};
use derive_builder::Builder;
use std::path::PathBuf;

mod load_api_domain;
mod load_api_key;
mod select_model;
mod select_proof_path;

// Maximum number of bytes that can be sent from prover to server
const MAX_SENT_DATA: usize = 1 << 10;
// Maximum number of bytes that can be received by prover from server
const MAX_RECV_DATA: usize = 1 << 14;

/// Privacy settings including topics to censor in requests and responses
#[allow(dead_code)]
pub(crate) struct PrivacyConfig {
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

#[derive(Builder)]
pub(crate) struct ModelConfig {
    /// The domain of the server hosting the model API
    pub(crate) domain: String,
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
    pub(crate) fn builder() -> ModelConfigBuilder {
        ModelConfigBuilder::default()
    }
}

#[derive(Builder, Clone, Copy)]
pub(crate) struct NotaryConfig {
    /// Maximum number of bytes that can be sent from prover to server
    pub(crate) max_sent_data: usize,
    /// Maximum number of bytes that can be received by prover from server
    pub(crate) max_recv_data: usize,
}

impl NotaryConfig {
    pub(crate) fn builder() -> NotaryConfigBuilder {
        NotaryConfigBuilder::default()
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Prove model interaction
    Prove(ProveArgs),

    /// Verify model interaction
    Verify(VerifyArgs),
}

#[derive(Args, Debug)]
pub struct ProveArgs {
    /// Specify the model to use (optional for proving)
    #[arg(long)]
    pub(crate) model_id: Option<String>,
    /// Path to environment file (default: ./.env). Can also use APP_ENV_FILE.
    #[arg(long, value_hint = ValueHint::FilePath, default_value = ".env", env = "APP_ENV_FILE", global = true
    )]
    pub(crate) env_file: PathBuf,
    /// Maximum number of bytes that can be sent from prover to server
    #[arg(long, default_value_t = MAX_SENT_DATA)]
    pub(crate) max_sent_data: usize,
    /// Maximum number of bytes that can be received by prover from server
    #[arg(long, default_value_t = MAX_RECV_DATA)]
    pub(crate) max_recv_data: usize,
}

#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Path to the generated proof to verify (optional)
    #[arg(
        long,
        value_hint = ValueHint::FilePath,
        env = "APP_PROOF_PATH"
    )]
    pub(crate) proof_path: Option<String>,
}

#[derive(Builder)]
#[builder(pattern = "owned")]
pub struct ProveConfig {
    pub(crate) model_config: ModelConfig,
    #[builder(default)]
    #[allow(dead_code)]
    pub(crate) privacy_config: PrivacyConfig,
    pub(crate) notary_config: NotaryConfig,
}

impl ProveConfig {
    pub(crate) fn builder() -> ProveConfigBuilder {
        ProveConfigBuilder::default()
    }

    pub(crate) async fn setup(args: ProveArgs) -> Result<ProveConfig> {
        let _ = dotenvy::from_filename(args.env_file);

        let api_domain = load_api_domain().context("Failed to load API domain")?;
        let api_key = load_api_key().context("Failed to load API key")?;

        let mut model_config_builder = ModelConfig::builder()
            .api_key(api_key)
            .domain(api_domain)
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
            .max_sent_data(args.max_sent_data)
            .max_recv_data(args.max_recv_data)
            .build()?;

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

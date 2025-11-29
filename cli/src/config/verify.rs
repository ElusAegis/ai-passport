use crate::cli::VerifyArgs;
use crate::config::load::proof_path::load_proof_path;
use derive_builder::Builder;
use dialoguer::console::style;
use std::path::PathBuf;
use tracing::info;

#[derive(Builder)]
pub struct VerifyConfig {
    pub(crate) proof_path: PathBuf,
    pub(crate) accept_key: bool,
}

impl VerifyConfig {
    pub(crate) fn builder() -> VerifyConfigBuilder {
        VerifyConfigBuilder::default()
    }

    pub(crate) fn setup(args: VerifyArgs) -> anyhow::Result<VerifyConfig> {
        let raw_path = if let Some(path) = args.proof_path {
            path
        } else {
            load_proof_path()?
        };

        // Prefer a canonical absolute path if possible
        let path = std::fs::canonicalize(&raw_path).unwrap_or(raw_path);

        // Consistent, concise summary line
        info!(target: "plain",
            "{} {} {}",
            style("✔").green(),
            style("Selected proof path").bold(),
            style(path.display().to_string()).dim()
        );

        Self::builder()
            .proof_path(path)
            .accept_key(args.accept_key)
            .build()
            .map_err(Into::into)
    }
}

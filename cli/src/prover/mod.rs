//! Prover implementations for different proving strategies.
//!
//! Each prover type holds its own configuration and implements the [`Prover`] trait.
//! The shared interaction logic lives in [`crate::providers::interaction`] and is used by TLS provers.

pub(super) mod capacity;
mod direct;
mod tls_per_message;
mod tls_single_shot;

pub use direct::DirectProver;
pub use tls_per_message::TlsPerMessageProver;
pub use tls_single_shot::TlsSingleShotProver;

use crate::cli::ProveArgs;
use crate::config::ProveConfig;
use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use strum::IntoStaticStr;

/// Trait for all prover implementations.
///
/// Each prover holds its own configuration and implements `run` to execute
/// the proving session with a given model configuration.
#[async_trait]
pub trait Prover: Send + Sync {
    /// Run the proving session.
    ///
    /// The prover uses its internal configuration (notary settings, size limits, etc.)
    /// combined with the provided model configuration (API endpoint, credentials, model ID).
    async fn run(&self, config: &ProveConfig) -> Result<()>;
}

/// Prover kind - used for CLI selection and configuration loading
#[derive(Debug, Clone)]
pub enum ProverKind {
    /// Direct passthrough, no proving
    Direct,
    /// Single TLS session, proof at end
    TlsSingleShot,
    /// Fresh TLS per message, proof per message
    TlsPerMessage,
}

/// Enum holding a concrete prover instance.
///
/// This allows runtime selection of prover type while keeping static dispatch
/// within each variant.
#[derive(Debug, Clone, IntoStaticStr, serde::Serialize, serde::Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum AgentProver {
    Direct(DirectProver),
    TlsSingleShot(TlsSingleShotProver),
    TlsPerMessage(TlsPerMessageProver),
}

#[async_trait]
impl Prover for AgentProver {
    async fn run(&self, config: &ProveConfig) -> Result<()> {
        match self {
            Self::Direct(p) => p.run(config).await,
            Self::TlsSingleShot(p) => p.run(config).await,
            Self::TlsPerMessage(p) => p.run(config).await,
        }
    }
}

impl TryFrom<ProveArgs> for AgentProver {
    type Error = anyhow::Error;

    fn try_from(args: ProveArgs) -> Result<Self, Self::Error> {
        match args.prover {
            ProverKind::Direct => Ok(Self::Direct(DirectProver::new())),
            ProverKind::TlsSingleShot => {
                let notary = args
                    .notary
                    .try_into()
                    .context("Notary config required for TLS provers")?;
                Ok(Self::TlsSingleShot(TlsSingleShotProver::new(notary)))
            }
            ProverKind::TlsPerMessage => {
                let notary = args
                    .notary
                    .try_into()
                    .context("Notary config required for TLS provers")?;
                Ok(Self::TlsPerMessage(TlsPerMessageProver::new(notary)))
            }
        }
    }
}

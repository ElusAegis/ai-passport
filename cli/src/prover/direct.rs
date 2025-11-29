//! Direct Prover (Passthrough)
//!
//! A simple passthrough prover that makes direct HTTP calls without TLSNotary.
//! Does not produce any cryptographic proofs.
//!
//! **Best for**: Testing, development, or when proofs aren't needed.

use super::Prover;
use crate::config::ProveConfig;
use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

/// Direct passthrough prover - no TLSNotary, no proofs.
#[derive(Debug, Clone, Default)]
pub struct DirectProver {}

impl DirectProver {
    /// Create a new direct prover.
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Prover for DirectProver {
    async fn run(&self, config: &ProveConfig) -> Result<()> {
        info!(target: "plain", "DirectProver: Running in passthrough mode (no proofs will be generated)");
        info!(target: "plain", "Model: {}:{}", config.provider.domain, config.provider.port);

        // TODO: Implement direct HTTP interaction without TLSNotary
        // This would use a standard HTTP client (reqwest or hyper directly)
        // to make requests to the model API.
        //
        // For now, this is a stub that returns an empty outcome.

        info!(target: "plain", "DirectProver: Not yet implemented - returning empty outcome");

        Ok(())
    }
}

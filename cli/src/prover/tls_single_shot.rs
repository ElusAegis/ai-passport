//! TLS Single-Shot Prover
//!
//! Establishes a single TLS connection for the entire conversation and produces
//! one proof at the end containing all exchanges.
//!
//! **Best for**: Short conversations where you want an atomic proof of the full exchange.
//!
//! **Trade-off**: Sent bytes grow O(n²) due to conversation history being re-sent with each request.

use super::Prover;
use crate::config::notary::NotaryConfig;
use crate::config::ProveConfig;
use crate::providers::interaction::single_interaction_round;
use crate::tlsn::notarise::notarise_session;
use crate::tlsn::save_proof::save_to_file;
use crate::tlsn::setup::setup;
use crate::ui::spinner::with_spinner_future;
use crate::ui::user_messages::display_proofs;
use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::debug;

/// Configuration for TLS Single-Shot proving.
#[derive(Debug, Clone)]
pub struct TlsSingleShotProver {
    /// Notary configuration (server, budgets, etc.)
    pub notary: NotaryConfig,
}

impl TlsSingleShotProver {
    /// Create a new TLS Single-Shot prover with the given configuration.
    pub fn new(notary: NotaryConfig) -> Self {
        Self { notary }
    }
}

#[async_trait]
impl Prover for TlsSingleShotProver {
    async fn run(&self, config: &ProveConfig) -> Result<()> {
        // 1) Setup TLS connection and prover
        let (prover_task, mut request_sender) = with_spinner_future(
            "Please wait while the system is setup...",
            setup(&self.notary, &config.provider.domain, config.provider.port),
        )
        .await?;

        // 2) Interaction loop
        let mut messages = vec![];

        loop {
            // Single-shot uses keep-alive (close_connection = false)
            let was_stopped =
                single_interaction_round(&mut request_sender, config, &mut messages, false).await?;

            if was_stopped {
                drop(request_sender);
                break;
            }
        }

        // 3) Notarize the session
        debug!("Notarizing the session...");
        let (attestation, secrets) = with_spinner_future(
            "Generating a cryptographic proof of the conversation...",
            notarise_session(prover_task.await??),
        )
        .await
        .context("Error notarizing the session")?;

        // 4) Save the proof
        let file_path = save_to_file(
            &format!("{}_single_shot_proof", config.model_id),
            &attestation,
            &config.provider,
            &secrets,
        )?;

        let file_paths = vec![file_path];

        // 5) Display success
        display_proofs(&file_paths);

        Ok(())
    }
}

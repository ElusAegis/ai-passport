//! TLS Single-Shot Prover
//!
//! Establishes a single TLS connection for the entire conversation and produces
//! one proof at the end containing all exchanges.
//!
//! **Best for**: Short conversations where you want an atomic proof of the full exchange.
//!
//! **Trade-off**: Sent bytes grow O(n²) due to conversation history being re-sent with each request.

use super::capacity::estimate_single_shot_capacity;
use super::Prover;
use crate::config::notary::NotaryConfig;
use crate::config::ProveConfig;
use crate::providers::budget::ChannelBudget;
use crate::providers::interaction::single_interaction_round;
use crate::tlsn::notarise::notarise_session;
use crate::tlsn::save_proof::save_to_file;
use crate::tlsn::setup::setup;
use crate::ui::spinner::with_spinner_future;
use crate::ui::user_messages::display_proofs;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Configuration for TLS Single-Shot proving.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        // 1) Estimate optimal capacity using provider's expected overhead
        let optimal_notary = estimate_single_shot_capacity(&self.notary, config)
            .context("Error estimating single-shot capacity")?;

        // 2) Setup TLS connection and prover with sized capacity
        let (prover_task, mut request_sender) = with_spinner_future(
            "Please wait while the system is setup...",
            setup(
                &optimal_notary,
                &config.provider.domain,
                config.provider.port,
            ),
        )
        .await?;

        // 3) Create budget for tracking actual usage during the session
        let mut budget = ChannelBudget::from_config(&optimal_notary, config);

        // 4) Interaction loop
        let mut all_messages = vec![];

        loop {
            // Single-shot uses keep-alive (close_connection = false)
            let was_stopped = single_interaction_round(
                &mut request_sender,
                config,
                &mut all_messages,
                false,
                &mut budget,
            )
            .await?;

            if was_stopped {
                drop(request_sender);
                break;
            }
        }

        // 5) Notarize the session
        debug!("Notarizing the session...");
        let (attestation, secrets) = with_spinner_future(
            "Generating a cryptographic proof of the conversation...",
            notarise_session(prover_task.await??),
        )
        .await
        .context("Error notarizing the session")?;

        // 6) Save the proof
        let file_path = save_to_file(
            &format!("tls_{}_single_shot", config.model_id),
            &attestation,
            &config.provider,
            &secrets,
        )?;

        let file_paths = vec![file_path];

        // 7) Display success
        display_proofs(&file_paths);

        Ok(())
    }
}

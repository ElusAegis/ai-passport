//! TLS Per-Message Prover
//!
//! Creates a fresh TLS connection for each request/response pair, producing
//! one proof per message exchange. Uses pre-warming to reduce latency.
//!
//! **Best for**: Longer conversations, or when you need per-message proofs.
//!
//! **Trade-off**: More TLS handshakes, but exact size budgeting per round.

use super::Prover;
use crate::config::notary::NotaryConfig;
use crate::config::ProveConfig;
use crate::prover::capacity::estimate_round_capacity;
use crate::providers::budget::{ChannelBudget, ChannelCapacity};
use crate::providers::interaction::single_interaction_round;
use crate::tlsn::notarise::notarise_session;
use crate::tlsn::save_proof::save_to_file;
use crate::tlsn::setup::setup;
use crate::ui::user_messages::display_proofs;
use crate::ChatMessage;
use anyhow::{Context, Result};
use async_trait::async_trait;
use hyper::client::conn::http1::SendRequest;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tlsn_prover::{state, Prover as TlsnProver, ProverError};
use tokio::task::JoinHandle;
use tracing::debug;

type ProverWithRequestSender = (
    JoinHandle<Result<TlsnProver<state::Committed>, ProverError>>,
    SendRequest<String>,
);

// Type alias: the async block returns (Result<ProverWithRequestSender>, NotaryConfig)
type SetupResult = (Result<ProverWithRequestSender>, NotaryConfig);

/// Configuration for TLS Per-Message proving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsPerMessageProver {
    /// Notary configuration (server, budgets, etc.)
    pub notary: NotaryConfig,
}

impl TlsPerMessageProver {
    /// Create a new TLS Per-Message prover with the given configuration.
    pub fn new(notary: NotaryConfig) -> Self {
        Self { notary }
    }
}

#[async_trait]
impl Prover for TlsPerMessageProver {
    async fn run(&self, config: &ProveConfig) -> Result<()> {
        let domain = &config.provider.domain;
        let port = config.provider.port;

        // Budget tracks overhead observations across rounds
        let mut budget = ChannelBudget::with_capacity(ChannelCapacity::from_notary(&self.notary));

        // Helper to spawn a notary setup for a given lookahead
        let spawn_setup = |messages: &[ChatMessage], budget: &ChannelBudget, lookahead| {
            let domain = domain.clone();
            let notary_config =
                estimate_round_capacity(&self.notary, config, messages, budget, lookahead);
            tokio::spawn(async move { (setup(&notary_config, &domain, port).await, notary_config) })
        };

        let mut stored_proofs = Vec::<PathBuf>::new();
        let mut all_messages = vec![];

        // Set up the current instance of the prover
        let mut current_instance_handle: JoinHandle<SetupResult> =
            spawn_setup(&all_messages, &budget, 1);

        // Pre-warm the next instance with capacity for second round (lookahead=2)
        let mut future_instance_handle: JoinHandle<SetupResult> =
            spawn_setup(&all_messages, &budget, 2);

        let mut counter = 0;
        loop {
            // Wait for the current instance to be ready
            let (prover_result, notary_config) = current_instance_handle.await?;
            let mut current_instance = prover_result?;

            budget
                .reset()
                .set_capacity(ChannelCapacity::from_notary(&notary_config));

            // Per-message uses close connection (close_connection = true)
            let stop = single_interaction_round(
                &mut current_instance.1,
                config,
                &mut all_messages,
                true,
                &mut budget,
            )
            .await?;

            if stop {
                break;
            }

            // Notarize the session
            debug!("Notarizing the session...");
            let (attestation, secrets) = notarise_session(current_instance.0.await??)
                .await
                .context("Error notarizing the session")?;

            // Save the proof to a file
            stored_proofs.push(save_to_file(
                &format!("{}_part_{counter}_per_message_proof", config.model_id),
                &attestation,
                &config.provider,
                &secrets,
            )?);

            // Prepare for the next iteration - use the pre-warmed instance
            current_instance_handle = future_instance_handle;

            // Pre-warm the next instance with updated capacity estimate
            future_instance_handle = spawn_setup(&all_messages, &budget, 2);
            counter += 1;
        }

        display_proofs(&stored_proofs);

        Ok(())
    }
}

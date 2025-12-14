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
use crate::prover::capacity::estimate_per_message_capacity;
use crate::providers::budget::ChannelBudget;
use crate::providers::interaction::single_interaction_round;
use crate::tlsn::notarise::notarise_session;
use crate::tlsn::save_proof::save_to_file;
use crate::tlsn::setup::setup;
use crate::ui::user_messages::display_proofs;
use crate::utils::with_optional_timeout;
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
        let mut budget = ChannelBudget::from_config(&self.notary, config);

        // Helper to spawn a notary setup for a given lookahead
        let setup_timeout = config.request_timeout;
        let spawn_setup = |messages: &[ChatMessage], budget: &ChannelBudget, lookahead| {
            let domain = domain.clone();
            let notary_config = estimate_per_message_capacity(
                &self.notary,
                config,
                messages,
                budget.overhead(),
                lookahead,
            )?;
            Ok::<_, anyhow::Error>(tokio::spawn(async move {
                if lookahead > 1 {
                    // Sleep for 50ms to allow previous setup to progress
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
                let setup_result =
                    with_optional_timeout(setup(&notary_config, &domain, port), setup_timeout)
                        .await;
                (setup_result, notary_config)
            }))
        };

        let mut stored_proofs = Vec::<PathBuf>::new();
        let mut all_messages = vec![];
        let mut exchange_count = 0u32;

        // Helper to check if we need more exchanges after a given count
        let needs_more = |count: u32| {
            config
                .expected_exchanges
                .map_or(true, |expected| count < expected)
        };

        // Set up the current instance of the prover
        let mut current_handle: JoinHandle<SetupResult> =
            spawn_setup(&all_messages, &budget, 1)?;

        // Pre-warm the next instance (skip if only 1 exchange expected)
        let mut next_handle: Option<JoinHandle<SetupResult>> =
            needs_more(1).then(|| spawn_setup(&all_messages, &budget, 2)).transpose()?;

        loop {
            exchange_count += 1;

            // Wait for the current instance to be ready
            let (prover_result, notary_config) = current_handle.await?;
            let mut current_instance = prover_result?;

            budget.reset().set_capacity((&notary_config).into());

            // Per-message uses close connection (close_connection = true)
            let was_stopped = single_interaction_round(
                &mut current_instance.1,
                config,
                &mut all_messages,
                true,
                &mut budget,
            )
            .await?;

            let should_continue = !was_stopped && needs_more(exchange_count);

            // Notarize the session
            debug!("Notarizing the session...");
            let (attestation, secrets) = notarise_session(current_instance.0.await??)
                .await
                .context("Error notarizing the session")?;

            // Save the proof to a file
            let current_exchanges = (all_messages.len() / 2) as u32;
            stored_proofs.push(save_to_file(
                &format!(
                    "tls_{}_part_{current_exchanges}_per_message",
                    config.model_id
                ),
                &attestation,
                &config.provider,
                &secrets,
            )?);

            if !should_continue {
                break;
            }

            // Use pre-warmed instance for next iteration
            current_handle = next_handle.take().expect("pre-warmed instance should exist");

            // Pre-warm next instance only if we'll need it
            next_handle = needs_more(exchange_count + 1)
                .then(|| spawn_setup(&all_messages, &budget, 2))
                .transpose()?;
        }

        display_proofs(&stored_proofs);

        Ok(())
    }
}

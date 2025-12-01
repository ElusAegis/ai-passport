//! Direct Prover (Passthrough)
//!
//! A simple passthrough prover that makes direct HTTP calls without TLSNotary.
//! Does not produce any cryptographic proofs.
//!
//! **Best for**: Testing, development, or when proofs aren't needed.
//!
//! **Note**: Uses unlimited byte budget since there are no TLS channel constraints.

use super::Prover;
use crate::config::ProveConfig;
use crate::providers::budget::ChannelBudget;
use crate::providers::interaction::single_interaction_round;
use anyhow::{Context, Result};
use async_trait::async_trait;
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, info};

/// Direct passthrough prover - no TLSNotary, no proofs.
#[derive(Debug, Clone, Default)]
pub struct DirectProver {}

impl DirectProver {
    /// Create a new direct prover.
    pub fn new() -> Self {
        Self {}
    }

    async fn setup_connection(config: &ProveConfig) -> Result<SendRequest<String>> {
        // Set up TLS configuration with native root certificates
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(tls_config));

        // Connect to the server
        let domain = &config.provider.domain;
        let port = config.provider.port;

        let tcp_stream = TcpStream::connect((domain.as_str(), port))
            .await
            .context("Failed to connect to server")?;

        let server_name = ServerName::try_from(domain.clone()).context("Invalid server name")?;

        let tls_stream = connector
            .connect(server_name, tcp_stream)
            .await
            .context("TLS handshake failed")?;

        // Wrap with hyper's TokioIo adapter
        let io = TokioIo::new(tls_stream);

        // Create HTTP/1.1 connection
        let (request_sender, connection) = hyper::client::conn::http1::handshake(io)
            .await
            .context("HTTP handshake failed")?;

        // Spawn the connection task
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                debug!("HTTP connection error: {}", e);
            }
        });
        Ok(request_sender)
    }
}

#[async_trait]
impl Prover for DirectProver {
    async fn run(&self, config: &ProveConfig) -> Result<()> {
        info!(target: "plain", "DirectProver: Running in passthrough mode (no proofs will be generated)");
        info!(target: "plain", "Model: {}:{}", config.provider.domain, config.provider.port);

        // Direct prover uses unlimited budget (no TLS channel constraints)
        let mut budget = ChannelBudget::unlimited();
        debug!("budget: using unlimited (direct/passthrough mode)");

        let mut request_sender = Self::setup_connection(config).await?;

        // Interaction loop
        let mut messages = vec![];

        loop {
            // Direct mode uses keep-alive (close_connection = false)
            let was_stopped = single_interaction_round(
                &mut request_sender,
                config,
                &mut messages,
                false,
                &mut budget,
            )
            .await?;

            if was_stopped {
                break;
            }
        }

        info!(target: "plain", "DirectProver: Session complete (no proofs generated)");

        Ok(())
    }
}

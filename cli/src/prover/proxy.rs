//! Proxy Prover
//!
//! A prover that connects through an attestation proxy server.
//! The proxy records the transcript and returns an attestation on demand.
//!
//! [`ProxyProver`] supports both:
//! - LLM interactions via the `Prover` trait (`run()`)
//! - Tool data fetching via `fetch()` method
//!
//! **Best for**: Getting attestations without TLSNotary overhead.

use super::Prover;
use crate::config::ProveConfig;
use crate::providers::budget::ChannelBudget;
use crate::providers::interaction::single_interaction_round;
use crate::providers::Provider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{ACCEPT, CONNECTION, HOST};
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, info};

const PROOFS_DIR: &str = "proofs";

/// Configuration for the proxy server connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub host: String,
    pub port: u16,
}

/// Result of an attested fetch operation.
#[derive(Debug)]
pub struct AttestedResponse {
    /// HTTP status code
    pub status: StatusCode,
    /// Response body as string
    pub body: String,
    /// Path to saved attestation file (if requested)
    pub attestation_path: Option<PathBuf>,
}

/// Proxy-based prover - connects through attestation proxy.
///
/// Supports both LLM interactions (via `Prover::run()`) and
/// tool data fetching (via `fetch()`).
///
/// # Example - Tool data fetching
///
/// ```ignore
/// let prover = ProxyProver::new(ProxyConfig {
///     host: "proxy-tee.example.com".to_string(),
///     port: 8443,
/// });
///
/// let response = prover.fetch(
///     "gamma-api.polymarket.com",
///     443,
///     "/markets?limit=5",
///     true, // save attestation
/// ).await?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyProver {
    pub proxy: ProxyConfig,
}

impl ProxyProver {
    pub fn new(proxy: ProxyConfig) -> Self {
        Self { proxy }
    }

    /// Connect to the proxy server and return an HTTP sender.
    async fn connect(&self) -> Result<SendRequest<String>> {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(tls_config));

        let tcp_stream = TcpStream::connect((&*self.proxy.host, self.proxy.port))
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to proxy at {}:{}",
                    self.proxy.host, self.proxy.port
                )
            })?;

        let server_name =
            ServerName::try_from(self.proxy.host.clone()).context("Invalid proxy server name")?;

        let tls_stream = connector
            .connect(server_name, tcp_stream)
            .await
            .context("Proxy TLS handshake failed")?;

        let (sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(tls_stream))
            .await
            .context("HTTP handshake with proxy failed")?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("Proxy connection closed: {}", e);
            }
        });

        Ok(sender)
    }

    /// Fetch data from a target API through the proxy (for tools).
    ///
    /// # Arguments
    ///
    /// * `target_domain` - The target API domain (e.g., "gamma-api.polymarket.com")
    /// * `target_port` - The target API port (usually 443)
    /// * `path` - The request path including query string (e.g., "/markets?limit=5")
    /// * `save_attestation` - Whether to request and save an attestation after the fetch
    pub async fn fetch(
        &self,
        target_domain: &str,
        target_port: u16,
        path: &str,
        save_attestation: bool,
    ) -> Result<AttestedResponse> {
        let mut sender = self.connect().await?;

        debug!(
            "ProxyProver: fetching https://{}:{}{} via proxy",
            target_domain, target_port, path
        );

        // Build the target host header (include port if non-standard)
        let host_header = if target_port == 443 {
            target_domain.to_string()
        } else {
            format!("{}:{}", target_domain, target_port)
        };

        let request = Request::builder()
            .method(Method::GET)
            .uri(path)
            .header(HOST, &host_header)
            .header(ACCEPT, "application/json")
            .header(CONNECTION, "keep-alive")
            .body(String::new())
            .context("Failed to build request")?;

        let response = sender
            .send_request(request)
            .await
            .context("Request to proxy failed")?;

        let status = response.status();
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .context("Failed to read response body")?
            .to_bytes();

        let body =
            String::from_utf8(body_bytes.to_vec()).context("Invalid UTF-8 in response")?;

        // Optionally request attestation
        let attestation_path = if save_attestation {
            Some(self.fetch_attestation(&mut sender, target_domain).await?)
        } else {
            None
        };

        Ok(AttestedResponse {
            status,
            body,
            attestation_path,
        })
    }

    /// Request an attestation from the proxy for a tool fetch.
    async fn fetch_attestation(
        &self,
        sender: &mut SendRequest<String>,
        target_domain: &str,
    ) -> Result<PathBuf> {
        info!("Requesting attestation from proxy for {}", target_domain);

        let request = Request::builder()
            .method(Method::GET)
            .uri("/__attest")
            .header(HOST, target_domain)
            .header(CONNECTION, "close")
            .body(String::new())
            .context("Failed to build attestation request")?;

        let response = sender
            .send_request(request)
            .await
            .context("Attestation request failed")?;

        if response.status() != StatusCode::OK {
            anyhow::bail!(
                "Attestation request failed with status: {}",
                response.status()
            );
        }

        let body = response
            .into_body()
            .collect()
            .await
            .context("Failed to read attestation response")?
            .to_bytes();

        let json = String::from_utf8(body.to_vec()).context("Invalid UTF-8 in attestation")?;

        save_attestation(&json, target_domain, "tool")
    }
}

#[async_trait]
impl Prover for ProxyProver {
    async fn run(&self, config: &ProveConfig) -> Result<()> {
        info!(target: "plain", "ProxyProver: Connecting to proxy at {}:{}", self.proxy.host, self.proxy.port);
        info!(target: "plain", "Target API: {}:{}", config.provider.domain, config.provider.port);

        let mut sender = self.connect().await?;
        let mut budget = ChannelBudget::unlimited();
        let mut messages = vec![];

        loop {
            let stopped =
                single_interaction_round(&mut sender, config, &mut messages, false, &mut budget)
                    .await?;
            if stopped {
                break;
            }
        }

        let path = fetch_attestation_with_censoring(&mut sender, config).await?;
        info!(target: "plain", "Attestation saved to: {}", path.display());

        Ok(())
    }
}

/// Fetch attestation for LLM prover (with header censoring for API keys).
async fn fetch_attestation_with_censoring(
    sender: &mut SendRequest<String>,
    config: &ProveConfig,
) -> Result<PathBuf> {
    let censor_headers: Vec<&str> = config
        .provider
        .request_censor_headers()
        .iter()
        .chain(config.provider.response_censor_headers().iter())
        .copied()
        .collect();

    info!(target: "plain", "Requesting attestation from proxy...");
    debug!("Censoring headers: {:?}", censor_headers);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/__attest")
        .header(HOST, config.provider.domain.as_str())
        .header("x-censor-headers", censor_headers.join(","))
        .header(CONNECTION, "close")
        .body(String::new())
        .context("Failed to build attestation request")?;

    let response = sender
        .send_request(request)
        .await
        .context("Attestation request failed")?;

    if response.status() != StatusCode::OK {
        anyhow::bail!(
            "Attestation request failed with status: {}",
            response.status()
        );
    }

    let body = response
        .into_body()
        .collect()
        .await
        .context("Failed to read attestation response")?
        .to_bytes();

    let json = String::from_utf8(body.to_vec()).context("Invalid UTF-8 in attestation")?;

    save_attestation(&json, &config.provider.domain, "proxy")
}

fn save_attestation(json: &str, domain: &str, prefix: &str) -> Result<PathBuf> {
    fs::create_dir_all(PROOFS_DIR)
        .with_context(|| format!("Failed to create {} directory", PROOFS_DIR))?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_secs();

    let sanitized_domain = domain.replace([' ', '/', '.'], "_");
    let path = Path::new(PROOFS_DIR).join(format!("{prefix}_{sanitized_domain}_{ts}.json"));

    fs::write(&path, json).context("Failed to write attestation file")?;

    Ok(path)
}

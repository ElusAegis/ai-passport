//! Proxy HTTP client for attested data fetching.
//!
//! This module provides a client that routes HTTP requests through the
//! AI Passport proxy server, allowing attestation of external API calls
//! (e.g., fetching data from Polymarket or price feeds).

use anyhow::{Context, Result};
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{CONNECTION, HOST};
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, info};

const ATTESTATIONS_DIR: &str = "attestations";

/// Configuration for the proxy server.
#[derive(Debug, Clone)]
pub struct ProxyClientConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ProxyClientConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8443,
        }
    }
}

/// A client that routes requests through the attestation proxy.
pub struct ProxyClient {
    config: ProxyClientConfig,
    sender: Option<SendRequest<String>>,
}

impl ProxyClient {
    /// Create a new proxy client with the given configuration.
    pub fn new(config: ProxyClientConfig) -> Self {
        Self {
            config,
            sender: None,
        }
    }

    /// Connect to the proxy server.
    pub async fn connect(&mut self) -> Result<()> {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(Arc::new(tls_config));

        let tcp_stream = TcpStream::connect((&*self.config.host, self.config.port))
            .await
            .with_context(|| {
                format!(
                    "Failed to connect to proxy at {}:{}",
                    self.config.host, self.config.port
                )
            })?;

        let server_name =
            ServerName::try_from(self.config.host.clone()).context("Invalid proxy server name")?;

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

        self.sender = Some(sender);
        info!(
            target: "plain",
            "Connected to proxy at {}:{}",
            self.config.host, self.config.port
        );

        Ok(())
    }

    /// Send a GET request through the proxy.
    ///
    /// The `target_domain` is the actual API endpoint (e.g., "gamma-api.polymarket.com").
    /// The request is routed through the proxy which records the transcript.
    pub async fn get(&mut self, target_domain: &str, path: &str) -> Result<Vec<u8>> {
        let sender = self
            .sender
            .as_mut()
            .context("Not connected to proxy - call connect() first")?;

        let request = Request::builder()
            .method(Method::GET)
            .uri(path)
            .header(HOST, target_domain)
            .header("accept", "application/json")
            .body(String::new())
            .context("Failed to build GET request")?;

        debug!("Sending GET {} to {} via proxy", path, target_domain);

        let response = sender
            .send_request(request)
            .await
            .context("GET request failed")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "GET request failed with status: {} for {}{}",
                response.status(),
                target_domain,
                path
            );
        }

        let body = response
            .into_body()
            .collect()
            .await
            .context("Failed to read response body")?
            .to_bytes()
            .to_vec();

        Ok(body)
    }

    /// Request an attestation from the proxy for all recorded requests.
    ///
    /// This should be called after all data fetching is complete.
    /// Returns the path to the saved attestation file.
    pub async fn request_attestation(&mut self, domain_hint: &str) -> Result<PathBuf> {
        let sender = self
            .sender
            .as_mut()
            .context("Not connected to proxy - call connect() first")?;

        info!(target: "plain", "Requesting attestation from proxy...");

        let request = Request::builder()
            .method(Method::GET)
            .uri("/__attest")
            .header(HOST, domain_hint)
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

        save_attestation(&json, domain_hint)
    }
}

fn save_attestation(json: &str, domain: &str) -> Result<PathBuf> {
    fs::create_dir_all(ATTESTATIONS_DIR)
        .with_context(|| format!("Failed to create {} directory", ATTESTATIONS_DIR))?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_secs();

    let sanitized_domain = domain.replace([' ', '/', '.'], "_");
    let path = Path::new(ATTESTATIONS_DIR).join(format!("{sanitized_domain}_{ts}.json"));

    fs::write(&path, json).context("Failed to write attestation file")?;

    info!(target: "plain", "Attestation saved to: {}", path.display());

    Ok(path)
}

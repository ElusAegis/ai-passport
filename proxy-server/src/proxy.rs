//! Main proxy server logic.
//!
//! Handles incoming connections, forwards requests to backends,
//! records transcripts, and serves attestations.

use crate::transcript::{Attestation, TranscriptEntry};
use anyhow::{Context, Result};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use k256::ecdsa::SigningKey;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_rustls::{rustls, TlsAcceptor, TlsConnector};
use tracing::{debug, error, info};

/// Configuration for the proxy server.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub listen_addr: SocketAddr,
    pub cert_path: String,
    pub key_path: String,
    pub signing_key_path: String,
}

/// Shared state for backend connections.
struct BackendClient {
    tls_connector: TlsConnector,
}

impl BackendClient {
    fn new() -> Self {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        Self {
            tls_connector: TlsConnector::from(Arc::new(tls_config)),
        }
    }

    async fn forward(
        &self,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Bytes,
    ) -> Result<Response<Incoming>> {
        let (hostname, port) = parse_host(host);

        debug!("Forwarding to {}:{}{}", hostname, port, path);

        let stream = TcpStream::connect((hostname, port))
            .await
            .with_context(|| format!("Failed to connect to {}:{}", hostname, port))?;

        let server_name = hostname
            .to_string()
            .try_into()
            .context("Invalid server name")?;

        let tls_stream = self
            .tls_connector
            .connect(server_name, stream)
            .await
            .context("Backend TLS handshake failed")?;

        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(tls_stream))
            .await
            .context("Backend HTTP handshake failed")?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("Backend connection closed: {}", e);
            }
        });

        let mut req_builder = Request::builder().method(method).uri(path);

        for (name, value) in headers {
            if !is_hop_by_hop(name) {
                req_builder = req_builder.header(name.as_str(), value.as_str());
            }
        }

        let request = req_builder
            .body(Full::new(body))
            .context("Failed to build backend request")?;

        sender
            .send_request(request)
            .await
            .context("Backend request failed")
    }
}

/// Per-connection session state.
struct Session {
    transcript: Vec<TranscriptEntry>,
    target_host: Option<String>,
}

impl Session {
    fn new() -> Self {
        Self {
            transcript: Vec::new(),
            target_host: None,
        }
    }
}

/// Run the proxy server.
pub async fn run_server(config: ProxyConfig) -> Result<()> {
    let certs = load_certs(&config.cert_path)?;
    let key = load_key(&config.key_path)?;
    let signing_key = load_signing_key(&config.signing_key_path)?;
    let signing_key = Arc::new(signing_key);

    let tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("Failed to build server TLS config")?;

    let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));
    let backend = Arc::new(BackendClient::new());
    let listener = TcpListener::bind(&config.listen_addr).await?;

    info!("Proxy server listening on {}", config.listen_addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let tls_acceptor = tls_acceptor.clone();
        let backend = backend.clone();
        let signing_key = signing_key.clone();

        tokio::spawn(async move {
            if let Err(e) =
                handle_connection(stream, peer_addr, tls_acceptor, backend, signing_key).await
            {
                error!("Connection error from {}: {}", peer_addr, e);
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    tls_acceptor: TlsAcceptor,
    backend: Arc<BackendClient>,
    signing_key: Arc<SigningKey>,
) -> Result<()> {
    debug!("New connection from {}", peer_addr);

    let tls_stream = tls_acceptor
        .accept(stream)
        .await
        .context("TLS handshake failed")?;

    let session = Arc::new(Mutex::new(Session::new()));

    http1::Builder::new()
        .preserve_header_case(true)
        .serve_connection(
            TokioIo::new(tls_stream),
            service_fn({
                let session = session.clone();
                let signing_key = signing_key.clone();
                move |req| {
                    let session = session.clone();
                    let backend = backend.clone();
                    let signing_key = signing_key.clone();
                    async move { handle_request(req, session, backend, signing_key).await }
                }
            }),
        )
        .await
        .context("HTTP connection error")?;

    debug!("Connection closed from {}", peer_addr);
    Ok(())
}

async fn handle_request(
    req: Request<Incoming>,
    session: Arc<Mutex<Session>>,
    backend: Arc<BackendClient>,
    signing_key: Arc<SigningKey>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let path = req.uri().path();

    if path == "/__attest" {
        return Ok(handle_attestation(req, session, &signing_key).await);
    }

    match handle_proxy(req, session, backend).await {
        Ok(response) => Ok(response),
        Err(e) => {
            error!("Proxy error: {}", e);
            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Full::new(Bytes::from(format!("Proxy error: {}", e))))
                .unwrap())
        }
    }
}

async fn handle_attestation(
    req: Request<Incoming>,
    session: Arc<Mutex<Session>>,
    signing_key: &SigningKey,
) -> Response<Full<Bytes>> {
    let censor_headers: Vec<String> = req
        .headers()
        .get("x-censor-headers")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').map(|h| h.trim().to_string()).collect())
        .unwrap_or_default();

    let (transcript, target_host) = {
        let session = session.lock().await;
        (
            session.transcript.clone(),
            session.target_host.clone().unwrap_or_default(),
        )
    };

    let attestation =
        Attestation::build_and_sign(transcript, target_host, &censor_headers, signing_key);

    match serde_json::to_string_pretty(&attestation) {
        Ok(json) => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(json)))
            .unwrap(),
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::new(Bytes::from(format!("JSON error: {}", e))))
            .unwrap(),
    }
}

async fn handle_proxy(
    req: Request<Incoming>,
    session: Arc<Mutex<Session>>,
    backend: Arc<BackendClient>,
) -> Result<Response<Full<Bytes>>> {
    let host = req
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .context("Missing Host header")?
        .to_string();

    let method = req.method().to_string();
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    let headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let body_bytes = req.collect().await?.to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();

    // Record request
    {
        let mut session = session.lock().await;
        session.target_host = Some(host.clone());
        session.transcript.push(TranscriptEntry::request(
            method.clone(),
            path.clone(),
            headers.clone(),
            body_str,
        ));
    }

    // Forward to backend
    let response = backend
        .forward(&host, &method, &path, &headers, body_bytes)
        .await?;

    let status = response.status().as_u16();
    let resp_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let resp_body_bytes = response.collect().await?.to_bytes();
    let resp_body_str = String::from_utf8_lossy(&resp_body_bytes).to_string();

    // Record response
    {
        let mut session = session.lock().await;
        session.transcript.push(TranscriptEntry::response(
            status,
            resp_headers.clone(),
            resp_body_str,
        ));
    }

    // Build client response
    let mut builder = Response::builder().status(status);
    for (name, value) in &resp_headers {
        if !is_hop_by_hop(name) {
            builder = builder.header(name.as_str(), value.as_str());
        }
    }

    Ok(builder.body(Full::new(resp_body_bytes)).unwrap())
}

fn parse_host(host: &str) -> (&str, u16) {
    match host.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().unwrap_or(443)),
        None => (host, 443),
    }
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let file = std::fs::File::open(path).with_context(|| format!("Failed to open {}", path))?;
    rustls_pemfile::certs(&mut std::io::BufReader::new(file))
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse certificates")
}

/// Load the TLS private key.
fn load_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    let file = std::fs::File::open(path).with_context(|| format!("Failed to open {}", path))?;
    let mut reader = std::io::BufReader::new(file);

    loop {
        match rustls_pemfile::read_one(&mut reader)? {
            Some(rustls_pemfile::Item::Pkcs1Key(key)) => return Ok(key.into()),
            Some(rustls_pemfile::Item::Pkcs8Key(key)) => return Ok(key.into()),
            Some(rustls_pemfile::Item::Sec1Key(key)) => return Ok(key.into()),
            None => anyhow::bail!("No private key found in {}", path),
            _ => continue,
        }
    }
}

/// Load the ECDSA secp256k1 signing key from a PEM file.
fn load_signing_key(path: &str) -> Result<SigningKey> {
    use k256::SecretKey;

    let pem = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read signing key from {}", path))?;

    // Try to parse as SEC1 PEM (EC PRIVATE KEY - from openssl ecparam)
    if let Ok(secret_key) = SecretKey::from_sec1_pem(&pem) {
        return SigningKey::from_bytes(&secret_key.to_bytes())
            .context("Failed to convert SEC1 key to signing key");
    }

    // Try to parse as PKCS8 PEM (PRIVATE KEY)
    use k256::pkcs8::DecodePrivateKey;
    if let Ok(secret_key) = SecretKey::from_pkcs8_pem(&pem) {
        return SigningKey::from_bytes(&secret_key.to_bytes())
            .context("Failed to convert PKCS8 key to signing key");
    }

    anyhow::bail!("Failed to parse signing key from {}. Expected SEC1 (EC PRIVATE KEY) or PKCS8 (PRIVATE KEY) PEM format.", path)
}

use crate::remote::attribution::config::Config;
use anyhow::{Context, Result};
use futures::{AsyncRead, AsyncWrite};
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use notary_client::{Accepted, NotarizationRequest, NotaryClient};
use p256::pkcs8::DecodePrivateKey;

use std::str;
use tlsn_core::SessionHeader;
use tlsn_prover::tls::state::Closed;
use tlsn_prover::tls::{Prover, ProverConfig, ProverControl, ProverError};
use tlsn_verifier::tls::{Verifier, VerifierConfig};
use tokio::task::JoinHandle;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::{debug, warn};

pub(super) async fn setup_connections(
    config: &Config,
) -> Result<(
    ProverControl,
    JoinHandle<Result<Prover<Closed>, ProverError>>,
    SendRequest<String>,
)> {
    let prover = if cfg!(feature = "dummy-notary") {
        println!("🚨 WARNING: Running in a test mode.");
        println!("🚨 WARNING: Authenticating output with a local dummy notary, which is not secure and should not be used in production.");
        let (prover_socket, notary_socket) = tokio::io::duplex(1 << 16);

        let connection_id = format!("{}_conversation", config.model_settings.id);

        // Start a local simple notary service
        tokio::spawn(run_dummy_notary(
            notary_socket.compat(),
            connection_id.clone(),
        ));

        // A Prover configuration
        let prover_config = ProverConfig::builder()
            .id(&connection_id)
            .server_dns(config.model_settings.api_settings.server_domain)
            .build()
            .context("Error building prover configuration")?;

        // Create a Prover and set it up with the Notary
        // This will set up the MPC backend prior to connecting to the server.
        Prover::new(prover_config)
            .setup(prover_socket.compat())
            .await
            .context("Error setting up prover")?
    } else {
        // Build a client to connect to the notary server.
        let notary_client = NotaryClient::builder()
            .host(config.notary_settings.host)
            .port(config.notary_settings.port)
            .path(config.notary_settings.path)
            .enable_tls(true)
            .build()
            .context("Error building notary client")?;

        // Send requests for configuration and notarization to the notary server.
        let notarization_request = NotarizationRequest::builder()
            .build()
            .context("Error building notarization request")?;

        let Accepted {
            io: notary_connection,
            id: session_id,
            ..
        } = notary_client
            .request_notarization(notarization_request)
            .await
            .context("Error requesting notarization")?;

        // Configure a new prover with the unique session id returned from notary client.
        let prover_config = ProverConfig::builder()
            .id(session_id)
            .server_dns(config.model_settings.api_settings.server_domain)
            .build()
            .context("Error building prover configuration")?;

        // Create a new prover and set up the MPC backend.
        Prover::new(prover_config)
            .setup(notary_connection.compat())
            .await
            .context("Error setting up prover")?
    };

    debug!("Prover setup complete!");
    // Open a new socket to the application server.
    let client_socket =
        tokio::net::TcpStream::connect((config.model_settings.api_settings.server_domain, 443))
            .await
            .context("Error connecting to server")?;

    // Bind the Prover to server connection
    let (tls_connection, prover_fut) = prover
        .connect(client_socket.compat())
        .await
        .context("Error connecting Prover to server")?;
    let tls_connection = TokioIo::new(tls_connection.compat());

    warn!("Test");
    // Grab a control handle to the Prover
    let prover_ctrl = prover_fut.control();

    warn!("Test 1");

    // Spawn the Prover to be run concurrently
    let prover_task = tokio::spawn(prover_fut);

    warn!("Test 2");

    // Attach the hyper HTTP client to the TLS connection
    let (request_sender, connection) = hyper::client::conn::http1::handshake(tls_connection)
        .await
        .context("Error establishing HTTP connection")?;

    warn!("Test 4");

    // Spawn the HTTP task to be run concurrently
    tokio::spawn(connection);

    warn!("Test 3");

    Ok((prover_ctrl, prover_task, request_sender))
}

/// Runs a simple Notary with the provided connection to the Prover.
pub async fn run_dummy_notary<T: AsyncWrite + AsyncRead + Send + Unpin + 'static>(
    conn: T,
    connection_id: String,
) -> Result<SessionHeader> {
    // Load the notary signing key
    let signing_key_str = str::from_utf8(include_bytes!("../../../tlsn/notary.key"))
        .context("Failed to read Notary key")?;
    let signing_key = p256::ecdsa::SigningKey::from_pkcs8_pem(signing_key_str)
        .context("Failed to parse Notary key")?;

    // Setup default config. Normally a different ID would be generated
    // for each notarization.
    let config = VerifierConfig::builder()
        .id(connection_id)
        .build()
        .context("Failed to build verifier config")?;

    Verifier::new(config)
        .notarize::<_, p256::ecdsa::Signature>(conn, &signing_key)
        .await
        .context("Error running dummy notary")
}

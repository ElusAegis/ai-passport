use crate::config::notary::{NotaryConfig, NotaryMode};
use anyhow::{Context, Error, Result};
use dialoguer::console::style;
use futures::{AsyncRead, AsyncWrite};
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use k256::{pkcs8::DecodePrivateKey, SecretKey};
use notary_client::{Accepted, NotarizationRequest, NotaryClient, NotaryConnection};
use tlsn_common::config::ProtocolConfig;
use tlsn_common::config::ProtocolConfigValidator;
use tlsn_core::attestation::AttestationConfig;
use tlsn_core::signing::SignatureAlgId;
use tlsn_core::CryptoProvider;
use tlsn_prover::state::Setup;
use tlsn_prover::{state, Prover, ProverConfig, ProverError};
use tlsn_verifier::{Verifier, VerifierConfig};
use tokio::task::JoinHandle;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::{debug, info};

pub async fn setup(
    nc: &NotaryConfig,
    domain: &str,
    port: u16,
) -> Result<(
    JoinHandle<Result<Prover<state::Committed>, ProverError>>,
    SendRequest<String>,
)> {
    // Set up protocol configuration for prover.
    let protocol_config = ProtocolConfig::builder()
        .max_sent_data(nc.max_total_sent)
        .max_recv_data(nc.max_total_recv)
        .max_recv_data_online(nc.max_decrypted_online)
        .defer_decryption_from_start(nc.defer_decryption)
        .network(nc.network_optimization)
        .build()
        .context("Error building protocol configuration")?;

    // Configure a new prover with the unique session id returned from notary client.
    let prover_config: ProverConfig = ProverConfig::builder()
        .server_name(domain)
        .protocol_config(protocol_config)
        .build()
        .context("Error building prover configuration")?;

    // Create a new prover and set up the MPC backend.
    let prover = init_prover(prover_config, nc)
        .await
        .context("Error setting up notary connection for the prover")?;

    debug!("Prover setup complete!");
    // Open a new socket to the application server.
    let client_socket = tokio::net::TcpStream::connect((domain, port))
        .await
        .context("Error connecting to server")?;

    // Bind the Prover to server connection
    let (tls_connection, prover_fut) = prover
        .connect(client_socket.compat())
        .await
        .context("Error connecting Prover to server")?;
    let tls_connection = TokioIo::new(tls_connection.compat());

    // Spawn the Prover to be run concurrently
    let prover_task = tokio::spawn(prover_fut);

    // Attach the hyper HTTP client to the TLS connection
    let (request_sender, connection) = hyper::client::conn::http1::handshake(tls_connection)
        .await
        .context("Error establishing HTTP connection")?;

    // Spawn the HTTP task to be run concurrently
    tokio::spawn(connection);

    Ok((prover_task, request_sender))
}

async fn init_prover(prover_config: ProverConfig, nc: &NotaryConfig) -> Result<Prover<Setup>> {
    let prover_init = Prover::new(prover_config);

    if matches!(nc.mode, NotaryMode::Ephemeral) {
        let prover_sock = setup_ephemeral_notary(nc)?;

        prover_init
            .setup(prover_sock)
            .await
            .context("setting up prover with ephemeral notary")
    } else {
        let prover_sock: NotaryConnection = setup_remote_notary(nc).await?;

        prover_init
            .setup(prover_sock.compat())
            .await
            .context("setting up prover with remote notary")
    }
}

/// Runs a simple Notary with the provided connection to the Prover.
fn setup_ephemeral_notary(
    nc: &NotaryConfig,
) -> Result<impl AsyncWrite + AsyncRead + Send + Unpin + 'static> {
    // Use an in‑process duplex pipe as the notary transport.
    let (prover_sock, notary_sock) = tokio::io::duplex(1 << 16);

    // Load the notary signing key
    let signing_key_str = include_str!("../../fixtures/ephemeral_notary.key");
    let signing_key = SecretKey::from_pkcs8_pem(signing_key_str)
        .context("Failed to parse Notary key")?
        .to_bytes();

    let mut provider = CryptoProvider::default();
    provider
        .signer
        .set_secp256k1(&signing_key)
        .context("Failed to set Notary key")?;

    // Setup the config. Normally a different ID would be generated
    // for each notarization.
    let config_validator = ProtocolConfigValidator::builder()
        .max_sent_data(nc.max_total_sent)
        .max_recv_data(nc.max_total_recv)
        .build()
        .context("Failed to build protocol config validator")?;

    let config = VerifierConfig::builder()
        .protocol_config_validator(config_validator)
        .crypto_provider(provider)
        .build()
        .context("Failed to build verifier config")?;

    let attestation_config = AttestationConfig::builder()
        .supported_signature_algs(vec![SignatureAlgId::SECP256K1])
        .build()
        .context("Failed to build attestation config")?;

    let verifier = Verifier::new(config);

    // Start a local dummy notary in the background.
    tokio::spawn(async move {
        #[allow(deprecated)]
        verifier
            .notarize(notary_sock.compat(), &attestation_config)
            .await
            .context("Failed to verify attestation")
    });

    Ok(prover_sock.compat())
}

async fn setup_remote_notary(nc: &NotaryConfig) -> Result<NotaryConnection> {
    let notary_client: NotaryClient = NotaryClient::builder()
        .host(&nc.domain)
        .port(nc.port)
        .path_prefix(&nc.path_prefix)
        .enable_tls(matches!(nc.mode, NotaryMode::RemoteTLS))
        .build()
        .context("Failed to build Notary client")?;

    let req = NotarizationRequest::builder()
        .max_sent_data(nc.max_total_sent)
        .max_recv_data(nc.max_total_recv)
        .build()
        .context("building notarization request")?;

    match notary_client
        .request_notarization(req)
        .await
        .context("requesting notarization")
    {
        Ok(Accepted { io, .. }) => Ok(io),
        Err(err) => handle_notary_setup_error(nc.max_total_sent, nc.max_total_recv, err),
    }
}

/// Helps the user understand why the notary setup failed and how to fix it.
/// We handle it so explicitly because the error can be very prominent
/// due to the likely chance of misconfiguration and exceeding the notary policy.
fn handle_notary_setup_error(
    total_sent: usize,
    total_recv: usize,
    err: Error,
) -> Result<NotaryConnection, Error> {
    info!(target: "plain",
        "{} {}",
        style("✖").red().bold(),
        style("Notary rejected the setup request").bold()
    );

    // Show both single-message caps and total channel caps (bytes).
    info!(target: "plain",
        "{}",
        style("   Current Configuration Requirements (bytes):").bold()
    );

    info!(target: "plain",
        "{}",
        style(format!(
            "   • Required channel sent max:   {total_sent} (bytes)"
        ))
        .dim()
    );
    info!(target: "plain",
        "{}",
        style(format!(
            "   • Required channel recv max:   {total_recv} (bytes)"
        ))
        .dim()
    );

    // Concise hint & fix
    info!(target: "plain", "{}", style("   Hint:").bold());
    info!(target: "plain",
        "{}",
        style(
            "   • Total limits can exceed the notary policy even if initial single-message caps look fine."
        )
        .dim()
    );

    info!(target: "plain", "{}", style("   How to fix:").bold());
    info!(target: "plain",
        "{}",
        style("   • Lower --max-single-request-size / --max-single-response-size or their respective env vars.")
            .dim()
    );
    info!(target: "plain", "{}", style("   • In single mode, total sent grows ~ O(n²); reduce n or increase totals within policy.").dim());
    info!(target: "plain",
        "{}",
        style("   • In multi mode, total recv grows ~ O(2n); reduce --max-req-num-sent or increase totals within policy.")
            .dim()
    );

    Err(err)
}

use crate::args::NotaryMode;
use crate::config::{NotarisationConfig, ProveConfig};
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

pub(super) async fn setup(
    config: &ProveConfig,
) -> Result<(
    JoinHandle<Result<Prover<state::Committed>, ProverError>>,
    SendRequest<String>,
)> {
    // Set up protocol configuration for prover.
    let protocol_config = build_protocol_config(&config.notarisation_config)
        .context("Error building protocol configuration")?;

    // Configure a new prover with the unique session id returned from notary client.
    let prover_config: ProverConfig = ProverConfig::builder()
        .server_name(config.model_config.domain.as_str())
        .protocol_config(protocol_config)
        .build()
        .context("Error building prover configuration")?;

    // Create a new prover and set up the MPC backend.
    let prover = init_prover(prover_config, &config.notarisation_config)
        .await
        .context("Error setting up notary connection for the prover")?;

    debug!("Prover setup complete!");
    // Open a new socket to the application server.
    let client_socket = tokio::net::TcpStream::connect((
        config.model_config.domain.as_str(),
        config.model_config.port,
    ))
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

pub async fn init_prover(
    prover_config: ProverConfig,
    config: &NotarisationConfig,
) -> Result<Prover<Setup>> {
    let prover_init = Prover::new(prover_config);

    if matches!(config.notary_config.mode, NotaryMode::Ephemeral) {
        let prover_sock = setup_ephemeral_notary(config)?;

        prover_init
            .setup(prover_sock)
            .await
            .context("setting up prover with ephemeral notary")
    } else {
        let prover_sock: NotaryConnection = setup_remote_notary(config).await?;

        prover_init
            .setup(prover_sock.compat())
            .await
            .context("setting up prover with remote notary")
    }
}

pub fn build_protocol_config(config: &NotarisationConfig) -> Result<ProtocolConfig> {
    let mut b = ProtocolConfig::builder();

    if matches!(config.mode, crate::args::SessionMode::MultiRound) {
        let (total_sent, total_recv) = get_total_sent_recv_max(config);

        let total_recv_online = total_recv; // TODO - we can optimize to rsp * (n - 1)

        b.defer_decryption_from_start(false)
            .max_recv_data_online(total_recv_online);
        b.max_sent_data(total_sent).max_recv_data(total_recv);
    } else {
        b.max_sent_data(config.max_single_request_size)
            .max_recv_data(config.max_single_response_size);
    }

    b.network(config.network_optimization)
        .build()
        .context("Error building protocol configuration")
}

pub fn get_total_sent_recv_max(config: &NotarisationConfig) -> (usize, usize) {
    let n = config.max_req_num_sent;

    let req = config.max_single_request_size;
    let rsp = config.max_single_response_size;

    if matches!(config.mode, crate::args::SessionMode::OneShot) {
        // --- One‑shot: exact, per‑round sizing --------------------------------
        //
        // We create a new protocol instance per request. We already know (or can
        // compute) precise sizes for this single request/response.
        // This is done before we invoke the setup.
        // This is the largest overhead given the number of requests
        // Note that only the last channel will have such size.
        (req * n + rsp * (n - 1), rsp)
    } else {
        // --- Multi‑round: stateless model API; sizes grow with history ----------
        //
        // Let:
        //   n   = max number of requests sent to the model API
        //   rsp = max_single_response_size (upper bound per response)
        //   req = max_single_request_size (upper bound per request)
        //
        // Because each new request re-sends prior context, cumulative *sent*
        // bytes across the session follow an arithmetic series that simplifies to:
        //
        //   total_sent_estimate = (req * (n - 1) * n + rsp * (n - 1) * (n - 2)) / 2

        let total_sent_max = ((req * (n + 1) * n) + rsp * (n - 1) * n) / 2;

        let total_recv_max = rsp * n;

        (total_sent_max, total_recv_max)
    }
}

/// Runs a simple Notary with the provided connection to the Prover.
pub fn setup_ephemeral_notary(
    notary_config: &NotarisationConfig,
) -> Result<impl AsyncWrite + AsyncRead + Send + Unpin + 'static> {
    // Use an in‑process duplex pipe as the notary transport.
    let (prover_sock, notary_sock) = tokio::io::duplex(1 << 16);

    // Load the notary signing key
    let signing_key_str = include_str!("../../tlsn/ephemeral_notary.key");
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
    let (total_sent, total_recv) = get_total_sent_recv_max(notary_config);

    let config_validator = ProtocolConfigValidator::builder()
        .max_sent_data(total_sent)
        .max_recv_data(total_recv)
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

async fn setup_remote_notary(config: &NotarisationConfig) -> Result<NotaryConnection> {
    let notary_config = &config.notary_config;

    let notary_client: NotaryClient = NotaryClient::builder()
        .host(&notary_config.domain)
        .port(notary_config.port)
        .path_prefix(&notary_config.path_prefix)
        .enable_tls(matches!(notary_config.mode, NotaryMode::RemoteTLS))
        .build()
        .context("Failed to build Notary client")?;

    // total channel caps (bytes) — computed from mode/rounds
    let (total_sent, total_recv) = get_total_sent_recv_max(config);

    let mut req_builder = NotarizationRequest::builder();

    let req = if matches!(config.mode, crate::args::SessionMode::MultiRound) {
        req_builder
            .max_sent_data(total_sent)
            .max_recv_data(total_recv)
    } else {
        req_builder
            .max_sent_data(config.max_single_request_size)
            .max_recv_data(config.max_single_response_size)
    }
    .build()
    .context("building notarization request")?;

    match notary_client
        .request_notarization(req)
        .await
        .context("requesting notarization")
    {
        Ok(Accepted { io, .. }) => Ok(io),
        Err(err) => handle_notary_setup_error(total_sent, total_recv, err),
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
    info!(target: "plain", "{}", style("   • In multi-round mode, total sent grows ~ O(n²); reduce n or increase totals within policy.").dim());
    info!(target: "plain",
        "{}",
        style("   • In one-shot mode, total recv grows ~ O(2n); reduce --max-req-num-sent or increase totals within policy.")
            .dim()
    );

    Err(err)
}

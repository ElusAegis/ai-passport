use crate::config::{NotaryConfig, ProveConfig};
use anyhow::{Context, Result};
#[cfg(feature = "ephemeral-notary")]
use futures::{AsyncRead, AsyncWrite};
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
#[cfg(feature = "ephemeral-notary")]
use k256::{pkcs8::DecodePrivateKey, SecretKey};
#[cfg(not(feature = "ephemeral-notary"))]
use notary_client::{Accepted, NotarizationRequest, NotaryClient, NotaryConnection};
use tlsn_common::config::ProtocolConfig;
#[cfg(feature = "ephemeral-notary")]
use tlsn_common::config::ProtocolConfigValidator;
#[cfg(feature = "ephemeral-notary")]
use tlsn_core::attestation::AttestationConfig;
#[cfg(feature = "ephemeral-notary")]
use tlsn_core::signing::SignatureAlgId;
#[cfg(feature = "ephemeral-notary")]
use tlsn_core::CryptoProvider;
use tlsn_prover::state::Setup;
use tlsn_prover::{state, Prover, ProverConfig, ProverError};
#[cfg(feature = "ephemeral-notary")]
use tlsn_verifier::{Verifier, VerifierConfig};
use tokio::task::JoinHandle;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::debug;

pub(super) async fn setup(
    config: &ProveConfig,
) -> Result<(
    JoinHandle<Result<Prover<state::Committed>, ProverError>>,
    SendRequest<String>,
)> {
    // Set up protocol configuration for prover.
    let protocol_config = build_protocol_config(&config.notary_config)
        .context("Error building protocol configuration")?;

    // Configure a new prover with the unique session id returned from notary client.
    let prover_config: ProverConfig = ProverConfig::builder()
        .server_name(config.model_config.domain.as_str())
        .protocol_config(protocol_config)
        .build()
        .context("Error building prover configuration")?;

    // Create a new prover and set up the MPC backend.
    let prover = init_prover(prover_config, &config.notary_config)
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
    config: &NotaryConfig,
) -> Result<Prover<Setup>> {
    let prover_init = Prover::new(prover_config);

    #[cfg(feature = "ephemeral-notary")]
    {
        let prover_sock = setup_ephemeral_notary(config)?;

        prover_init
            .setup(prover_sock)
            .await
            .context("setting up prover with ephemeral notary")
    }

    #[cfg(not(feature = "ephemeral-notary"))]
    {
        let prover_sock: NotaryConnection = setup_remote_notary(config).await?;

        prover_init
            .setup(prover_sock.compat())
            .await
            .context("setting up prover with remote notary")
    }
}

pub fn build_protocol_config(config: &NotaryConfig) -> Result<ProtocolConfig> {
    let mut b = ProtocolConfig::builder();

    let (total_sent, total_recv) = get_total_sent_recv_max(config);

    if !config.is_one_shot_mode {
        let n = config.max_req_num_sent;
        let rsp = config.max_single_response_size; // We need to prematurely decrypt all responses, but the last one
        let total_recv_online = rsp * (n - 1);

        b.defer_decryption_from_start(false)
            .max_recv_data_online(total_recv_online);
    }

    b.max_sent_data(total_sent)
        .max_recv_data(total_recv)
        .network(config.network_optimization)
        .build()
        .context("Error building protocol configuration")
}

pub fn get_total_sent_recv_max(config: &NotaryConfig) -> (usize, usize) {
    if config.is_one_shot_mode {
        // --- One‑shot: exact, per‑round sizing --------------------------------
        //
        // We create a new protocol instance per request. We already know (or can
        // compute) precise sizes for this single request/response.
        // This is done before we invoke the setup.
        (
            config.max_single_request_size,
            config.max_single_response_size,
        )
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
        let n = config.max_req_num_sent;
        let req = config.max_single_request_size;
        let rsp = config.max_single_response_size;

        let total_sent_max = ((req * (n - 1) * n) + rsp * (n - 1) * (n - 2)) / 2;

        let total_recv_max = rsp * n;

        (total_sent_max, total_recv_max)
    }
}

/// Runs a simple Notary with the provided connection to the Prover.
#[cfg(feature = "ephemeral-notary")]
pub fn setup_ephemeral_notary(
    notary_config: &NotaryConfig,
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

#[cfg(not(feature = "ephemeral-notary"))]
async fn setup_remote_notary(notary_config: &NotaryConfig) -> Result<NotaryConnection> {
    let notary_client: NotaryClient = build_notary_client().context("building notary client")?;

    let (total_sent, total_recv) = get_total_sent_recv_max(notary_config);

    let req = NotarizationRequest::builder()
        .max_sent_data(total_sent)
        .max_recv_data(total_recv)
        .build()
        .context("building notarization request")?;

    debug!("Requesting notarization…");

    let Accepted { io, .. } = notary_client
        .request_notarization(req)
        .await
        .context("requesting notarization")?;

    Ok(io)
}

/// Builds a `NotaryClient` configured for either a local or remote notary server,
/// depending on the `LOCAL_NOTARY` environment variable.
/// Connects to a local server without TLS if set, otherwise uses the remote notary with TLS.
/// Returns the configured `NotaryClient` or an error.
///
/// To run a local notary server, run `cargo run --release --bin notary-server`
/// in the `[tlsn](https://github.com/tlsnotary/tlsn)` repository.
#[cfg(not(feature = "ephemeral-notary"))]
fn build_notary_client() -> Result<NotaryClient> {
    let mut notary_builder = NotaryClient::builder();

    if std::env::var("LOCAL_NOTARY").is_ok() {
        notary_builder
            .host("localhost".to_string())
            .port(7047)
            .path_prefix("")
            .enable_tls(false)
    } else {
        notary_builder
            .host("notary.pse.dev")
            .port(443)
            .path_prefix("v0.1.0-alpha.12")
            .enable_tls(true)
    };

    notary_builder
        .build()
        .context("Failed to build NotaryClient")
}

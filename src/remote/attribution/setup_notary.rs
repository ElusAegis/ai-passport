use crate::remote::attribution::config::ApplicationConfig;
use anyhow::{Context, Result};
use futures::{AsyncRead, AsyncWrite};
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use k256::{pkcs8::DecodePrivateKey, SecretKey};
use notary_client::{Accepted, NotarizationRequest, NotaryClient};
use tlsn_common::config::{NetworkSetting, ProtocolConfig, ProtocolConfigValidator};
use tlsn_core::attestation::AttestationConfig;
use tlsn_core::signing::SignatureAlgId;
use tlsn_core::CryptoProvider;
use tlsn_prover::{state, Prover, ProverConfig, ProverError};
use tlsn_verifier::{Verifier, VerifierConfig};
use tokio::task::JoinHandle;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::compat::TokioAsyncReadCompatExt;
use tracing::debug;

// Maximum number of bytes that can be sent from prover to server
const MAX_SENT_DATA: usize = 1 << 12;
// Maximum number of bytes that can be received by prover from server
const MAX_RECV_DATA: usize = 1 << 14;

pub(super) async fn setup_connections(
    config: &ApplicationConfig,
) -> Result<(
    JoinHandle<Result<Prover<state::Committed>, ProverError>>,
    SendRequest<String>,
)> {
    let prover = if cfg!(feature = "dummy-notary") {
        println!("🚨 WARNING: Running in a test mode.");
        println!("🚨 WARNING: Authenticating output with a local dummy notary, which is not secure and should not be used in production.");
        let (prover_socket, notary_socket) = tokio::io::duplex(1 << 16);

        // Start a local simple notary service
        tokio::spawn(run_notary(notary_socket.compat()));

        // A Prover configuration
        let prover_config = ProverConfig::builder()
            .server_name(config.model_settings.api_settings.server_domain)
            .protocol_config(
                ProtocolConfig::builder()
                    // We must configure the amount of data we expect to exchange beforehand, which will
                    // be preprocessed prior to the connection. Reducing these limits will improve
                    // performance.
                    .max_sent_data(1024)
                    .max_recv_data(4096)
                    .build()
                    .context("Error building protocol configuration")?,
            )
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
        let notary_client: NotaryClient = NotaryClient::builder()
            .host(config.notary_settings.host)
            .port(config.notary_settings.port)
            .path_prefix(config.notary_settings.path)
            .enable_tls(config.notary_settings.enable_tls)
            .build()
            .context("Error building notary client")?;

        // Send requests for configuration and notarization to the notary server.
        let notarization_request: NotarizationRequest = NotarizationRequest::builder()
            .max_sent_data(MAX_SENT_DATA)
            .max_recv_data(MAX_RECV_DATA)
            .build()
            .context("Error building notarization request")?;

        debug!("Requesting notarization...");

        let Accepted {
            io: notary_connection,
            id: _,
            ..
        } = notary_client
            .request_notarization(notarization_request)
            .await?;

        debug!("Notary connection established!");

        // Set up protocol configuration for prover.
        let protocol_config: ProtocolConfig = ProtocolConfig::builder()
            // .max_sent_records(1) // TODO - make sure this is what we want
            .max_sent_data(MAX_SENT_DATA)
            // .max_recv_records_online(1) // TODO - make sure this is what we want
            // .network(
            //     NetworkSetting::Latency, // TODO - make sure this is what we want
            // )
            .max_recv_data(MAX_RECV_DATA)
            .build()
            .context("Error building protocol configuration")?;

        // Configure a new prover with the unique session id returned from notary client.
        let prover_config: ProverConfig = ProverConfig::builder()
            .server_name(config.model_settings.api_settings.server_domain)
            .protocol_config(protocol_config)
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

/// Runs a simple Notary with the provided connection to the Prover.
pub async fn run_notary<T: AsyncWrite + AsyncRead + Send + Unpin + 'static>(conn: T) -> Result<()> {
    // Load the notary signing key
    let signing_key_str = include_str!("../../../tlsn/notary.key");
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
        .max_sent_data(MAX_SENT_DATA)
        .max_recv_data(MAX_RECV_DATA)
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

    #[allow(deprecated)]
    Verifier::new(config)
        .notarize(conn, &attestation_config)
        .await
        .context("Failed to notarize")?;

    Ok(())
}

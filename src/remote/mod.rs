use futures::{AsyncRead, AsyncWrite};
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{CONNECTION, CONTENT_TYPE, HOST};
use hyper::{HeaderMap, Method, StatusCode};
use hyper_util::rt::TokioIo;
use notary_client::{Accepted, NotarizationRequest, NotaryClient};
use p256::pkcs8::DecodePrivateKey;
use serde_json::json;
use std::ops::Range;
use std::{env, str};
use tlsn_core::commitment::CommitmentId;
use tlsn_core::{proof::TlsProof, NotarizedSession};
use tlsn_prover::tls::state::Closed;
use tlsn_prover::tls::{Prover, ProverConfig, ProverControl, ProverError};
use tlsn_verifier::tls::{Verifier, VerifierConfig};
use tokio::io::AsyncWriteExt as _;
use tokio::task::JoinHandle;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{debug, info};

// Setting of the application server
const SERVER_DOMAIN: &str = "api.anthropic.com";
const ROUTE: &str = "/v1/messages";
const SETUP_PROMPT: &str = "Setup Prompt: YOU ARE GOING TO BE ACTING AS A HELPFUL ASSISTANT";
const REQUEST_TOPICS_TO_CENSOR: [&str; 1] = ["x-api-key"];
const RESPONSE_TOPICS_TO_CENSOR: [&str; 6] = [
    "anthropic-ratelimit-requests-reset",
    "anthropic-ratelimit-tokens-reset",
    "request-id",
    "x-cloud-trace-context",
    "cf-ray",
    "date",
];

const NOTARY_HOST: &str = "notary.pse.dev";
const NOTARY_PORT: u16 = 443;
const NOTARY_PATH: &str = "v0.1.0-alpha.6";

pub async fn generate_proof_of_conversation() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the logger
    tracing_subscriber::fmt::init();

    // Load secret variables from environment for Anthropic API connection, otherwise return an error
    dotenv::dotenv().ok();
    let api_key = env::var("ANTHROPIC_API_KEY").map_err(|_| {
        "Please set the ANTHROPIC_API_KEY environment variable to connect to the Anthropic API."
    })?;

    let (prover_ctrl, prover_task, mut request_sender) = setup_connections()
        .await
        .map_err(|e| format!("Error setting up connections: {}", e))?;

    let mut messages = vec![];

    let mut request_index = 1;

    let mut recv_private_data = vec![];
    let mut sent_private_data = vec![];

    // Print the rules on how to use the application
    info!("🌟 Welcome to the Anthropic Prover CLI! 🌟");
    info!(
        "This application will interact with the Anthropic API to generate a cryptographic proof of your conversation."
    );
    info!("💬 First, you will engage in a conversation with the assistant.");
    info!("The assistant will respond to your messages in real time.");
    info!("📝 When you're done, simply type 'exit' or press Enter without typing a message to end the conversation.");
    info!("🔒 Once finished, a proof of the conversation will be generated.");
    info!("💾 The proof will be saved as 'claude_conversation_proof.json' for your records.");
    info!("✨ Let's get started! Begin by sending your first message.");

    loop {
        let mut user_message = String::new();
        if request_index == 1 {
            user_message = SETUP_PROMPT.to_string();
            debug!("Sending setup prompt to Anthropic API: {}", user_message);
            // TODO - consider how to make it optional and not get a timeout error
        } else {
            info!("💬 Your message\n(type 'exit' to end): ");

            print!("> "); // Simple user prompt indicator like a terminal prompt.
            std::io::stdin().read_line(&mut user_message).unwrap();
            print!("");
        }

        if user_message.trim().is_empty() || user_message.trim() == "exit" {
            info!("🔒 Generating a cryptographic proof of the conversation. Please wait...");
            break;
        }

        let user_message = user_message.trim();
        let user_message = json!(
            {
                "role": "user",
                "content": user_message
            }
        );

        messages.push(user_message);

        // Prepare the Request to send to the Anthropic API
        let request = generate_request(&mut messages, &api_key)
            .map_err(|e| format!("Request {request_index} failed with error: {}", e))?;

        // Collect the sent private data
        extract_private_data(
            &mut sent_private_data,
            request.headers(),
            REQUEST_TOPICS_TO_CENSOR.as_slice(),
        );

        debug!("Request {request_index}: {:?}", request);

        debug!("Sending request {request_index} to Anthropic API...");

        let response = request_sender
            .send_request(request)
            .await
            .map_err(|e| format!("Request {request_index} failed with error: {}", e))?;

        debug!("Received response {request_index} from Anthropic");

        debug!("Raw response {request_index}: {:?}", response);

        if response.status() != StatusCode::OK {
            // TODO - do a graceful shutdown
            panic!(
                "Request {request_index} failed with status: {}",
                response.status()
            );
        }

        // Collect the received private data
        extract_private_data(
            &mut recv_private_data,
            response.headers(),
            RESPONSE_TOPICS_TO_CENSOR.as_slice(),
        );

        // Collect the body
        let payload = response.into_body().collect().await.unwrap().to_bytes();

        let parsed =
            serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&payload)).unwrap();

        // Pretty printing the response
        debug!(
            "Response {request_index}: {}",
            serde_json::to_string_pretty(&parsed).unwrap()
        );

        debug!("Request {request_index} to Anthropic succeeded");

        let received_assistant_message =
            json!({"role": "assistant", "content": parsed["content"][0]["text"]});
        messages.push(received_assistant_message);

        info!(
            "🤖 Assistant's response:\n\n{}",
            parsed["content"][0]["text"]
        );

        request_index += 1;
    }

    // Shutdown the connection by sending a final dummy request to the API
    shutdown_connection(prover_ctrl, &mut request_sender, &mut recv_private_data).await;

    // Notarize the session
    let (sent_commitment_ids, received_commitment_ids, notarized_session) =
        notirise_session(prover_task, &recv_private_data, &sent_private_data)
            .await
            .map_err(|e| format!("Error notarizing the session: {}", e))?;

    // Build the proof

    let proof = build_proof(
        sent_commitment_ids,
        received_commitment_ids,
        notarized_session,
    );

    // Dump the proof to a file.
    let mut file = tokio::fs::File::create("claud_conversation_proof.json")
        .await
        .unwrap();
    file.write_all(serde_json::to_string_pretty(&proof).unwrap().as_bytes())
        .await
        .unwrap();

    info!("✅ Proof successfully saved to `claude_conversation_proof.json`.");
    info!(
        "\n🔍 You can share this proof or inspect it at: https://explorer.tlsnotary.org/.\n\
        📂 Simply upload the proof, and anyone can verify its authenticity and inspect the details."
    );

    #[cfg(feature = "dummy-notary")]
    {
        let public_key = include_str!("../../tlsn/notary.pub");

        // Dummy notary is used for testing purposes only
        // It is not secure and should not be used in production
        info!("🚨 PUBLIC KEY: \n{}", public_key);
        info!("🚨 WARNING: Dummy notary is used for testing purposes only. It is not secure and should not be used in production.");
    }

    Ok(())
}

async fn shutdown_connection(
    prover_ctrl: ProverControl,
    request_sender: &mut SendRequest<String>,
    recv_private_data: &mut Vec<Vec<u8>>,
) {
    debug!(
        "Conversation ended, sending final request to Anthropic API to shut down the session..."
    );

    // Prepare final request to close the session
    let close_connection_request = hyper::Request::builder()
        .header(HOST, SERVER_DOMAIN)
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "close") // This will instruct the server to close the connection
        .body(String::new())
        .unwrap();

    debug!("Sending final request to Anthropic API...");

    // As this is the last request, we can defer decryption until the end.
    prover_ctrl.defer_decryption().await.unwrap();

    let response = request_sender
        .send_request(close_connection_request)
        .await
        .unwrap();

    // Collect the received private data
    extract_private_data(
        recv_private_data,
        response.headers(),
        RESPONSE_TOPICS_TO_CENSOR.as_slice(),
    );

    // Collect the body
    let payload = response.into_body().collect().await.unwrap().to_bytes();

    let parsed =
        serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&payload)).unwrap();

    // Pretty printing the response
    debug!(
        "Shutdown response (error response is expected ): {}",
        serde_json::to_string_pretty(&parsed).unwrap()
    );

    // Pretty printing the response
    debug!(
        "Shutdown response (error response is expected ): {}",
        serde_json::to_string_pretty(&parsed).unwrap()
    );
}

fn build_proof(
    sent_commitment_ids: Vec<CommitmentId>,
    received_commitment_ids: Vec<CommitmentId>,
    notarized_session: NotarizedSession,
) -> TlsProof {
    let session_proof = notarized_session.session_proof();

    let mut proof_builder = notarized_session.data().build_substrings_proof();

    for id in sent_commitment_ids {
        proof_builder.reveal_by_id(id).unwrap();
    }
    for id in received_commitment_ids {
        proof_builder.reveal_by_id(id).unwrap();
    }

    let substrings_proof = proof_builder.build().unwrap();

    TlsProof {
        session: session_proof,
        substrings: substrings_proof,
    }
}

async fn notirise_session(
    prover_task: JoinHandle<Result<Prover<Closed>, ProverError>>,
    recv_private_data: &[Vec<u8>],
    sent_private_data: &[Vec<u8>],
) -> Result<(Vec<CommitmentId>, Vec<CommitmentId>, NotarizedSession), String> {
    // The Prover task should be done now, so we can grab it.
    let prover = prover_task.await.unwrap().unwrap();

    // Prepare for notarization
    let mut prover = prover.start_notarize();

    // Notarize the session
    let (public_sent_commitment_ids, _) = find_ranges(
        prover.sent_transcript().data(),
        &sent_private_data
            .iter()
            .map(|v| v.as_slice())
            .collect::<Vec<&[u8]>>(),
    );

    let (public_received_commitment_ids, _) = find_ranges(
        prover.recv_transcript().data(),
        &recv_private_data
            .iter()
            .map(|v| v.as_slice())
            .collect::<Vec<&[u8]>>(),
    );

    let builder = prover.commitment_builder();

    let sent_commitment_ids = public_sent_commitment_ids
        .iter()
        .map(|range| builder.commit_sent(range).unwrap())
        .collect::<Vec<_>>();

    let recived_commitment_ids = public_received_commitment_ids
        .iter()
        .map(|range| builder.commit_recv(range).unwrap())
        .collect::<Vec<_>>();

    // Finalize, returning the notarized session
    let notarized_session = prover.finalize().await.map_err(|e| {
        format!(
            "Error finalizing not
            arization: {}",
            e
        )
    })?;

    debug!("Notarization complete!");

    Ok((
        sent_commitment_ids,
        recived_commitment_ids,
        notarized_session,
    ))
}

fn extract_private_data(
    recv_private_data: &mut Vec<Vec<u8>>,
    headers: &HeaderMap,
    topics_to_censor: &[&str],
) {
    for (header_name, header_value) in headers {
        if topics_to_censor.contains(&header_name.as_str()) {
            let header_value = header_value.as_bytes().to_vec();
            if !recv_private_data.contains(&header_value) {
                recv_private_data.push(header_value);
            }
        }
    }
}

fn find_ranges(seq: &[u8], sub_seq: &[&[u8]]) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut private_ranges = Vec::new();
    for s in sub_seq {
        for (idx, w) in seq.windows(s.len()).enumerate() {
            if w == *s {
                private_ranges.push(idx..(idx + w.len()));
            }
        }
    }

    let mut sorted_ranges = private_ranges.clone();
    sorted_ranges.sort_by_key(|r| r.start);

    let mut public_ranges = Vec::new();
    let mut last_end = 0;
    for r in sorted_ranges {
        if r.start > last_end {
            public_ranges.push(last_end..r.start);
        }
        last_end = r.end;
    }

    if last_end < seq.len() {
        public_ranges.push(last_end..seq.len());
    }

    (public_ranges, private_ranges)
}

async fn setup_connections() -> Result<
    (
        ProverControl,
        JoinHandle<Result<Prover<Closed>, ProverError>>,
        SendRequest<String>,
    ),
    String,
> {
    let prover = if cfg!(feature = "dummy-notary") {
        info!("🚨 WARNING: Dummy notary is used for testing purposes only. It is not secure and should not be used in production.");
        let (prover_socket, notary_socket) = tokio::io::duplex(1 << 16);

        // Start a local simple notary service
        tokio::spawn(run_dummy_notary(notary_socket.compat()));

        // A Prover configuration
        let config = ProverConfig::builder()
            .id("example")
            .server_dns(SERVER_DOMAIN)
            .build()
            .unwrap();

        // Create a Prover and set it up with the Notary
        // This will set up the MPC backend prior to connecting to the server.
        Prover::new(config)
            .setup(prover_socket.compat())
            .await
            .unwrap()
    } else {
        // Build a client to connect to the notary server.
        let notary_client = NotaryClient::builder()
            .host(NOTARY_HOST)
            .port(NOTARY_PORT)
            .path(NOTARY_PATH)
            .enable_tls(true)
            .build()
            .unwrap();

        // Send requests for configuration and notarization to the notary server.
        let notarization_request = NotarizationRequest::builder()
            .build()
            .map_err(|e| format!("Error creating notarization request: {}", e))?;

        let Accepted {
            io: notary_connection,
            id: session_id,
            ..
        } = notary_client
            .request_notarization(notarization_request)
            .await
            .map_err(|e| format!("Error requesting notarization: {}", e))?;

        // Configure a new prover with the unique session id returned from notary client.
        let prover_config = ProverConfig::builder()
            .id(session_id)
            .server_dns(SERVER_DOMAIN)
            .build()
            .map_err(|e| format!("Error creating prover configuration: {}", e))?;

        // Create a new prover and set up the MPC backend.
        Prover::new(prover_config)
            .setup(notary_connection.compat())
            .await
            .map_err(|e| format!("Error setting up prover: {}", e))?
    };

    debug!("Prover setup complete!");
    // Open a new socket to the application server.
    let client_socket = tokio::net::TcpStream::connect((SERVER_DOMAIN, 443))
        .await
        .map_err(|e| format!("Error establishing Socket connection to server: {}", e))?;

    // Bind the Prover to server connection
    let (tls_connection, prover_fut) = prover
        .connect(client_socket.compat())
        .await
        .map_err(|e| format!("Error establishing TLS connection to server: {}", e))?;
    let tls_connection = TokioIo::new(tls_connection.compat());

    // Grab a control handle to the Prover
    let prover_ctrl = prover_fut.control();

    // Spawn the Prover to be run concurrently
    let prover_task = tokio::spawn(prover_fut);

    // Attach the hyper HTTP client to the TLS connection
    let (request_sender, connection) = hyper::client::conn::http1::handshake(tls_connection)
        .await
        .map_err(|e| format!("Error during handshake connecting to server: {}", e))?;

    // Spawn the HTTP task to be run concurrently
    tokio::spawn(connection);
    Ok((prover_ctrl, prover_task, request_sender))
}

fn generate_request(
    messages: &mut Vec<serde_json::Value>,
    api_key: &str,
) -> Result<hyper::Request<String>, String> {
    let messages = serde_json::to_value(messages).unwrap();
    let mut json_body = serde_json::Map::new();
    json_body.insert("model".to_string(), json!("claude-3-5-sonnet-20240620"));
    json_body.insert("max_tokens".to_string(), json!(1024));
    json_body.insert("messages".to_string(), messages);
    let json_body = serde_json::Value::Object(json_body);

    // Build the HTTP request to send the prompt to Anthropic API
    hyper::Request::builder()
        .method(Method::POST)
        .uri(ROUTE)
        .header(HOST, SERVER_DOMAIN)
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "keep-alive")
        .header(CONTENT_TYPE, "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .body(json_body.to_string())
        .map_err(|e| format!("Error building request: {}", e))
}

/// Runs a simple Notary with the provided connection to the Prover.
pub async fn run_dummy_notary<T: AsyncWrite + AsyncRead + Send + Unpin + 'static>(conn: T) {
    // Load the notary signing key
    let signing_key_str = str::from_utf8(include_bytes!("../../tlsn/notary.key")).unwrap();
    let signing_key = p256::ecdsa::SigningKey::from_pkcs8_pem(signing_key_str).unwrap();

    // Setup default config. Normally a different ID would be generated
    // for each notarization.
    let config = VerifierConfig::builder().id("example").build().unwrap();

    Verifier::new(config)
        .notarize::<_, p256::ecdsa::Signature>(conn, &signing_key)
        .await
        .unwrap();
}

use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{CONNECTION, CONTENT_TYPE, HOST};
use hyper::{HeaderMap, Method, StatusCode};
use hyper_util::rt::TokioIo;
use notary_client::{Accepted, NotarizationRequest, NotaryClient};
use serde_json::json;
use std::ops::Range;
use std::{env, str};
use tlsn_core::commitment::CommitmentId;
use tlsn_core::{proof::TlsProof, NotarizedSession};
use tlsn_prover::tls::state::Closed;
use tlsn_prover::tls::{Prover, ProverConfig, ProverControl, ProverError};
use tokio::io::AsyncWriteExt as _;
use tokio::task::JoinHandle;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::debug;
use tracing::log::info;

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

// Setting of the notary server — make sure these are the same with the config in ../../../notary/server
const NOTARY_HOST: &str = "0.0.0.0";
const NOTARY_PORT: u16 = 7047;

#[tokio::main]
async fn main() {
    let (api_key, prover_ctrl, prover_task, mut request_sender) = setup_connections().await;

    let mut messages = vec![];

    let mut request_index = 1;

    let mut recv_private_data = vec![];
    let mut sent_private_data = vec![];

    loop {
        let mut user_message = String::new();
        if request_index == 1 {
            user_message = SETUP_PROMPT.to_string();
            debug!("Sending setup prompt to Antropic API: {}", user_message);
            // TODO - consider how to make it optional and not get a timeout error
        } else {
            // Prompt the user to provide a message to send to the assistant
            info!("Please provide a message to send to the assistant:");
            info!("(Type 'exit' or press Enter to exit the conversation)");
            std::io::stdin().read_line(&mut user_message).unwrap();
        }

        if user_message.trim().is_empty() || user_message.trim() == "exit" {
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

        // Prepare the Request to send to the Antropic API
        let request = generate_request(&mut messages, &api_key);

        // Collect the sent private data
        extract_private_data(
            &mut sent_private_data,
            request.headers(),
            REQUEST_TOPICS_TO_CENSOR.as_slice(),
        );

        debug!("Request {request_index}: {:?}", request);

        debug!("Sending request {request_index} to Antropic API...");

        let response = request_sender.send_request(request).await.unwrap();

        debug!("Received response {request_index} from Antropic");

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

        debug!("Request {request_index} to Antropic succeeded");

        let received_assistant_message =
            json!({"role": "assistant", "content": parsed["content"][0]["text"]});
        messages.push(received_assistant_message);

        request_index += 1;
    }

    // Shutdown the connection by sending a final dummy request to the API
    shutdown_connection(prover_ctrl, &mut request_sender, &mut recv_private_data).await;

    // Notarize the session
    let (sent_commitment_ids, received_commitment_ids, notarized_session) =
        notirise_session(prover_task, &mut recv_private_data, &mut sent_private_data).await;

    // Build the proof

    let proof = build_proof(
        sent_commitment_ids,
        received_commitment_ids,
        notarized_session,
    );

    // Dump the proof to a file.
    let mut file = tokio::fs::File::create("claud_response_proof.json")
        .await
        .unwrap();
    file.write_all(serde_json::to_string_pretty(&proof).unwrap().as_bytes())
        .await
        .unwrap();
}

async fn shutdown_connection(
    prover_ctrl: ProverControl,
    request_sender: &mut SendRequest<String>,
    mut recv_private_data: &mut Vec<Vec<u8>>,
) {
    debug!("Conversation ended, sending final request to Antropic API to shut down the session...");

    // Prepare final request to close the session
    let close_connection_request = hyper::Request::builder()
        .header(HOST, SERVER_DOMAIN)
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "close") // This will instruct the server to close the connection
        .body(String::new())
        .unwrap();

    debug!("Sending final request to Antropic API...");

    // As this is the last request, we can defer decryption until the end.
    prover_ctrl.defer_decryption().await.unwrap();

    let response = request_sender
        .send_request(close_connection_request)
        .await
        .unwrap();

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

    let proof = TlsProof {
        session: session_proof,
        substrings: substrings_proof,
    };
    proof
}

async fn notirise_session(
    prover_task: JoinHandle<Result<Prover<Closed>, ProverError>>,
    recv_private_data: &mut Vec<Vec<u8>>,
    sent_private_data: &mut Vec<Vec<u8>>,
) -> (Vec<CommitmentId>, Vec<CommitmentId>, NotarizedSession) {
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
    let notarized_session = prover.finalize().await.unwrap();

    debug!("Notarization complete!");

    (
        sent_commitment_ids,
        recived_commitment_ids,
        notarized_session,
    )
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

async fn setup_connections() -> (
    String,
    ProverControl,
    JoinHandle<Result<Prover<Closed>, ProverError>>,
    SendRequest<String>,
) {
    tracing_subscriber::fmt::init();

    // Load secret variables from environment for Antropic API connection
    dotenv::dotenv().ok();
    let api_key = env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");

    // Build a client to connect to the notary server.
    let notary_client = NotaryClient::builder()
        .host(NOTARY_HOST)
        .port(NOTARY_PORT)
        .enable_tls(false)
        .build()
        .unwrap();

    // Send requests for configuration and notarization to the notary server.
    let notarization_request = NotarizationRequest::builder().build().unwrap();

    let Accepted {
        io: notary_connection,
        id: session_id,
        ..
    } = notary_client
        .request_notarization(notarization_request)
        .await
        .unwrap();

    // Configure a new prover with the unique session id returned from notary client.
    let prover_config = ProverConfig::builder()
        .id(session_id)
        .server_dns(SERVER_DOMAIN)
        .build()
        .unwrap();

    // Create a new prover and set up the MPC backend.
    let prover = Prover::new(prover_config)
        .setup(notary_connection.compat())
        .await
        .unwrap();

    println!("Prover setup complete!");
    // Open a new socket to the application server.
    let client_socket = tokio::net::TcpStream::connect((SERVER_DOMAIN, 443))
        .await
        .unwrap();

    // Bind the Prover to server connection
    let (tls_connection, prover_fut) = prover.connect(client_socket.compat()).await.unwrap();
    let tls_connection = TokioIo::new(tls_connection.compat());

    // Grab a control handle to the Prover
    let prover_ctrl = prover_fut.control();

    // Spawn the Prover to be run concurrently
    let prover_task = tokio::spawn(prover_fut);

    // Attach the hyper HTTP client to the TLS connection
    let (request_sender, connection) = hyper::client::conn::http1::handshake(tls_connection)
        .await
        .unwrap();

    // Spawn the HTTP task to be run concurrently
    tokio::spawn(connection);
    (api_key, prover_ctrl, prover_task, request_sender)
}

fn generate_request(
    messages: &mut Vec<serde_json::Value>,
    api_key: &str,
) -> hyper::Request<String> {
    let messages = serde_json::to_value(messages).unwrap();
    let mut json_body = serde_json::Map::new();
    json_body.insert("model".to_string(), json!("claude-3-5-sonnet-20240620"));
    json_body.insert("max_tokens".to_string(), json!(1024));
    json_body.insert("messages".to_string(), messages);
    let json_body = serde_json::Value::Object(json_body);

    // Build the HTTP request to send the prompt to Antropic API
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
        .unwrap()
}

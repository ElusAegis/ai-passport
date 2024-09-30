use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{CONNECTION, CONTENT_TYPE, HOST};
use hyper::{Method, StatusCode};
use hyper_util::rt::TokioIo;
use notary_client::{Accepted, NotarizationRequest, NotaryClient};
use serde_json::json;
use spansy::http::{Request, Response};
use std::{env, str};
use tlsn_core::proof::{SessionProof, SubstringsProofBuilder};
use tlsn_core::{commitment::CommitmentKind, proof::TlsProof};
use tlsn_formats::http::NotarizedHttpSession;
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

// Setting of the notary server — make sure these are the same with the config in ../../../notary/server
const NOTARY_HOST: &str = "0.0.0.0";
const NOTARY_PORT: u16 = 7047;

#[tokio::main]
async fn main() {
    let (api_key, prover_ctrl, prover_task, mut request_sender) = setup_connections().await;


    let mut messages = vec![];

    let mut request_index = 1;


    loop {
        
        let mut user_message = String::new();
        if request_index == 1 {
            user_message = SETUP_PROMPT.to_string();
            debug!("Sending setup prompt to OpenAI API: {}", user_message);
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

        // Prepare the Request to send to the OpenAI API
        let request = generate_request(&mut messages, &api_key);

        debug!("Request {request_index}: {:?}", request);

        debug!("Sending request {request_index} to OpenAI API...");

        let response = request_sender.send_request(request).await.unwrap();

        debug!("Received response {request_index} from OpenAI");

        debug!("Raw response {request_index}: {:?}", response);

        if response.status() != StatusCode::OK {
            // TODO - do a graceful shutdown
            panic!("Request {request_index} failed with status: {}", response.status());
        }

        // Collect the body
        let payload = response.into_body().collect().await.unwrap().to_bytes();

        let parsed =
            serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&payload)).unwrap();

        // Pretty printing the response
        debug!("Response {request_index}: {}", serde_json::to_string_pretty(&parsed).unwrap());


        debug!("Request {request_index} to OpenAI succeeded");

        let received_assistant_message = json!({"role": "assistant", "content": parsed["content"][0]["text"]});
        messages.push(received_assistant_message);

        request_index += 1;
    }

    debug!("Conversation ended, sending final request to OpenAI API to shut down the session...");

    // Prepare final request to close the session
    let close_connection_request = hyper::Request::builder()
        .method(Method::POST)
        .uri(ROUTE) // The same endpoint you're working with
        .header(HOST, SERVER_DOMAIN)
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "close") // This will instruct the server to close the connection
        .header(CONTENT_TYPE, "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .body(String::new())
        .unwrap();

    debug!("Sending final request to OpenAI API...");

    // As this is the last request, we can defer decryption until the end.
    prover_ctrl.defer_decryption().await.unwrap();

    let shutdown_response = request_sender.send_request(close_connection_request).await.unwrap();

    // Collect the body
    let payload = shutdown_response.into_body().collect().await.unwrap().to_bytes();

    let parsed =
        serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&payload)).unwrap();

    // Pretty printing the response
    // TODO - do another shutdown request that doesn't return an error
    debug!("Shutdown response (expeted error): {}", serde_json::to_string_pretty(&parsed).unwrap());


    // Notarize the session
    let notarized_session = notarise_the_session(prover_task).await;
    debug!("Notarization complete!");


    let session_proof : SessionProof = notarized_session.session_proof();

    let mut proof_builder : SubstringsProofBuilder = notarized_session.session().data().build_substrings_proof();


    // For each request unless the last one, reveal the request and response substrings
    for i in 0..request_index - 1 {
        let request = &notarized_session.transcript().requests[i];
        let response = &notarized_session.transcript().responses[i];

        reveal_request(&mut proof_builder, request);
        reveal_response(&mut proof_builder, response);
    }


    // Build the proof
    let substrings_proof = proof_builder.build().unwrap();

    let proof = TlsProof {
        session: session_proof,
        substrings: substrings_proof,
    };

    // Dump the proof to a file.
    let mut file = tokio::fs::File::create("claud_response_proof.json")
        .await
        .unwrap();
    file.write_all(serde_json::to_string_pretty(&proof).unwrap().as_bytes())
        .await
        .unwrap();
}

fn reveal_response(proof_builder: &mut SubstringsProofBuilder, response: &Response) {
    proof_builder
        .reveal_recv(response, CommitmentKind::Blake3)
        .unwrap();

    // let disclosed_headers = vec!["via", "content-length", "anthropic-version", "content-type"];
    // let shown_headers = vec![];
    // 
    // for header in &response.headers {
    //     if disclosed_headers.contains(&header.name.as_str().to_ascii_lowercase().as_str()) {
    //         proof_builder
    //             .reveal_sent(header, CommitmentKind::Blake3)
    //             .unwrap();
    //     } else if shown_headers.contains(&header.name.as_str().to_ascii_lowercase().as_str()) {
    //         proof_builder
    //             .reveal_sent(&header.without_value(), CommitmentKind::Blake3)
    //             .unwrap();
    //     }
    // }
}

fn reveal_request(proof_builder: &mut SubstringsProofBuilder, request: &Request) {
    proof_builder
        .reveal_sent(request, CommitmentKind::Blake3)
        .unwrap();

    // let disclosed_headers = vec!["host", "content-length", "anthropic-version", "accept-encoding"];
    // let shown_headers = vec![""];
    //
    // for header in &request.headers {
    //     // Only reveal the Host and Authorization headers
    //     if disclosed_headers.contains(&header.name.as_str().to_ascii_lowercase().as_str()) {
    //         proof_builder
    //             .reveal_sent(header, CommitmentKind::Blake3)
    //             .unwrap();
    //     } else if shown_headers.contains(&header.name.as_str().to_ascii_lowercase().as_str()) {
    //         proof_builder
    //             .reveal_sent(&header.without_value(), CommitmentKind::Blake3)
    //             .unwrap();
    //     }
    // }
}

async fn notarise_the_session(prover_task: JoinHandle<Result<Prover<Closed>, ProverError>>) -> NotarizedHttpSession {
    // The Prover task should be done now, so we can grab it.
    let prover = prover_task.await.unwrap().unwrap();

    // Upgrade the prover to an HTTP prover, and start notarization.
    let mut prover = prover.to_http().unwrap().start_notarize();

    // Commit to the transcript with the default committer, which will commit using BLAKE3.
    prover.commit().unwrap();

    // Finalize, returning the notarized HTTP session
    let notarized_session = prover.finalize().await.unwrap();
    notarized_session
}

async fn setup_connections() -> (String, ProverControl, JoinHandle<Result<Prover<Closed>, ProverError>>, SendRequest<String>) {
    tracing_subscriber::fmt::init();

    // Load secret variables from environment for OpenAI API connection
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

fn generate_request(messages: &mut Vec<serde_json::Value>, api_key: &str) -> hyper::Request<String> {
    let messages = serde_json::to_value(messages).unwrap();
    let mut json_body = serde_json::Map::new();
    json_body.insert("model".to_string(), json!("claude-3-5-sonnet-20240620"));
    json_body.insert("max_tokens".to_string(), json!(1024));
    json_body.insert("messages".to_string(), messages);
    let json_body = serde_json::Value::Object(json_body);


    // Build the HTTP request to send the prompt to OpenAI API
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
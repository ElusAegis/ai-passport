//! Shared interaction logic for TLS-based provers.
//!
//! This module contains the core request/response handling that is shared
//! between [`TlsSingleShotProver`] and [`TlsPerMessageProver`].

use crate::config::ProveConfig;
use crate::providers::Provider;
use crate::ui::io_input::try_read_user_input_from_ctx;
use crate::ui::spinner::with_spinner_future;
use anyhow::{Context, Result};
use dialoguer::console::style;
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, HOST, TRANSFER_ENCODING};
use hyper::{Method, Request, StatusCode};
use serde_json::Value;
use tracing::{debug, info};

/// Execute a single interaction round (user input -> model response).
///
/// # Arguments
/// * `request_sender` - The HTTP sender connected to the model API
/// * `config` - Prove configuration (domain, API key, model ID)
/// * `messages` - Accumulated conversation messages (modified in place)
/// * `close_connection` - Whether to send `Connection: close` header
///
/// # Returns
/// * `Ok(true)` - Stop the interaction loop (user typed "exit" or empty input)
/// * `Ok(false)` - Continue the interaction loop
pub async fn single_interaction_round(
    request_sender: &mut SendRequest<String>,
    config: &ProveConfig,
    messages: &mut Vec<Value>,
    close_connection: bool,
) -> Result<bool> {
    // 1) Read user input
    let Some(user_input) = try_read_user_input_from_ctx().context("failed to read user input")?
    else {
        return Ok(true);
    };

    // 2) Add user message to history
    messages.push(serde_json::json!({
        "role": "user",
        "content": user_input
    }));

    // 3) Build and send request
    let request =
        generate_request(messages, config, close_connection).context("Error generating request")?;

    debug!("Request: {:?}", request);
    debug!("Sending request to Model's API...");

    let received_assistant_message: Value = with_spinner_future(
        "processing...",
        get_response(request_sender, request, config),
    )
    .await?;

    // 4) Display response
    let header = style("🤖 Assistant's response:").bold().magenta().dim();
    let content = received_assistant_message
        .get("content")
        .and_then(|v| v.as_str())
        .context("Failed to get assistant's message content")?;
    let body = style(content);
    info!(target: "plain", "\n{header}\n({}) {body}\n", config.model_id);

    // 5) Add assistant message to history
    messages.push(received_assistant_message);

    Ok(false)
}

/// Send request and parse response from the model API.
async fn get_response(
    request_sender: &mut SendRequest<String>,
    request: Request<String>,
    config: &ProveConfig,
) -> Result<Value> {
    let response = request_sender
        .send_request(request)
        .await
        .context("Request failed")?;

    debug!("Received response from Model: {:?}", response.status());

    if response.status() != StatusCode::OK {
        anyhow::bail!("Request failed with status: {}", response.status());
    }

    // Check for Transfer-Encoding header (TLSNotary doesn't support chunked)
    if let Some(te) = response.headers().get(TRANSFER_ENCODING) {
        if te
            .to_str()
            .is_ok_and(|te| te.eq_ignore_ascii_case("chunked"))
        {
            anyhow::bail!(
                "Server returned Transfer-Encoding: chunked which is not supported by TLSNotary. \
                 Ensure streaming is disabled in the request."
            );
        }
    }

    // Collect the response body
    let payload = response
        .into_body()
        .collect()
        .await
        .context("Error reading response body")?
        .to_bytes();

    let parsed: Value = serde_json::from_slice(&payload).context("Error parsing the response")?;

    debug!(
        "Response: {}",
        serde_json::to_string_pretty(&parsed).context("Error pretty printing the response")?
    );

    let content = config
        .provider
        .parse_chat_content(&parsed)
        .context("Failed to parse assistant content from response")?;

    let received_assistant_message = serde_json::json!({"role": "assistant", "content": content});
    Ok(received_assistant_message)
}

/// Build an HTTP request for the model API.
pub fn generate_request(
    messages: &[Value],
    config: &ProveConfig,
    close_connection: bool,
) -> Result<Request<String>> {
    debug!("Using provider: {:?}", config.provider);
    let json_body = config.provider.build_chat_body(&config.model_id, messages);
    debug!("Request body: {}", json_body);

    let chat_endpoint = config.provider.chat_endpoint();

    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(chat_endpoint)
        .header(HOST, config.provider.domain.as_str())
        .header(ACCEPT_ENCODING, "identity")
        .header(
            CONNECTION,
            if close_connection {
                "close"
            } else {
                "keep-alive"
            },
        )
        .header(CONTENT_TYPE, "application/json");

    for (name, value) in config.provider.chat_headers() {
        builder = builder.header(name, value);
    }

    builder
        .body(json_body.to_string())
        .context("Error building the request")
}

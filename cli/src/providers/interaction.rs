//! Shared interaction logic for TLS-based provers.
//!
//! This module contains the core request/response handling that is shared
//! between [`TlsSingleShotProver`] and [`TlsPerMessageProver`].

use crate::config::ProveConfig;
use crate::providers::budget::ByteBudget;
use crate::providers::Provider;
use crate::ui::io_input::try_read_user_input_with_budget;
use crate::ui::spinner::with_spinner_future;
use anyhow::{Context, Result};
use dialoguer::console::style;
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, HOST, TRANSFER_ENCODING};
use hyper::{Method, Request, StatusCode};
use serde_json::Value;
use tracing::{debug, info, trace};

/// Execute a single interaction round (user input -> model response).
///
/// # Arguments
/// * `request_sender` - The HTTP sender connected to the model API
/// * `config` - Prove configuration (domain, API key, model ID)
/// * `messages` - Accumulated conversation messages (modified in place)
/// * `close_connection` - Whether to send `Connection: close` header
/// * `budget` - Byte budget for tracking send/receive limits (includes shared overhead state)
///
/// # Returns
/// * `Ok(true)` - Stop the interaction loop (user typed "exit" or empty input)
/// * `Ok(false)` - Continue the interaction loop
pub async fn single_interaction_round(
    request_sender: &mut SendRequest<String>,
    config: &ProveConfig,
    messages: &mut Vec<Value>,
    close_connection: bool,
    budget: &mut ByteBudget,
) -> Result<bool> {
    // 1) Read user input (with budget info displayed)
    let Some(user_input) =
        try_read_user_input_with_budget(budget).context("failed to read user input")?
    else {
        return Ok(true);
    };

    // 2) Add user message to history
    messages.push(serde_json::json!({
        "role": "user",
        "content": user_input
    }));

    // 3) Calculate max_tokens from remaining receive budget
    let max_tokens = budget.max_tokens_for_response();
    debug!("budget: max_tokens for response = {:?}", max_tokens);

    // 4) Build request with budget-aware max_tokens
    let request = generate_request_with_limit(messages, config, close_connection, max_tokens)
        .context("Error generating request")?;

    // 5) Check send budget before sending (using actual total size)
    let request_total_len = budget.check_send(&request)?;
    let user_input_len = user_input.len();

    trace!("Request: {:?}", request);
    trace!("Sending request to Model's API...");

    // 6) Send request and get response
    let (received_assistant_message, response_total_len): (Value, usize) = with_spinner_future(
        "processing...",
        get_response_with_sizes(request_sender, request, config),
    )
    .await?;

    // 7) Display response and get content length
    let header = style("🤖 Assistant's response:").bold().magenta().dim();
    let content = received_assistant_message
        .get("content")
        .and_then(|v| v.as_str())
        .context("Failed to get assistant's message content")?;
    let content_len = content.len();

    let body = style(content);
    info!(target: "plain", "\n{header}\n({}) {body}\n", config.model_id);

    // 8) Record sent bytes with content size (updates overhead tracking)
    budget.record_sent(request_total_len, user_input_len);

    // 9) Record received bytes with content size (updates overhead tracking)
    budget.record_recv(response_total_len, content_len);

    // Debug: log overhead ratios
    debug!(
        "overhead ↑: total={} content={} (ratio={:.1}x)",
        request_total_len,
        user_input_len,
        request_total_len as f64 / user_input_len.max(1) as f64
    );
    debug!(
        "overhead ↓: total={} content={} (ratio={:.1}x)",
        response_total_len,
        content_len,
        response_total_len as f64 / content_len.max(1) as f64
    );

    // 10) Add assistant message to history
    messages.push(received_assistant_message);

    Ok(false)
}

/// Send request and parse response from the model API.
/// Returns (parsed_message, total_bytes).
async fn get_response_with_sizes(
    request_sender: &mut SendRequest<String>,
    request: Request<String>,
    config: &ProveConfig,
) -> Result<(Value, usize)> {
    let response = request_sender
        .send_request(request)
        .await
        .context("Request failed")?;

    trace!("Received response from Model: {:?}", response.status());

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

    let headers = response.headers().clone();

    // Collect the response body
    let payload = response
        .into_body()
        .collect()
        .await
        .context("Error reading response body")?
        .to_bytes();

    let total_len = ByteBudget::calculate_response_size(&headers, &payload);

    let parsed: Value = serde_json::from_slice(&payload).context("Error parsing the response")?;

    trace!(
        "Response: {}",
        serde_json::to_string_pretty(&parsed).context("Error pretty printing the response")?
    );

    let content = config
        .provider
        .parse_chat_content(&parsed)
        .context("Failed to parse assistant content from response")?;

    let received_assistant_message = serde_json::json!({"role": "assistant", "content": content});
    Ok((received_assistant_message, total_len))
}

/// Build an HTTP request for the model API with optional max_tokens limit.
fn generate_request_with_limit(
    messages: &[Value],
    config: &ProveConfig,
    close_connection: bool,
    max_tokens: Option<u32>,
) -> Result<Request<String>> {
    let json_body =
        config
            .provider
            .build_chat_body_with_limit(&config.model_id, messages, max_tokens);

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

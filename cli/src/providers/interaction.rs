//! Shared interaction logic for TLS-based provers.
//!
//! This module contains the core request/response handling that is shared
//! between [`TlsSingleShotProver`] and [`TlsPerMessageProver`].

use crate::config::ProveConfig;
use crate::providers::budget::ChannelBudget;
use crate::providers::message::ChatMessage;
use crate::providers::Provider;
use crate::ui::io_input::get_new_user_message;
use crate::ui::spinner::with_spinner_future;
use anyhow::{Context, Result};
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, HOST, TRANSFER_ENCODING};
use hyper::{Method, Request, StatusCode};
use serde_json::Value;
use tracing::{debug, trace};

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
    messages: &mut Vec<ChatMessage>,
    close_connection: bool,
    budget: &mut ChannelBudget,
) -> Result<bool> {
    // 1) Read user input (with budget info displayed)
    let Some(user_message) =
        get_new_user_message(budget, config, messages).context("failed to read user input")?
    else {
        return Ok(true);
    };
    let user_message_len = user_message.content().len();

    // 2) Add user message to history
    messages.push(user_message);

    // 4) Build request with budget-aware max_tokens
    let (request, request_total_len) =
        generate_request_with_limit(messages, config, close_connection, budget)
            .context("Error generating request")?;

    trace!("Request: {:?}", request);
    trace!("Sending request to Model's API...");

    // 6) Send request and get response
    let (received_assistant_message, response_total_len): (ChatMessage, usize) =
        with_spinner_future(
            "processing...",
            get_response_with_sizes(request_sender, request, config),
        )
        .await?;
    let assistant_message_len = received_assistant_message.content().len();

    // 8) Record sent and received bytes with content size (updates overhead tracking)
    budget.record_sent(request_total_len, user_message_len);
    budget.record_recv(response_total_len, assistant_message_len);

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
) -> Result<(ChatMessage, usize)> {
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

    let total_len = ChannelBudget::calculate_response_size(&headers, &payload);

    let parsed: Value = serde_json::from_slice(&payload).context("Error parsing the response")?;

    trace!(
        "Response: {}",
        serde_json::to_string_pretty(&parsed).context("Error pretty printing the response")?
    );

    let received_assistant_message = config
        .provider
        .parse_chat_reply_message(&parsed)
        .context("Failed to parse assistant content from response")?;

    Ok((received_assistant_message, total_len))
}

/// Build an HTTP request for the model API with optional max_tokens limit.
fn generate_request_with_limit(
    messages: &[ChatMessage],
    config: &ProveConfig,
    close_connection: bool,
    budget: &ChannelBudget,
) -> Result<(Request<String>, usize)> {
    // Calculate max_tokens from remaining receive budget
    let max_tokens = if let Some(config_max) = config.max_response_tokens {
        Some(config_max)
    } else {
        budget.max_tokens_for_response()
    };

    debug!("budget: max_tokens for response = {:?}", max_tokens);

    let json_body = config
        .provider
        .build_chat_body(&config.model_id, messages, max_tokens);

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

    let request = builder
        .body(json_body.to_string())
        .context("Error building the request")?;

    // Get total length of the request and check against budget
    let total_len = ChannelBudget::calculate_request_size(&request);
    budget
        .check_request_fits(total_len)
        .context("Request exceeds available budget")?;

    Ok((request, total_len))
}

use crate::config::{ModelConfig, ProveConfig, SessionMode};
use crate::providers::Provider;
use crate::utils::io_input::try_read_user_input_from_ctx;
use crate::utils::spinner::with_spinner_future;
use anyhow::Context;
use anyhow::Result;
use dialoguer::console::style;
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{ACCEPT_ENCODING, CONNECTION, CONTENT_TYPE, HOST, TRANSFER_ENCODING};
use hyper::{Method, Request, StatusCode};
use serde_json::Value;
use tracing::{debug, info};

/// Return value convention:
/// - Ok(true)  => stop interaction loop
/// - Ok(false) => continue interaction loop
pub(super) async fn single_interaction_round(
    request_sender: &mut SendRequest<String>,
    config: &ProveConfig,
    messages: &mut Vec<Value>,
) -> Result<bool> {
    // ---- 1) Read user input -------------------------------------------------
    // Prefer using context; fall back to stdin if absent.
    let maybe_line: Option<String> =
        try_read_user_input_from_ctx().context("failed to read user input")??;

    // exit if empty or "exit" (case-insensitive)
    let Some(user_input) = maybe_line.filter(|s| !s.trim().eq_ignore_ascii_case("exit")) else {
        return Ok(true);
    };

    // ---- 3) Normal request path ---------------------------------------------
    messages.push(serde_json::json!({
        "role": "user",
        "content": user_input
    }));

    let request = generate_request(
        messages,
        &config.model,
        matches!(config.session.mode, SessionMode::Multi),
    )
    .context("Error generating request")?;

    debug!("Request: {:?}", request);
    debug!("Sending request to Model's API...");

    let received_assistant_message: Value = with_spinner_future(
        "processing...",
        get_response(request_sender, request, &config.model),
    )
    .await?;

    let header = style("🤖 Assistant's response:").bold().magenta().dim();
    let content = received_assistant_message
        .get("content")
        .and_then(|v| v.as_str())
        .context("Failed to get assistant's message content")?;
    let body = style(content);
    info!(target: "plain", "\n{header}\n({}) {body}\n", config.model.model_id);

    messages.push(received_assistant_message);

    // Tell caller to continue the loop.
    Ok(false)
}

async fn get_response(
    request_sender: &mut SendRequest<String>,
    request: Request<String>,
    model_settings: &ModelConfig,
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

    // Collect the body (only on normal path)
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

    let provider = model_settings.server.provider();
    let content = provider
        .parse_chat_content(&parsed)
        .context("Failed to parse assistant content from response")?;

    let received_assistant_message = serde_json::json!({"role": "assistant", "content": content});
    Ok(received_assistant_message)
}

pub(crate) fn generate_request(
    messages: &[Value],
    model_settings: &ModelConfig,
    close_connection: bool,
) -> Result<Request<String>> {
    let provider = model_settings.server.provider();
    debug!("Using provider: {:?}", provider);
    let json_body = provider.build_chat_body(&model_settings.model_id, messages);
    debug!("Request body: {}", json_body);

    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(model_settings.inference_route.as_str())
        .header(HOST, model_settings.server.domain.as_str())
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

    for (name, value) in provider.chat_headers(&model_settings.api_key) {
        builder = builder.header(name, value);
    }

    builder
        .body(json_body.to_string())
        .context("Error building the request")
}

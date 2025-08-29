use crate::config::{ModelConfig, ProveConfig, SessionMode};
use crate::utils::io_input::try_read_user_input_from_ctx;
use crate::utils::spinner::with_spinner_future;
use anyhow::Context;
use anyhow::Result;
use dialoguer::console::style;
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{
    ACCEPT_ENCODING, AUTHORIZATION, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST,
};
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
        if matches!(config.session.mode, SessionMode::Single) {
            send_connection_close(request_sender, &config.model)
                .await
                .context("failed to send close request")?;
        }
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

    let received_assistant_message: Value =
        with_spinner_future("processing...", get_response(request_sender, request)).await?;

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
) -> Result<Value> {
    let response = request_sender
        .send_request(request)
        .await
        .context("Request failed")?;

    debug!("Received response from Model: {:?}", response.status());

    if response.status() != StatusCode::OK {
        anyhow::bail!("Request failed with status: {}", response.status());
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

    let received_assistant_message = serde_json::json!({"role": "assistant", "content": parsed["choices"][0]["message"]["content"]});
    Ok(received_assistant_message)
}

/// Build and send a minimal empty request that politely asks the server
/// to close the HTTP/1.1 connection after the response.
/// We do NOT read the body; we just send and return.
pub(crate) async fn send_connection_close(
    request_sender: &mut SendRequest<String>,
    model_settings: &ModelConfig,
) -> Result<()> {
    let req = Request::builder()
        .method(Method::GET) // or HEAD if your endpoint allows it
        .uri(model_settings.inference_route.as_str())
        .header(HOST, model_settings.server.domain.as_str())
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "close")
        .header(CONTENT_LENGTH, "0")
        .header(AUTHORIZATION, format!("Bearer {}", model_settings.api_key))
        .body(String::new())
        .context("build close request")?;

    // Send the request and discard the response without reading the body.
    // We await the response head to ensure the request is actually written.
    let _ = request_sender.send_request(req).await;

    Ok(())
}

pub(crate) fn generate_request(
    messages: &Vec<Value>,
    model_settings: &ModelConfig,
    close_connection: bool,
) -> Result<Request<String>> {
    let messages_val = serde_json::to_value(messages).context("Error serializing messages")?;

    let mut json_body = serde_json::Map::new();
    json_body.insert(
        "model".to_string(),
        serde_json::json!(model_settings.model_id),
    );
    json_body.insert("messages".to_string(), messages_val);
    let json_body = Value::Object(json_body);

    Request::builder()
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
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", model_settings.api_key))
        .body(json_body.to_string())
        .context("Error building the request")
}

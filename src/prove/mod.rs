mod setup;
mod tlsn_operations;

use crate::config::{ModelConfig, ProveConfig};
use crate::prove::setup::setup;
use crate::prove::tlsn_operations::notarise_session;
use crate::utils::spinner::with_spinner_future;
use anyhow::{Context, Result};
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{AUTHORIZATION, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST};
use hyper::{Method, Request, StatusCode};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::str;
use tracing::debug;

pub(crate) async fn run_prove(app_config: &ProveConfig) -> Result<()> {
    let (prover_task, mut request_sender) =
        with_spinner_future("Please wait while the system is setup", setup(app_config)).await?;

    println!(
        "💬 Now, you can engage in a conversation with the `{}` model.",
        app_config.model_config.model_id
    );
    println!("The assistant will respond to your messages in real time.");
    println!("📝 When you're done, simply type 'exit' or press `Enter` without typing a message to end the conversation.");

    println!("🔒 Once finished, a proof of the conversation will be generated and saved for your records.");

    println!("✨ Let's get started! Once the setup is complete, you can begin the conversation.\n");

    let mut messages = vec![];

    let mut recv_private_data = vec![];
    let mut sent_private_data = vec![];

    model_interaction(
        app_config,
        &mut request_sender,
        &mut messages,
        &mut recv_private_data,
        &mut sent_private_data,
    )
    .await?;

    println!("🔒 Generating a cryptographic proof of the conversation. Please wait...");

    // Notarize the session
    debug!("Notarizing the session...");
    let (notarised_session, _) =
        notarise_session(prover_task.await??, &recv_private_data, &sent_private_data)
            .await
            .context("Error notarizing the session")?;

    // Save the proof to a file
    let file_path = save_proof_to_file(&notarised_session, &app_config.model_config.model_id)?;

    println!("✅ Proof successfully saved to `{}`.", file_path.display());
    println!(
            "\n🔍 You can share this proof or inspect it at: https://explorer.tlsnotary.org/.\n\
        📂 Simply upload the proof, and anyone can verify its authenticity and inspect the details."
        );

    #[cfg(feature = "ephemeral-notary")]
    {
        let public_key = include_str!("../../tlsn/ephemeral_notary.pub");

        // Dummy notary is used for testing purposes only
        // It is not secure and should not be used in production
        println!("🚨 PUBLIC KEY: \n{}", public_key);
        println!("🚨 WARNING: Dummy notary is used for testing purposes only. It is not secure and should not be used in production.");
    }

    Ok(())
}

async fn model_interaction(
    app_config: &ProveConfig,
    mut request_sender: &mut SendRequest<String>,
    mut messages: &mut Vec<Value>,
    mut recv_private_data: &mut Vec<Vec<u8>>,
    mut sent_private_data: &mut Vec<Vec<u8>>,
) -> Result<()> {
    loop {
        let stop = single_interaction_round(
            &mut request_sender,
            app_config,
            &mut messages,
            &mut recv_private_data,
            &mut sent_private_data,
        )
        .await?;

        if stop {
            break;
        }
    }
    Ok(())
}

/// Return value convention:
/// - Ok(true)  => stop interaction loop
/// - Ok(false) => continue interaction loop
async fn single_interaction_round(
    request_sender: &mut SendRequest<String>,
    config: &ProveConfig,
    messages: &mut Vec<serde_json::Value>,
    _recv_private_data: &mut Vec<Vec<u8>>,
    _sent_private_data: &mut Vec<Vec<u8>>,
) -> Result<bool> {
    // ---- 1) Read user input -------------------------------------------------
    println!("\n💬 Your message\n(type 'exit' to end): ");
    print!("> ");
    std::io::stdout()
        .flush()
        .context("Failed to flush stdout")?;

    let mut user_input = String::new();
    std::io::stdin()
        .read_line(&mut user_input)
        .context("Failed to read user input to the model")?;
    let user_input = user_input.trim();

    // ---- 2) Exit path: send lean close-request and stop ---------------------
    if user_input.is_empty() || user_input.eq_ignore_ascii_case("exit") {
        send_connection_close(request_sender, &config.model_config)
            .await
            .context("failed to send close request")?;

        // We’re done: tell the caller to stop the loop.
        return Ok(true);
    }

    println!("processing...");

    // ---- 3) Normal request path ---------------------------------------------
    messages.push(serde_json::json!({
        "role": "user",
        "content": user_input
    }));

    let request =
        generate_request(messages, &config.model_config).context("Error generating request")?;

    debug!("Request: {:?}", request);
    debug!("Sending request to Model's API...");

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

    let parsed: serde_json::Value =
        serde_json::from_slice(&payload).context("Error parsing the response")?;

    debug!(
        "Response: {}",
        serde_json::to_string_pretty(&parsed).context("Error pretty printing the response")?
    );

    let received_assistant_message = serde_json::json!({"role": "assistant", "content": parsed["choices"][0]["message"]["content"]});
    messages.push(received_assistant_message);

    println!(
        "\n🤖 Assistant's response:\n\n{}\n",
        parsed["choices"][0]["message"]["content"]
    );

    // Tell caller to continue the loop.
    Ok(false)
}

/// Build and send a minimal empty request that politely asks the server
/// to close the HTTP/1.1 connection after the response.
/// We do NOT read the body; we just send and return.
async fn send_connection_close(
    request_sender: &mut SendRequest<String>,
    model_settings: &ModelConfig,
) -> Result<()> {
    let req = Request::builder()
        .method(Method::GET) // or HEAD if your endpoint allows it
        .uri(model_settings.inference_route.as_str())
        .header(HOST, model_settings.domain.as_str())
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "close")
        .header(CONTENT_LENGTH, "0")
        .header(AUTHORIZATION, format!("Bearer {}", model_settings.api_key))
        .body(String::new())
        .context("build close request")?;

    // Send the request and discard the response without reading the body.
    // We await the response head to ensure the request is actually written.
    let _ = request_sender.send_request(req).await?;
    Ok(())
}

fn generate_request(
    messages: &mut Vec<serde_json::Value>,
    model_settings: &ModelConfig,
) -> Result<hyper::Request<String>> {
    let messages_val = serde_json::to_value(messages).context("Error serializing messages")?;

    let mut json_body = serde_json::Map::new();
    json_body.insert(
        "model".to_string(),
        serde_json::json!(model_settings.model_id),
    );
    json_body.insert("messages".to_string(), messages_val);
    let json_body = serde_json::Value::Object(json_body);

    Request::builder()
        .method(Method::POST)
        .uri(model_settings.inference_route.as_str())
        .header(HOST, model_settings.domain.as_str())
        .header("Accept-Encoding", "identity")
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", model_settings.api_key))
        .body(json_body.to_string())
        .context("Error building the request")
}

pub fn save_proof_to_file<T: Serialize>(proof: &T, model_id: &str) -> Result<PathBuf> {
    // Generate timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    // Create file path
    let sanitised_model_id = model_id.replace(" ", "_").replace("/", "_");
    let file_path = format!(
        "{}_{}_conversation_proof.json",
        sanitised_model_id, timestamp
    );
    let path_buf = PathBuf::from(&file_path);

    // Create and write to file
    let mut file = File::create(&path_buf).context("Failed to create proof file")?;

    let proof_content = serde_json::to_string_pretty(proof).context("Failed to serialize proof")?;

    file.write_all(proof_content.as_bytes())
        .context("Failed to write proof to file")?;

    Ok(path_buf)
}

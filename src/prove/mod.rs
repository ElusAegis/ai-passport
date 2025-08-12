mod setup_notary;
mod tlsn_operations;

use crate::config::{ModelConfig, ProveConfig};
use crate::prove::setup_notary::setup_connections;
use crate::prove::tlsn_operations::notarise_session;
use anyhow::{Context, Result};
use hyper::client::conn::http1::SendRequest;
use hyper::header::{AUTHORIZATION, CONNECTION, CONTENT_TYPE, HOST};
use hyper::{Method, StatusCode};
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::str;
use tracing::debug;

pub(crate) async fn run_prove(app_config: &ProveConfig) -> Result<()> {
    println!("⏱️ Please wait while the system is setup...");

    let (prover_task, mut request_sender) = setup_connections(app_config).await?;

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

    single_interaction_round(
        &mut request_sender,
        &app_config,
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

    #[cfg(feature = "dummy-notary")]
    {
        let public_key = include_str!("../../tlsn/notary.pub");

        // Dummy notary is used for testing purposes only
        // It is not secure and should not be used in production
        println!("🚨 PUBLIC KEY: \n{}", public_key);
        println!("🚨 WARNING: Dummy notary is used for testing purposes only. It is not secure and should not be used in production.");
    }

    Ok(())
}

async fn single_interaction_round(
    request_sender: &mut SendRequest<String>,
    config: &ProveConfig,
    messages: &mut Vec<serde_json::Value>,
    _recv_private_data: &mut Vec<Vec<u8>>,
    _sent_private_data: &mut Vec<Vec<u8>>,
) -> Result<()> {
    let mut user_message = String::new();

    println!("\n💬 Your message\n(type 'exit' to end): ");

    print!("> ");
    std::io::stdout()
        .flush()
        .context("Failed to flush stdout")?;

    std::io::stdin()
        .read_line(&mut user_message)
        .context("Failed to read user input to the model")?;

    println!("processing...");

    let user_message = user_message.trim();
    let user_message = serde_json::json!(
        {
            "role": "user",
            "content": user_message
        }
    );

    messages.push(user_message);

    // Prepare the Request to send to the model's API
    let request =
        generate_request(messages, &config.model_config).context("Error generating request")?;

    debug!("Request: {:?}", request);

    debug!("Sending request to Model's API...");

    let response = request_sender
        .send_request(request)
        .await
        .context("Request failed")?;

    debug!("Received response from Model");

    debug!("Raw response: {:?}", response);

    if response.status() != StatusCode::OK {
        panic!("Request failed with status: {}", response.status());
    }
    //
    // // Collect the body
    // let payload = response
    //     .into_body()
    //     .collect()
    //     .await
    //     .context("Error reading response body")?
    //     .to_bytes();
    //
    // let parsed = serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&payload))
    //     .context("Error parsing the response")?;
    //
    // // Pretty printing the response
    // debug!(
    //     "Response: {}",
    //     serde_json::to_string_pretty(&parsed).context("Error pretty printing the response")?
    // );
    //
    // debug!("Request to Model succeeded");
    //
    // let received_assistant_message = serde_json::json!({"role": "assistant", "content": parsed["choices"][0]["message"]["content"]});
    // messages.push(received_assistant_message);
    //
    // println!(
    //     "\n🤖 Assistant's response:\n\n{}\n",
    //     parsed["choices"][0]["message"]["content"]
    // );

    Ok(())
}

fn generate_request(
    messages: &mut Vec<serde_json::Value>,
    model_settings: &ModelConfig,
) -> Result<hyper::Request<String>> {
    let messages = serde_json::to_value(messages).context("Error serializing messages")?;
    let mut json_body = serde_json::Map::new();
    json_body.insert(
        "model".to_string(),
        serde_json::json!(model_settings.model_id),
    );
    json_body.insert("messages".to_string(), messages);
    let json_body = serde_json::Value::Object(json_body);

    println!("Inference route: {}", model_settings.inference_route);
    println!("Body: {}", json_body);

    // Build the HTTP request to send the prompt to Model's API
    hyper::Request::builder()
        .method(Method::POST)
        .uri(model_settings.inference_route.as_str())
        .header(HOST, model_settings.domain.as_str())
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "close")
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

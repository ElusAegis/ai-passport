mod config;
mod setup_notary;
mod tlsn_operations;

use crate::remote::attribution::config::{setup_config, Config, ModelSettings};
use crate::remote::attribution::setup_notary::setup_connections;
use crate::remote::attribution::tlsn_operations::{
    build_proof, extract_private_data, notarise_session,
};
use anyhow::{Context, Result};
use http_body_util::BodyExt;
use hyper::client::conn::http1::SendRequest;
use hyper::header::{AUTHORIZATION, CONNECTION, CONTENT_TYPE, HOST};
use hyper::{Method, StatusCode};
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::str;
use tlsn_prover::tls::ProverControl;
use tracing::{debug, warn};

pub async fn generate_conversation_attribution() -> Result<()> {
    // Print the rules on how to use the application
    println!("🌟 Welcome to the Multi-Model Prover CLI! 🌟");
    println!("This application allows you to interact with various AI models and then generate a cryptographic proof of your conversation.");

    println!("⚙️ First, you will need to set up your assistant model.");
    let config = setup_config().await.context("Error setting up config")?;

    println!("🔐 Next, please wait while the system is setup...");

    let (prover_ctrl, prover_task, mut request_sender) = setup_connections(&config)
        .await
        .context("Error setting up connections")?;

    println!(
        "💬 Now, you can engage in a conversation with the `{}` model.",
        config.model_settings.id
    );
    println!("The assistant will respond to your messages in real time.");
    println!("📝 When you're done, simply type 'exit' or press `Enter` without typing a message to end the conversation.");

    println!("🔒 Once finished, a proof of the conversation will be generated and saved for your records.");

    println!("✨ Let's get started! Once the setup is complete, you can begin the conversation.\n");

    let mut messages = vec![];

    let mut request_index = 1;

    let mut recv_private_data = vec![];
    let mut sent_private_data = vec![];

    loop {
        let stop = single_interaction_round(
            &mut request_sender,
            &config,
            &mut messages,
            request_index,
            &mut recv_private_data,
            &mut sent_private_data,
        )
        .await?;

        if stop {
            break;
        }
        request_index += 1;
    }

    debug!("Shutting down the connection with the API...");

    // Shutdown the connection by sending a final dummy request to the API
    shutdown_connection(
        prover_ctrl,
        &mut request_sender,
        &mut recv_private_data,
        &config,
    )
    .await;

    println!("🔒 Generating a cryptographic proof of the conversation. Please wait...");

    // Notarize the session
    debug!("Notarizing the session...");
    let notarised_session = notarise_session(prover_task, &recv_private_data, &sent_private_data)
        .await
        .context("Error notarizing the session")?;

    // Build the proof
    debug!("Building the proof...");
    let proof = build_proof(notarised_session);

    // Save the proof to a file
    let file_path = save_proof_to_file(&proof, &config.model_settings.id)?;

    println!("✅ Proof successfully saved to `{}`.", file_path.display());
    println!(
        "\n🔍 You can share this proof or inspect it at: https://explorer.tlsnotary.org/.\n\
        📂 Simply upload the proof, and anyone can verify its authenticity and inspect the details."
    );

    #[cfg(feature = "dummy-notary")]
    {
        let public_key = include_str!("../../../tlsn/notary.pub");

        // Dummy notary is used for testing purposes only
        // It is not secure and should not be used in production
        println!("🚨 PUBLIC KEY: \n{}", public_key);
        println!("🚨 WARNING: Dummy notary is used for testing purposes only. It is not secure and should not be used in production.");
    }

    Ok(())
}

async fn single_interaction_round(
    request_sender: &mut SendRequest<String>,
    config: &Config,
    mut messages: &mut Vec<serde_json::Value>,
    request_index: i32,
    mut recv_private_data: &mut Vec<Vec<u8>>,
    mut sent_private_data: &mut Vec<Vec<u8>>,
) -> Result<bool> {
    let mut user_message = String::new();
    // The first request is the setup prompt
    if request_index == 1 {
        user_message = config.model_settings.setup_prompt.to_string();
        debug!(
            "Sending setup prompt to `{}` model API: {}",
            config.model_settings.id, user_message
        );
    } else {
        println!("\n💬 Your message\n(type 'exit' to end): ");

        print!("> ");
        std::io::stdout()
            .flush()
            .context("Failed to flush stdout")?;

        std::io::stdin()
            .read_line(&mut user_message)
            .context("Failed to read user input to the model")?;
        println!("processing...");
    }

    if user_message.trim().is_empty() || user_message.trim() == "exit" {
        return Ok(true);
    }

    let user_message = user_message.trim();
    let user_message = serde_json::json!(
        {
            "role": "user",
            "content": user_message
        }
    );

    messages.push(user_message);

    // Prepare the Request to send to the model's API
    let request = generate_request(&mut messages, &config.model_settings)
        .context(format!("Error generating #{request_index} request"))?;

    // Collect the private data transmitted in the request
    extract_private_data(
        &mut sent_private_data,
        request.headers(),
        config.privacy_settings.request_topics_to_censor,
    );

    debug!("Request {request_index}: {:?}", request);

    debug!("Sending request {request_index} to Model's API...");

    let response = request_sender
        .send_request(request)
        .await
        .context(format!("Request #{request_index} failed"))?;

    debug!("Received response {request_index} from Model");

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
        config.privacy_settings.response_topics_to_censor,
    );

    // Collect the body
    let payload = response
        .into_body()
        .collect()
        .await
        .context("Error reading response body")?
        .to_bytes();

    let parsed = serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&payload))
        .context("Error parsing the response")?;

    // Pretty printing the response
    debug!(
        "Response {request_index}: {}",
        serde_json::to_string_pretty(&parsed).context("Error pretty printing the response")?
    );

    debug!("Request {request_index} to Model succeeded");

    let received_assistant_message = serde_json::json!({"role": "assistant", "content": parsed["choices"][0]["message"]["content"]});
    messages.push(received_assistant_message);

    if request_index != 1 {
        println!(
            "\n🤖 Assistant's response:\n\n{}\n",
            parsed["choices"][0]["message"]["content"]
        );
    }

    Ok(false)
}

fn generate_request(
    messages: &mut Vec<serde_json::Value>,
    model_settings: &ModelSettings,
) -> Result<hyper::Request<String>> {
    let messages = serde_json::to_value(messages).context("Error serializing messages")?;
    let mut json_body = serde_json::Map::new();
    json_body.insert("model".to_string(), serde_json::json!(model_settings.id));
    json_body.insert("messages".to_string(), messages);
    let json_body = serde_json::Value::Object(json_body);

    // Build the HTTP request to send the prompt to Model's API
    hyper::Request::builder()
        .method(Method::POST)
        .uri(model_settings.api_settings.inference_route)
        .header(HOST, model_settings.api_settings.server_domain)
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "keep-alive")
        .header(CONTENT_TYPE, "application/json")
        .header(
            AUTHORIZATION,
            format!("Bearer {}", model_settings.api_settings.api_key),
        )
        .body(json_body.to_string())
        .context("Error building the request")
}

async fn shutdown_connection(
    prover_ctrl: ProverControl,
    request_sender: &mut SendRequest<String>,
    recv_private_data: &mut Vec<Vec<u8>>,
    config: &Config,
) {
    debug!("Conversation ended, sending final request to Model's API to shut down the session...");

    // Prepare final request to close the session
    let close_connection_request = hyper::Request::builder()
        .header(HOST, config.model_settings.api_settings.server_domain)
        .uri(config.model_settings.api_settings.inference_route)
        .header("Accept-Encoding", "identity")
        .header(CONNECTION, "close") // This will instruct the server to close the connection
        .body(String::new())
        .unwrap();

    debug!("Sending final request to Model's API...");

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
        config.privacy_settings.response_topics_to_censor,
    );

    // Collect the body
    let payload = response.into_body().collect().await.unwrap().to_bytes();

    let parsed = serde_json::from_str::<serde_json::Value>(&String::from_utf8_lossy(&payload))
        .unwrap_or_else(|_| {
            warn!("Error parsing the response");
            serde_json::json!({
                "error": "Error parsing the response"
            })
        });

    // Pretty printing the response
    debug!(
        "Shutdown response (error response is expected ): {}",
        serde_json::to_string_pretty(&parsed).unwrap()
    );
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

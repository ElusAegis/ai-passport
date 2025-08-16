mod live_interact;
mod notarise;
mod setup;
mod share;

use crate::config::ProveConfig;
use crate::prove::live_interact::{generate_request, request_reply_loop, send_connection_close};
use crate::prove::notarise::notarise_session;
use crate::prove::setup::setup;
use crate::prove::share::store_interaction_proof_to_file;
use crate::utils::spinner::with_spinner_future;
use anyhow::{Context, Result};
use hyper::client::conn::http1::SendRequest;
use hyper::StatusCode;
use serde_json::Value;
use spansy::http::BodyContent;
use spansy::json::JsonValue;
use spansy::Spanned;
use std::io::Write;
use tlsn_formats::http::HttpTranscript;
use tlsn_prover::{state, Prover, ProverError};
use tokio::task::JoinHandle;
use tracing::debug;

type ProverWithRequestSender = (
    JoinHandle<Result<Prover<state::Committed>, ProverError>>,
    SendRequest<String>,
);

pub(crate) async fn run_prove(app_config: &ProveConfig) -> Result<()> {
    if app_config.notary_config.is_one_shot_mode {
        one_shot_interaction_proving(app_config).await
    } else {
        multi_round_interaction_proving(app_config).await
    }
}

pub(crate) async fn one_shot_interaction_proving(app_config: &ProveConfig) -> Result<()> {
    let _total_sent = 0;
    let _total_recv = 0;
    const _PREPARE_IN_ADVANCE: usize = 3; // TODO - consider turning this into a config option

    let mut evolving_config = app_config.clone();

    let cloned_evolving_config = evolving_config.clone();
    let mut current_instance_handle: JoinHandle<Result<ProverWithRequestSender>> =
        tokio::spawn(async move { setup(&cloned_evolving_config).await });
    let mut future_instance_handle: JoinHandle<Result<ProverWithRequestSender>>;

    let mut messages: Vec<Value> = vec![];
    let mut counter = 0;

    loop {
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
            let mut current_instance = current_instance_handle.await??;
            send_connection_close(&mut current_instance.1, &app_config.model_config)
                .await
                .context("failed to send close request")?;

            // current_instance = if let Some(future_instance_handle) = future_instance_handle {
            //     future_instance_handle.await??
            // } else {
            //     current_instance
            // };
            //
            // send_connection_close(&mut current_instance.1, &app_config.model_config)
            //     .await
            //     .context("failed to send close request")?;

            break;
        }

        println!("processing...");

        // ---- 2.1) Prepare the next prover instance -----------------------------

        evolving_config.notary_config.max_single_request_size =
            app_config.notary_config.max_single_request_size
                + app_config.notary_config.max_single_response_size;

        let cloned_evolving_config = evolving_config.clone();

        future_instance_handle = tokio::spawn(async move { setup(&cloned_evolving_config).await });

        // ---- 3) Normal request path ---------------------------------------------
        messages.push(serde_json::json!({
            "role": "user",
            "content": user_input
        }));

        let request = generate_request(&messages, &app_config.model_config, true)
            .context("Error generating request")?;

        let mut current_instance = current_instance_handle.await??;

        let response = current_instance
            .1
            .send_request(request)
            .await
            .context("Request failed")?;

        debug!("Received response from Model: {:?}", response.status());

        if response.status() != StatusCode::OK {
            anyhow::bail!("Request failed with status: {}", response.status());
        }

        // Notarize the session
        debug!("Notarizing the session...");
        let (attestation, secrets) = notarise_session(current_instance.0.await??)
            .await
            .context("Error notarizing the session")?;

        // Save the proof to a file
        let _file_path = store_interaction_proof_to_file(
            format!("part_{}", counter).as_str(),
            &attestation,
            &app_config.privacy_config,
            &secrets,
            &app_config.model_config.model_id,
        )?;

        let transcript = HttpTranscript::parse(secrets.transcript())?;
        let body: BodyContent = transcript.responses[0]
            .body
            .as_ref()
            .unwrap()
            .content
            .clone();

        match body {
            BodyContent::Json(json) => match json.get("choices.0.message.content").unwrap() {
                JsonValue::String(value) => {
                    let string_value = value.span().as_str();
                    println!("\n🤖 Assistant's response:\n\n{}\n", string_value);
                    let received_assistant_message = serde_json::json!({
                        "role": "assistant",
                        "content": string_value
                    });
                    messages.push(received_assistant_message);
                }
                _ => {
                    anyhow::bail!("Received response body is not in expected JSON format");
                }
            },
            BodyContent::Unknown(_) => {
                anyhow::bail!("Received response body is not in JSON format");
            }
            _ => {
                anyhow::bail!("Received response body is not in JSON format");
            }
        }

        current_instance_handle = future_instance_handle;
        counter += 1;
    }

    Ok(())
}

pub(crate) async fn multi_round_interaction_proving(app_config: &ProveConfig) -> Result<()> {
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

    request_reply_loop(app_config, &mut request_sender, &mut messages).await?;

    println!("🔒 Generating a cryptographic proof of the conversation. Please wait...");

    // Notarize the session
    debug!("Notarizing the session...");
    let (attestation, secrets) = notarise_session(prover_task.await??)
        .await
        .context("Error notarizing the session")?;

    // Save the proof to a file
    let file_path = store_interaction_proof_to_file(
        "multi_round",
        &attestation,
        &app_config.privacy_config,
        &secrets,
        &app_config.model_config.model_id,
    )?;

    println!("✅ Proof successfully saved to `{}`.", file_path.display());
    println!(
            "\n🔍 You can share this proof or inspect it at: https://explorer.tlsnotary.org/.\n\
        📂 Simply upload the proof, and anyone can verify its authenticity and inspect the details."
        );

    Ok(())
}

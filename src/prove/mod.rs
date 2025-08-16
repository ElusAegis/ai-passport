mod live_interact;
mod notarise;
mod setup;
mod share;

use crate::config::ProveConfig;
use crate::prove::live_interact::{request_reply_loop, single_interaction_round};
use crate::prove::notarise::notarise_session;
use crate::prove::setup::setup;
use crate::prove::share::store_interaction_proof_to_file;
use crate::utils::spinner::with_spinner_future;
use anyhow::{Context, Result};
use hyper::client::conn::http1::SendRequest;
use serde_json::Value;
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

    let cloned_app_config = app_config.clone();
    let mut current_instance_handle: JoinHandle<Result<ProverWithRequestSender>> =
        tokio::spawn(async move { setup(&cloned_app_config).await });
    let mut cloned_app_config = app_config.clone();
    cloned_app_config.notary_config.max_single_request_size +=
        app_config.notary_config.max_single_request_size
            + app_config.notary_config.max_single_response_size;
    let mut future_instance_handle: JoinHandle<Result<ProverWithRequestSender>> =
        tokio::spawn(async move { setup(&cloned_app_config).await });

    let mut messages: Vec<Value> = vec![];
    let mut counter = 0;

    loop {
        let mut current_instance = current_instance_handle.await??;

        let stop =
            single_interaction_round(&mut current_instance.1, app_config, &mut messages).await?;

        if stop {
            break;
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

        current_instance_handle = future_instance_handle;

        let mut cloned_app_config = app_config.clone();
        let encoded_messages =
            serde_json::to_string(&messages).context("Failed to encode messages to JSON")?;
        let message_byte_size = encoded_messages.len();
        cloned_app_config.notary_config.max_single_request_size = message_byte_size
            + app_config.notary_config.max_single_request_size
            + app_config.notary_config.max_single_response_size;
        future_instance_handle = tokio::spawn(async move { setup(&cloned_app_config).await });

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

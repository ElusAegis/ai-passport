mod live_interact;
mod notarise;
mod setup;
mod share;

use crate::config::ProveConfig;
use crate::prove::live_interact::request_reply_loop;
use crate::prove::notarise::notarise_session;
use crate::prove::setup::setup;
use crate::prove::share::store_interaction_proof_to_file;
use crate::utils::spinner::with_spinner_future;
use anyhow::{Context, Result};
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

    request_reply_loop(
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
    let (attestation, secrets) =
        notarise_session(prover_task.await??, &recv_private_data, &sent_private_data)
            .await
            .context("Error notarizing the session")?;

    // Save the proof to a file
    let file_path = store_interaction_proof_to_file(
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

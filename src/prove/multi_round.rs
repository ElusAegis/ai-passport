use crate::prove::live_interact::single_interaction_round;
use crate::prove::notarise::notarise_session;
use crate::prove::setup::setup;
use crate::prove::share::store_interaction_proof_to_file;
use crate::utils::spinner::with_spinner_future;
use crate::ProveConfig;
use anyhow::Context;
use dialoguer::console::style;
use tracing::{debug, info};

pub(crate) async fn run_multi_round_prove(app_config: &ProveConfig) -> anyhow::Result<()> {
    let (prover_task, mut request_sender) =
        with_spinner_future("Please wait while the system is setup", setup(app_config)).await?;

    let mut messages = vec![];

    loop {
        let stop = single_interaction_round(&mut request_sender, app_config, &mut messages).await?;

        if stop {
            break;
        }
    }

    // Notarize the session
    debug!("Notarizing the session...");
    let (attestation, secrets) = with_spinner_future(
        "Generating a cryptographic proof of the conversation...",
        notarise_session(prover_task.await??),
    )
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

    info!(target: "plain",
        "\n{} {}",
        style("✔").green(),
        style("Proof successfully saved").bold(),
    );

    info!(target: "plain", "{} {}", style("📂").dim(), file_path.display());

    info!(target: "plain",
        "\n{} {}",
        style("🔍").yellow(),
        style("You can verify this proof anytime with the CLI: `verify <proof_file>`").dim()
    );

    Ok(())
}

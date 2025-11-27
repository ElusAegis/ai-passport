use crate::config::ProveConfig;
use crate::prove::live_interact::single_interaction_round;
use crate::tlsn::notarise::notarise_session;
use crate::tlsn::save_proof::save_to_file;
use crate::tlsn::setup::setup;
use crate::utils::spinner::with_spinner_future;
use anyhow::Context;
use dialoguer::console::style;
use tracing::{debug, info};

pub(crate) async fn run_single(app_config: &ProveConfig) -> anyhow::Result<()> {
    let (prover_task, mut request_sender) = with_spinner_future(
        "Please wait while the system is setup...",
        setup(
            &app_config.notary,
            &app_config.model.server.domain,
            app_config.model.server.port,
        ),
    )
    .await?;

    let mut messages = vec![];

    for i in 0..app_config.session.max_msg_num {
        let was_stopped =
            single_interaction_round(&mut request_sender, app_config, &mut messages).await?;

        if was_stopped || i + 1 >= app_config.session.max_msg_num {
            drop(request_sender);
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
    let file_path = save_to_file(
        &format!(
            "{}_single_setup_interaction_proof",
            app_config.model.model_id
        ),
        &attestation,
        &app_config.privacy,
        &secrets,
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

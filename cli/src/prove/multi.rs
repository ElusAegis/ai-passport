use crate::config::notary::NotaryConfig;
use crate::config::ProveConfig;
use crate::prove::live_interact::single_interaction_round;
use crate::prove::notarise::notarise_session;
use crate::prove::ProverWithRequestSender;
use crate::tlsn::save_proof::save_to_file;
use crate::tlsn::setup::setup;
use anyhow::{Context, Result};
use dialoguer::console::style;
use serde_json::Value;
use std::path::PathBuf;
use tokio::task::JoinHandle;
use tracing::{debug, info};

pub(crate) async fn run_multi(app_config: &ProveConfig) -> Result<()> {
    let app_max_single_request_size = app_config.session.max_single_request_size;
    let app_max_single_response_size = app_config.session.max_single_response_size;
    let max_req_num = app_config.session.max_msg_num;

    let spawn_setup = |notary_config: NotaryConfig| {
        let port = app_config.model.server.port;
        let domain = app_config.model.server.domain.clone();
        tokio::spawn(async move { setup(&notary_config, &domain, port).await })
    };

    let mut stored_proofs = Vec::<PathBuf>::new();

    // Set up the current instance of the prover
    let mut current_instance_handle: JoinHandle<Result<ProverWithRequestSender>> =
        spawn_setup(app_config.notary.clone());

    // Set up the future instance of the prover
    let future_notary_config = app_config
        .notary
        .increase_total_sent(app_max_single_request_size + app_max_single_response_size);
    let mut future_instance_handle: Option<JoinHandle<Result<ProverWithRequestSender>>> =
        if max_req_num > 1 {
            Some(spawn_setup(future_notary_config))
        } else {
            None
        };

    let mut all_messages: Vec<Value> = vec![];

    for counter in 0..max_req_num {
        // Wait for the current instance to be ready
        let mut current_instance = current_instance_handle.await??;

        let stop = single_interaction_round(&mut current_instance.1, app_config, &mut all_messages)
            .await?;

        if stop {
            break;
        }

        // Notarize the session
        debug!("Notarizing the session...");
        let (attestation, secrets) = notarise_session(current_instance.0.await??)
            .await
            .context("Error notarizing the session")?;

        // Save the proof to a file
        stored_proofs.push(save_to_file(
            format!(
                "{}_part_{counter}_one_shot_interaction_proof",
                &app_config.model.model_id
            )
            .as_str(),
            &attestation,
            &app_config.privacy,
            &secrets,
        )?);

        // If we are processing the last request, we can exit early
        if counter + 1 >= max_req_num {
            break;
        }

        // Prepare for the next iteration
        current_instance_handle =
            future_instance_handle.context("Future notarization instance does not exist")?;

        // Calculate the size of messages that will be sent in the next request
        let encoded_messages =
            serde_json::to_string(&all_messages).context("Failed to encode messages to JSON")?;
        let message_byte_size = encoded_messages.len();

        // Prepare the next iteration's future instance handle
        let future_notary_config = app_config.notary.increase_total_sent(
            message_byte_size + app_max_single_request_size + app_max_single_response_size,
        );

        future_instance_handle = if counter < max_req_num {
            Some(spawn_setup(future_notary_config))
        } else {
            None
        };
    }

    if !stored_proofs.is_empty() {
        info!(target: "plain",
            "\n{} {}",
            style("✔").green(),
            style("All proofs successfully saved").bold(),
        );

        for (i, proof) in stored_proofs.iter().enumerate() {
            info!(target: "plain", "{} Assistant message {} → {}", style("📂").dim(), i + 1, proof.display());
        }

        info!(target: "plain",
            "\n{} {}",
            style("🔍").yellow(),
            style("You can verify these proofs anytime with the CLI: `verify <proof_file>`").dim()
        );
    } else {
        info!(target: "plain", "No proofs were generated during this session.");
    }

    Ok(())
}

use crate::config::PrivacyConfig;
use anyhow::Context;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tlsn_core::attestation::Attestation;
use tlsn_core::presentation::Presentation;
use tlsn_core::{CryptoProvider, Secrets};
use tlsn_formats::http::HttpTranscript;

pub(super) fn store_interaction_proof_to_file(
    attestation: &Attestation,
    privacy_config: &PrivacyConfig,
    secrets: &Secrets,
    model_id: &str,
) -> anyhow::Result<PathBuf> {
    let transcript = HttpTranscript::parse(secrets.transcript())?;

    // Build a transcript proof.
    let mut builder = secrets.transcript_proof_builder();

    for request in transcript.requests.iter() {
        builder.reveal_sent(&request.without_data())?;
        builder.reveal_sent(&request.request.target)?;

        if request.body.is_some() {
            let content = &request.body.as_ref().unwrap().content;
            builder
                .reveal_sent(content)
                .context("Failed to reveal sent content")?;
        }

        for header in request.headers.iter() {
            if privacy_config
                .request_topics_to_censor
                .contains(&header.name.as_str().to_lowercase().as_str())
            {
                builder.reveal_sent(&header.without_value())?;
            } else {
                builder.reveal_sent(header)?;
            }
        }
    }

    for response in transcript.responses.iter() {
        builder.reveal_recv(&response.without_data())?;

        if response.body.is_some() {
            let content = &response.body.as_ref().unwrap().content;
            builder
                .reveal_recv(content)
                .context("Failed to reveal received content")?;
        }

        for header in response.headers.iter() {
            if privacy_config
                .response_topics_to_censor
                .contains(&header.name.as_str().to_lowercase().as_str())
            {
                builder.reveal_recv(&header.without_value())?;
            } else {
                builder.reveal_recv(header)?;
            }
        }
    }

    let transcript_proof = builder.build()?;

    // Use default crypto provider to build the presentation.
    let provider = CryptoProvider::default();

    let mut builder = attestation.presentation_builder(&provider);

    builder
        .identity_proof(secrets.identity_proof())
        .transcript_proof(transcript_proof);

    let presentation: Presentation = builder.build()?;

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

    let attestation_content =
        serde_json::to_string_pretty(&presentation).context("Failed to serialize presentation")?;

    file.write_all(attestation_content.as_bytes())
        .context("Failed to write interaction proof to file")?;

    Ok(path_buf)
}

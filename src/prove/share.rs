use crate::config::PrivacyConfig;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tlsn_core::attestation::Attestation;
use tlsn_core::presentation::Presentation;
use tlsn_core::transcript::TranscriptProof;
use tlsn_core::{CryptoProvider, Secrets};
use tlsn_formats::http::HttpTranscript;

const PROOFS_DIR: &str = "model_ips";

pub(super) fn store_interaction_proof_to_file(
    postfix: &str,
    attestation: &Attestation,
    privacy_config: &PrivacyConfig,
    secrets: &Secrets,
    model_id: &str,
) -> Result<PathBuf> {
    // 1) Build transcript proof with selective disclosure
    let transcript_proof =
        build_transcript_proof(secrets, privacy_config).context("building transcript proof")?;

    // 2) Build the final presentation (identity + transcript proofs)
    let presentation = build_presentation(attestation, secrets, transcript_proof)
        .context("building presentation")?;

    // 3) Ensure proofs/ exists and construct the output file path
    ensure_dir(PROOFS_DIR).context("creating model_ips/ directory")?;
    let file_path = proof_path(PROOFS_DIR, model_id, postfix);

    // 4) Serialize and write JSON
    let json =
        serde_json::to_string_pretty(&presentation).context("serializing presentation to JSON")?;
    fs::write(&file_path, json).context("writing interaction proof to file")?;

    Ok(file_path)
}

// --- helpers ---

fn build_transcript_proof(secrets: &Secrets, privacy: &PrivacyConfig) -> Result<TranscriptProof> {
    let transcript =
        HttpTranscript::parse(secrets.transcript()).context("parsing HTTP transcript")?;

    // Precompute lowercased header names to censor
    let req_censor: HashSet<String> = privacy
        .request_topics_to_censor
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    let resp_censor: HashSet<String> = privacy
        .response_topics_to_censor
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut b = secrets.transcript_proof_builder();

    // Requests
    for req in &transcript.requests {
        b.reveal_sent(&req.without_data())?;
        b.reveal_sent(&req.request.target)?;
        if let Some(body) = &req.body {
            b.reveal_sent(&body.content).context("reveal sent body")?;
        }
        for h in &req.headers {
            if req_censor.contains(&h.name.as_str().to_lowercase()) {
                b.reveal_sent(&h.without_value())?;
            } else {
                b.reveal_sent(h)?;
            }
        }
    }

    // Responses
    for resp in &transcript.responses {
        b.reveal_recv(&resp.without_data())?;
        if let Some(body) = &resp.body {
            b.reveal_recv(&body.content).context("reveal recv body")?;
        }
        for h in &resp.headers {
            if resp_censor.contains(&h.name.as_str().to_lowercase()) {
                b.reveal_recv(&h.without_value())?;
            } else {
                b.reveal_recv(h)?;
            }
        }
    }

    let proof = b.build().context("finalizing transcript proof")?;
    Ok(proof)
}

fn build_presentation(
    attestation: &Attestation,
    secrets: &Secrets,
    transcript_proof: TranscriptProof,
) -> Result<Presentation> {
    let provider = CryptoProvider::default();
    let mut pb = attestation.presentation_builder(&provider);
    pb.identity_proof(secrets.identity_proof())
        .transcript_proof(transcript_proof);
    Ok(pb.build()?)
}

fn ensure_dir<P: AsRef<Path>>(dir: P) -> Result<()> {
    fs::create_dir_all(&dir).with_context(|| format!("mkdir -p {}", dir.as_ref().display()))
}

fn proof_path(dir: &str, model_id: &str, postfix: &str) -> PathBuf {
    let ts = unix_ts();
    let model = sanitize_model_id(model_id);
    let filename = format!("{model}_{ts}_{postfix}_interaction_proof.json");
    Path::new(dir).join(filename)
}

fn unix_ts() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_secs()
}

fn sanitize_model_id(s: &str) -> String {
    s.replace([' ', '/'], "_")
}

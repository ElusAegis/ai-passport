use anyhow::{anyhow, Result};
use spansy::Spanned;
use tlsn_core::attestation::Attestation;
use tlsn_core::request::RequestConfig;
use tlsn_core::transcript::TranscriptCommitConfig;
use tlsn_core::Secrets;
use tlsn_formats::http::{DefaultHttpCommitter, HttpCommit, HttpTranscript};
use tlsn_prover::{state, Prover};
use tracing::debug;

pub(super) async fn notarise_session(
    mut prover: Prover<state::Committed>,
    _recv_private_data: &[Vec<u8>],
    _sent_private_data: &[Vec<u8>],
) -> Result<(Attestation, Secrets)> {
    // Parse the HTTP transcript.
    let transcript = HttpTranscript::parse(prover.transcript())?;

    let body_content = &transcript.responses[0].body.as_ref().unwrap().content;
    let body = String::from_utf8_lossy(body_content.span().as_bytes());
    debug!("Response body: {}", body);

    // Commit to the transcript.
    let mut builder = TranscriptCommitConfig::builder(prover.transcript());

    // This commits to various parts of the transcript separately (e.g. request
    // headers, response headers, response body and more). See https://docs.tlsnotary.org//protocol/commit_strategy.html
    // for other strategies that can be used to generate commitments.
    DefaultHttpCommitter::default().commit_transcript(&mut builder, &transcript)?;

    // Finalize, returning the notarized session
    let transcript_commit = builder
        .build()
        .map_err(|e| anyhow!("Error building transcript commit: {:?}", e))?;

    // Build an attestation request.
    let mut builder = RequestConfig::builder();

    builder.transcript_commit(transcript_commit);

    let request_config = builder.build()?;

    #[allow(deprecated)]
    let (attestation, secrets) = prover.notarize(&request_config).await?;

    debug!("Notarization complete!");

    Ok((attestation, secrets))
}

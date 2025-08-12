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

    // // Commit to public ranges
    // builder
    //     .commit_sent(&RangeSet::from(public_sent_commitment_ids))
    //     .context("Error committing to public sent ranges")?;
    // builder
    //     .commit_recv(&RangeSet::from(public_received_commitment_ids))
    //     .context("Error committing to public received ranges")?;

    builder.transcript_commit(transcript_commit);

    let request_config = builder.build()?;

    #[allow(deprecated)]
    let (attestation, secrets) = prover.notarize(&request_config).await?;

    debug!("Notarization complete!");

    Ok((attestation, secrets))
}

// pub(super) fn extract_private_data(
//     recv_private_data: &mut Vec<Vec<u8>>,
//     headers: &HeaderMap,
//     topics_to_censor: &[&str],
// ) {
//     for (header_name, header_value) in headers {
//         if topics_to_censor.contains(&header_name.as_str()) {
//             let header_value = header_value.as_bytes().to_vec();
//             if !recv_private_data.contains(&header_value) {
//                 recv_private_data.push(header_value);
//             }
//         }
//     }
// }

// fn find_ranges(seq: &[u8], sub_seq: &[&[u8]]) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
//     let mut private_ranges = Vec::new();
//     for s in sub_seq {
//         for (idx, w) in seq.windows(s.len()).enumerate() {
//             if w == *s {
//                 private_ranges.push(idx..(idx + w.len()));
//             }
//         }
//     }
//
//     let mut sorted_ranges = private_ranges.clone();
//     sorted_ranges.sort_by_key(|r| r.start);
//
//     let mut public_ranges = Vec::new();
//     let mut last_end = 0;
//     for r in sorted_ranges {
//         if r.start > last_end {
//             public_ranges.push(last_end..r.start);
//         }
//         last_end = r.end;
//     }
//
//     if last_end < seq.len() {
//         public_ranges.push(last_end..seq.len());
//     }
//
//     (public_ranges, private_ranges)
// }

use anyhow::{Context, Error};
use hyper::HeaderMap;
use std::ops::Range;
use tlsn_core::attestation::Attestation;
use tlsn_core::request::RequestConfig;
use tlsn_core::transcript::TranscriptCommitConfig;
use tlsn_core::Secrets;
use tlsn_prover::state::Closed;
use tlsn_prover::{Prover, ProverError};
use tokio::task::JoinHandle;
use tracing::debug;
use utils::range::RangeSet;

pub(super) async fn notarise_session(
    prover_task: JoinHandle<anyhow::Result<Prover<Closed>, ProverError>>,
    recv_private_data: &[Vec<u8>],
    sent_private_data: &[Vec<u8>],
) -> Result<(Attestation, Secrets), Error> {
    // The Prover task should be done now, so we can grab it.
    let prover = prover_task
        .await
        .context("Error waiting for prover task")??;

    // Prepare for notarization
    let mut prover = prover.start_notarize();

    // Notarize the session
    let (public_sent_commitment_ids, _) = find_ranges(
        prover.transcript().sent(),
        &sent_private_data
            .iter()
            .map(|v| v.as_slice())
            .collect::<Vec<&[u8]>>(),
    );

    let (public_received_commitment_ids, _) = find_ranges(
        prover.transcript().received(),
        &recv_private_data
            .iter()
            .map(|v| v.as_slice())
            .collect::<Vec<&[u8]>>(),
    );

    let mut builder = TranscriptCommitConfig::builder(prover.transcript());

    // Commit to public ranges
    builder
        .commit_sent(&RangeSet::from(public_sent_commitment_ids))
        .context("Error committing to public sent ranges")?;
    builder
        .commit_recv(&RangeSet::from(public_received_commitment_ids))
        .context("Error committing to public received ranges")?;

    // Finalize, returning the notarized session
    let config = builder
        .build()
        .context("Error building transcript commit config")?;

    prover.transcript_commit(config);

    // Finalize, returning the notarized session
    let request_config = RequestConfig::default();
    let (attestation, secrets) = prover
        .finalize(&request_config)
        .await
        .context("Error finalizing prover")?;

    debug!("Notarization complete!");

    Ok((attestation, secrets))
}

pub(super) fn extract_private_data(
    recv_private_data: &mut Vec<Vec<u8>>,
    headers: &HeaderMap,
    topics_to_censor: &[&str],
) {
    for (header_name, header_value) in headers {
        if topics_to_censor.contains(&header_name.as_str()) {
            let header_value = header_value.as_bytes().to_vec();
            if !recv_private_data.contains(&header_value) {
                recv_private_data.push(header_value);
            }
        }
    }
}

fn find_ranges(seq: &[u8], sub_seq: &[&[u8]]) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut private_ranges = Vec::new();
    for s in sub_seq {
        for (idx, w) in seq.windows(s.len()).enumerate() {
            if w == *s {
                private_ranges.push(idx..(idx + w.len()));
            }
        }
    }

    let mut sorted_ranges = private_ranges.clone();
    sorted_ranges.sort_by_key(|r| r.start);

    let mut public_ranges = Vec::new();
    let mut last_end = 0;
    for r in sorted_ranges {
        if r.start > last_end {
            public_ranges.push(last_end..r.start);
        }
        last_end = r.end;
    }

    if last_end < seq.len() {
        public_ranges.push(last_end..seq.len());
    }

    (public_ranges, private_ranges)
}

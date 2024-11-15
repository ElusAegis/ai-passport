use anyhow::Context;
use hyper::HeaderMap;
use std::ops::Range;
use tlsn_core::commitment::CommitmentId;
use tlsn_core::proof::TlsProof;
use tlsn_core::NotarizedSession;
use tlsn_prover::tls::state::Closed;
use tlsn_prover::tls::{Prover, ProverError};
use tokio::task::JoinHandle;
use tracing::debug;

pub(super) async fn notarise_session(
    prover_task: JoinHandle<anyhow::Result<Prover<Closed>, ProverError>>,
    recv_private_data: &[Vec<u8>],
    sent_private_data: &[Vec<u8>],
) -> anyhow::Result<(Vec<CommitmentId>, Vec<CommitmentId>, NotarizedSession)> {
    // The Prover task should be done now, so we can grab it.
    let prover = prover_task
        .await
        .context("Error waiting for prover task")??;

    // Prepare for notarization
    let mut prover = prover.start_notarize();

    // Notarize the session
    let (public_sent_commitment_ids, _) = find_ranges(
        prover.sent_transcript().data(),
        &sent_private_data
            .iter()
            .map(|v| v.as_slice())
            .collect::<Vec<&[u8]>>(),
    );

    let (public_received_commitment_ids, _) = find_ranges(
        prover.recv_transcript().data(),
        &recv_private_data
            .iter()
            .map(|v| v.as_slice())
            .collect::<Vec<&[u8]>>(),
    );

    let builder = prover.commitment_builder();

    let sent_commitment_ids = public_sent_commitment_ids
        .iter()
        .map(|range| builder.commit_sent(range).unwrap())
        .collect::<Vec<_>>();

    let recived_commitment_ids = public_received_commitment_ids
        .iter()
        .map(|range| builder.commit_recv(range).unwrap())
        .collect::<Vec<_>>();

    // Finalize, returning the notarized session
    let notarized_session = prover
        .finalize()
        .await
        .context("Error finalizing notarization")?;

    debug!("Notarization complete!");

    Ok((
        sent_commitment_ids,
        recived_commitment_ids,
        notarized_session,
    ))
}

pub(super) fn build_proof(
    (sent_commitment_ids, received_commitment_ids, notarized_session): (
        Vec<CommitmentId>,
        Vec<CommitmentId>,
        NotarizedSession,
    ),
) -> TlsProof {
    let session_proof = notarized_session.session_proof();

    let mut proof_builder = notarized_session.data().build_substrings_proof();

    for id in sent_commitment_ids {
        proof_builder.reveal_by_id(id).unwrap();
    }
    for id in received_commitment_ids {
        proof_builder.reveal_by_id(id).unwrap();
    }

    let substrings_proof = proof_builder.build().unwrap();

    TlsProof {
        session: session_proof,
        substrings: substrings_proof,
    }
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

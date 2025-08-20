mod live_interact;
mod multi;
mod single;

use crate::config::{ProveConfig, SessionMode};
use crate::prove::multi::run_multi;
use crate::prove::single::run_single;
use anyhow::Result;
use hyper::client::conn::http1::SendRequest;
use tlsn_prover::{state, Prover, ProverError};
use tokio::task::JoinHandle;

type ProverWithRequestSender = (
    JoinHandle<Result<Prover<state::Committed>, ProverError>>,
    SendRequest<String>,
);

pub async fn run_prove(app_config: &ProveConfig) -> Result<()> {
    if matches!(app_config.session.mode, SessionMode::Multi) {
        run_multi(app_config).await
    } else {
        run_single(app_config).await
    }
}

mod live_interact;
mod multi_round;
mod notarise;
mod one_shot;
pub(crate) mod setup;
mod share;

use crate::args::SessionMode;
use crate::config::ProveConfig;
use crate::prove::multi_round::run_multi_round_prove;
use crate::prove::one_shot::run_one_shot_prove;
use anyhow::Result;
use hyper::client::conn::http1::SendRequest;
use tlsn_prover::{state, Prover, ProverError};
use tokio::task::JoinHandle;

type ProverWithRequestSender = (
    JoinHandle<Result<Prover<state::Committed>, ProverError>>,
    SendRequest<String>,
);

pub async fn run_prove(app_config: &ProveConfig) -> Result<()> {
    if matches!(app_config.notarisation_config.mode, SessionMode::OneShot) {
        run_one_shot_prove(app_config).await
    } else {
        run_multi_round_prove(app_config).await
    }
}

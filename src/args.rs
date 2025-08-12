use clap::ValueHint;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

// Maximum number of bytes that can be sent from prover to server
const MAX_SENT_DATA: usize = 1 << 10;
// Maximum number of bytes that can be received by prover from server
const MAX_RECV_DATA: usize = 1 << 14;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Prove model interaction
    Prove(ProveArgs),

    /// Verify model interaction
    Verify(VerifyArgs),
}

#[derive(Args, Debug)]
pub(crate) struct ProveArgs {
    /// Specify the model to use (optional for proving)
    #[arg(long)]
    pub(crate) model_id: Option<String>,
    /// Path to environment file (default: ./.env). Can also use APP_ENV_FILE.
    #[arg(long, value_hint = ValueHint::FilePath, default_value = ".env", env = "APP_ENV_FILE", global = true
    )]
    pub(crate) env_file: PathBuf,
    /// Maximum number of bytes that can be sent from prover to server
    #[arg(long, default_value_t = MAX_SENT_DATA)]
    pub(crate) max_sent_data: usize,
    /// Maximum number of bytes that can be received by prover from server
    #[arg(long, default_value_t = MAX_RECV_DATA)]
    pub(crate) max_recv_data: usize,
}

#[derive(Args, Debug)]
pub(crate) struct VerifyArgs {
    /// Path to the generated proof to verify (optional)
    #[arg(
        long,
        value_hint = ValueHint::FilePath,
        env = "APP_PROOF_PATH"
    )]
    pub(crate) proof_path: Option<String>,
}

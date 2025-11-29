use crate::cli::{Cli, Command};
use crate::config::prove::ProveConfig;
use crate::config::verify::VerifyConfig;
use crate::prover::AgentProver;
use crate::prover::Prover;
use crate::verify::run_verify;
use crate::{with_input_source, StdinInputSource};
use clap::Parser;

/// Run the application from CLI arguments
pub async fn run() -> anyhow::Result<()> {
    // Preload environment variables from .env file if it exists before parsing CLI args
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.cmd {
        Command::Prove(prove_args) => {
            let config = ProveConfig::from_args(&prove_args).await?;
            let prover = AgentProver::try_from(prove_args)?;

            with_input_source(StdinInputSource {}, prover.run(&config)).await
        }
        Command::Verify(verify_args) => {
            let config = VerifyConfig::setup(verify_args)?;
            run_verify(&config)
        }
    }
}

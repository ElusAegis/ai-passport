use crate::args::{Cli, Command};
use crate::config::{ProveConfig, VerifyConfig};
use crate::prove::run_prove;
use crate::verify::run_verify;
use clap::Parser;

pub enum Application {
    Prove(ProveConfig),
    Verify(VerifyConfig),
}

impl Application {
    pub async fn init() -> anyhow::Result<Application> {
        let cli = Cli::parse();

        let application = match cli.cmd {
            Command::Prove(prove_args) => Application::Prove(ProveConfig::setup(prove_args).await?),
            Command::Verify(verify_args) => Application::Verify(VerifyConfig::setup(verify_args)?),
        };

        Ok(application)
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        match self {
            Self::Prove(prove_conf) => run_prove(prove_conf).await,
            Self::Verify(verify_conf) => run_verify(verify_conf),
        }
    }
}

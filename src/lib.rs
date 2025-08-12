mod config;
mod prove;
mod verify;

use crate::config::{ProveConfig, VerifyConfig};
use crate::prove::run_prove;
use crate::verify::run_verify;
pub use config::Command;

pub enum Application {
    Prove(ProveConfig),
    Verify(VerifyConfig),
}

impl Application {
    pub async fn init(args: Command) -> anyhow::Result<Application> {
        let application = match args {
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

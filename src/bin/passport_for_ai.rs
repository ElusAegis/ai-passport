use anyhow::Result;
use clap::Parser;
use passport_for_ai::{Application, Command};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    // Print the rules on how to use the application
    println!("🌟 Welcome to the Multi-Model CLI! 🌟");
    println!("This application allows you to interact with various AI models and then generate a cryptographic proof of your conversation as well as verify it.");

    let application = Application::init(cli.cmd).await?;

    application.run().await
}

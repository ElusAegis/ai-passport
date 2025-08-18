use anyhow::Result;
use passport_for_ai::Application;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                // Only show WARN+ from all crates by default…
                .add_directive("warn".parse()?)
                // …but still allow INFO for your crate
                .add_directive("passport_for_ai=info".parse()?),
        )
        .init();

    print_welcome_message();

    let application = Application::init().await?;

    application.run().await
}

use dialoguer::console::style;

fn print_welcome_message() {
    println!();
    println!(
        "{} {} {}",
        style("◆").blue().bold(),
        style("Welcome to the Proofs-of-Autonomy CLI").bold(),
        style("◆").blue().bold(),
    );
    println!(
        "{}",
        style("Create and verify cryptographic proofs of model conversations.").dim()
    );
    println!();
}

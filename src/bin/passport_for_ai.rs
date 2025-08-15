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

fn print_welcome_message() {
    // Print the rules on how to use the application
    println!();
    println!();
    println!("🌟 Welcome to the Proofs-of-Autonomy CLI! 🌟");
    println!("Create and verify cryptographic proofs of model conversations.");
    println!();
    println!();
}

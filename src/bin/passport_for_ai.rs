use anyhow::Result;
use passport_for_ai::Application;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

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

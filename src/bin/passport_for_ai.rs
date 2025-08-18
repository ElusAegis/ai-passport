use anyhow::{Context, Result};
use dialoguer::console::style;
use passport_for_ai::Application;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter, Layer};

#[tokio::main]
async fn main() -> Result<()> {
    init_logging().context("initializing logging")?;

    print_welcome();

    let application = Application::init().await?;

    application.run().await
}

pub fn init_logging() -> Result<()> {
    // Plain, no-frills layer (only for target = "plain")
    let plain_fmt = fmt::format()
        .without_time()
        .with_level(false)
        .with_target(false)
        .compact();

    // Only handle events with target="plain"
    let plain_layer = fmt::layer()
        .event_format(plain_fmt)
        .with_filter(Targets::new().with_target("plain", LevelFilter::TRACE));

    // Normal, rich formatting for everything else (env controlled)
    let rich_layer = fmt::layer().with_filter(
        EnvFilter::from_default_env()
            .add_directive("warn".parse()?) // default WARN+
            .add_directive("passport_for_ai=info".parse()?), // your crate at INFO
    );

    tracing_subscriber::registry()
        .with(plain_layer)
        .with(rich_layer)
        .init();

    Ok(())
}

pub fn print_welcome() {
    let sep = style("◆").blue().bold();
    let title = style("Welcome to the Proofs-of-Autonomy CLI").bold();
    let subtitle = style("Create and verify cryptographic proofs of model conversations.").dim();

    info!(target: "plain", "\n{sep} {title} {sep}\n{subtitle}\n");
}

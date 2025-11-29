use anyhow::{Context, Result};
use dialoguer::console::style;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

#[tokio::main]
async fn main() -> Result<()> {
    init_logging().context("initializing logging")?;

    print_welcome();

    ai_passport::run().await
}

fn init_logging() -> anyhow::Result<()> {
    // plain layer (only target="plain")
    let plain_fmt = tracing_subscriber::fmt::format()
        .without_time()
        .with_level(false)
        .with_target(false)
        .compact();
    let plain_layer = tracing_subscriber::fmt::layer()
        .event_format(plain_fmt)
        .with_filter(Targets::new().with_target("plain", LevelFilter::TRACE));

    // build filter: use RUST_LOG if provided; otherwise default
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // sensible fallback; you can keep INFO for your crate here
        EnvFilter::new("warn,passport_for_ai=info")
    });

    let rich_layer = tracing_subscriber::fmt::layer().with_filter(filter);

    tracing_subscriber::registry()
        .with(plain_layer)
        .with(rich_layer)
        .init();

    Ok(())
}

fn print_welcome() {
    let sep = style("◆").blue().bold();
    let title = style("Welcome to the Proofs-of-Autonomy CLI").bold();
    let subtitle = style("Create and verify cryptographic proofs of model conversations.").dim();

    info!(target: "plain", "\n{sep} {title} {sep}\n{subtitle}\n");
}

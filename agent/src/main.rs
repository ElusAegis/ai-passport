#![feature(assert_matches)]

use crate::polymarket::agent_msg::build_polymarket_context;
use crate::polymarket::fetch::Market;
use crate::portfolio::fetch::fetch_current;
use crate::portfolio::price_feed::coingeko::CoingeckoProvider;
use crate::portfolio::price_feed::context::build_portfolio_context;
use crate::portfolio::price_feed::enrich::with_prices;
use crate::utils::logging::init_logging;
use crate::utils::notary_config::gen_cfg;
use ai_passport::{run_prove, with_input_source, VecInputSource};

mod decision;
mod polymarket;
mod portfolio;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    const KIB: usize = 1024;
    const LIMIT: usize = 16 * KIB;

    let markets = Market::get(3).await?;
    let polymarket_ctx = build_polymarket_context(&markets, 12 * KIB)?;
    println!("Polymarket context size: {} bytes", polymarket_ctx.len());

    let portfolio = fetch_current().await;
    let provider = CoingeckoProvider::new();

    // later: swap DummyPriceProvider for Coingecko/Binance impl
    let priced = with_prices(&portfolio, &provider).await?;

    let portfolio_ctx = build_portfolio_context(priced, 2 * KIB);
    println!("Portfolio context size: {} bytes", portfolio_ctx.len());

    let decision_json = decision::build_decision_request(&polymarket_ctx, &portfolio_ctx, LIMIT)?;
    println!("{decision_json}");
    println!("Decision request size: {} bytes", decision_json.len());

    let src = VecInputSource::new(vec![Some(decision_json), None]);
    let cfg = gen_cfg(LIMIT, 16 * KIB)?;

    with_input_source(src, async {
        let _ = run_prove(&cfg)
            .await
            .map_err(|e| anyhow::anyhow!("Prove failed: {}", e));
    })
    .await;

    Ok(())
}

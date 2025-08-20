#![feature(assert_matches)]

use crate::polymarket::agent_msg::build_polymarket_context;
use crate::polymarket::fetch::Market;
use crate::portfolio::fetch::fetch_current;
use crate::portfolio::price_feed::coingeko::CoingeckoProvider;
use crate::portfolio::price_feed::context::build_portfolio_context;
use crate::portfolio::price_feed::enrich::with_prices;
use crate::utils::logging::init_logging;

mod model;
mod polymarket;
mod portfolio;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    // This fetch does not use TLSN because the data is public and contains no sensitive information.
    // Instead, the notary can directly perform the request and sign off on the result,
    // which provides simpler and sufficient proof.
    let markets = Market::get(10).await?;
    println!("{}", build_polymarket_context(&markets, 16 * 1024)?);

    let portfolio = fetch_current().await;
    let provider = CoingeckoProvider::new();

    // later: swap DummyPriceProvider for Coingecko/Binance impl
    let priced = with_prices(&portfolio, &provider).await?;

    let ctx = build_portfolio_context(priced, 2048);
    println!("{ctx}");

    Ok(())
}

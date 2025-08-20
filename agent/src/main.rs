use crate::polymarket::agent_msg::build_polymarket_context;
use crate::polymarket::fetch::Market;
use crate::utils::logging::init_logging;

mod model;
mod polymarket;
mod portfolio;
mod price_feed;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    // This fetch does not use TLSN because the data is public and contains no sensitive information.
    // Instead, the notary can directly perform the request and sign off on the result,
    // which provides simpler and sufficient proof.
    let markets = Market::get(10).await?;

    // Print the extracted fields for all objects
    for (i, m) in markets.iter().enumerate() {
        println!("=== Market {i} ===");
        println!("id:              {}", m.id);
        println!("question:        {}", m.question.as_deref().unwrap_or(""));
        println!("slug:            {}", m.slug.as_deref().unwrap_or(""));
        println!("startDate:       {}", m.startDate.as_deref().unwrap_or(""));
        println!("endDate:         {}", m.endDate.as_deref().unwrap_or(""));
        println!(
            "liquidity:       {}",
            m.liquidity.map(|x| x.to_string()).as_deref().unwrap_or("")
        );
        println!(
            "volume:          {}",
            m.volume.map(|x| x.to_string()).as_deref().unwrap_or("")
        );
        println!("outcomes:        {:?}", m.outcomes);
        println!("outcomePrices:   {:?}", m.outcomePrices);
        println!(
            "description:     {}",
            m.description.as_deref().unwrap_or("")
        );
        println!();
    }
    //
    //     OBJECTIVE 1) Summarize sentiment; 2) Note notable signals & drivers; 3) Provide portfolio actions. Reply JSON:
    // {\"summary\":str,\"observations\":[{\"slug\":str,\"sentiment\":\"bullish|bearish|neutral\",\"why\":str,\"conf\":0..1}],\"portfolio\":str}

    println!("{}", build_polymarket_context(&markets, 16 * 1024)?);

    Ok(())
}

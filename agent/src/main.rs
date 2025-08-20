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


    let url = "https://gamma-api.polymarket.com/markets?limit=4&order=total-volume&ascending=false&active=true&closed=false&volume_num_min=50000";
    let client = Client::new();
    let resp = client
        .get(url)
        .header("accept", "application/json")
        .send()
        .await
        .with_context(|| "failed to send request")?
        .error_for_status()
        .with_context(|| "non-success status from Polymarket")?;

    // Read the full body as bytes
    let bytes = resp.bytes().await?;
    println!("Total response size: {} bytes", bytes.len());

    // The API returns a JSON array at the top level
    let markets: Vec<Market> = serde_json::from_slice(&bytes)
        .with_context(|| "failed to parse Polymarket response as Vec<Market>")?;

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

    Ok(())
}

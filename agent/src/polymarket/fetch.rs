use crate::utils::serialization::de_opt_f64;
use crate::utils::serialization::de_vec_string_flexible;
use anyhow::Context;
use reqwest::{Client, Url};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub(crate) struct Market {
    // Always present as string
    pub(crate) id: String,

    // These can be missing in some markets — make them Option
    pub(crate) question: Option<String>,
    pub(crate) slug: Option<String>,
    pub(crate) endDate: Option<String>,

    // Often numbers encoded as strings; accept number or string
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub(crate) liquidity: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    pub(crate) volume: Option<f64>,

    // These are sometimes stringified JSON arrays, sometimes proper arrays
    #[serde(default, deserialize_with = "de_vec_string_flexible")]
    pub(crate) outcomes: Vec<String>,
    #[serde(default, deserialize_with = "de_vec_string_flexible")]
    pub(crate) outcomePrices: Vec<String>,
}

impl Market {
    pub(crate) async fn get(limit: usize) -> anyhow::Result<Vec<Market>> {
        // Base URL
        let mut url =
            Url::parse("https://gamma-api.polymarket.com/markets").context("Invalid base URL")?;

        // Enumerate query parameters explicitly
        url.query_pairs_mut()
            .append_pair("limit", &limit.to_string())
            .append_pair("order", "total-volume")
            .append_pair("ascending", "false")
            .append_pair("active", "true")
            .append_pair("closed", "false")
            .append_pair("volume_num_min", "50000");

        let client = Client::new();

        // Build the request
        let resp = client
            .get(url)
            .header("accept", "application/json")
            .send()
            .await
            .context("Failed to send request")?
            .error_for_status()
            .context("Non-success status from Polymarket")?;

        // Get the http response body and deserialize it into a Vec<Market>
        let bytes = resp
            .bytes()
            .await
            .context("Failed to read response body")?
            .to_vec();

        // The API returns a JSON array at the top level
        let markets: Vec<Market> = serde_json::from_slice(&bytes)
            .context("Failed to parse Polymarket response as Vec<Market>")?;

        Ok(markets)
    }
}

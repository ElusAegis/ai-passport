use crate::utils::serialization::{de_opt_f64, de_vec_string_flexible};
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
    pub(crate) startDate: Option<String>,
    pub(crate) description: Option<String>,

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

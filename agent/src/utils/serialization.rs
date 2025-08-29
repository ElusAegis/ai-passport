use serde::{de, Deserialize, Deserializer};
use serde_json::Value;

pub(crate) fn de_opt_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(deserializer)?;
    match v {
        None => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(Value::String(s)) => {
            if s.trim().is_empty() {
                Ok(None)
            } else {
                s.parse::<f64>().ok().map(Some).ok_or_else(|| {
                    de::Error::custom(format!("could not parse f64 from string: {s}"))
                })
            }
        }
        Some(other) => Err(de::Error::custom(format!(
            "expected number or string, got: {other}"
        ))),
    }
}

/// Accepts:
/// - a JSON array of strings: ["Yes","No"]
/// - a **stringified** JSON array: "[\"Yes\",\"No\"]"
/// - a plain string: "Yes"  (we'll treat it as a single-element vector)
pub(crate) fn de_vec_string_flexible<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(deserializer)?;
    let Some(v) = v else { return Ok(vec![]) };

    match v {
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    Value::String(s) => out.push(s),
                    other => out.push(other.to_string()), // be forgiving
                }
            }
            Ok(out)
        }
        Value::String(s) => {
            // Try to parse the string as JSON array first
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&s) {
                return Ok(parsed);
            }
            // If it wasn’t a JSON array string, treat it as a single element
            Ok(vec![s])
        }
        other => Ok(vec![other.to_string()]),
    }
}

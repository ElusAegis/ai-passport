use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

pub fn build_decision_request(
    polymarket_ctx: &str,
    portfolio_ctx: &str,
    max_bytes: usize,
) -> Result<String> {
    // Parse input JSON strings
    let polymarket_val: Value = serde_json::from_str(polymarket_ctx)
        .map_err(|e| anyhow!("Failed to parse polymarket context JSON: {}", e))?;
    let portfolio_val: Value = serde_json::from_str(portfolio_ctx)
        .map_err(|e| anyhow!("Failed to parse portfolio context JSON: {}", e))?;

    // Constraints description (full, verbose)
    let full_constraints = vec![
        "Allowed actions:",
        "1. Only sell an asset X in favour of asset Y by Z% of the total holding of X.",
        "2. Z must be > 0 and <= 50.",
        "3. At most 5 such moves can be proposed.",
        "4. Symbols X and Y must exist in the portfolio context.",
        "5. The sum of all Z percentages must be <= 100.",
        "6. Numeric values must be formatted with up to 3 decimal places.",
        "7. No other actions or free text outside this schema is allowed.",
    ];

    // Constraints short (numbered, terse)
    let short_constraints = vec![
        "1. Sell X for Y by Z% (0<Z<=50).",
        "2. Max 5 moves.",
        "3. X,Y in portfolio.",
        "4. Sum Z <=100.",
        "5. Numeric ≤3 decimals.",
        "6. No other actions/text.",
    ];

    // Reply schema with guidance text inside (to be trimmed if needed)
    let reply_schema = json!({
        "summary": "string",
        "observations": [
            {
                "title": "string",
                "insight": "string"
            }
        ],
        "moves": [
            {
                "from": "string",
                "to": "string",
                "pct": 0.0
            }
        ]
    });

    // Build the envelope
    let mut envelope = json!({
        "role": "ai_agent",
        "kind": "decision_request",
        "constraints": full_constraints,
        "reply_schema": reply_schema,
        "contexts": {
            "polymarket": polymarket_val,
            "portfolio": portfolio_val,
        }
    });

    // Serialize compactly
    let mut serialized = serde_json::to_vec(&envelope)
        .map_err(|e| anyhow!("Failed to serialize envelope: {}", e))?;

    // If too large, trim observations guidance text inside reply_schema
    if serialized.len() > max_bytes {
        // Remove observations guidance: drop the "observations" key from reply_schema
        if let Some(reply_schema_obj) = envelope.get_mut("reply_schema") {
            if reply_schema_obj.is_object() {
                reply_schema_obj
                    .as_object_mut()
                    .unwrap()
                    .remove("observations");
            }
        }
        serialized = serde_json::to_vec(&envelope)?;
    }

    // If still too large, trim constraints verbiage to short
    if serialized.len() > max_bytes {
        envelope["constraints"] = json!(short_constraints);
        serialized = serde_json::to_vec(&envelope)?;
    }

    // // Final size check
    if serialized.len() > max_bytes {
        bail!("Decision request exceeds max_bytes after trimming");
    }

    // Return compact string
    Ok(String::from_utf8(serialized)?)
}

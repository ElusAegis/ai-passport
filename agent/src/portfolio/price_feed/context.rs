use crate::portfolio::price_feed::enrich::PricedPortfolio;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;

fn classify_symbol(sym: &str) -> &'static str {
    match sym.to_ascii_uppercase().as_str() {
        "BTC" | "ETH" | "SOL" | "LTC" | "ADA" | "DOT" | "BNB" | "XRP" => "crypto",
        "USDT" | "USDC" | "DAI" | "BUSD" | "TUSD" => "stable",
        "GOLD" | "XAU" | "PAXG" => "commodity",
        _ => "other",
    }
}

/// Build a compact context envelope for either **raw** Portfolio or **priced** Portfolio.
/// Picks the priced view if provided; otherwise falls back to raw.
pub fn build_portfolio_context(priced: PricedPortfolio, max_bytes: usize) -> String {
    // —— legend & hint (can be dropped if over budget) ——
    let lg = Some("Legend: pf=portfolio; pos=positions; sym=symbol; amt=amount(units); \
px=last price USD; val=position value USD; bs=basis_usd; cls=class crypto|stable|commodity|fiat|other; \
agg=aggregates; tv=total portfolio USD; cnt=position count.");
    let hint = Some("Context: CURRENT portfolio snapshot. This payload is combined with Polymarket and \
recent-updates elsewhere. Use your own world knowledge and price feeds if prices are missing. Do NOT execute trades here.");

    // —— aggregate by class ——
    let mut class_counts: HashMap<&str, usize> = HashMap::new();
    for p in &priced.positions {
        *class_counts.entry(classify_symbol(&p.symbol)).or_insert(0) += 1;
    }
    let classes = ["crypto", "stable", "commodity", "fiat", "other"];
    let by_cls = classes
        .iter()
        .map(|c| json!([c, class_counts.get(c).copied().unwrap_or(0)]))
        .collect::<Vec<_>>();

    // —— positions array (prefer priced view) ——
    let positions_json = json!(priced
        .positions
        .iter()
        .map(|p| {
            json!({
                "sym": p.symbol,
                "amt": p.amount,
                "px":  p.price_usd,
                "val": p.value_usd,
                "bs":  p.basis_usd,
                "cls": classify_symbol(&p.symbol)
            })
        })
        .collect::<Vec<_>>());

    // —— total value if priced ——
    let tv = priced.total_value_usd;

    // —— envelope ——
    #[derive(Serialize)]
    struct Snapshot<'a> {
        #[serde(rename = "source")]
        source: &'a str,
        #[serde(rename = "as_of_utc")]
        as_of: String,
        #[serde(rename = "scope")]
        scope: &'a str,
        #[serde(rename = "cnt")]
        cnt: usize,
    }
    let snapshot = Snapshot {
        source: "portfolio_module",
        as_of: Utc::now().to_rfc3339(),
        scope: "current",
        cnt: priced.positions.len(),
    };

    let mut env = json!({
        "role":"context",
        "kind":"portfolio_current",
        "lg": lg,
        "hint": hint,
        "snapshot": snapshot,
        "pf": {
            "pos": positions_json,
            "agg": { "by_cls": by_cls },
            "tv": tv
        }
    });

    // —— size control (drop in priority order) ——
    let mut s = serde_json::to_string(&env).unwrap();
    if s.len() > max_bytes {
        env["hint"] = json!(null);
        s = serde_json::to_string(&env).unwrap();
    }
    if s.len() > max_bytes {
        env["lg"] = json!(null);
        s = serde_json::to_string(&env).unwrap();
    }
    if s.len() > max_bytes {
        // drop basis
        if let Some(pos) = env["pf"]["pos"].as_array_mut() {
            for p in pos {
                p.as_object_mut().map(|o| o.remove("bs"));
            }
        }
        s = serde_json::to_string(&env).unwrap();
    }
    if s.len() > max_bytes {
        // drop agg
        env["pf"].as_object_mut().map(|o| {
            o.remove("agg");
        });
        s = serde_json::to_string(&env).unwrap();
    }
    if s.len() > max_bytes {
        // drop px/val fields (fall back to raw shape)
        if let Some(pos) = env["pf"]["pos"].as_array_mut() {
            for p in pos {
                if let Some(o) = p.as_object_mut() {
                    o.remove("px");
                    o.remove("val");
                }
            }
        }
        s = serde_json::to_string(&env).unwrap();
    }
    if s.len() > max_bytes {
        // drop tail positions, keep at least 1
        while s.len() > max_bytes {
            if let Some(pos) = env["pf"]["pos"].as_array_mut() {
                if pos.len() <= 1 {
                    break;
                }
                pos.pop();
                env["snapshot"]["cnt"] = json!(pos.len());
                s = serde_json::to_string(&env).unwrap();
            } else {
                break;
            }
        }
    }
    s
}

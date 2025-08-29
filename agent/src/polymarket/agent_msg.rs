// src/context/polymarket.rs
use crate::polymarket::fetch::Market;
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;

/// Compact envelope sent to the agent.
/// NOTE: Short keys to minimize tokens/bytes.
#[derive(Serialize)]
struct PolyCtx<'a> {
    /// Source + “largest markets” context
    s: &'a str, // "polymarket/top"
    asof: String, // ISO8601
    /// Tiny legend so the agent knows abbreviations
    lg: &'a str, // legend text
    /// Brief guidance: what the agent should do with this context
    hint: &'a str,
    /// Markets (already sorted by importance)
    m: Vec<PxMarket>,
}

#[derive(Serialize)]
struct PxMarket {
    id: String,         // market id (stable ref)
    sl: Option<String>, // short slug
    q: String,          // shortened question
    /// End timestamps
    e: Option<String>, // ISO end
    t: Option<i64>,     // seconds to end (can be negative if past-due)
    /// Liquidity (e) and Volume (v)
    #[serde(skip_serializing_if = "Option::is_none")]
    e_liq: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    v_vol: Option<f64>,
    /// Outcome prices: array of [label, price] pairs.
    /// Kept small (top-k by price) and numeric prices as f64.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    px: Vec<[serde_json::Value; 2]>, // ["Yes", 0.37], supports multi-outcome
    /// Optional coarse class (routing prior)
    #[serde(skip_serializing_if = "Option::is_none")]
    c: Option<String>,
}

fn secs_to_end(end_iso: Option<&str>, now: DateTime<Utc>) -> Option<i64> {
    end_iso
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .map(|end| (end - now).num_seconds())
}

fn shorten(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max).collect();
    if let Some(i) = t.rfind(' ') {
        t.truncate(i);
    }
    t.push('…');
    t
}

fn parse_price_pairs(
    outcomes: &[String],
    prices: &[String],
    top_k: usize,
) -> Vec<[serde_json::Value; 2]> {
    // Pair outcomes with numeric prices (best-effort parse), drop invalids,
    // then keep top_k by price desc.
    let mut pairs: Vec<(String, f64)> = outcomes
        .iter()
        .zip(prices.iter())
        .filter_map(|(o, p)| p.parse::<f64>().ok().map(|fp| (o.clone(), fp)))
        .collect();
    pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    pairs.truncate(top_k);
    pairs
        .into_iter()
        .map(|(o, v)| [serde_json::Value::String(o), serde_json::Value::from(v)])
        .collect()
}

fn classify(question: &str, slug: Option<&str>) -> Option<String> {
    let q = question.to_lowercase();
    let s = slug.unwrap_or("").to_lowercase();
    let has = |k: &str| q.contains(k) || s.contains(k);
    if [
        "bitcoin", "btc", "eth", "ethereum", "solana", "crypto", "altcoin",
    ]
    .iter()
    .any(|&k| has(k))
    {
        Some("crypto".into())
    } else if has("fed") || has("rate") || has("inflation") {
        Some("macro".into())
    } else if has("election") || has("president") || has("parliament") || has("putin") {
        Some("politics".into())
    } else if has("nfl") || has("nba") || has("match") || has("tournament") {
        Some("sports".into())
    } else {
        None
    }
}

/// Build a compact Polymarket context JSON string for the agent.
/// - `max_bytes`: hard cap; we drop least-important markets until it fits.
pub fn build_polymarket_context(markets: &[Market], max_bytes: usize) -> anyhow::Result<String> {
    let now = Utc::now();

    // Rank by importance: volume desc, then liquidity desc, then sooner end.
    let mut ranked: Vec<&Market> = markets.iter().collect();
    ranked.sort_by(|a, b| {
        let av = a.volume.unwrap_or(0.0);
        let bv = b.volume.unwrap_or(0.0);
        let al = a.liquidity.unwrap_or(0.0);
        let bl = b.liquidity.unwrap_or(0.0);
        let ae = a
            .endDate
            .as_deref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        let be = b
            .endDate
            .as_deref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        // Desc volume, desc liquidity, asc end date
        bv.partial_cmp(&av)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(bl.partial_cmp(&al).unwrap_or(std::cmp::Ordering::Equal))
            .then(ae.cmp(&be))
    });

    // Convert to compact payload
    let pxm: Vec<PxMarket> = ranked
        .iter()
        .map(|m| {
            let q_short = m
                .question
                .as_deref()
                .map(|q| shorten(q, 120))
                .unwrap_or_default();
            let px = parse_price_pairs(&m.outcomes, &m.outcomePrices, 3); // support multi-outcome
            PxMarket {
                id: m.id.clone(),
                sl: m.slug.clone(),
                q: q_short,
                e: m.endDate.clone(),
                t: secs_to_end(m.endDate.as_deref(), now),
                e_liq: m.liquidity,
                v_vol: m.volume,
                px,
                c: classify(m.question.as_deref().unwrap_or(""), m.slug.as_deref()),
            }
        })
        .collect();

    // Envelope with tiny legend & hint. No reply schema here.
    let legend = "Legend: s=source; asof=ISO time; lg=this legend; hint=what to do; m=markets; \
id=market id; sl=slug; q=question; e=end ISO; t=secs to end; e_liq=liquidity; v_vol=volume; \
px=[[outcome,price]…]; c=class. Source: Polymarket (largest markets).";
    let hint =
        "Objective: Read these Polymarket markets (largest by activity). Extract notable signals, \
drivers, and risks from world knowledge. This section is only the Polymarket context; \
portfolio and last-updates come separately.";

    let mut env = PolyCtx {
        s: "polymarket/top",
        asof: now.to_rfc3339(),
        lg: legend,
        hint,
        m: pxm,
    };

    // Serialize compactly; trim until we fit max_bytes
    // Strategy: remove tail markets (least important) and slightly shorten questions.
    let mut json = serde_json::to_string(&env).context("serialize polymarket context")?;
    if json.len() <= max_bytes {
        return Ok(json);
    }

    // First pass: drop markets until under cap.
    while env.m.len() > 1 && json.len() > max_bytes {
        env.m.pop(); // drop least-important (end of ranked list)
        json = serde_json::to_string(&env)?;
    }

    // Second pass (if still too big): aggressively shorten q to 80 chars and drop class.
    if json.len() > max_bytes {
        for mk in &mut env.m {
            mk.q = shorten(&mk.q, 80);
            mk.c = None;
        }
        json = serde_json::to_string(&env)?;
    }

    // Final safety: if still over, drop legend text (agent can still infer from keys).
    if json.len() > max_bytes {
        env.lg = "";
        json = serde_json::to_string(&env)?;
    }

    // As a last resort, drop px tails to top-2 outcomes.
    if json.len() > max_bytes {
        for mk in &mut env.m {
            if mk.px.len() > 2 {
                mk.px.truncate(2);
            }
        }
        json = serde_json::to_string(&env)?;
    }

    // If we’re still over, keep removing markets (we keep at least 1).
    while env.m.len() > 1 && json.len() > max_bytes {
        env.m.pop();
        json = serde_json::to_string(&env)?;
    }

    Ok(json)
}

//! "Point du jour" (`web/src/engine/brief.ts`): the day in a few lines for the user's radar and holdings — the market
//! climate (macro), what can be bought now (same rule as the notifications), the biggest moves since the last daily
//! close and the top story.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::js::fr;
use crate::types::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketLevel {
    Calm,
    Tense,
    High,
}

impl MarketLevel {
    /// `MARKET_LABEL[level]`.
    pub fn label(self) -> &'static str {
        match self {
            MarketLevel::Calm => "Contexte calme",
            MarketLevel::Tense => "Contexte tendu",
            MarketLevel::High => "Tension élevée",
        }
    }
}

/// `MARKET_LABEL`.
pub const MARKET_LABEL: [(MarketLevel, &str); 3] =
    [(MarketLevel::Calm, "Contexte calme"), (MarketLevel::Tense, "Contexte tendu"), (MarketLevel::High, "Tension élevée")];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mover {
    pub symbol: String,
    pub kind: Kind,
    pub price: f64,
    /// Change since the last daily close, in %.
    pub change: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefBuy {
    pub symbol: String,
    pub kind: Kind,
    pub strong: bool,
    pub title: String,
}

/// One price given to `movers` (`{ symbol, kind, price: number | null }`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceNow {
    pub symbol: String,
    pub kind: Kind,
    pub price: Option<f64>,
}

fn pct(v: f64) -> String {
    format!("{}{}\u{a0}%", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}

/// Moves since the last daily close, largest first (either direction); an asset without both prices is left out.
/// `previous_close` is keyed `"<kind>:<symbol>"`.
pub fn movers(prices: &[PriceNow], previous_close: &HashMap<String, Option<f64>>) -> Vec<Mover> {
    let mut out: Vec<Mover> = Vec::new();
    for p in prices {
        let prev = previous_close.get(&format!("{}:{}", p.kind.as_str(), p.symbol)).copied().flatten();
        let (Some(price), Some(prev)) = (p.price, prev) else { continue };
        let positive = |x: f64| x > 0.0;
        if !positive(price) || !positive(prev) {
            continue;
        }
        out.push(Mover { symbol: p.symbol.clone(), kind: p.kind, price, change: (price / prev - 1.0) * 100.0 });
    }
    out.sort_by(|a, b| b.change.abs().partial_cmp(&a.change.abs()).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// One sentence: "Contexte calme · 3 achetables (BTC, ETH, AAPL) · NVDA −3,4 %, SOL +2,8 %".
pub fn headline(level: Option<MarketLevel>, buyable: &[BriefBuy], moves: &[Mover]) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(l) = level {
        parts.push(l.label().into());
    }
    if buyable.is_empty() {
        parts.push("rien d'achetable pour l'instant".into());
    } else {
        let names = buyable.iter().take(3).map(|b| b.symbol.as_str()).collect::<Vec<_>>().join(", ") + if buyable.len() > 3 { "…" } else { "" };
        parts.push(format!("{} achetable{} ({names})", buyable.len(), if buyable.len() > 1 { "s" } else { "" }));
    }
    let big: Vec<&Mover> = moves.iter().filter(|m| m.change.abs() >= 1.0).take(2).collect();
    if !big.is_empty() {
        parts.push(big.iter().map(|m| format!("{} {}", m.symbol, pct(m.change))).collect::<Vec<_>>().join(", "));
    }
    let s = parts.join(" · ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

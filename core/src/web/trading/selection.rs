//! Pure part of the Sélection screen (web/src/webapp/Selection.tsx): the saved choices (budget with its currency,
//! market, holding duration), the `/api/selection` report as the client reads it, and the split of the budget.
use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use super::js_number;
use crate::engine::screener::{ByCriterion, Criterion, Edge, Horizon, RankRule};
use crate::js::{fr, number_to_string, round};
use crate::types::Kind;
use crate::web::money::{Currency, MoneyDisplay, NBSP};

pub const BUDGET_KEY: &str = "altim.selection.budget";
pub const MARKET_KEY: &str = "altim.selection.market";
pub const HORIZON_KEY: &str = "altim.selection.horizon";

/// Order of the criteria in "Pourquoi celle-ci ? Le détail".
pub const ORDER: [Criterion; 5] = [Criterion::Momentum, Criterion::Zone, Criterion::Trend, Criterion::Risk, Criterion::Signal];

/// `RANK_TEXT[rule]`.
pub fn rank_text(r: RankRule) -> &'static str {
    match r {
        RankRule::Signal => "signal technique d'Altim",
        RankRule::Momentum => "force relative, les plus en hausse d'abord",
        RankRule::Reversal => "rebond, les plus en baisse d'abord",
        RankRule::LowRisk => "les plus calmes d'abord",
    }
}

/// Saved holding duration (default one month).
pub fn read_horizon(raw: Option<&str>) -> Horizon {
    raw.and_then(Horizon::parse).unwrap_or(Horizon::Mo1)
}

/// Saved market (default stocks).
pub fn read_market(raw: Option<&str>) -> Kind {
    if raw == Some("crypto") { Kind::Crypto } else { Kind::Stock }
}

/// A budget and the currency it was typed in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    pub amount: f64,
    pub currency: Currency,
}

/// Saved budget and the currency it was typed in: `{ amount, currency }`, or a bare number (older versions: dollars).
pub fn parse_budget(raw: Option<&str>) -> Option<Budget> {
    let v: Value = serde_json::from_str(raw?).ok()?;
    if let Some(n) = v.as_f64().filter(|_| v.is_number()) {
        return (n.is_finite() && n > 0.0).then_some(Budget { amount: n, currency: Currency::Usd });
    }
    let amount = v.get("amount").filter(|a| a.is_number()).and_then(Value::as_f64).filter(|a| a.is_finite() && *a > 0.0)?;
    let currency = if v.get("currency").and_then(Value::as_str) == Some("EUR") { Currency::Eur } else { Currency::Usd };
    Some(Budget { amount, currency })
}

/// The saved form of a typed budget (`JSON.stringify({ amount, currency })`).
pub fn budget_json(amount: f64, currency: Currency) -> String {
    format!("{{\"amount\":{},\"currency\":\"{}\"}}", number_to_string(amount), currency.as_str())
}

/// The budget field: the saved amount converted to the display currency (as typed when no rate allows it), else
/// the cash of « Mes avoirs ». Returns the text and the currency it is in.
pub fn initial_budget(saved: Option<Budget>, cash_usd: f64, d: &MoneyDisplay) -> (String, Currency) {
    let cur = d.currency();
    if let Some(s) = saved {
        let v = d.convert(s.amount, s.currency, cur);
        return if v.is_finite() { (number_to_string(round(v)), cur) } else { (number_to_string(s.amount), s.currency) };
    }
    let cash = d.convert(cash_usd, Currency::Usd, cur);
    (if cash_usd > 0.0 && cash.is_finite() { number_to_string(round(cash)) } else { String::new() }, cur)
}

/// `Number(text.replace(/\s/g, "").replace(",", ".")) || 0`.
pub fn typed_budget(text: &str) -> f64 {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let v = js_number(&t.replacen(',', ".", 1));
    if v.is_nan() { 0.0 } else { v }
}

// ---------- /api/selection (only the fields the screen reads; optional ones default) ----------

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickPlan {
    pub entry: f64,
    #[serde(default)]
    pub limit: Option<f64>,
    pub stop: f64,
    pub target: f64,
    #[serde(default)]
    pub atr_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickTrack {
    pub trades: f64,
    pub win_rate: f64,
    pub avg_return: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PickCheck {
    pub label: String,
    pub ok: bool,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionCandidate {
    pub rank: usize,
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub sector: String,
    #[serde(default)]
    pub market_cap: f64,
    pub price: f64,
    pub scores: ByCriterion<f64>,
    pub why: ByCriterion<String>,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub zone_status: String,
    #[serde(default)]
    pub plan: Option<PickPlan>,
    #[serde(default)]
    pub track: Option<PickTrack>,
    #[serde(default)]
    pub checks: Vec<PickCheck>,
    /// "À surveiller": why it failed a check.
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SetAside {
    pub symbol: String,
    #[serde(default)]
    pub name: String,
    pub reason: String,
}

/// What the method would have given on the past (`Validation` of engine/screener.ts).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionValidation {
    pub periods: usize,
    pub top: f64,
    pub universe: f64,
    pub beat_rate: f64,
    #[serde(default)]
    pub from: Option<f64>,
    #[serde(default)]
    pub benchmark: Option<f64>,
    #[serde(default)]
    pub cost: f64,
    pub edge: Edge,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionReport {
    pub market: Kind,
    pub horizon: Horizon,
    pub as_of: f64,
    pub scanned: usize,
    pub rank_by: Criterion,
    pub rank_rule: RankRule,
    #[serde(default)]
    pub hold_text: String,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub market_closed: bool,
    pub criteria: ByCriterion<String>,
    pub roles: ByCriterion<String>,
    pub buy: Vec<SelectionCandidate>,
    #[serde(default)]
    pub watch: Vec<SelectionCandidate>,
    #[serde(default)]
    pub set_aside: Vec<SetAside>,
    #[serde(default)]
    pub validation: Option<SelectionValidation>,
}

/// `/api/selection?horizon=…&kind=…`.
pub fn selection_url(horizon: Horizon, market: Kind) -> String {
    format!("/api/selection?horizon={}&kind={}", horizon.as_str(), market.as_str())
}

/// Amount for each pick: the budget is split so that each line risks the same amount if its stop is hit (a volatile
/// one gets less money), and no line exceeds the maximum size set in Réglages. By symbol.
pub fn allocate(picks: &[SelectionCandidate], budget: f64, max_pct: f64) -> HashMap<String, f64> {
    let with_plan: Vec<(&SelectionCandidate, &PickPlan)> = picks.iter().filter_map(|p| p.plan.as_ref().map(|pl| (p, pl))).collect();
    let inv: Vec<f64> = with_plan.iter().map(|(_, pl)| 1.0 / super::js_max(0.5, (1.0 - pl.stop / pl.limit.unwrap_or(pl.entry)) * 100.0)).collect();
    let sum = inv.iter().fold(0.0, |a, b| a + b);
    let cap = (budget * max_pct) / 100.0;
    with_plan.iter().zip(&inv).map(|((p, _), i)| (p.symbol.clone(), super::js_min(cap, (budget * i) / sum))).collect()
}

/// `Number(x.toPrecision(6))`.
pub fn precision6(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    format!("{x:.5e}").parse().unwrap_or(x)
}

/// Shares (whole for a stock, 6 significant digits for a crypto) bought with `amount` at `buy_at`; 0 without amount.
pub fn pick_quantity(amount: Option<f64>, buy_at: f64, kind: Kind) -> f64 {
    match amount.filter(|a| *a != 0.0 && !a.is_nan()) {
        Some(a) if buy_at > 0.0 => {
            if kind == Kind::Stock {
                (a / buy_at).floor()
            } else {
                precision6(a / buy_at)
            }
        }
        _ => 0.0,
    }
}

/// "+2,5 %" (plain space, as on the screen).
pub fn pct(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}

/// Amounts of the screen: whole units from 100, cents below, in the display currency.
pub fn usd(d: &MoneyDisplay, v: f64) -> String {
    d.money_fmt(v, |x| fr(x, 0, if x >= 100.0 { 0 } else { 2 }), NBSP)
}

/// The rank criterion's short name on a card ("force", "rebond", "calme"…).
pub fn rank_label(rank_by: Criterion, rule: RankRule) -> &'static str {
    match rank_by {
        Criterion::Signal => "signal",
        Criterion::Momentum => {
            if rule == RankRule::Reversal {
                "rebond"
            } else {
                "force"
            }
        }
        Criterion::Risk => "calme",
        Criterion::Trend => "tendance",
        Criterion::Zone => "zone",
    }
}

/// The score shown next to the rank label (a reversal ranks the weakest momentum first: 100 − momentum).
pub fn rank_score(c: &SelectionCandidate, rank_by: Criterion, rule: RankRule) -> f64 {
    round(if rule == RankRule::Reversal { 100.0 - c.scores.momentum } else { *c.scores.get(rank_by) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::money::FxRate;

    fn eur() -> MoneyDisplay {
        MoneyDisplay::new(
            Currency::Eur,
            Some(FxRate { rate: 0.88, usd_per_eur: 1.0 / 0.88, time: 0.0, source: "Yahoo Finance".into(), fetched_at: 0.0, stale: false }),
        )
    }

    // money.test.ts "Selection budget"
    #[test]
    fn budget() {
        assert_eq!(parse_budget(Some("5000")), Some(Budget { amount: 5000.0, currency: Currency::Usd }));
        assert_eq!(parse_budget(Some(r#"{"amount":4000,"currency":"EUR"}"#)), Some(Budget { amount: 4000.0, currency: Currency::Eur }));
        assert_eq!(parse_budget(Some("-3")), None);
        assert_eq!(parse_budget(None), None);
        assert_eq!(parse_budget(Some(r#""5000""#)), None);
        assert_eq!(budget_json(4000.0, Currency::Eur), r#"{"amount":4000,"currency":"EUR"}"#);
    }

    #[test]
    fn initial_budget_in_the_display_currency() {
        let saved = Some(Budget { amount: 5000.0, currency: Currency::Usd });
        assert_eq!(initial_budget(saved, 0.0, &eur()), ("4400".into(), Currency::Eur));
        // No rate: kept as typed, in its own currency.
        let euros = Some(Budget { amount: 4000.0, currency: Currency::Eur });
        assert_eq!(initial_budget(euros, 0.0, &MoneyDisplay::usd()), ("4000".into(), Currency::Eur));
        assert_eq!(initial_budget(None, 1000.0, &eur()), ("880".into(), Currency::Eur));
        assert_eq!(initial_budget(None, 0.0, &eur()), (String::new(), Currency::Eur));
        assert_eq!(typed_budget("10 000,5"), 10_000.5);
        assert_eq!(typed_budget("abc"), 0.0);
        assert_eq!(typed_budget(""), 0.0);
    }

    #[test]
    fn saved_choices() {
        assert_eq!(read_horizon(Some("3m")), Horizon::Mo3);
        assert_eq!(read_horizon(Some("2y")), Horizon::Mo1);
        assert_eq!(read_market(Some("crypto")), Kind::Crypto);
        assert_eq!(read_market(None), Kind::Stock);
    }

    fn pick(symbol: &str, entry: f64, limit: Option<f64>, stop: f64) -> SelectionCandidate {
        serde_json::from_value(serde_json::json!({
            "rank": 1, "symbol": symbol, "name": symbol, "sector": "Tech", "marketCap": 1e9, "price": entry,
            "scores": { "signal": 50, "trend": 60, "momentum": 70, "zone": 40, "risk": 30 },
            "why": { "signal": "s", "trend": "t", "momentum": "m", "zone": "z", "risk": "r" },
            "action": "buy", "zoneStatus": "inZone",
            "plan": { "entry": entry, "limit": limit, "stop": stop, "target": entry * 1.2, "atrPct": 2 },
            "track": null, "checks": [],
        }))
        .unwrap()
    }

    #[test]
    fn allocation_by_risk_capped() {
        // Stops at −5 % and −10 %: the calmer line gets twice as much, each capped at 60 % of the budget.
        let picks = [pick("A", 100.0, None, 95.0), pick("B", 100.0, Some(100.0), 90.0)];
        let a = allocate(&picks, 1000.0, 60.0);
        assert!((a["A"] - 600.0).abs() < 1e-9, "{a:?}");
        assert!((a["B"] - 1000.0 / 3.0).abs() < 1e-9);
        assert_eq!(pick_quantity(Some(600.0), 95.0, Kind::Stock), 6.0);
        assert_eq!(pick_quantity(Some(100.0), 3.0, Kind::Crypto), 33.3333);
        assert_eq!(pick_quantity(None, 3.0, Kind::Crypto), 0.0);
        assert_eq!(pct(-2.345), "−2,3 %");
        assert_eq!(usd(&MoneyDisplay::usd(), 1234.5), "1\u{202f}235\u{a0}$");
        assert_eq!(rank_label(Criterion::Momentum, RankRule::Reversal), "rebond");
        assert_eq!(rank_score(&picks[0], Criterion::Momentum, RankRule::Reversal), 30.0);
    }
}

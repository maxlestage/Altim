//! Paper trading: a virtual portfolio that follows Altim's decisions with no real money, to check whether
//! "signal → exécution → résultat" holds. Everything stays on the device (like the holdings). Port of
//! web/src/engine/paper.ts: same rules, same JSON ("altim.paper.v1"), checked on the same reference scenario
//! (web/test/paper-fixture.json, shared with the iPhone and Android apps).
//!
//! Honest rules:
//! - every order pays fees (0,1 %) and slippage (0,05 %, against you) on each side, like the signal's track record;
//! - a position with a stop or a target is closed automatically when a daily candle AFTER the opening reaches it;
//!   a candle that reaches both counts as the stop (the worst case: the order inside the day is not known); a gap
//!   below the stop is filled at the open, a target at the target (never better than planned);
//! - the candle of the opening day is not used (its low may be earlier than the purchase): a stop or target
//!   reached that same day is only seen the next day;
//! - results are grouped by the decision shown at the purchase, to see which verdicts actually worked.
use std::collections::HashMap;
use std::ops::Deref;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{js_min, locale_cmp, positive, round_to};
use crate::types::Kind;
use crate::web::sorting::Sorting;

pub const FEE_RATE: f64 = 0.001;
pub const SLIPPAGE: f64 = 0.0005;
pub const DEFAULT_CAPITAL: f64 = 10_000.0;

/// `round(v, d = 8)` of paper.ts.
fn round(v: f64, d: i32) -> f64 {
    round_to(v, d)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaperReason {
    Stop,
    Target,
    Manual,
}

/// The decision shown when the position was opened (None: opened without one).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperDecision {
    pub verdict: String,
    pub label: String,
    pub confidence: f64,
    pub as_of: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperPosition {
    pub id: String,
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub opened_at: f64,
    /// Fill price (market price + slippage).
    pub entry: f64,
    pub quantity: f64,
    /// Amount taken from the cash, fees included.
    pub invested: f64,
    #[serde(default)]
    pub stop: Option<f64>,
    #[serde(default)]
    pub target: Option<f64>,
    #[serde(default)]
    pub decision: Option<PaperDecision>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperTrade {
    #[serde(flatten)]
    pub position: PaperPosition,
    pub closed_at: f64,
    /// Fill price (market price − slippage, or the stop / gap open).
    pub exit: f64,
    pub reason: PaperReason,
    /// Cash received, fees deducted.
    pub proceeds: f64,
    pub pnl: f64,
    pub pnl_pct: f64,
}

/// `PaperTrade` without its flattened `position`, read from the same JSON object (see `deserialize_flattened!`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaperTradeFields {
    closed_at: f64,
    exit: f64,
    reason: PaperReason,
    proceeds: f64,
    pnl: f64,
    pnl_pct: f64,
}

crate::web::json::deserialize_flattened!(PaperTrade, position, PaperTradeFields { closed_at, exit, reason, proceeds, pnl, pnl_pct });

impl Deref for PaperTrade {
    type Target = PaperPosition;
    fn deref(&self) -> &PaperPosition {
        &self.position
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperState {
    pub version: u32,
    pub start_capital: f64,
    pub cash: f64,
    #[serde(default)]
    pub started_at: f64,
    pub positions: Vec<PaperPosition>,
    pub trades: Vec<PaperTrade>,
}

pub fn new_paper(capital: f64, now: f64) -> PaperState {
    let c = if capital.is_finite() && capital > 0.0 { round(capital, 2) } else { DEFAULT_CAPITAL };
    PaperState { version: 1, start_capital: c, cash: c, started_at: now, positions: vec![], trades: vec![] }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOrder {
    pub id: String,
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    /// Market price now.
    pub price: f64,
    /// Amount to invest, fees included.
    pub amount: f64,
    #[serde(default)]
    pub stop: Option<f64>,
    #[serde(default)]
    pub target: Option<f64>,
    #[serde(default)]
    pub decision: Option<PaperDecision>,
}

/// Buys `amount` $ of the asset at `price` + slippage, fees deducted. Err: the French reason of the refusal (the
/// state is then unchanged).
pub fn open_position(s: &PaperState, o: &OpenOrder, now: f64) -> Result<PaperState, String> {
    if !positive(o.price) || !o.price.is_finite() {
        return Err("Prix indisponible.".into());
    }
    if !positive(o.amount) || !o.amount.is_finite() {
        return Err("Montant invalide.".into());
    }
    if o.amount > s.cash + 1e-9 {
        return Err(format!("Liquidités simulées insuffisantes ({} $ disponibles).", crate::js::number_to_string(round(s.cash, 2))));
    }
    let entry = o.price * (1.0 + SLIPPAGE);
    let stop = o.stop.filter(|v| *v > 0.0 && *v < entry);
    let target = o.target.filter(|v| *v > entry);
    let fee = o.amount * FEE_RATE;
    let position = PaperPosition {
        id: o.id.clone(),
        symbol: o.symbol.clone(),
        kind: o.kind,
        name: o.name.clone(),
        opened_at: now,
        entry: round(entry, 8),
        quantity: round((o.amount - fee) / entry, 10),
        invested: round(o.amount, 2),
        stop,
        target,
        decision: o.decision.clone(),
    };
    let mut next = s.clone();
    next.cash = round(s.cash - o.amount, 2);
    next.positions.push(position);
    Ok(next)
}

fn close(s: &PaperState, p: &PaperPosition, fill: f64, at: f64, reason: PaperReason) -> PaperState {
    let gross = p.quantity * fill;
    let proceeds = round(gross - gross * FEE_RATE, 2);
    let pnl = round(proceeds - p.invested, 2);
    let trade =
        PaperTrade { position: p.clone(), closed_at: at, exit: round(fill, 8), reason, proceeds, pnl, pnl_pct: round((pnl / p.invested) * 100.0, 4) };
    let mut next = s.clone();
    next.cash = round(s.cash + proceeds, 2);
    next.positions.retain(|x| x.id != p.id);
    next.trades.push(trade);
    next
}

/// Sells the whole position at the market price − slippage, fees deducted.
pub fn close_position(s: &PaperState, id: &str, price: f64, now: f64) -> Result<PaperState, String> {
    let Some(p) = s.positions.iter().find(|x| x.id == id) else { return Err("Position introuvable.".into()) };
    if !positive(price) || !price.is_finite() {
        return Err("Prix indisponible.".into());
    }
    Ok(close(s, p, price * (1.0 - SLIPPAGE), now, PaperReason::Manual))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DailyCandle {
    pub time: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// Closes the positions whose stop or target was reached by a daily candle that started after the opening
/// (candles by "kind:symbol", any order). Returns the new state and the trades just closed.
pub fn check_exits(s: &PaperState, candles: &HashMap<String, Vec<DailyCandle>>) -> (PaperState, Vec<PaperTrade>) {
    let mut state = s.clone();
    let mut closed = Vec::new();
    for p in &s.positions {
        if p.stop.is_none() && p.target.is_none() {
            continue;
        }
        let key = format!("{}:{}", p.kind.as_str(), p.symbol);
        let mut list: Vec<DailyCandle> = candles
            .get(&key)
            .map(|v| v.iter().filter(|c| c.time > p.opened_at && c.low > 0.0 && c.high >= c.low).copied().collect())
            .unwrap_or_default();
        list.sort_by_dyn(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        for c in &list {
            let hit = match (p.stop, p.target) {
                (Some(stop), _) if c.low <= stop => Some((js_min(stop, c.open) * (1.0 - SLIPPAGE), PaperReason::Stop)),
                (_, Some(target)) if c.high >= target => Some((target * (1.0 - SLIPPAGE), PaperReason::Target)),
                _ => None,
            };
            if let Some((fill, reason)) = hit {
                state = close(&state, p, fill, c.time, reason);
                closed.push(state.trades[state.trades.len() - 1].clone());
                break;
            }
        }
    }
    (state, closed)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenLine {
    #[serde(flatten)]
    pub position: PaperPosition,
    pub price: Option<f64>,
    pub value: Option<f64>,
    pub pnl: Option<f64>,
    pub pnl_pct: Option<f64>,
}

impl Deref for OpenLine {
    type Target = PaperPosition;
    fn deref(&self) -> &PaperPosition {
        &self.position
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperValuation {
    pub cash: f64,
    /// Value of the open positions at the given prices (sale fees and slippage deducted, as if sold now).
    pub positions_value: f64,
    pub equity: f64,
    pub pnl: f64,
    pub pnl_pct: f64,
    pub lines: Vec<OpenLine>,
    /// Positions without a price: valued at their cost.
    pub unpriced: usize,
}

/// Valuation at the prices of `price_of("kind:symbol")` (None or ≤ 0: no price, the position counts at its cost).
pub fn valuation(s: &PaperState, price_of: impl Fn(&str) -> Option<f64>) -> PaperValuation {
    let mut positions_value = 0.0;
    let mut unpriced = 0;
    let lines = s
        .positions
        .iter()
        .map(|p| {
            let price = price_of(&format!("{}:{}", p.kind.as_str(), p.symbol)).filter(|v| *v > 0.0);
            let Some(price) = price else {
                unpriced += 1;
                positions_value += p.invested;
                return OpenLine { position: p.clone(), price: None, value: None, pnl: None, pnl_pct: None };
            };
            let gross = p.quantity * price * (1.0 - SLIPPAGE);
            let value = round(gross - gross * FEE_RATE, 2);
            positions_value += value;
            let pnl = round(value - p.invested, 2);
            OpenLine {
                position: p.clone(),
                price: Some(price),
                value: Some(value),
                pnl: Some(pnl),
                pnl_pct: Some(round((pnl / p.invested) * 100.0, 4)),
            }
        })
        .collect();
    let equity = round(s.cash + positions_value, 2);
    let pnl = round(equity - s.start_capital, 2);
    PaperValuation {
        cash: s.cash,
        positions_value: round(positions_value, 2),
        equity,
        pnl,
        pnl_pct: round((pnl / s.start_capital) * 100.0, 4),
        lines,
        unpriced,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictStats {
    pub verdict: String,
    pub label: String,
    pub trades: usize,
    pub wins: usize,
    pub win_rate: f64,
    pub avg_pnl_pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ByReason {
    pub stop: usize,
    pub target: usize,
    pub manual: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperStats {
    pub trades: usize,
    pub wins: usize,
    pub win_rate: f64,
    pub avg_win_pct: Option<f64>,
    pub avg_loss_pct: Option<f64>,
    /// Sum of gains ÷ sum of losses (None without a losing trade).
    pub profit_factor: Option<f64>,
    pub realized_pnl: f64,
    pub best: Option<PaperTrade>,
    pub worst: Option<PaperTrade>,
    /// Worst fall of the realized capital (start + closed trades in order), %.
    pub max_drawdown_pct: f64,
    pub by_reason: ByReason,
    /// Results grouped by the decision shown at the purchase ("Sans décision" when opened without one).
    pub by_verdict: Vec<VerdictStats>,
}

fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| round(v.iter().fold(0.0, |a, b| a + b) / v.len() as f64, 4))
}

pub fn paper_stats(s: &PaperState) -> PaperStats {
    let mut t: Vec<&PaperTrade> = s.trades.iter().collect();
    t.sort_by_dyn(|a, b| a.closed_at.partial_cmp(&b.closed_at).unwrap_or(std::cmp::Ordering::Equal));
    let wins: Vec<&PaperTrade> = t.iter().copied().filter(|x| x.pnl > 0.0).collect();
    let losses: Vec<&PaperTrade> = t.iter().copied().filter(|x| x.pnl <= 0.0).collect();
    let gains = wins.iter().fold(0.0, |a, x| a + x.pnl);
    let lost = -losses.iter().fold(0.0, |a, x| a + x.pnl);
    let mut capital = s.start_capital;
    let mut peak = capital;
    let mut max_dd: f64 = 0.0;
    for x in &t {
        capital += x.pnl;
        peak = super::js_max(peak, capital);
        max_dd = js_min(max_dd, (capital / peak - 1.0) * 100.0);
    }
    // Groups in order of first appearance (a JavaScript Map).
    let mut groups: Vec<(String, Vec<&PaperTrade>)> = Vec::new();
    for x in &t {
        let k = x.decision.as_ref().map(|d| d.verdict.clone()).unwrap_or_else(|| "none".into());
        match groups.iter_mut().find(|(g, _)| *g == k) {
            Some((_, list)) => list.push(x),
            None => groups.push((k, vec![x])),
        }
    }
    let mut by_verdict: Vec<VerdictStats> = groups
        .into_iter()
        .map(|(verdict, list)| {
            let w = list.iter().filter(|x| x.pnl > 0.0).count();
            VerdictStats {
                verdict,
                label: list[0].decision.as_ref().map(|d| d.label.clone()).unwrap_or_else(|| "Sans décision".into()),
                trades: list.len(),
                wins: w,
                win_rate: round((w as f64 / list.len() as f64) * 100.0, 4),
                avg_pnl_pct: mean(&list.iter().map(|x| x.pnl_pct).collect::<Vec<_>>()).unwrap_or(f64::NAN),
            }
        })
        .collect();
    by_verdict.sort_by_dyn(|a, b| b.trades.cmp(&a.trades).then_with(|| locale_cmp(&a.verdict, &b.verdict)));
    let mut by_pct = t.clone();
    by_pct.sort_by_dyn(|a, b| b.pnl_pct.partial_cmp(&a.pnl_pct).unwrap_or(std::cmp::Ordering::Equal));
    let count = |r: PaperReason| t.iter().filter(|x| x.reason == r).count();
    PaperStats {
        trades: t.len(),
        wins: wins.len(),
        win_rate: if t.is_empty() { 0.0 } else { round((wins.len() as f64 / t.len() as f64) * 100.0, 4) },
        avg_win_pct: mean(&wins.iter().map(|x| x.pnl_pct).collect::<Vec<_>>()),
        avg_loss_pct: mean(&losses.iter().map(|x| x.pnl_pct).collect::<Vec<_>>()),
        profit_factor: (lost > 0.0).then(|| round(gains / lost, 4)),
        realized_pnl: round(t.iter().fold(0.0, |a, x| a + x.pnl), 2),
        best: by_pct.first().map(|x| (*x).clone()),
        worst: by_pct.last().map(|x| (*x).clone()),
        max_drawdown_pct: round(max_dd, 4),
        by_reason: ByReason { stop: count(PaperReason::Stop), target: count(PaperReason::Target), manual: count(PaperReason::Manual) },
        by_verdict,
    }
}

fn finite(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| x.is_finite())
}

/// Accepts only a well-formed saved state (localStorage / files can be edited by hand): `isPaperState`.
pub fn is_paper_state(v: &Value) -> bool {
    let Some(o) = v.as_object() else { return false };
    let positive = |p: &Value, k: &str| p.get(k).and_then(Value::as_f64).is_some_and(|x| x > 0.0);
    o.get("version").and_then(Value::as_f64) == Some(1.0)
        && finite(o.get("startCapital")).is_some_and(|c| c > 0.0)
        && finite(o.get("cash")).is_some()
        && o.get("trades").is_some_and(Value::is_array)
        && o.get("positions").and_then(Value::as_array).is_some_and(|ps| {
            ps.iter().all(|p| {
                p.get("id").is_some_and(Value::is_string)
                    && p.get("symbol").is_some_and(Value::is_string)
                    && positive(p, "quantity")
                    && positive(p, "entry")
                    && positive(p, "invested")
            })
        })
}

/// A saved state checked by `is_paper_state` and read (None when it fails either).
pub fn parse_paper_state(v: &Value) -> Option<PaperState> {
    if !is_paper_state(v) {
        return None;
    }
    crate::web::json::from_value(v).ok()
}

/// `JSON.stringify(state)`: integral numbers without ".0", the TypeScript key order.
pub fn paper_json(s: &PaperState) -> String {
    crate::js::to_value(s).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jstime::date_utc;

    fn t0() -> f64 {
        date_utc(2026.0, 8.0, 1.0, 14.0, 0.0)
    }

    fn day(d: f64) -> f64 {
        date_utc(2026.0, 8.0, d, 0.0, 0.0)
    }

    fn order(id: &str, symbol: &str, kind: Kind, price: f64, amount: f64, stop: Option<f64>, target: Option<f64>) -> OpenOrder {
        OpenOrder { id: id.into(), symbol: symbol.into(), kind, name: symbol.into(), price, amount, stop, target, decision: None }
    }

    fn candles(key: &str, list: Vec<DailyCandle>) -> HashMap<String, Vec<DailyCandle>> {
        HashMap::from([(key.to_string(), list)])
    }

    fn c(time: f64, open: f64, high: f64, low: f64, close: f64) -> DailyCandle {
        DailyCandle { time, open, high, low, close }
    }

    fn close_to(a: f64, b: f64, digits: i32) {
        assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≉ {b}");
    }

    // paper.test.ts
    #[test]
    fn buy_pays_fees_and_slippage() {
        let s = open_position(&new_paper(10_000.0, t0()), &order("a", "BTC", Kind::Crypto, 64_000.0, 3_000.0, Some(60_000.0), Some(72_000.0)), t0())
            .unwrap();
        let p = &s.positions[0];
        close_to(p.entry, 64_000.0 * (1.0 + SLIPPAGE), 6);
        close_to(p.quantity, (3_000.0 * (1.0 - FEE_RATE)) / (64_000.0 * 1.0005), 9);
        assert_eq!(s.cash, 7_000.0);
    }

    #[test]
    fn target_reached_the_next_day() {
        let s = open_position(&new_paper(10_000.0, t0()), &order("a", "BTC", Kind::Crypto, 64_000.0, 3_000.0, Some(60_000.0), Some(72_000.0)), t0())
            .unwrap();
        // The opening day (low 59 000 < stop) is not used: its low may be before the purchase.
        let (s, _) = check_exits(&s, &candles("crypto:BTC", vec![c(day(1.0), 63_000.0, 65_000.0, 59_000.0, 64_500.0)]));
        assert_eq!(s.positions.len(), 1);
        let (_, closed) = check_exits(&s, &candles("crypto:BTC", vec![c(day(3.0), 70_000.0, 73_000.0, 69_000.0, 72_500.0)]));
        let t = &closed[0];
        assert_eq!(t.reason, PaperReason::Target);
        close_to(t.exit, 72_000.0 * 0.9995, 6);
        let qty = (3_000.0 * 0.999) / (64_000.0 * 1.0005);
        close_to(t.proceeds, qty * 71_964.0 * 0.999, 2);
        assert_eq!(t.pnl, 364.89);
    }

    #[test]
    fn gap_below_stop_and_both_levels() {
        let s = open_position(&new_paper(10_000.0, t0()), &order("e", "ETH", Kind::Crypto, 2_500.0, 1_000.0, Some(2_300.0), Some(3_000.0)), t0())
            .unwrap();
        let (_, closed) = check_exits(&s, &candles("crypto:ETH", vec![c(day(2.0), 2_200.0, 2_250.0, 2_150.0, 2_210.0)]));
        close_to(closed[0].exit, 2_200.0 * 0.9995, 6);
        let s = open_position(&new_paper(10_000.0, t0()), &order("b", "BTC", Kind::Crypto, 70_000.0, 2_000.0, Some(66_000.0), Some(76_000.0)), t0())
            .unwrap();
        let (_, closed) = check_exits(&s, &candles("crypto:BTC", vec![c(day(2.0), 70_000.0, 77_000.0, 65_000.0, 71_000.0)]));
        assert_eq!(closed[0].reason, PaperReason::Stop);
    }

    #[test]
    fn refusals() {
        let s = new_paper(1_000.0, t0());
        let o = order("x", "AAPL", Kind::Stock, 250.0, 500.0, None, None);
        assert!(open_position(&s, &OpenOrder { amount: 1_500.0, ..o.clone() }, t0()).unwrap_err().contains("insuffisantes"));
        assert_eq!(open_position(&s, &OpenOrder { price: f64::NAN, ..o.clone() }, t0()).unwrap_err(), "Prix indisponible.");
        assert_eq!(open_position(&s, &OpenOrder { amount: -1.0, ..o.clone() }, t0()).unwrap_err(), "Montant invalide.");
        assert_eq!(open_position(&s, &OpenOrder { stop: Some(260.0), ..o.clone() }, t0()).unwrap().positions[0].stop, None);
        assert_eq!(close_position(&s, "nope", 10.0, t0()).unwrap_err(), "Position introuvable.");
    }

    #[test]
    fn valuation_as_if_sold_now() {
        let s = open_position(&new_paper(10_000.0, t0()), &order("a", "SOL", Kind::Crypto, 100.0, 1_000.0, None, None), t0()).unwrap();
        let v = valuation(&s, |k| (k == "crypto:SOL").then_some(110.0));
        let qty = 999.0 / 100.05;
        close_to(v.lines[0].value.unwrap(), qty * 110.0 * 0.9995 * 0.999, 2);
        close_to(v.equity, 9_000.0 + v.lines[0].value.unwrap(), 2);
        let none = valuation(&s, |_| None);
        assert_eq!(none.unpriced, 1);
        assert_eq!(none.equity, 10_000.0);
    }

    #[test]
    fn empty_stats() {
        let st = paper_stats(&new_paper(1_000.0, t0()));
        assert_eq!(st.trades, 0);
        assert_eq!(st.profit_factor, None);
        assert_eq!(st.max_drawdown_pct, 0.0);
    }

    #[test]
    fn saved_state_checked() {
        assert!(is_paper_state(&crate::js::to_value(&new_paper(5_000.0, t0()))));
        assert!(!is_paper_state(&serde_json::json!({ "version": 1, "startCapital": 1000, "cash": 1000, "positions": [{ "id": 1 }], "trades": [] })));
        assert!(!is_paper_state(&Value::Null));
    }

    /// Numbers compared as numbers (the fixture writes 10000, serde 10000.0), everything else structurally.
    fn same(a: &Value, b: &Value, path: &str) {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => assert_eq!(x.as_f64(), y.as_f64(), "{path}"),
            (Value::Array(x), Value::Array(y)) => {
                assert_eq!(x.len(), y.len(), "{path}");
                for (i, (x, y)) in x.iter().zip(y).enumerate() {
                    same(x, y, &format!("{path}[{i}]"));
                }
            }
            (Value::Object(x), Value::Object(y)) => {
                let (kx, ky): (Vec<_>, Vec<_>) = (x.keys().collect(), y.keys().collect());
                assert_eq!(kx, ky, "{path}");
                for (k, v) in x {
                    same(v, &y[k], &format!("{path}.{k}"));
                }
            }
            _ => assert_eq!(a, b, "{path}"),
        }
    }

    /// test/paper-scenario.ts replayed on the steps of test/paper-fixture.json: same states, errors, exits,
    /// valuations and statistics as the TypeScript (and the iPhone and Android ports).
    #[test]
    fn reference_scenario() {
        let fixture: Value = serde_json::from_str(include_str!("../../../../backend/tests/samples/paper-fixture.json")).unwrap();
        let mut s = new_paper(10_000.0, t0());
        for (i, row) in fixture.as_array().unwrap().iter().enumerate() {
            let step = &row["step"];
            let mut out = serde_json::Map::new();
            out.insert("step".into(), step.clone());
            match step["op"].as_str().unwrap() {
                "new" => s = new_paper(step["capital"].as_f64().unwrap(), step["now"].as_f64().unwrap()),
                "open" => {
                    let o: OpenOrder = serde_json::from_value(step["order"].clone()).unwrap();
                    match open_position(&s, &o, step["now"].as_f64().unwrap()) {
                        Ok(n) => s = n,
                        Err(e) => {
                            out.insert("error".into(), e.into());
                        }
                    }
                }
                "close" => match close_position(&s, step["id"].as_str().unwrap(), step["price"].as_f64().unwrap(), step["now"].as_f64().unwrap()) {
                    Ok(n) => s = n,
                    Err(e) => {
                        out.insert("error".into(), e.into());
                    }
                },
                "exits" => {
                    let candles: HashMap<String, Vec<DailyCandle>> = serde_json::from_value(step["candles"].clone()).unwrap();
                    let (n, closed) = check_exits(&s, &candles);
                    s = n;
                    out.insert("closed".into(), closed.iter().map(|t| Value::from(t.id.clone())).collect());
                }
                _ => {
                    let prices: HashMap<String, Option<f64>> = serde_json::from_value(step["prices"].clone()).unwrap();
                    out.insert("state".into(), crate::js::to_value(&s));
                    out.insert("valuation".into(), crate::js::to_value(&valuation(&s, |k| prices.get(k).copied().flatten())));
                }
            }
            out.insert("state".into(), crate::js::to_value(&s));
            out.insert("stats".into(), crate::js::to_value(&paper_stats(&s)));
            // Same keys in the fixture's order (step, error?, closed?, state, valuation?, stats).
            let mut ordered = serde_json::Map::new();
            for k in ["step", "error", "closed", "state", "valuation", "stats"] {
                if let Some(v) = out.remove(k) {
                    ordered.insert(k.into(), v);
                }
            }
            same(&Value::Object(ordered), row, &format!("step {i}"));
        }
        let last = fixture.as_array().unwrap().last().unwrap();
        assert_eq!(last["state"]["cash"].as_f64(), Some(10_509.12));
        assert_eq!(s.cash, 10_509.12);
        let st = paper_stats(&s);
        assert_eq!((st.trades, st.win_rate), (5, 40.0));
        assert_eq!(st.by_reason, ByReason { stop: 2, target: 2, manual: 1 });
        let v: Vec<(&str, usize)> = st.by_verdict.iter().map(|v| (v.verdict.as_str(), v.trades)).collect();
        assert_eq!(v, [("buy", 2), ("buyZone", 1), ("none", 1), ("wait", 1)]);
    }
}

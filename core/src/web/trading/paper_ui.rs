//! Pure helpers of the "Simulation" screens (paper trading), port of web/src/webapp/paper-ui.ts: reading the saved
//! state, default amounts, labels, notices, French formats. No browser access (the clock, the display currency and
//! the stored text are passed in). The rules themselves live in `paper` and are not changed here.
use chrono::{Datelike, TimeZone, Timelike};
use serde_json::Value;

use super::paper::{DEFAULT_CAPITAL, DailyCandle, PaperPosition, PaperReason, PaperState, PaperTrade, VerdictStats, parse_paper_state};
use super::{js_min, locale_cmp};
use crate::js::{fr, to_fixed};
use crate::types::{Candle, Kind};
use crate::web::money::MoneyDisplay;

pub const PAPER_KEY: &str = "altim.paper.v1";
/// Where unreadable saved data is kept aside the first time a new simulation overwrites it.
pub const PAPER_INVALID_KEY: &str = "altim.paper.v1.invalid";

/// Result of reading localStorage: a valid state, nothing saved yet, or unreadable data (ignored, with a message).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SavedPaper {
    pub state: Option<PaperState>,
    pub error: Option<String>,
}

pub const BAD_DATA_MESSAGE: &str = "Les données de simulation enregistrées dans ce navigateur sont illisibles ou abîmées : elles ont été ignorées. Recommencez une simulation pour repartir de zéro.";

pub fn parse_saved_paper(raw: Option<&str>) -> SavedPaper {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else { return SavedPaper::default() };
    match serde_json::from_str::<Value>(raw).ok().as_ref().and_then(parse_paper_state) {
        Some(s) => SavedPaper { state: Some(s), error: None },
        None => SavedPaper { state: None, error: Some(BAD_DATA_MESSAGE.into()) },
    }
}

/// French decimal input ("1 234,5", "1234.5", "10 000 $") → number, NaN when unreadable. Empty → None.
pub fn parse_amount(text: &str) -> Option<f64> {
    let t: String = text.chars().filter(|c| !(c.is_whitespace() || *c == '\u{feff}' || *c == '$' || *c == '€')).collect();
    let t = t.replacen(',', ".", 1);
    if t.is_empty() {
        return None;
    }
    // /^\d*\.?\d+$|^\d+\.$/
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let ok = match t.split_once('.') {
        None => digits(&t),
        Some((a, b)) => digits(a) && digits(b) && (!b.is_empty() || !a.is_empty()),
    };
    Some(if ok { t.parse().unwrap_or(f64::NAN) } else { f64::NAN })
}

/// Default amount of a simulated purchase: 10 % of the simulated value, never more than the cash (cents, rounded down).
pub fn default_amount(equity: f64, cash: f64) -> f64 {
    let v = js_min(equity * 0.1, cash);
    if v > 0.0 && v.is_finite() { (v * 100.0).floor() / 100.0 } else { 0.0 }
}

/// A starting capital typed by the user: positive and finite, else the default (10 000 $).
pub fn start_capital(text: &str) -> f64 {
    match parse_amount(text) {
        Some(v) if v.is_finite() && v > 0.0 => v,
        _ => DEFAULT_CAPITAL,
    }
}

/// A price put in an input: French decimal comma, no grouping, 2 decimals from 1 $ up, 6 significant digits below.
pub fn input_price(v: Option<f64>) -> String {
    let Some(v) = v.filter(|v| v.is_finite() && *v > 0.0) else { return String::new() };
    let digits = if v >= 1.0 { 2 } else { (5 - v.log10().floor() as i64).min(12) as usize };
    let s = to_fixed(v, digits);
    // .replace(/\.?0+$/, "")
    let s = if s.ends_with('0') {
        let t = s.trim_end_matches('0');
        t.strip_suffix('.').unwrap_or(t).to_string()
    } else {
        s
    };
    s.replacen('.', ",", 1)
}

/// Verdicts considered as "buy" by the decision: simulating another one is simulating against it.
pub const BUYING_VERDICTS: [&str; 2] = ["buy", "buyZone"];

pub fn against_decision(verdict: &str) -> bool {
    !BUYING_VERDICTS.contains(&verdict)
}

pub fn against_text(label: &str) -> String {
    format!("La décision actuelle est « {label} » : vous simulez contre elle.")
}

/// Warnings on the stop and target typed in the form, mirroring what `open_position` does with them
/// (a stop at or above the fill price, or a target at or below it, is dropped).
pub fn level_warnings(fill: f64, stop: Option<f64>, target: Option<f64>) -> Vec<&'static str> {
    let mut w = Vec::new();
    if let Some(s) = stop.filter(|s| s.is_finite()) {
        if s <= 0.0 || s >= fill {
            w.push("Le stop doit être sous le prix d'achat : il sera ignoré.");
        }
    }
    if let Some(t) = target.filter(|t| t.is_finite()) {
        if t <= fill {
            w.push("L'objectif doit être au-dessus du prix d'achat : il sera ignoré.");
        }
    }
    w
}

/// `REASON_LABEL`.
pub fn reason_label(r: PaperReason) -> &'static str {
    match r {
        PaperReason::Target => "objectif atteint",
        PaperReason::Stop => "stop touché",
        PaperReason::Manual => "vente manuelle",
    }
}

const MONTHS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];

/// Candle day (UTC midnight for daily candles) or moment (Paris time), in French: "3 septembre 2026",
/// "3 septembre 2026 à 14:05" with the time.
pub fn fr_date(ms: f64, with_time: bool) -> String {
    let Some(utc) = chrono::DateTime::from_timestamp_millis(ms as i64).filter(|_| ms.is_finite()) else { return "Invalid Date".into() };
    let day = ms % 86_400_000.0 == 0.0;
    let fmt = |d: u32, m: u32, y: i32, time: Option<(u32, u32)>| {
        let head = format!("{d} {} {y}", MONTHS[(m - 1) as usize]);
        match time {
            Some((h, min)) => format!("{head} à {h:02}:{min:02}"),
            None => head,
        }
    };
    if day {
        return fmt(utc.day(), utc.month(), utc.year(), None);
    }
    let p = chrono_tz::Europe::Paris.from_utc_datetime(&utc.naive_utc());
    fmt(p.day(), p.month(), p.year(), with_time.then(|| (p.hour(), p.minute())))
}

/// Notice for a position just closed by `check_exits`: "Stop touché le 3 septembre 2026 : Bitcoin (BTC), −212,40 $ (−2,12 %).".
pub fn exit_notice(t: &PaperTrade, d: &MoneyDisplay) -> String {
    let head = match t.reason {
        PaperReason::Stop => "Stop touché",
        PaperReason::Target => "Objectif atteint",
        PaperReason::Manual => "Vente manuelle",
    };
    format!("{head} le {} : {} ({}), {} ({}).", fr_date(t.closed_at, false), t.name, t.symbol, signed_usd(d, t.pnl), signed_pct(t.pnl_pct, 2))
}

/// The open positions whose exits must be checked (a stop or a target), one entry per asset, first seen first.
pub fn assets_to_check(positions: &[PaperPosition]) -> Vec<(String, Kind)> {
    let mut out: Vec<(String, Kind)> = Vec::new();
    for p in positions.iter().filter(|p| p.stop.is_some() || p.target.is_some()) {
        if !out.iter().any(|(s, k)| *s == p.symbol && *k == p.kind) {
            out.push((p.symbol.clone(), p.kind));
        }
    }
    out
}

/// Server candles → the engine's daily candles, bad rows dropped.
pub fn to_daily(candles: &[Candle]) -> Vec<DailyCandle> {
    candles
        .iter()
        .filter(|c| [c.open, c.high, c.low, c.close].iter().all(|v| v.is_finite()))
        .map(|c| DailyCandle { time: c.time as f64, open: c.open, high: c.high, low: c.low, close: c.close })
        .collect()
}

/// Journal order: most recent exit first.
pub fn journal(trades: &[PaperTrade]) -> Vec<PaperTrade> {
    let mut v = trades.to_vec();
    v.sort_by(|a, b| {
        let by = |x: f64, y: f64| y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal);
        by(a.closed_at, b.closed_at).then_with(|| by(a.opened_at, b.opened_at))
    });
    v
}

const VERDICT_ORDER: [&str; 6] = ["buy", "buyZone", "wait", "noPosition", "trim", "sell"];

/// "Résultats par décision" rows in the order of the decision scale, "Sans décision" last.
pub fn verdict_rows(rows: &[VerdictStats]) -> Vec<VerdictStats> {
    let rank = |v: &str| {
        if v == "none" { 99 } else { VERDICT_ORDER.iter().position(|x| *x == v).unwrap_or(50) }
    };
    let mut v = rows.to_vec();
    v.sort_by(|a, b| rank(&a.verdict).cmp(&rank(&b.verdict)).then_with(|| locale_cmp(&a.verdict, &b.verdict)));
    v
}

/// Under this many closed trades the statistics mean little.
pub const FEW_TRADES: usize = 20;
pub const FEW_TRADES_NOTE: &str = "Sous une vingtaine de trades clôturés, ces chiffres veulent dire peu : quelques trades chanceux ou malchanceux suffisent à les renverser. Ils deviennent parlants avec le temps.";

// ---------- French formatting ----------

pub const NNBSP: &str = "\u{202f}";

/// Dollar amounts of the simulation, shown in the display currency.
pub fn usd(d: &MoneyDisplay, v: f64) -> String {
    d.money(v, 2, 2, NNBSP)
}

/// Prices: more decimals for small values (0,000012 €).
pub fn price(d: &MoneyDisplay, v: f64) -> String {
    let x = d.to_display(v);
    let (min, max) = if x >= 1.0 { (2, 2) } else { (4, 8) };
    format!("{}{NNBSP}{}", fr(x, min, max), d.symbol())
}

fn sign(v: f64) -> &'static str {
    if v > 0.0 {
        "+"
    } else if v < 0.0 {
        "−"
    } else {
        ""
    }
}

pub fn signed_usd(d: &MoneyDisplay, v: f64) -> String {
    format!("{}{}", sign(v), usd(d, v.abs()))
}

pub fn signed_pct(v: f64, digits: usize) -> String {
    format!("{}{}{NNBSP}%", sign(v), fr(v.abs(), digits, digits))
}

pub fn pct(v: f64, digits: usize) -> String {
    format!("{}{NNBSP}%", fr(v, digits, digits))
}

pub fn qty(v: f64) -> String {
    fr(v, 0, if v >= 1.0 { 6 } else { 10 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js::to_value;
    use crate::jstime::date_utc;
    use crate::web::trading::paper::{OpenOrder, check_exits, new_paper, open_position, paper_stats};
    use std::collections::HashMap;

    const NB: &str = "\u{202f}";

    fn t0() -> f64 {
        date_utc(2026.0, 8.0, 1.0, 14.0, 0.0)
    }

    fn order(id: &str, symbol: &str, name: &str, price: f64, amount: f64, stop: Option<f64>, target: Option<f64>) -> OpenOrder {
        OpenOrder { id: id.into(), symbol: symbol.into(), kind: Kind::Crypto, name: name.into(), price, amount, stop, target, decision: None }
    }

    // paper-ui.test.ts
    #[test]
    fn saved_paper() {
        assert_eq!(parse_saved_paper(None), SavedPaper::default());
        assert_eq!(parse_saved_paper(Some("")), SavedPaper::default());
        let s = new_paper(5_000.0, t0());
        assert_eq!(parse_saved_paper(Some(&to_value(&s).to_string())), SavedPaper { state: Some(s), error: None });
        let mut bad_pos = to_value(&new_paper(1000.0, t0()));
        bad_pos["positions"] = serde_json::json!([{ "id": 1 }]);
        let mut bad_cash = to_value(&new_paper(1000.0, t0()));
        bad_cash["cash"] = "x".into();
        for raw in ["{".to_string(), "null".into(), "42".into(), r#"{"version":2}"#.into(), bad_pos.to_string(), bad_cash.to_string()] {
            assert_eq!(parse_saved_paper(Some(&raw)), SavedPaper { state: None, error: Some(BAD_DATA_MESSAGE.into()) }, "{raw}");
        }
    }

    #[test]
    fn amounts() {
        assert_eq!(parse_amount("1 234,5"), Some(1234.5));
        assert_eq!(parse_amount("10 000 $"), Some(10_000.0));
        assert_eq!(parse_amount("0.25"), Some(0.25));
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_amount("  "), None);
        assert!(parse_amount("abc").unwrap().is_nan());
        assert!(parse_amount("1,2,3").unwrap().is_nan());
        assert!(parse_amount("-5").unwrap().is_nan());
        assert_eq!(parse_amount("5."), Some(5.0));
        assert_eq!(parse_amount(",5"), Some(0.5));
        assert!(parse_amount(".").unwrap().is_nan());
        // What the holdings tools type (What-if amounts and shocks).
        assert_eq!(parse_amount("1 000,50 €"), Some(1000.5));
        assert_eq!(parse_amount("\u{feff}15"), Some(15.0));
        assert!(parse_amount("1e3").unwrap().is_nan());

        assert_eq!(default_amount(10_000.0, 10_000.0), 1_000.0);
        assert_eq!(default_amount(10_000.0, 400.0), 400.0);
        assert_eq!(default_amount(12_345.678, 9_000.0), 1_234.56);
        assert_eq!(default_amount(10_000.0, 0.0), 0.0);
        assert_eq!(default_amount(f64::NAN, 100.0), 0.0);

        assert_eq!(input_price(Some(63575.19787210)), "63575,2");
        assert_eq!(input_price(Some(1000.0)), "1000");
        assert_eq!(input_price(Some(0.000012345678)), "0,0000123457");
        assert_eq!(input_price(Some(0.5)), "0,5");
        assert_eq!(input_price(None), "");
        assert_eq!(input_price(Some(0.0)), "");
        for v in [63575.2, 0.0000123457, 12.34] {
            assert!((parse_amount(&input_price(Some(v))).unwrap() - v).abs() < 5e-11);
        }

        assert_eq!(start_capital("25 000"), 25_000.0);
        assert_eq!(start_capital("0"), 10_000.0);
        assert_eq!(start_capital("abc"), 10_000.0);
        assert_eq!(start_capital(""), 10_000.0);
    }

    #[test]
    fn decision_and_levels() {
        assert!(!against_decision("buy"));
        assert!(!against_decision("buyZone"));
        for v in ["wait", "noPosition", "trim", "sell"] {
            assert!(against_decision(v));
        }
        assert_eq!(against_text("ATTENDRE"), "La décision actuelle est « ATTENDRE » : vous simulez contre elle.");
        let fill = 100.0 * 1.0005;
        assert!(level_warnings(fill, Some(90.0), Some(120.0)).is_empty());
        assert!(level_warnings(fill, None, None).is_empty());
        assert_eq!(level_warnings(fill, Some(101.0), Some(100.0)).len(), 2);
        // Same verdict as the engine: a stop at or above the fill is dropped, a target at or below too.
        let s = open_position(&new_paper(10_000.0, t0()), &order("a", "X", "X", 100.0, 100.0, Some(101.0), Some(100.0)), t0()).unwrap();
        assert_eq!(s.positions[0].stop, None);
        assert_eq!(s.positions[0].target, None);
    }

    fn base() -> PaperState {
        let s = open_position(&new_paper(10_000.0, t0()), &order("p1", "BTC", "Bitcoin", 64_000.0, 1_000.0, Some(60_000.0), Some(72_000.0)), t0())
            .unwrap();
        open_position(&s, &order("p2", "SOL", "Solana", 150.0, 500.0, None, None), t0()).unwrap()
    }

    #[test]
    fn exits_and_journal() {
        let base = base();
        assert_eq!(assets_to_check(&base.positions), vec![("BTC".to_string(), Kind::Crypto)]);

        let rows = [
            Candle { time: 1, open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: 9.0 },
            Candle { time: 2, open: f64::NAN, high: 1.0, low: 1.0, close: 1.0, volume: 0.0 },
        ];
        assert_eq!(to_daily(&rows), vec![DailyCandle { time: 1.0, open: 1.0, high: 2.0, low: 0.5, close: 1.5 }]);

        let day = date_utc(2026.0, 8.0, 3.0, 0.0, 0.0);
        let candles = HashMap::from([(
            "crypto:BTC".to_string(),
            vec![DailyCandle { time: day, open: 63_000.0, high: 63_500.0, low: 59_000.0, close: 61_000.0 }],
        )]);
        let (_, closed) = check_exits(&base, &candles);
        assert_eq!(closed.len(), 1);
        let text = exit_notice(&closed[0], &MoneyDisplay::usd());
        assert!(text.starts_with("Stop touché le 3 septembre 2026 : Bitcoin (BTC), −"), "{text}");
        assert!(text.contains(&format!("{NB}%")));

        let t = |id: &str, closed_at: f64| {
            let mut x = closed[0].clone();
            x.position.id = id.into();
            x.closed_at = closed_at;
            x.position.opened_at = 0.0;
            x
        };
        let ids: Vec<String> = journal(&[t("a", 1.0), t("b", 3.0), t("c", 2.0)]).iter().map(|x| x.id.clone()).collect();
        assert_eq!(ids, ["b", "c", "a"]);

        let row =
            |verdict: &str| VerdictStats { verdict: verdict.into(), label: verdict.into(), trades: 1, wins: 0, win_rate: 0.0, avg_pnl_pct: 0.0 };
        let order: Vec<String> =
            verdict_rows(&[row("none"), row("wait"), row("sell"), row("buy"), row("buyZone")]).into_iter().map(|r| r.verdict).collect();
        assert_eq!(order, ["buy", "buyZone", "wait", "sell", "none"]);
        assert!(verdict_rows(&paper_stats(&base).by_verdict).is_empty());
    }

    #[test]
    fn formatting() {
        let d = MoneyDisplay::usd();
        assert_eq!(usd(&d, 1234.5), format!("1{NB}234,50{NB}$"));
        assert_eq!(signed_usd(&d, -12.3), format!("−12,30{NB}$"));
        assert_eq!(signed_usd(&d, 5.0), format!("+5,00{NB}$"));
        assert_eq!(signed_usd(&d, 0.0), format!("0,00{NB}$"));
        assert_eq!(signed_pct(-1.234, 2), format!("−1,23{NB}%"));
        assert_eq!(fr_date(date_utc(2026.0, 8.0, 3.0, 0.0, 0.0), false), "3 septembre 2026");
        // A moment: Paris time (summer: UTC+2), with the time when asked.
        assert_eq!(fr_date(date_utc(2026.0, 8.0, 3.0, 12.0, 5.0), true), "3 septembre 2026 à 14:05");
        assert_eq!(fr_date(date_utc(2026.0, 11.0, 31.0, 23.0, 30.0), false), "1 janvier 2027");
        assert_eq!(price(&d, 0.000012), format!("0,000012{NB}$"));
        assert_eq!(qty(0.0468), "0,0468");
        assert_eq!(pct(40.0, 0), format!("40{NB}%"));
    }
}

//! Query validation, same rules and messages as app.ts (a bad value is a 400, never an upstream call).
use std::sync::LazyLock;

use regex::Regex;

use super::error::{ApiResult, bad};
use crate::types::{Asset, Interval, Kind};

static CRYPTO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z0-9]{1,12}$").unwrap());
static STOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9.\-]{0,9}$").unwrap());

/// Absent or "crypto" → crypto, "stock" → stock.
pub fn parse_kind(v: Option<&str>) -> ApiResult<Kind> {
    match v {
        None | Some("crypto") => Ok(Kind::Crypto),
        Some("stock") => Ok(Kind::Stock),
        _ => bad("kind invalide (crypto | stock)"),
    }
}

/// `String(v ?? "").toUpperCase().trim()` then the pattern of the kind.
pub fn parse_symbol(v: Option<&str>, kind: Kind) -> ApiResult<String> {
    let s = v.unwrap_or("").to_uppercase();
    let s = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}').to_string();
    let ok = match kind {
        Kind::Crypto => CRYPTO.is_match(&s),
        Kind::Stock => STOCK.is_match(&s),
    };
    if ok { Ok(s) } else { bad("symbole invalide") }
}

pub fn parse_interval(v: Option<&str>) -> ApiResult<Interval> {
    v.and_then(Interval::parse).map_or_else(|| bad("interval invalide (1h | 4h | 1d | 4d | 1w)"), Ok)
}

/// Assets of one request or subscription at most (`/api/live`, `/api/alerts`, `/api/ws`).
pub const MAX_ASSETS: usize = 20;

/// "BTC:crypto,AAPL:stock" → list of assets (20 max); absent or empty → the default list.
pub fn parse_assets(v: Option<&str>, default: &[Asset], make: impl Fn(&str, Kind) -> Asset) -> ApiResult<Vec<Asset>> {
    let Some(v) = v.filter(|s| !s.is_empty()) else { return Ok(default.to_vec()) };
    v.split(',')
        .take(MAX_ASSETS)
        .map(|item| {
            let mut parts = item.split(':');
            let sym = parts.next();
            let kind = parse_kind(Some(parts.next().unwrap_or("crypto")))?;
            Ok(make(&parse_symbol(sym, kind)?, kind))
        })
        .collect()
}

/// The user's average cost (`cost=`): absent or empty → None, otherwise a positive finite number.
pub fn parse_cost(v: Option<&str>) -> ApiResult<Option<f64>> {
    let Some(v) = v.filter(|s| !s.trim().is_empty()) else { return Ok(None) };
    let n = js_number(v);
    if n.is_finite() && n > 0.0 { Ok(Some(n)) } else { bad("cost invalide (prix d'achat moyen : nombre positif)") }
}

/// Currency of the texts (`cur=EUR` by default, `cur=USD` for a client showing dollars): true for dollars.
pub fn parse_currency(v: Option<&str>) -> ApiResult<bool> {
    match v {
        None | Some("") | Some("EUR") => Ok(false),
        Some("USD") => Ok(true),
        _ => bad("cur invalide (EUR | USD)"),
    }
}

/// Weights of the composite score (`w=tech:32,mom:18,…`): integers 0 – 100, total above 0 (see `synthesis`).
pub fn parse_score_weights(v: Option<&str>) -> ApiResult<Option<crate::engine::synthesis::ScoreWeights>> {
    crate::engine::synthesis::parse_weights(v).or_else(bad)
}

/// Maximum number of lines in `weights=`.
pub const MAX_WEIGHTS: usize = 20;

/// The user's weights (`weights=BTC:crypto:40,AAPL:stock:25`): 20 lines at most, each weight 0 – 100 %, 100 % in
/// total at most (1 point of rounding tolerated). Absent or empty → none.
pub fn parse_weights(v: Option<&str>) -> ApiResult<Vec<super::data::Weight>> {
    let Some(v) = v.filter(|s| !s.trim().is_empty()) else { return Ok(vec![]) };
    let items: Vec<&str> = v.split(',').collect();
    if items.len() > MAX_WEIGHTS {
        return bad(format!("weights invalide : {MAX_WEIGHTS} lignes au plus"));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let parts: Vec<&str> = item.split(':').collect();
        let [sym, kind, pct] = parts.as_slice() else {
            return bad("weights invalide (SYMBOLE:crypto|stock:pourcentage, séparés par des virgules)");
        };
        let kind = parse_kind(Some(kind))?;
        let symbol = parse_symbol(Some(sym), kind)?;
        let raw = pct.trim();
        let pct = js_number(raw);
        if raw.is_empty() || !(pct.is_finite() && (0.0..=100.0).contains(&pct)) {
            return bad("weights invalide : chaque pondération entre 0 et 100 %");
        }
        out.push(super::data::Weight { symbol, kind, pct });
    }
    if out.iter().map(|w| w.pct).sum::<f64>() > 101.0 {
        return bad("weights invalide : total supérieur à 100 %");
    }
    Ok(out)
}

/// Number of days of a window (`days=`): absent or empty → `default`, otherwise an integer from 1 to `max`.
pub fn parse_days(v: Option<&str>, default: u32, max: u32) -> ApiResult<u32> {
    let Some(v) = v.filter(|s| !s.trim().is_empty()) else { return Ok(default) };
    let n = js_number(v);
    if n.is_finite() && n.fract() == 0.0 && n >= 1.0 && n <= f64::from(max) { Ok(n as u32) } else { bad(format!("days invalide (1 à {max})")) }
}

/// Maximum number of symbols in `symbols=` of the calendar.
pub const MAX_SYMBOLS: usize = 50;

/// Stock symbols (`symbols=AAPL,NVDA`, a ":stock" suffix accepted, ":crypto" entries ignored: no company events).
/// Absent or empty → None (no filter).
pub fn parse_symbol_list(v: Option<&str>) -> ApiResult<Option<Vec<String>>> {
    let Some(v) = v.filter(|s| !s.trim().is_empty()) else { return Ok(None) };
    let items: Vec<&str> = v.split(',').filter(|s| !s.trim().is_empty()).collect();
    if items.len() > MAX_SYMBOLS {
        return bad(format!("symbols invalide : {MAX_SYMBOLS} au plus"));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let (sym, kind) = item.split_once(':').unwrap_or((item, "stock"));
        match parse_kind(Some(kind.trim()))? {
            Kind::Crypto => continue,
            Kind::Stock => out.push(parse_symbol(Some(sym), Kind::Stock)?),
        }
    }
    Ok(Some(out))
}

/// `Math.floor(Number(v) || fallback)` for the paging parameters.
pub fn int_or(v: Option<&str>, fallback: f64) -> f64 {
    let n = v.map(js_number).unwrap_or(f64::NAN);
    let n = if n.is_nan() || n == 0.0 { fallback } else { n };
    n.floor()
}

/// `Number(string)`: trimmed, "" → 0, invalid → NaN (hex and Infinity accepted like JavaScript).
pub fn js_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(h, 16).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if t.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E') {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert_eq!(parse_kind(None).unwrap(), Kind::Crypto);
        assert!(parse_kind(Some("forex")).is_err());
        assert_eq!(parse_symbol(Some(" btc "), Kind::Crypto).unwrap(), "BTC");
        assert!(parse_symbol(Some("../x"), Kind::Crypto).is_err());
        assert_eq!(parse_symbol(Some("brk-b"), Kind::Stock).unwrap(), "BRK-B");
        assert!(parse_symbol(Some("1ABC"), Kind::Stock).is_err());
        assert!(parse_interval(Some("5m")).is_err());
        assert_eq!(int_or(Some("abc"), 20.0), 20.0);
        assert_eq!(int_or(Some("7.9"), 20.0), 7.0);
        assert_eq!(js_number(" 12 "), 12.0);
        assert!(js_number("12px").is_nan());
        assert_eq!(parse_days(None, 14, 30).unwrap(), 14);
        assert_eq!(parse_days(Some("30"), 14, 30).unwrap(), 30);
        assert!(parse_days(Some("31"), 14, 30).is_err());
        assert!(parse_days(Some("2.5"), 14, 30).is_err());
        assert!(parse_days(Some("0"), 14, 30).is_err());
        assert_eq!(parse_symbol_list(Some("aapl,BRK-B:stock,BTC:crypto")).unwrap(), Some(vec!["AAPL".into(), "BRK-B".into()]));
        assert_eq!(parse_symbol_list(Some("")).unwrap(), None);
        assert!(parse_symbol_list(Some("../x")).is_err());
        assert!(parse_symbol_list(Some(&vec!["A"; 51].join(","))).is_err());
    }
}

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
    v.and_then(Interval::parse).map_or_else(|| bad("interval invalide (1h | 4h | 1d)"), Ok)
}

/// "BTC:crypto,AAPL:stock" → list of assets (20 max); absent or empty → the default list.
pub fn parse_assets(v: Option<&str>, default: &[Asset], make: impl Fn(&str, Kind) -> Asset) -> ApiResult<Vec<Asset>> {
    let Some(v) = v.filter(|s| !s.is_empty()) else { return Ok(default.to_vec()) };
    v.split(',')
        .take(20)
        .map(|item| {
            let mut parts = item.split(':');
            let sym = parts.next();
            let kind = parse_kind(Some(parts.next().unwrap_or("crypto")))?;
            Ok(make(&parse_symbol(sym, kind)?, kind))
        })
        .collect()
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
    }
}

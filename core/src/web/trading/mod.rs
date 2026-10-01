//! Pure logic of the selection, opportunities, simulation and journal screens (from engine/paper.ts, engine/journal.ts, paper-ui.ts, journal-store.ts (pure part), Selection's parseBudget).
//! One file per TypeScript module, declared here; tests next to the code (`cargo test -p altim-core`).
//!
//! - `paper`: paper trading engine (engine/paper.ts), same JSON as "altim.paper.v1" and test/paper-fixture.json;
//! - `paper_ui`: helpers of the Simulation screens (paper-ui.ts): saved state, amounts, labels, notices;
//! - `journal`: automatic trading journal (engine/journal.ts), same JSON as "altim.journal.v1";
//! - `journal_store`: pure part of journal-store.ts (saved state, real trades, background enrichment);
//! - `selection`: budget, saved choices, report contract and allocation of the Sélection screen (Selection.tsx);
//! - `opportunities`: report contract, filters and saved choices of the Opportunités screen (engine/opportunities.ts).
pub mod journal;
pub mod journal_store;
pub mod opportunities;
pub mod paper;
pub mod paper_ui;
pub mod selection;

use std::cmp::Ordering;

/// `a.localeCompare(b)` for the ASCII keys the screens sort on ("buy", "buyZone", "noPosition", "none"…): letters
/// compared without case first (ICU primary strength), then lower case before upper case.
pub fn locale_cmp(a: &str, b: &str) -> Ordering {
    let fold = |s: &str| s.to_lowercase();
    fold(a).cmp(&fold(b)).then_with(|| {
        // Same letters: the lower-case variant first (ICU tertiary order), e.g. "a" < "A".
        let flip = |s: &str| s.chars().map(|c| if c.is_lowercase() { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }).collect::<String>();
        flip(a).cmp(&flip(b))
    })
}

/// `Math.round(v * 10 ** d) / 10 ** d`, the rounding helper of the engines.
pub fn round_to(v: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    crate::js::round(v * p) / p
}

/// `x > 0` (false for NaN): the TypeScript's `!(x > 0)` guards read `!positive(x)`.
pub fn positive(x: f64) -> bool {
    x > 0.0
}

/// `Math.min` (NaN when either is NaN, unlike `f64::min`).
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// `Math.max` (NaN when either is NaN).
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

/// `s.slice(0, n)` counted in UTF-16 code units, as JavaScript strings are (never splits a character here).
pub fn slice_utf16(s: &str, n: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in s.chars() {
        used += c.len_utf16();
        if used > n {
            break;
        }
        out.push(c);
    }
    out
}

/// `Number(s)` for what a user types (already stripped of spaces): "" → 0, "12.5" → 12.5, "1e3" → 1000, "Infinity",
/// anything else NaN (no hexadecimal).
pub fn js_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if t.chars().any(|c| c.is_alphabetic() && c != 'e' && c != 'E') {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

/// The JSON string of a unit enum (`"buyZone"`), as the TypeScript objects carry them.
pub fn enum_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::sorting::Sorting;

    #[test]
    fn locale_order() {
        let mut v = vec!["wait", "noPosition", "none", "buyZone", "buy", "Buy"];
        v.sort_by_dyn(|a, b| locale_cmp(a, b));
        assert_eq!(v, ["buy", "Buy", "buyZone", "none", "noPosition", "wait"]);
    }

    #[test]
    fn helpers() {
        assert_eq!(round_to(1.23456789, 2), 1.23);
        assert_eq!(round_to(-2.5, 0), -2.0);
        assert_eq!(slice_utf16("abc", 2), "ab");
        assert_eq!(slice_utf16("a😀b", 2), "a");
        assert_eq!(js_number(""), 0.0);
        assert_eq!(js_number("12.5"), 12.5);
        assert!(js_number("abc").is_nan());
        assert!(js_number("inf").is_nan());
        assert!(js_min(1.0, f64::NAN).is_nan());
    }
}

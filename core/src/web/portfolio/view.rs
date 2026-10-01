//! Pure helpers of the Mes avoirs screens (MyHoldings.tsx, ToolCards.tsx, WhatIfCard.tsx): how typed amounts are
//! read (JavaScript `Number` semantics, French decimal comma), how saved amounts go back into an input, the notes on
//! amounts typed in the other currency, the spreadsheet export and the saved rebalancing target ("altim.rebalance").
use serde_json::{Map, Value, json};

use super::holdings::PortfolioAnalysis;
use crate::js::{number_to_string, round, to_fixed};
use crate::types::Kind;
use crate::web::money::{Currency, MoneyDisplay, stored_currency, symbol};
use crate::web::store::{StoredHolding, shown};

/// `Number(s)`: blank → 0, decimal or exponent notation, "Infinity", hex / octal / binary integers; anything else NaN.
pub fn js_number(s: &str) -> f64 {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if t.is_empty() {
        return 0.0;
    }
    let (sign, body) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = t.strip_prefix(prefix) {
            return u64::from_str_radix(digits, radix).map(|v| v as f64).unwrap_or(f64::NAN);
        }
    }
    // Rust also reads "inf", "nan" and "infinity", which JavaScript does not.
    let ok = !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'));
    if !ok || !body.bytes().next().is_some_and(|b| b.is_ascii_digit() || b == b'.') {
        return f64::NAN;
    }
    body.parse::<f64>().map(|v| sign * v).unwrap_or(f64::NAN)
}

/// `Number(s.replace(/\s/g, "").replace(",", "."))`: what the forms read from a French number typed by hand.
pub fn parse_decimal(s: &str) -> f64 {
    let t: String = s.chars().filter(|c| !c.is_whitespace() && *c != '\u{feff}').collect();
    js_number(&t.replacen(',', ".", 1))
}

/// A saved amount put back in an input, in the display currency ("" when no rate allows it): 8 decimals at most,
/// decimal comma, no grouping.
pub fn input_text(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    number_to_string(round(v * 1e8) / 1e8).replace('.', ",")
}

/// `v.toPrecision(p)` for a positive finite number.
pub fn to_precision(v: f64, p: usize) -> String {
    if v == 0.0 {
        return to_fixed(0.0, p.saturating_sub(1));
    }
    let mut e = v.abs().log10().floor() as i32;
    // Rounding can carry to the next power of ten (0,99996 → 1,000).
    let fixed = |e: i32| to_fixed(v, (p as i32 - 1 - e).max(0) as usize);
    if e < p as i32 && crate::js::parse_number(&fixed(e)).abs() >= 10f64.powi(e + 1) {
        e += 1;
    }
    if e < -6 || e >= p as i32 {
        let m = v / 10f64.powi(e);
        let sign = if e < 0 { "-" } else { "+" };
        return format!("{}e{sign}{}", to_fixed(m, p - 1), e.abs());
    }
    fixed(e)
}

/// A price put in an input of the tools: 2 decimals from 1, 4 significant digits below, decimal comma (`plain`).
pub fn plain(v: f64) -> String {
    if v >= 1.0 { to_fixed(v, 2) } else { to_precision(v, 4) }.replace('.', ",")
}

/// « Prix de revient saisi en $, converti au taux du jour. » when the cost was typed in the other currency (and can
/// be converted).
pub fn cost_note(h: &StoredHolding, d: &MoneyDisplay) -> Option<String> {
    let typed = stored_currency(h.cost_currency);
    if typed == d.currency() || !shown(h.average_price, h.cost_currency, d).is_finite() {
        return None;
    }
    Some(format!("Prix de revient saisi en {}, converti au taux du jour.", symbol(typed)))
}

/// Liquidités typed in the other currency: « Montant saisi en €, converti au taux du jour. ».
pub fn cash_note(cash: f64, cash_currency: Option<Currency>, cash_unconverted: bool, d: &MoneyDisplay) -> Option<String> {
    let typed = stored_currency(cash_currency);
    (cash > 0.0 && typed != d.currency() && !cash_unconverted).then(|| format!("Montant saisi en {}, converti au taux du jour.", symbol(typed)))
}

// ---------- Spreadsheet export ----------

// CSV columns: quantities and prices keep their precision (small cryptos), the rest 2 decimals.
const CSV_DIGITS: [i32; 11] = [0, 0, 0, 8, 6, 6, 2, 2, 2, 2, 0];

enum Cell {
    Text(String),
    Num(Option<f64>),
}

fn cell(v: &Cell, digits: i32) -> String {
    match v {
        Cell::Num(None) => String::new(),
        Cell::Num(Some(n)) if !n.is_finite() => String::new(),
        Cell::Num(Some(n)) => {
            let p = 10f64.powi(digits);
            number_to_string(round(n * p) / p).replace('.', ",")
        }
        Cell::Text(s) => {
            // Texts starting like a formula are neutralised.
            let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.clone() };
            if s.contains([';', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
        }
    }
}

/// Spreadsheet export (French Excel: ";" separator, decimal comma, UTF-8 with BOM), amounts in the display currency.
/// `unconverted`: symbols whose euro cost could not be converted (their gains are left empty).
pub fn holdings_csv(a: &PortfolioAnalysis, unconverted: &[String], d: &MoneyDisplay) -> String {
    let sym = d.symbol();
    let text = |s: &str| Cell::Text(s.into());
    let mut rows: Vec<Vec<Cell>> = vec![
        [
            "Symbole".to_string(),
            "Nom".into(),
            "Type".into(),
            "Quantité".into(),
            format!("Prix moyen ({sym})"),
            format!("Cours ({sym})"),
            format!("Valeur ({sym})"),
            format!("Plus-value ({sym})"),
            "Plus-value (%)".into(),
            "Poids (%)".into(),
            "Conseil Altim".into(),
        ]
        .into_iter()
        .map(Cell::Text)
        .collect(),
    ];
    for l in &a.lines {
        let unconv = unconverted.contains(&l.symbol);
        rows.push(vec![
            text(&l.symbol),
            text(&l.name),
            text(if l.kind == Kind::Crypto { "Crypto" } else { "Action" }),
            Cell::Num(Some(l.quantity)),
            Cell::Num(Some(d.to_display(l.average_price))),
            Cell::Num(l.price.map(|p| d.to_display(p))),
            Cell::Num(Some(d.to_display(l.value))),
            Cell::Num((!unconv).then(|| d.to_display(l.pnl))),
            Cell::Num((!unconv).then_some(l.pnl_percent)),
            Cell::Num(Some(l.weight)),
            text(l.recommendation.label()),
        ]);
    }
    let mut cash = vec![text("Liquidités"), text(""), text(""), text(""), text(""), text("")];
    cash.push(Cell::Num(Some(d.to_display(a.cash))));
    cash.extend((0..4).map(|_| text("")));
    rows.push(cash);
    let body: Vec<String> = rows.iter().map(|r| r.iter().enumerate().map(|(i, v)| cell(v, CSV_DIGITS[i])).collect::<Vec<_>>().join(";")).collect();
    format!("\u{feff}{}", body.join("\r\n"))
}

// ---------- Rebalancing target ("altim.rebalance") ----------

pub const REBALANCE_KEY: &str = "altim.rebalance";
pub const DEFAULT_REBALANCE: [&str; 3] = ["40", "50", "10"];
const CLASS_KEYS: [&str; 3] = ["crypto", "stock", "cash"];

/// The target as typed (crypto, stock, cash), from the saved JSON `{ crypto: "40", stock: "50", cash: "10" }`.
pub fn parse_rebalance(raw: Option<&str>) -> [String; 3] {
    let saved = raw.and_then(|r| serde_json::from_str::<Value>(r).ok()).and_then(|v| v.as_object().cloned());
    std::array::from_fn(|i| match saved.as_ref().and_then(|o| o.get(CLASS_KEYS[i])) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => number_to_string(n.as_f64().unwrap_or(0.0)),
        _ => DEFAULT_REBALANCE[i].to_string(),
    })
}

pub fn rebalance_json(t: &[String; 3]) -> String {
    let m: Map<String, Value> = CLASS_KEYS.iter().zip(t).map(|(k, v)| (k.to_string(), json!(v))).collect();
    Value::Object(m).to_string()
}

#[cfg(test)]
mod tests {
    use super::super::holdings::{Holding, analyze_portfolio};
    use super::*;
    use crate::web::money::FxRate;
    use std::collections::HashMap;

    fn eur() -> MoneyDisplay {
        MoneyDisplay::new(
            Currency::Eur,
            Some(FxRate { rate: 0.88, usd_per_eur: 1.0 / 0.88, time: 0.0, source: "t".into(), fetched_at: 0.0, stale: false }),
        )
    }

    #[test]
    fn numbers_typed() {
        assert_eq!(js_number(""), 0.0);
        assert_eq!(js_number(" 12.5 "), 12.5);
        assert_eq!(js_number("1e3"), 1000.0);
        assert_eq!(js_number("-3"), -3.0);
        assert_eq!(js_number("0x10"), 16.0);
        assert_eq!(js_number(".5"), 0.5);
        assert_eq!(js_number("5."), 5.0);
        assert!(js_number("inf").is_nan() && js_number("1,5").is_nan() && js_number("abc").is_nan());
        assert_eq!(parse_decimal("1\u{202f}234,5"), 1234.5);
        assert_eq!(parse_decimal(""), 0.0);
        assert!(parse_decimal("1,2,3").is_nan());
        assert_eq!(input_text(1234.5), "1234,5");
        assert_eq!(input_text(0.123456789), "0,12345679");
        assert_eq!(input_text(f64::NAN), "");
        assert_eq!(plain(64210.456), "64210,46");
        assert_eq!(plain(0.012346), "0,01235");
        assert_eq!(plain(0.99996), "1,000");
        assert_eq!(to_precision(0.0000001234, 4), "1.234e-7");
    }

    fn line(cost_currency: Option<Currency>) -> StoredHolding {
        StoredHolding {
            id: "a".into(),
            symbol: "BTC".into(),
            kind: Kind::Crypto,
            name: "Bitcoin".into(),
            quantity: 1.0,
            average_price: 50_000.0,
            stop: None,
            cost_currency,
            stop_currency: None,
            extra: Map::new(),
        }
    }

    #[test]
    fn notes_on_the_other_currency() {
        // An old line (no tag: dollars) shown in euros.
        assert_eq!(cost_note(&line(None), &eur()).as_deref(), Some("Prix de revient saisi en $, converti au taux du jour."));
        assert_eq!(cost_note(&line(Some(Currency::Eur)), &eur()), None);
        // Euros typed, dollars shown without a rate: not convertible, no note (the unconverted warning says it).
        assert_eq!(cost_note(&line(Some(Currency::Eur)), &MoneyDisplay::usd()), None);
        assert_eq!(cash_note(100.0, None, false, &eur()).as_deref(), Some("Montant saisi en $, converti au taux du jour."));
        assert_eq!(cash_note(0.0, None, false, &eur()), None);
    }

    #[test]
    fn spreadsheet() {
        let h = |id: &str, symbol: &str, kind: Kind, q: f64, p: f64| Holding {
            id: id.into(),
            symbol: symbol.into(),
            kind,
            name: format!("={symbol}; \"x\""),
            quantity: q,
            average_price: p,
            stop: None,
        };
        let a = analyze_portfolio(&[h("1", "BTC", Kind::Crypto, 0.5, 40_000.0), h("2", "AAPL", Kind::Stock, 2.0, 150.0)], 1000.0, &HashMap::new());
        let csv = holdings_csv(&a, &["AAPL".to_string()], &MoneyDisplay::usd());
        let rows: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(
            rows[0],
            "\u{feff}Symbole;Nom;Type;Quantité;Prix moyen ($);Cours ($);Valeur ($);Plus-value ($);Plus-value (%);Poids (%);Conseil Altim"
        );
        assert_eq!(rows[1], "BTC;\"'=BTC; \"\"x\"\"\";Crypto;0,5;40000;;20000;0;0;93,9;Données insuffisantes");
        assert_eq!(rows[2], "AAPL;\"'=AAPL; \"\"x\"\"\";Action;2;150;;300;;;1,41;Données insuffisantes");
        assert_eq!(rows[3], "Liquidités;;;;;;1000;;;;");
    }

    #[test]
    fn rebalance_target() {
        assert_eq!(parse_rebalance(None), ["40", "50", "10"]);
        let t = parse_rebalance(Some(r#"{"crypto":"30","stock":"60","cash":"10"}"#));
        assert_eq!(t, ["30", "60", "10"]);
        assert_eq!(rebalance_json(&t), r#"{"crypto":"30","stock":"60","cash":"10"}"#);
        assert_eq!(parse_rebalance(Some("{")), ["40", "50", "10"]);
    }
}

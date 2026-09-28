//! Price and percentage formatting of `web/src/market.ts` (`formatPrice`, `formatPercent`), used by the alerts.
use crate::js::fr;

/// `formatPrice`: 2 decimals from 1, 4 from 0.01, 8 below (fr-FR).
pub fn format_price(v: f64) -> String {
    let digits = if v >= 1.0 {
        2
    } else if v >= 0.01 {
        4
    } else {
        8
    };
    fr(v, digits, digits)
}

/// `formatPercent`: sign (+ or −), two decimals, " %".
pub fn format_percent(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 2, 2))
}

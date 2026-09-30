//! Euro display of the engines' French texts. Engines compute in dollars; the host (the server, or the browser)
//! installs once the function that gives the current rate (euros for 1 dollar), None meaning "show dollars". Never
//! invented: without an installed provider or a rate, amounts stay in dollars with the "$" sign.
use std::sync::OnceLock;

use crate::engine::format::format_price;

static PROVIDER: OnceLock<fn() -> Option<f64>> = OnceLock::new();

/// Sets the rate provider (first call wins; the server installs its cached rate, the browser its /api/fx rate).
pub fn install(provider: fn() -> Option<f64>) {
    let _ = PROVIDER.set(provider);
}

/// Euros for 1 dollar now, when the host knows a valid rate.
pub fn rate() -> Option<f64> {
    PROVIDER.get().and_then(|p| p()).filter(|r| r.is_finite() && *r > 0.0)
}

/// `usd` converted for display: (value, symbol) in euros with a rate, else in dollars (never a made-up rate).
pub fn convert(usd: f64) -> (f64, &'static str) {
    match rate() {
        Some(r) => (usd * r, "€"),
        None => (usd, "$"),
    }
}

/// The one helper of the engines' texts: "212,40 €" (formatPrice digits), "212,40 $" without a rate.
pub fn money(usd: f64) -> String {
    money_with(usd, format_price)
}

/// Same with the caller's number format, applied to the converted value.
pub fn money_with(usd: f64, fmt: impl Fn(f64) -> String) -> String {
    let (v, sym) = convert(usd);
    format!("{} {sym}", fmt(v))
}

/// Unit suffix glued to a scale ("M€", "Md$"): `unit("M")`.
pub fn unit(scale: &str) -> String {
    format!("{scale}{}", convert(0.0).1)
}

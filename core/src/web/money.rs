//! Display currency of every amount (`web/src/money.ts`): engines and the server compute in dollars, the display
//! converts to euros with the EUR/USD rate of /api/fx. Without a rate the amounts stay in dollars with "$" (a rate is
//! never made up).
use serde::{Deserialize, Serialize};

use crate::js::fr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Currency {
    #[default]
    #[serde(rename = "EUR")]
    Eur,
    #[serde(rename = "USD")]
    Usd,
}

impl Currency {
    pub fn as_str(self) -> &'static str {
        match self {
            Currency::Eur => "EUR",
            Currency::Usd => "USD",
        }
    }
}

/// Tagged stored amount (localStorage): the currency it was typed in; old data without the tag is in dollars.
pub fn stored_currency(c: Option<Currency>) -> Currency {
    c.unwrap_or(Currency::Usd)
}

/// `GET /api/fx` with a rate (euros for 1 dollar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxRate {
    pub rate: f64,
    pub usd_per_eur: f64,
    /// Time of the quote (ms).
    pub time: f64,
    /// "Yahoo Finance", "BCE" or "Frankfurter (BCE)".
    pub source: String,
    pub fetched_at: f64,
    pub stale: bool,
}

/// What the formatters use: the currency chosen in Réglages and the last known rate (`setMoneyDisplay`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoneyDisplay {
    wanted: Currency,
    fx: Option<FxRate>,
}

/// No-break space between an amount and its symbol (money.ts default separator).
pub const NBSP: &str = "\u{a0}";

impl MoneyDisplay {
    pub fn new(wanted: Currency, fx: Option<FxRate>) -> Self {
        let fx = fx.filter(|r| r.rate.is_finite() && r.rate > 0.0);
        MoneyDisplay { wanted, fx }
    }

    /// Dollars, no rate: what the page shows before the state is read.
    pub fn usd() -> Self {
        MoneyDisplay { wanted: Currency::Usd, fx: None }
    }

    pub fn fx(&self) -> Option<&FxRate> {
        self.fx.as_ref()
    }

    /// Currency actually shown: euros only when chosen and a rate is known.
    pub fn currency(&self) -> Currency {
        if self.wanted == Currency::Eur && self.fx.is_some() { Currency::Eur } else { Currency::Usd }
    }

    pub fn symbol(&self) -> &'static str {
        symbol(self.currency())
    }

    /// `v` from one currency to another at the current rate; NaN when no rate allows it.
    pub fn convert(&self, v: f64, from: Currency, to: Currency) -> f64 {
        if from == to {
            return v;
        }
        match &self.fx {
            None => f64::NAN,
            Some(fx) if from == Currency::Usd => v * fx.rate,
            Some(fx) => v / fx.rate,
        }
    }

    /// Dollars → display currency.
    pub fn to_display(&self, usd: f64) -> f64 {
        self.convert(usd, Currency::Usd, self.currency())
    }

    /// Display currency → dollars (what the user typed, for the engines and the server).
    pub fn from_display(&self, v: f64) -> f64 {
        self.convert(v, self.currency(), Currency::Usd)
    }

    /// A dollar amount in the display currency with `min`–`max` decimals: "212,40 €" (or "$" without a rate).
    pub fn money(&self, usd: f64, min: usize, max: usize, sep: &str) -> String {
        format!("{}{sep}{}", fr(self.to_display(usd), min, max), self.symbol())
    }

    /// A dollar amount in the display currency with the caller's number format (applied to the converted value).
    pub fn money_fmt(&self, usd: f64, fmt: impl Fn(f64) -> String, sep: &str) -> String {
        format!("{}{sep}{}", fmt(self.to_display(usd)), self.symbol())
    }

    /// A price with `formatPrice` digits (2 from 1, 4 from 0.01, 8 below), chosen on the converted value.
    pub fn money_price(&self, usd: f64, sep: &str) -> String {
        let v = self.to_display(usd);
        let digits = if v >= 1.0 {
            2
        } else if v >= 0.01 {
            4
        } else {
            8
        };
        format!("{}{sep}{}", fr(v, digits, digits), self.symbol())
    }

    /// Large amounts: "421 Md€", "3,16 Md€", "850 M€", "12,5 k€".
    pub fn money_compact(&self, usd: f64, sep: &str) -> String {
        let v = self.to_display(usd);
        let a = v.abs();
        let sym = self.symbol();
        let (div, unit) = if a >= 1e9 {
            (1e9, format!("Md{sym}"))
        } else if a >= 1e6 {
            (1e6, format!("M{sym}"))
        } else if a >= 1e4 {
            (1e3, format!("k{sym}"))
        } else {
            (1.0, sym.to_string())
        };
        let x = v / div;
        let digits = if x.abs() >= 100.0 {
            0
        } else if x.abs() >= 10.0 {
            1
        } else {
            2
        };
        format!("{}{sep}{unit}", fr(x, 0, digits))
    }
}

pub fn symbol(c: Currency) -> &'static str {
    match c {
        Currency::Eur => "€",
        Currency::Usd => "$",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx() -> FxRate {
        FxRate { rate: 0.88, usd_per_eur: 1.0 / 0.88, time: 0.0, source: "Yahoo Finance".into(), fetched_at: 0.0, stale: false }
    }

    // money.test.ts "display currency"
    #[test]
    fn dollars_by_default_and_without_rate() {
        let d = MoneyDisplay::usd();
        assert_eq!(d.currency(), Currency::Usd);
        assert_eq!(d.money(212.4, 2, 2, NBSP), "212,40\u{a0}$");
        let d = MoneyDisplay::new(Currency::Eur, None);
        assert_eq!(d.currency(), Currency::Usd);
        assert_eq!(d.money_price(212.4, NBSP), "212,40\u{a0}$");
        assert!(d.convert(100.0, Currency::Eur, Currency::Usd).is_nan());
    }

    #[test]
    fn euros_with_rate() {
        let d = MoneyDisplay::new(Currency::Eur, Some(fx()));
        assert_eq!(d.currency(), Currency::Eur);
        assert_eq!(d.money(241.3636, 2, 2, NBSP), "212,40\u{a0}€");
        assert_eq!(d.money_price(0.5, NBSP), "0,4400\u{a0}€");
        assert_eq!(d.money_compact(1e10, NBSP), "8,8\u{a0}Md€");
        assert!((d.to_display(100.0) - 88.0).abs() < 1e-10);
        assert!((d.from_display(88.0) - 100.0).abs() < 1e-10);
    }

    #[test]
    fn dollars_chosen() {
        let d = MoneyDisplay::new(Currency::Usd, Some(fx()));
        assert_eq!(d.money(100.0, 2, 2, NBSP), "100,00\u{a0}$");
        assert_eq!(d.from_display(100.0), 100.0);
    }

    #[test]
    fn invalid_rate_ignored() {
        let mut r = fx();
        r.rate = 0.0;
        assert_eq!(MoneyDisplay::new(Currency::Eur, Some(r)).currency(), Currency::Usd);
    }
}

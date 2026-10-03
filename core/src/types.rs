//! Types shared by the engines and the server.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Candle {
    /// Milliseconds since the epoch (integer: the apps decode it as a 64-bit integer).
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Crypto,
    Stock,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Crypto => "crypto",
            Kind::Stock => "stock",
        }
    }
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "crypto" => Some(Kind::Crypto),
            "stock" => Some(Kind::Stock),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Interval {
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "4h")]
    H4,
    #[serde(rename = "1d")]
    D1,
    /// 4 calendar days, buckets anchored on the Unix epoch (see `candles::bucket`).
    #[serde(rename = "4d")]
    D4,
    /// One week, Monday 00:00 UTC (Binance's weekly candles).
    #[serde(rename = "1w")]
    W1,
}

impl Interval {
    pub fn as_str(self) -> &'static str {
        match self {
            Interval::H1 => "1h",
            Interval::H4 => "4h",
            Interval::D1 => "1d",
            Interval::D4 => "4d",
            Interval::W1 => "1w",
        }
    }
    /// Every timeframe, shortest first.
    pub const ALL: [Interval; 5] = [Interval::H1, Interval::H4, Interval::D1, Interval::D4, Interval::W1];
    /// French label of the choosers ("1 h", "4 h", "1 j", "4 j", "1 sem.").
    pub fn label(self) -> &'static str {
        match self {
            Interval::H1 => "1 h",
            Interval::H4 => "4 h",
            Interval::D1 => "1 j",
            Interval::D4 => "4 j",
            Interval::W1 => "1 sem.",
        }
    }
    /// Built from daily candles (or a provider's native weekly ones): 4 d and 1 w.
    pub fn multi_day(self) -> bool {
        matches!(self, Interval::D4 | Interval::W1)
    }
    pub fn parse(s: &str) -> Option<Interval> {
        match s {
            "1h" => Some(Interval::H1),
            "4h" => Some(Interval::H4),
            "1d" => Some(Interval::D1),
            "4d" => Some(Interval::D4),
            "1w" => Some(Interval::W1),
            _ => None,
        }
    }
    /// Candle duration in ms (`STEP` in market.ts).
    pub fn step(self) -> i64 {
        match self {
            Interval::H1 => 3_600_000,
            Interval::H4 => 14_400_000,
            Interval::D1 => 86_400_000,
            Interval::D4 => 4 * 86_400_000,
            Interval::W1 => 7 * 86_400_000,
        }
    }
    /// Next timeframe up (`HIGHER`). 1 d keeps none (unchanged since its validation); 4 d is confirmed by the week.
    pub fn higher(self) -> Option<Interval> {
        match self {
            Interval::H1 => Some(Interval::H4),
            Interval::H4 => Some(Interval::D1),
            Interval::D1 => None,
            Interval::D4 => Some(Interval::W1),
            Interval::W1 => None,
        }
    }
}

pub const DAY_MS: i64 = 86_400_000;

/// An asset shown by the app (`Asset` in quotes.ts).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asset {
    pub symbol: String,
    pub name: String,
    pub kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gecko: Option<String>,
}

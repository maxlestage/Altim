//! Presentation-site market data (`web/src/market.ts`): the consolidated tickers and candles of the server, the
//! public fallbacks, and the price / percentage formats.
use serde::{Deserialize, Serialize};

pub use crate::engine::format::{format_percent, format_price};
use crate::types::{Candle, Kind};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coin {
    pub symbol: &'static str,
    pub name: &'static str,
    pub gecko: &'static str,
}

pub const COINS: [Coin; 6] = [
    Coin { symbol: "BTCUSDT", name: "Bitcoin", gecko: "bitcoin" },
    Coin { symbol: "ETHUSDT", name: "Ethereum", gecko: "ethereum" },
    Coin { symbol: "SOLUSDT", name: "Solana", gecko: "solana" },
    Coin { symbol: "BNBUSDT", name: "BNB", gecko: "binancecoin" },
    Coin { symbol: "XRPUSDT", name: "XRP", gecko: "ripple" },
    Coin { symbol: "ADAUSDT", name: "Cardano", gecko: "cardano" },
];

impl Coin {
    /// "BTC" of "BTCUSDT".
    pub fn base(&self) -> &'static str {
        self.symbol.strip_suffix("USDT").unwrap_or(self.symbol)
    }
}

/// `/api/tickers` row (consensus of the price sources).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    pub symbol: String,
    pub name: String,
    pub kind: Kind,
    pub price: f64,
    pub change: Option<f64>,
    /// Sources that agree on the price / sources queried.
    pub agreeing: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceStatus {
    pub name: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deviation: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `/api/candles?base=…&interval=…` (the fields the site reads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandlesReply {
    pub candles: Vec<Candle>,
    pub source: String,
    #[serde(default)]
    pub sources: Vec<SourceStatus>,
    #[serde(default)]
    pub agreeing: u32,
}

/// Binance `ticker/24hr` fallback row.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceTicker {
    pub symbol: String,
    pub last_price: String,
    pub price_change_percent: String,
}

/// CoinGecko `simple/price` entry.
#[derive(Debug, Clone, Deserialize)]
pub struct GeckoPrice {
    pub usd: f64,
    #[serde(default)]
    pub usd_24h_change: Option<f64>,
}

/// Fallback ticks when the Altim server is unreachable: Binance and CoinGecko queried directly (either may be None).
pub fn fallback_ticks(binance: Option<&[BinanceTicker]>, gecko: Option<&std::collections::HashMap<String, GeckoPrice>>) -> Vec<Tick> {
    COINS
        .iter()
        .filter_map(|c| {
            let mut prices = Vec::new();
            let mut changes = Vec::new();
            if let Some(r) = binance.and_then(|b| b.iter().find(|x| x.symbol == c.symbol)) {
                prices.push(crate::js::parse_number(&r.last_price));
                changes.push(crate::js::parse_number(&r.price_change_percent));
            }
            if let Some(g) = gecko.and_then(|g| g.get(c.gecko)) {
                prices.push(g.usd);
                changes.push(g.usd_24h_change.unwrap_or(f64::NAN));
            }
            if prices.is_empty() {
                return None;
            }
            let agree = if prices.len() == 2 && (prices[0] / prices[1] - 1.0).abs() < 0.005 { 2 } else { 1 };
            Some(Tick {
                symbol: c.base().into(),
                name: c.name.into(),
                kind: Kind::Crypto,
                price: prices[0],
                change: changes.first().copied(),
                agreeing: agree,
                total: 2,
            })
        })
        .collect()
}

/// Binance klines → closed candles (`Number(r[6]) < now`), the site's fallback for the live signal.
pub fn binance_klines(rows: &[Vec<serde_json::Value>], now: f64) -> Vec<Candle> {
    let num = |v: &serde_json::Value| v.as_f64().unwrap_or_else(|| v.as_str().map(crate::js::parse_number).unwrap_or(f64::NAN));
    rows.iter()
        .filter(|r| r.len() > 6 && num(&r[6]) < now)
        .map(|r| Candle { time: num(&r[0]) as i64, open: num(&r[1]), high: num(&r[2]), low: num(&r[3]), close: num(&r[4]), volume: num(&r[5]) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_agreement() {
        let b = vec![BinanceTicker { symbol: "BTCUSDT".into(), last_price: "100000".into(), price_change_percent: "1.5".into() }];
        let mut g = std::collections::HashMap::new();
        g.insert("bitcoin".to_string(), GeckoPrice { usd: 100_100.0, usd_24h_change: Some(1.4) });
        let t = fallback_ticks(Some(&b), Some(&g));
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].symbol.as_str(), t[0].price, t[0].agreeing, t[0].change), ("BTC", 100_000.0, 2, Some(1.5)));
        assert!(fallback_ticks(None, None).is_empty());
    }

    #[test]
    fn klines_keep_closed_only() {
        let rows: Vec<Vec<serde_json::Value>> =
            serde_json::from_str(r#"[[1,"1","2","0.5","1.5","10",99],[2,"1","2","0.5","1.5","10",200]]"#).unwrap();
        let c = binance_klines(&rows, 100.0);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].time, c[0].close), (1, 1.5));
    }
}

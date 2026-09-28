//! Liquidity at the time of a decision: bid/ask spread from the top of the order book (crypto: OKX, else Coinbase;
//! stocks and funds: Nasdaq, else Robinhood), average daily traded value and relative volume from the daily candles.
//! A figure that cannot be measured stays `None`.
use std::time::Duration;

use serde_json::Value;

use crate::cache::cached;
use crate::engine::decision_types::Liquidity;
use crate::fundamentals::{num, round_to};
use crate::http::{Error, Result, get_json_with};
use crate::types::{Candle, Kind};

const TIMEOUT: Duration = Duration::from_secs(8);
const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
/// Days of the averages.
pub const DAYS: usize = 20;

/// Spread in % of the mid price; None when the book is empty, crossed or absurd (above 10 %).
pub fn spread_pct(bid: f64, ask: f64) -> Option<f64> {
    if !(bid.is_finite() && ask.is_finite() && bid > 0.0 && ask >= bid) {
        return None;
    }
    let s = (ask - bid) / ((ask + bid) / 2.0) * 100.0;
    (s <= 10.0).then(|| round_to(s, 6))
}

pub mod parse {
    use super::*;

    /// OKX `/api/v5/market/ticker`: {"code":"0","data":[{"bidPx","askPx"}]}.
    pub fn okx(d: &Value) -> Option<(f64, f64)> {
        if d.get("code").and_then(|c| c.as_str()) != Some("0") {
            return None;
        }
        let t = d.get("data")?.get(0)?;
        Some((num(t.get("bidPx"))?, num(t.get("askPx"))?))
    }

    /// Coinbase `/products/{P}/book?level=1`: {"bids":[["price","size",n]],"asks":[…]}.
    pub fn coinbase(d: &Value) -> Option<(f64, f64)> {
        Some((num(d.get("bids")?.get(0)?.get(0))?, num(d.get("asks")?.get(0)?.get(0))?))
    }

    /// Nasdaq `/api/quote/{SYM}/info`: primaryData.bidPrice / askPrice ("$341.33").
    pub fn nasdaq(d: &Value) -> Option<(f64, f64)> {
        let p = d.get("data")?.get("primaryData")?;
        Some((num(p.get("bidPrice"))?, num(p.get("askPrice"))?))
    }

    /// Robinhood `/quotes/?symbols=SYM`: results[0].bid_price / ask_price.
    pub fn robinhood(d: &Value) -> Option<(f64, f64)> {
        let q = d.get("results")?.get(0)?;
        Some((num(q.get("bid_price"))?, num(q.get("ask_price"))?))
    }
}

/// Average daily traded value (close × volume, last 20 candles) and the last volume ÷ the average volume of the 20
/// candles before it. None when there are too few candles or a volume is missing.
pub fn traded(candles: &[Candle]) -> (Option<f64>, Option<f64>) {
    let ok = |c: &&Candle| c.close.is_finite() && c.close > 0.0 && c.volume.is_finite() && c.volume >= 0.0;
    let value = (candles.len() >= DAYS)
        .then(|| &candles[candles.len() - DAYS..])
        .filter(|w| w.iter().all(|c| ok(&c)))
        .map(|w| w.iter().map(|c| c.close * c.volume).sum::<f64>() / DAYS as f64)
        .filter(|v| *v > 0.0);
    let relative = (candles.len() > DAYS)
        .then(|| (&candles[candles.len() - DAYS - 1..candles.len() - 1], candles[candles.len() - 1]))
        .filter(|(w, last)| w.iter().all(|c| ok(&c)) && ok(&last))
        .and_then(|(w, last)| {
            let mean = w.iter().map(|c| c.volume).sum::<f64>() / DAYS as f64;
            (mean > 0.0).then(|| round_to(last.volume / mean, 2))
        });
    (value, relative)
}

async fn get(url: &str, ua: &str) -> Result<Value> {
    get_json_with(url, &[("User-Agent", ua), ("Accept-Language", "en-US,en;q=0.9")], TIMEOUT).await
}

/// (spread %, source name)
async fn spread(symbol: &str, kind: Kind) -> Option<(f64, &'static str)> {
    let sym = symbol.trim().to_uppercase();
    let key = format!("spread:{}:{sym}", kind.as_str());
    let r = cached(&key, 60_000, move || async move {
        let enc = crate::guard::encode_uri_component(&sym);
        let tries: Vec<(&'static str, String, fn(&Value) -> Option<(f64, f64)>)> = match kind {
            Kind::Crypto => vec![
                ("OKX", format!("https://www.okx.com/api/v5/market/ticker?instId={enc}-USDT"), parse::okx),
                ("Coinbase", format!("https://api.exchange.coinbase.com/products/{enc}-USD/book?level=1"), parse::coinbase),
            ],
            Kind::Stock => {
                let dotted = crate::guard::encode_uri_component(&sym.replace('-', "."));
                vec![
                    ("Nasdaq", format!("https://api.nasdaq.com/api/quote/{dotted}/info?assetclass=stocks"), parse::nasdaq),
                    ("Nasdaq", format!("https://api.nasdaq.com/api/quote/{dotted}/info?assetclass=etf"), parse::nasdaq),
                    ("Robinhood", format!("https://api.robinhood.com/quotes/?symbols={dotted}"), parse::robinhood),
                ]
            }
        };
        for (name, url, parser) in tries {
            if let Some(s) = get(&url, BROWSER_UA).await.ok().as_ref().and_then(parser).and_then(|(b, a)| spread_pct(b, a)) {
                return Ok((s, name));
            }
        }
        Err(Error("carnet d'ordres indisponible".into()))
    })
    .await;
    r.ok().map(|v| *v)
}

/// Liquidity of an asset; `candles_daily` are the daily candles already fetched for the decision (oldest first).
pub async fn liquidity(symbol: &str, kind: Kind, candles_daily: &[Candle]) -> Liquidity {
    let (daily_value, relative_volume) = traded(candles_daily);
    let spread = spread(symbol, kind).await;
    let mut sources: Vec<String> = Vec::new();
    if let Some((_, name)) = spread {
        sources.push(format!("{name} (meilleurs prix acheteur / vendeur)"));
    }
    if daily_value.is_some() || relative_volume.is_some() {
        sources.push(match kind {
            // The consensus candles carry the volume of the exchange they come from, not of the whole market.
            Kind::Crypto => format!("bougies journalières ({DAYS} jours ; volume de la plateforme source, pas du marché entier)"),
            Kind::Stock => format!("bougies journalières ({DAYS} jours)"),
        });
    }
    Liquidity {
        spread_pct: spread.map(|s| s.0),
        daily_value,
        relative_volume,
        source: if sources.is_empty() { "Non mesurable : carnet d'ordres et volumes indisponibles".to_string() } else { sources.join(", ") },
    }
}

//! Long daily histories for the « Bot Altim » v2 (training and walk-forward test), fetched once per report:
//! - stocks and ETFs: Yahoo Finance's daily chart over the last `STOCK_YEARS` years (`period1` / `period2`, one call;
//!   prices adjusted for splits, not for dividends);
//! - cryptos: Bitstamp's daily candles paged backwards (`end`, 1 000 per call) since the pair's listing, and Yahoo's
//!   `<SYMBOL>-USD` daily chart; the longer of the two is kept (never chosen on performance), its name reported.
//!
//! Only closed candles are kept. A failure is returned with its reason: the route then falls back to Altim's usual
//! long history (≈ 3-5 years, several sources) for the validation's basket, and leaves the extra training asset out.
use std::time::Duration;

use crate::http::{Error, Result, get_json_with};
use crate::jsval::encode_uri_component as enc;
use crate::market::{closed_only_at, parse, parse_stock, stock_closed_at};
use crate::types::{Candle, DAY_MS, Interval, Kind};

/// Years of daily stock history asked of Yahoo (≈ 5 000 sessions).
pub const STOCK_YEARS: i64 = 20;
/// Candles per Bitstamp call, and the most calls for one pair (16 000 days: since 2011 for bitcoin).
pub const BITSTAMP_LIMIT: usize = 1000;
pub const MAX_PAGES: usize = 16;
const TIMEOUT: Duration = Duration::from_secs(25);

pub fn yahoo_url(symbol: &str, from_s: i64, to_s: i64) -> String {
    format!("https://query2.finance.yahoo.com/v8/finance/chart/{}?interval=1d&period1={from_s}&period2={to_s}&includePrePost=false", enc(symbol))
}

pub fn bitstamp_url(symbol: &str, end_s: i64) -> String {
    format!("https://www.bitstamp.net/api/v2/ohlc/{}usd/?step=86400&limit={BITSTAMP_LIMIT}&end={end_s}", symbol.to_lowercase())
}

/// Yahoo's ticker of a crypto quoted in dollars.
pub fn yahoo_crypto(symbol: &str) -> String {
    format!("{}-USD", symbol.to_uppercase())
}

/// The `end` (s) of the page before `page` (Bitstamp: candles up to `end` included), or None when this page is the
/// first one (shorter than a full page, or empty).
pub fn next_end(page: &[Candle], limit: usize) -> Option<i64> {
    let first = page.iter().map(|c| c.time).min()?;
    (page.len() >= limit).then_some(first / 1000 - 1)
}

/// Pages in any order → one sorted history, one candle per time.
pub fn merge_pages(pages: Vec<Vec<Candle>>) -> Vec<Candle> {
    let mut all: Vec<Candle> = pages.into_iter().flatten().collect();
    all.sort_by_key(|c| c.time);
    all.dedup_by_key(|c| c.time);
    all
}

/// Stock daily candles over the last `STOCK_YEARS` years (closed sessions only).
pub async fn stock_history(symbol: &str, now: i64) -> Result<Vec<Candle>> {
    let to = now / 1000;
    let from = to - STOCK_YEARS * 365 * 86_400 - 30 * 86_400;
    let c = parse_stock::yahoo(&get_json_with(&yahoo_url(symbol, from, to), &[], TIMEOUT).await?)?;
    let c = stock_closed_at(c, Interval::D1, now);
    if c.is_empty() { Err(Error("Yahoo : aucune bougie".into())) } else { Ok(c) }
}

/// Bitstamp's whole daily history of `<symbol>/USD`, paged backwards from `now`.
pub async fn bitstamp_history(symbol: &str, now: i64) -> Result<Vec<Candle>> {
    let mut pages = Vec::new();
    let mut end = now / 1000;
    for _ in 0..MAX_PAGES {
        let page = parse::bitstamp(&get_json_with(&bitstamp_url(symbol, end), &[], TIMEOUT).await?)?;
        let next = next_end(&page, BITSTAMP_LIMIT);
        pages.push(page);
        match next {
            Some(e) => end = e,
            None => break,
        }
    }
    let c = closed_only_at(merge_pages(pages), Interval::D1, now);
    if c.is_empty() { Err(Error("Bitstamp : paire non cotée".into())) } else { Ok(c) }
}

pub async fn yahoo_crypto_history(symbol: &str, now: i64) -> Result<Vec<Candle>> {
    let c = parse_stock::yahoo(&get_json_with(&yahoo_url(&yahoo_crypto(symbol), 0, now / 1000), &[], TIMEOUT).await?)?;
    let c = closed_only_at(c, Interval::D1, now);
    if c.is_empty() { Err(Error("Yahoo : aucune bougie".into())) } else { Ok(c) }
}

/// Of two fetched histories, the one with more candles (ties: the first); both errors joined when neither came.
pub fn longer(a: (&str, Result<Vec<Candle>>), b: (&str, Result<Vec<Candle>>)) -> Result<(Vec<Candle>, String)> {
    match (a, b) {
        ((na, Ok(ca)), (nb, Ok(cb))) => Ok(if cb.len() > ca.len() { (cb, nb.into()) } else { (ca, na.into()) }),
        ((na, Ok(ca)), _) => Ok((ca, na.into())),
        (_, (nb, Ok(cb))) => Ok((cb, nb.into())),
        ((na, Err(ea)), (nb, Err(eb))) => Err(Error(format!("{na} : {} ; {nb} : {}", ea.0, eb.0))),
    }
}

/// One asset's long history and the name of its source.
pub async fn long_history(symbol: &str, kind: Kind, now: i64) -> Result<(Vec<Candle>, String)> {
    match kind {
        Kind::Stock => stock_history(symbol, now).await.map(|c| (c, format!("Yahoo Finance ({STOCK_YEARS} ans max.)"))),
        Kind::Crypto => {
            let (b, y) = tokio::join!(bitstamp_history(symbol, now), yahoo_crypto_history(symbol, now));
            longer(("Bitstamp", b), ("Yahoo Finance", y))
        }
    }
}

/// Years covered by a history (first to last candle).
pub fn years(c: &[Candle]) -> f64 {
    match (c.first(), c.last()) {
        (Some(a), Some(b)) => (b.time - a.time) as f64 / (365.25 * DAY_MS as f64),
        _ => 0.0,
    }
}

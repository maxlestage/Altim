//! Macro-economic and geopolitical context (`web/server/macro.ts`, market-wide, shared by every asset): VIX,
//! S&P 500, oil, gold, dollar and US 10-year yields (Yahoo Finance, 5 years of daily closes), plus world headlines
//! (Google News).
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::cache::cached;
use crate::engine::guard::NewsItem;
use crate::engine::macro_ctx::{MacroKey, MacroPoint, MacroReport, MacroSeries, macro_report};
use crate::guard::{encode_uri_component, parse_guard};
use crate::http::{Error, Result, err, get_json_with, get_text_with};

const UA: &str = "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0";
const TIMEOUT: Duration = Duration::from_secs(8);

pub const MACRO_SYMBOLS: [(MacroKey, &str); 6] = [
    (MacroKey::Vix, "^VIX"),
    (MacroKey::Spx, "^GSPC"),
    (MacroKey::Oil, "CL=F"),
    (MacroKey::Gold, "GC=F"),
    (MacroKey::Dollar, "DX-Y.NYB"),
    (MacroKey::Rates, "^TNX"),
];

const QUERIES: [&str; 2] = [
    "(war OR invasion OR missile OR airstrike OR sanctions OR military OR nuclear OR ceasefire OR strait)",
    "(\"Federal Reserve\" OR Fed OR inflation OR tariffs OR recession OR \"interest rates\" OR \"stock market\" OR \"sell-off\" OR \"trade war\" OR \"bank\")",
];

/// Daily closes of a Yahoo chart response.
pub fn parse_series(d: &Value) -> Result<Vec<MacroPoint>> {
    Ok(crate::market::parse_stock::yahoo(d)?.into_iter().map(|c| MacroPoint { time: c.time, close: c.close }).collect())
}

async fn series(symbol: &str) -> Result<Vec<MacroPoint>> {
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=5y", encode_uri_component(symbol));
    parse_series(&get_json_with(&url, &[("User-Agent", UA)], TIMEOUT).await?)
}

/// Series kept (non-empty ones), error when neither the VIX nor the S&P 500 is available.
pub fn assemble_series(entries: Vec<(MacroKey, Vec<MacroPoint>)>) -> Result<MacroSeries> {
    let mut out = MacroSeries::default();
    for (k, v) in entries {
        if !v.is_empty() {
            out.set(k, Some(v));
        }
    }
    if out.vix.is_none() && out.spx.is_none() {
        return err("données macro indisponibles");
    }
    Ok(out)
}

pub async fn macro_series() -> Result<Arc<MacroSeries>> {
    cached("macro:series", 3_600_000, || async {
        let entries = futures::future::join_all(MACRO_SYMBOLS.iter().map(|(k, s)| async move { (*k, series(s).await.unwrap_or_default()) })).await;
        assemble_series(entries)
    })
    .await
}

/// Headlines of all queries, first occurrence of each title kept.
pub fn dedupe_news(lists: Vec<Vec<NewsItem>>) -> Vec<NewsItem> {
    let mut seen = std::collections::HashSet::new();
    lists.into_iter().flatten().filter(|n| seen.insert(n.title.clone())).collect()
}

pub async fn world_news() -> Vec<NewsItem> {
    cached("macro:news", 600_000, || async {
        let lists = futures::future::join_all(QUERIES.iter().map(|q| async move {
            let url = format!("https://news.google.com/rss/search?q={}&hl=en-US&gl=US&ceid=US:en", encode_uri_component(&format!("{q} when:1d")));
            match get_text_with(&url, &[("User-Agent", UA)], TIMEOUT).await {
                Ok(xml) => Ok(parse_guard::rss(&xml)),
                // `r.ok ? … : []`: an HTTP status only empties this query; a network failure fails them all.
                Err(Error(m)) if m.starts_with("HTTP ") => Ok(vec![]),
                Err(e) => Err(e),
            }
        }))
        .await;
        Ok::<_, Error>(dedupe_news(lists.into_iter().collect::<Result<Vec<_>>>()?))
    })
    .await
    .map(|v| (*v).clone())
    .unwrap_or_default()
}

/// `macro(now)`: the current macro report.
pub async fn report(now: i64) -> Result<MacroReport> {
    let (s, news) = tokio::join!(macro_series(), world_news());
    let s = s?;
    Ok(macro_report(&s, &news, now))
}

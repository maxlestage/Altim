//! Data for the market guard (`web/server/guard.ts`): crowd positioning (OKX perpetuals), sentiment (Fear & Greed,
//! StockTwits), news (Google News, per asset) and market-wide fear (VIX). Each input is optional: a missing one only
//! removes its factors, never blocks the guard.
use std::future::Future;
use std::sync::LazyLock;
use std::time::Duration;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::cache::cached;
use crate::engine::Evidence;
use crate::engine::guard::{GuardInput, GuardResult, MacroContext, NewsItem, NewsTone, Positioning, SentimentInput, guard, news_tone};
use crate::engine::macro_ctx::MacroReport;
use crate::http::{Result, get_json_with, get_text_with};
use crate::js::parse_date;
use crate::types::{Candle, Interval, Kind};

pub const UA: &str = "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0";
const TIMEOUT: Duration = Duration::from_secs(8);

async fn get_json(url: &str) -> Result<Value> {
    get_json_with(url, &[("User-Agent", UA)], TIMEOUT).await
}

async fn get_text(url: &str) -> Result<String> {
    get_text_with(url, &[("User-Agent", UA)], TIMEOUT).await
}

/// `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    const SET: &AsciiSet =
        &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'!').remove(b'~').remove(b'*').remove(b'\'').remove(b'(').remove(b')');
    utf8_percent_encode(s, SET).to_string()
}

/// `Number(x)` for a JSON value (`None` = `undefined`).
pub fn js_number(v: Option<&Value>) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => string_to_number(s),
        Some(Value::Array(a)) => match a.len() {
            0 => 0.0,
            // Number([x]) = Number(String(x)).
            1 => match &a[0] {
                Value::Array(_) => js_number(Some(&a[0])),
                Value::Null => 0.0,
                Value::String(s) => string_to_number(s),
                Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
                Value::Bool(_) | Value::Object(_) => f64::NAN,
            },
            _ => f64::NAN,
        },
        Some(Value::Object(_)) => f64::NAN,
    }
}

fn string_to_number(s: &str) -> f64 {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(d) = t.strip_prefix(prefix) {
            return if !d.is_empty() && d.chars().all(|c| c.is_digit(radix)) {
                d.chars().fold(0.0, |acc, c| acc * radix as f64 + c.to_digit(radix).unwrap() as f64)
            } else {
                f64::NAN
            };
        }
    }
    // Rust accepts "inf", "nan", "infinity": JavaScript does not.
    if t.bytes().any(|b| !(b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))) {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

// ---------- Parsers (formats checked on real responses) ----------

static CDATA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!\[CDATA\[(.*?)\]\]>").unwrap());
static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<item>(.*?)</item>").unwrap());
static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<title>(.*?)</title>").unwrap());
static PUB_DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<pubDate>(.*?)</pubDate>").unwrap());
static SOURCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<source[^>]*>(.*?)</source>").unwrap());

fn decode(s: &str) -> String {
    let s = CDATA.replace_all(s, "${1}");
    let s = s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'");
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}').to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct StockTwits {
    pub bullish: Option<f64>,
    pub sample: usize,
}

fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn array(v: Option<&Value>) -> &[Value] {
    v.and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}

/// `parseGuard`.
pub mod parse_guard {
    use super::*;

    /// OKX current funding rate: {"code":"0","data":[{"fundingRate":"-0.0000101"}]}.
    pub fn funding(d: &Value) -> Option<f64> {
        let v = js_number(d.get("data").and_then(|x| x.get(0)).and_then(|x| x.get("fundingRate")));
        if d.get("code").and_then(|c| c.as_str()) == Some("0") && v.is_finite() { Some(v) } else { None }
    }

    /// OKX rubik series [[ts, value, …]], most recent first → values oldest first.
    pub fn rubik(d: &Value, column: usize) -> Vec<f64> {
        if d.get("code").and_then(|c| c.as_str()) != Some("0") {
            return vec![];
        }
        let mut v: Vec<f64> = array(d.get("data")).iter().map(|r| js_number(r.get(column))).filter(|x| x.is_finite()).collect();
        v.reverse();
        v
    }

    /// alternative.me Fear & Greed history, most recent first → oldest first.
    pub fn fear_greed(d: &Value) -> Vec<f64> {
        let mut v: Vec<f64> = array(d.get("data")).iter().map(|x| js_number(x.get("value"))).filter(|x| x.is_finite()).collect();
        v.reverse();
        v
    }

    pub fn stocktwits(d: &Value) -> StockTwits {
        let tags: Vec<&Value> = array(d.get("messages"))
            .iter()
            .filter_map(|m| m.get("entities").and_then(|e| e.get("sentiment")).and_then(|s| s.get("basic")))
            .filter(|t| js_truthy(t))
            .collect();
        let bullish = if tags.is_empty() {
            None
        } else {
            Some((tags.iter().filter(|t| t.as_str() == Some("Bullish")).count() as f64 / tags.len() as f64) * 100.0)
        };
        StockTwits { bullish, sample: tags.len() }
    }

    /// RSS 2.0 (Google News and others): title, date and source of each item.
    pub fn rss(xml: &str) -> Vec<NewsItem> {
        ITEM.captures_iter(xml)
            .filter_map(|cap| {
                let item = cap.get(1).map_or("", |m| m.as_str());
                let title = decode(TITLE.captures(item).and_then(|c| c.get(1)).map_or("", |m| m.as_str()));
                let time = parse_date(PUB_DATE.captures(item).and_then(|c| c.get(1)).map_or("", |m| m.as_str()));
                let source = decode(SOURCE.captures(item).and_then(|c| c.get(1)).map_or("", |m| m.as_str()));
                match time {
                    Some(time) if !title.is_empty() => Some(NewsItem { title, time, source: if source.is_empty() { None } else { Some(source) } }),
                    _ => None,
                }
            })
            .collect()
    }

    /// Cboe VIX history: {"data": [{"date", "close": "17.24"}]} → closes, oldest first.
    pub fn vix(d: &Value) -> Vec<f64> {
        let a = array(d.get("data"));
        a[a.len().saturating_sub(60)..].iter().map(|x| js_number(x.get("close"))).filter(|x| x.is_finite()).collect()
    }
}

// ---------- Fetchers ----------

pub const FEAR_GREED_URL: &str = "https://api.alternative.me/fng/?limit=30";
pub const VIX_URL: &str = "https://cdn.cboe.com/api/global/delayed_quotes/charts/historical/_VIX.json";

/// OKX funding rate, long/short ratio and open interest URLs of a crypto.
pub fn okx_urls(base: &str) -> [String; 3] {
    [
        format!("https://www.okx.com/api/v5/public/funding-rate?instId={base}-USDT-SWAP"),
        format!("https://www.okx.com/api/v5/rubik/stat/contracts/long-short-account-ratio?ccy={base}&period=1H"),
        format!("https://www.okx.com/api/v5/rubik/stat/contracts/open-interest-volume?ccy={base}&period=1H"),
    ]
}

pub fn stocktwits_url(symbol: &str, kind: Kind) -> String {
    let s = if kind == Kind::Crypto { format!("{symbol}.X") } else { symbol.to_string() };
    format!("https://api.stocktwits.com/api/2/streams/symbol/{}.json", encode_uri_component(&s))
}

/// Positioning from the three OKX answers (`None` = request failed); `None` when none of them is usable.
pub fn assemble_positioning(funding: Option<&Value>, ls: Option<&Value>, oi: Option<&Value>) -> Option<Positioning> {
    let funding = funding.and_then(parse_guard::funding);
    let ls = ls.map(|d| parse_guard::rubik(d, 1));
    let oi = oi.map(|d| parse_guard::rubik(d, 1));
    if funding.is_none() && ls.as_ref().is_none_or(|v| v.is_empty()) && oi.as_ref().is_none_or(|v| v.is_empty()) {
        return None;
    }
    Some(Positioning { funding_rate: funding, long_short_ratio: ls.unwrap_or_default(), open_interest: oi.unwrap_or_default() })
}

/// Sentiment from the Fear & Greed history and the StockTwits count (`None` = unavailable).
pub fn assemble_sentiment(fg: Option<Vec<f64>>, st: Option<StockTwits>) -> SentimentInput {
    SentimentInput {
        fear_greed: fg.unwrap_or_default(),
        social_bullish: st.and_then(|s| s.bullish),
        social_sample: st.map(|s| s.sample as f64).unwrap_or(0.0),
    }
}

async fn okx_positioning(base: String) -> Result<Option<Positioning>> {
    let [u1, u2, u3] = okx_urls(&base);
    let (funding, ls, oi) = tokio::join!(get_json(&u1), get_json(&u2), get_json(&u3));
    Ok(assemble_positioning(funding.ok().as_ref(), ls.ok().as_ref(), oi.ok().as_ref()))
}

pub async fn positioning(base: &str) -> Option<Positioning> {
    let b = base.to_string();
    cached(&format!("pos:{base}"), 300_000, move || okx_positioning(b)).await.ok().and_then(|p| (*p).clone())
}

pub async fn sentiment_input(symbol: &str, kind: Kind) -> SentimentInput {
    let url = stocktwits_url(symbol, kind);
    let (fg, st) = tokio::join!(
        async {
            if kind != Kind::Crypto {
                return None;
            }
            cached("fng30", 1_800_000, || async { Ok(parse_guard::fear_greed(&get_json(FEAR_GREED_URL).await?)) }).await.ok()
        },
        async {
            cached(&format!("st2:{}:{symbol}", kind.as_str()), 300_000, move || async move { Ok(parse_guard::stocktwits(&get_json(&url).await?)) })
                .await
                .ok()
        },
    );
    assemble_sentiment(fg.map(|v| (*v).clone()), st.map(|s| *s))
}

static COMPANY_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i),? (Inc|Corp|Corporation|Ltd|plc)\.?$").unwrap());

/// Google News query of an asset (`q` in `news`).
pub fn news_query(symbol: &str, kind: Kind, name: &str) -> String {
    if kind == Kind::Crypto {
        format!("\"{name}\" crypto")
    } else {
        format!("{} stock \"{}\"", symbol.replace('-', "."), COMPANY_SUFFIX.replace(name, ""))
    }
}

pub fn news_url(symbol: &str, kind: Kind, name: &str) -> String {
    let q = news_query(symbol, kind, name);
    format!("https://news.google.com/rss/search?q={}&hl=en-US&gl=US&ceid=US:en", encode_uri_component(&format!("{q} when:7d")))
}

pub async fn news(symbol: &str, kind: Kind, name: &str) -> Vec<NewsItem> {
    let url = news_url(symbol, kind, name);
    cached(&format!("news:{}:{symbol}", kind.as_str()), 600_000, move || async move { Ok(parse_guard::rss(&get_text(&url).await?)) })
        .await
        .map(|v| (*v).clone())
        .unwrap_or_default()
}

pub async fn vix() -> Vec<f64> {
    cached("vix", 3_600_000, || async { Ok(parse_guard::vix(&get_json(VIX_URL).await?)) }).await.map(|v| (*v).clone()).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardInputsSummary {
    pub funding_rate: Option<f64>,
    pub long_short_ratio: Option<f64>,
    pub open_interest_usd: Option<f64>,
    pub fear_greed: Option<f64>,
    pub social_bullish: Option<f64>,
    pub social_sample: f64,
    pub news24h: usize,
    pub news_tone: NewsTone,
    pub headlines: Vec<NewsItem>,
    pub vix: Option<f64>,
}

/// `MacroReport & { evidence }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MacroWithEvidence {
    #[serde(flatten)]
    pub report: MacroReport,
    pub evidence: Option<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardReport {
    #[serde(flatten)]
    pub result: GuardResult,
    pub symbol: String,
    pub kind: Kind,
    pub as_of: i64,
    pub price: Option<f64>,
    pub inputs: GuardInputsSummary,
    /// Macro / geopolitical context and what its stress announced on this asset.
    #[serde(rename = "macro")]
    pub macro_ctx: Option<MacroWithEvidence>,
}

/// Pure part of `guardReport`: the guard and the report from inputs already fetched.
pub fn build_report(
    symbol: &str,
    kind: Kind,
    now: i64,
    daily: &[Candle],
    h4: &[Candle],
    h1: &[Candle],
    pos: Option<&Positioning>,
    sent: &SentimentInput,
    items: &[NewsItem],
    v: &[f64],
    mac: Option<&MacroContext>,
) -> GuardReport {
    let result =
        guard(&GuardInput { kind, daily, h4, h1, positioning: pos, sentiment: Some(sent), news: Some(items), vix: Some(v), macro_ctx: mac, now });
    let day: Vec<&NewsItem> = items.iter().filter(|n| n.time >= now - 86_400_000 && n.time <= now).collect();
    let mut headlines: Vec<NewsItem> = day.iter().map(|n| (*n).clone()).collect();
    headlines.sort_by(|a, b| b.time.cmp(&a.time));
    headlines.truncate(5);
    GuardReport {
        result,
        symbol: symbol.to_string(),
        kind,
        as_of: now,
        price: h1.last().or(h4.last()).or(daily.last()).map(|c| c.close),
        inputs: GuardInputsSummary {
            funding_rate: pos.and_then(|p| p.funding_rate),
            long_short_ratio: pos.and_then(|p| p.long_short_ratio.last().copied()),
            open_interest_usd: pos.and_then(|p| p.open_interest.last().copied()),
            fear_greed: sent.fear_greed.last().copied(),
            social_bullish: sent.social_bullish,
            social_sample: sent.social_sample,
            news24h: day.len(),
            news_tone: news_tone(day.iter().copied()),
            headlines,
            vix: v.last().copied(),
        },
        macro_ctx: mac.map(|m| MacroWithEvidence { report: m.report.clone(), evidence: m.evidence }),
    }
}

/// Full guard for one asset; candles come from the multi-source consensus (injected, already cached). The daily and
/// 4 h candles are required (their error is returned), the hourly ones and the macro context are optional.
pub async fn guard_report<F, Fut, M>(symbol: &str, kind: Kind, name: &str, candles: F, now: i64, macro_context: M) -> Result<GuardReport>
where
    F: Fn(Interval) -> Fut,
    Fut: Future<Output = Result<Vec<Candle>>>,
    M: Future<Output = Result<Option<MacroContext>>>,
{
    let (daily, h4, h1, pos, sent, items, v, mac) = tokio::join!(
        candles(Interval::D1),
        candles(Interval::H4),
        candles(Interval::H1),
        async { if kind == Kind::Crypto { positioning(symbol).await } else { None } },
        sentiment_input(symbol, kind),
        news(symbol, kind, name),
        async { if kind == Kind::Stock { vix().await } else { vec![] } },
        macro_context,
    );
    let (daily, h4) = (daily?, h4?);
    let h1 = h1.unwrap_or_default();
    let mac = mac.ok().flatten();
    Ok(build_report(symbol, kind, now, &daily, &h4, &h1, pos.as_ref(), &sent, &items, &v, mac.as_ref()))
}

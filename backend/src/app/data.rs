//! Data behind the routes (the "Data" part of app.ts): cached snapshots, long history, macro context, guard,
//! selection, buy zones, quotes, radar items, buy alerts, news, search, sentiment.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;

use super::extras::extras;
use crate::cache::cached;
use crate::engine::Evidence;
use crate::engine::alerts::{AlertInput, AlertMacro, AlertShock, AlertSignal, AlertTrend, AlertZone, BuyAlert, buy_alert};
use crate::engine::decision::{DecisionInput, ExposureInput, HoldingSeries, MarketInputs, decide};
use crate::engine::decision_types::Decision;
use crate::engine::fibonacci::{FibZone, fib_zones};
use crate::engine::guard::{Direction, MacroContext, ShockLevel, Trend};
use crate::engine::macro_ctx::{MacroLevel, MacroReport, macro_advice, macro_evidence};
use crate::engine::news::{FeedItems, MAX_AGE_MS, NewsDigest, NewsItem, aggregate, news_digest, top_stories};
use crate::engine::reliability::{Reliability, gate};
use crate::engine::screener::{Horizon, spec};
use crate::engine::signal::{Action, AnalyzeOptions, analyze};
use crate::engine::structure::Benchmark;
use crate::engine::synthesis::{MarketRegime, ScoreWeights, market_regime};
use crate::guard::{GuardReport, guard_report};
use crate::http::{Error, Result};
use crate::js::now_ms;
use crate::live::us_market_open;
use crate::market::{Consensus, Snapshot, long_daily, snapshot};
use crate::quotes::{ConsensusQuote, QUOTE_SOURCES, consensus_quotes, make_asset};
use crate::screener::{ScreenResult, Verified, screen};
use crate::types::{Asset, Candle, Interval, Kind};
use crate::universe::{UniverseEntry, crypto_universe, search_all, stock_universe, universe};

/// Only closed candles are analysed: a snapshot changes at most once per candle.
fn snap_ttl(i: Interval) -> i64 {
    match i {
        Interval::H1 => 60_000,
        Interval::H4 => 120_000,
        Interval::D1 => 300_000,
    }
}

pub async fn snap(symbol: &str, kind: Kind, interval: Interval) -> Result<Arc<Snapshot>> {
    let s = symbol.to_string();
    cached(
        &format!("snap:{}:{symbol}:{}", kind.as_str(), interval.as_str()),
        snap_ttl(interval),
        || async move { snapshot(&s, kind, interval).await },
    )
    .await
}

/// Long daily history (≈ 3 years), for the long-term horizon and the macro evidence.
pub async fn long(symbol: &str, kind: Kind) -> Result<Arc<Consensus>> {
    let s = symbol.to_string();
    cached(&format!("long:{}:{symbol}", kind.as_str()), 3_600_000, || async move { long_daily(&s, kind).await }).await
}

pub async fn macro_now() -> Result<Arc<MacroReport>> {
    cached("macro:report", 300_000, || async { crate::macro_data::report(now_ms()).await }).await
}

/// Macro context + what its stress announced on this asset (its long daily history).
pub async fn macro_context(symbol: &str, kind: Kind) -> Result<(Arc<MacroReport>, Option<Evidence>)> {
    let (report, series, hist) = tokio::join!(macro_now(), crate::macro_data::macro_series(), long(symbol, kind));
    let (report, series) = (report?, series?);
    let evidence = match hist {
        Ok(h) => {
            *cached(&format!("macroEv:{}:{symbol}", kind.as_str()), 3_600_000, || async move { Ok::<_, Error>(macro_evidence(&h.candles, &series)) })
                .await?
        }
        Err(_) => None,
    };
    Ok((report, evidence))
}

/// Full market guard of an asset (cached 60 s).
pub async fn guard_for(symbol: &str, kind: Kind) -> Result<Arc<GuardReport>> {
    let list = universe(kind).await.ok();
    let name =
        list.as_ref().and_then(|l| l.iter().find(|e| e.0 == symbol).map(|e| e.1.clone())).unwrap_or_else(|| make_asset(symbol, kind, None).name);
    let s = symbol.to_string();
    cached(&format!("guard:{}:{symbol}", kind.as_str()), 60_000, || async move {
        let sym = s.clone();
        let candles = move |i: Interval| {
            let sym = sym.clone();
            async move { snap(&sym, kind, i).await.map(|x| x.candles.clone()) }
        };
        let ctx = async {
            let (report, evidence) = macro_context(&s, kind).await?;
            Ok(Some(MacroContext { report: (*report).clone(), evidence }))
        };
        guard_report(&s, kind, &name, candles, now_ms(), ctx).await
    })
    .await
}

/// Selection of a market and horizon (screener.rs), finalists checked with the consensus and the guard. Cached
/// according to the duration (4 min for 30 minutes, 30 min for daily horizons).
pub async fn selection(h: Horizon, market: Kind) -> Result<Arc<ScreenResult>> {
    let ttl = spec(market, h).interval.ttl() as i64;
    cached(&format!("selection:{}:{}", market.as_str(), h.as_str()), ttl, || async move {
        let verify = move |symbol: String, interval: crate::engine::screener::CandleInterval| async move {
            let iv = if interval == crate::engine::screener::CandleInterval::D1 { Interval::D1 } else { Interval::H1 };
            let (d, g) = tokio::join!(snap(&symbol, market, iv), guard_for(&symbol, market));
            let d = d?;
            let g = g.ok();
            Ok(Verified {
                reliability: d.reliability.level,
                shock: g
                    .as_ref()
                    .map_or("calm", |g| match g.result.shock.level {
                        ShockLevel::Calm => "calm",
                        ShockLevel::Agitated => "agitated",
                        ShockLevel::Shock => "shock",
                    })
                    .to_string(),
                reversal_down: g.as_ref().is_some_and(|g| g.result.reversal.direction == Some(Direction::Down) && g.result.reversal.score >= 50.0),
                daily: d.candles.clone(),
            })
        };
        screen(h, verify, market, 10, !us_market_open(now_ms())).await
    })
    .await
}

/// Production: the daily selections and the coming days of the calendar are computed at start-up and every 25 minutes,
/// so nobody waits.
pub fn warm_selections() {
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let mut every = tokio::time::interval(Duration::from_secs(25 * 60));
        loop {
            every.tick().await;
            // The calendar's macro days, so a decision's announcement check does not wait on a cold Nasdaq (≈ 2 s a day).
            let _ = crate::calendar::upcoming_for("BTC", Kind::Crypto, 7).await;
            // Daily horizons ahead of time; intraday ones are computed on demand (they go stale within minutes).
            for m in [Kind::Stock, Kind::Crypto] {
                for h in [Horizon::Mo1, Horizon::Mo3, Horizon::Mo6, Horizon::D7, Horizon::D14] {
                    let _ = selection(h, m).await;
                }
            }
        }
    });
}

pub fn quotes_key(assets: &[Asset]) -> String {
    let mut keys: Vec<String> = assets.iter().map(|a| format!("{}:{}", a.kind.as_str(), a.symbol)).collect();
    keys.sort();
    keys.join(",")
}

pub async fn quotes(assets: &[Asset]) -> Result<Arc<Vec<ConsensusQuote>>> {
    let list = assets.to_vec();
    cached(&format!("quotes:{}", quotes_key(assets)), 15_000, || async move { Ok(consensus_quotes(&list, &QUOTE_SOURCES).await) }).await
}

// ---------- Buy zones ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneOut {
    #[serde(flatten)]
    pub zone: FibZone,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_note: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MacroOut {
    #[serde(flatten)]
    pub report: MacroReport,
    pub evidence: Option<Evidence>,
    /// Risk-on / risk-off / neutre (macro stress and the S&P 500's trend).
    pub regime: Option<MarketRegime>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Zones {
    pub symbol: String,
    pub kind: Kind,
    pub price: Option<f64>,
    pub as_of: i64,
    pub zones: Vec<ZoneOut>,
    #[serde(rename = "macro")]
    pub macro_ctx: Option<MacroOut>,
}

/// Buy zones by horizon (Fibonacci) with the macro context.
pub async fn zones(symbol: &str, kind: Kind) -> Result<Zones> {
    let asset = make_asset(symbol, kind, None);
    let (h4, d, hist, q, ctx) = tokio::join!(
        snap(symbol, kind, Interval::H4),
        snap(symbol, kind, Interval::D1),
        long(symbol, kind),
        quotes(std::slice::from_ref(&asset)),
        macro_context(symbol, kind)
    );
    let (h4, d) = (h4?, d?);
    let hist = hist.ok();
    let ctx = ctx.ok();
    let price = q.ok().and_then(|q| q.first().map(|x| x.price)).or_else(|| h4.candles.last().map(|c| c.close));
    let long_candles: Vec<_> = hist.as_ref().map(|h| h.candles.clone()).unwrap_or_default();
    let (h4c, dc, lc) = (h4.candles.clone(), d.candles.clone(), long_candles.clone());
    let computed =
        cached(&format!("zones:{}:{symbol}", kind.as_str()), 300_000, || async move { Ok::<_, Error>(fib_zones(&h4c, &dc, &lc, price, true)) })
            .await?;
    // Status and distance follow the current price (the levels only change with the candles).
    let current: Vec<FibZone> = if price.is_some_and(|p| p != 0.0 && !p.is_nan()) {
        fib_zones(&h4.candles, &d.candles, &long_candles, price, false)
            .into_iter()
            .enumerate()
            .map(|(i, z)| FibZone { evidence: computed.get(i).and_then(|c| c.evidence), ..z })
            .collect()
    } else {
        (*computed).clone()
    };
    let level = ctx.as_ref().map_or(MacroLevel::Calm, |c| c.0.level);
    let regime = match &ctx {
        Some((r, _)) => macro_regime(r).await,
        None => None,
    };
    Ok(Zones {
        symbol: symbol.to_string(),
        kind,
        price,
        as_of: now_ms(),
        zones: current.into_iter().map(|z| ZoneOut { macro_note: macro_advice(level, z.horizon.as_str()), zone: z }).collect(),
        macro_ctx: ctx.map(|(report, evidence)| MacroOut { report: (*report).clone(), evidence, regime }),
    })
}

// ---------- Radar ----------

#[derive(Debug, Clone, Serialize)]
pub struct RadarSignal {
    pub action: Action,
    pub score: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarOk {
    pub symbol: String,
    pub kind: Kind,
    pub signal: Option<RadarSignal>,
    pub reliability: Reliability,
    pub agreeing: usize,
    pub sources: usize,
    pub sparkline: Vec<f64>,
    pub last_close: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RadarErr {
    pub symbol: String,
    pub kind: Kind,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RadarItem {
    Ok(RadarOk),
    Err(RadarErr),
}

pub async fn radar_item(asset: &Asset, interval: Interval) -> RadarItem {
    let hi = interval.higher();
    let (s, higher) = tokio::join!(snap(&asset.symbol, asset.kind, interval), async {
        match hi {
            Some(h) => snap(&asset.symbol, asset.kind, h).await.ok(),
            None => None,
        }
    });
    let s = match s {
        Ok(s) => s,
        Err(e) => return RadarItem::Err(RadarErr { symbol: asset.symbol.clone(), kind: asset.kind, error: e.0 }),
    };
    let raw =
        analyze(&s.candles, &AnalyzeOptions { higher: higher.as_ref().map(|h| h.candles.as_slice()), interval_ms: Some(interval.step()), now: None });
    let signal = raw.map(|r| gate(&r, &s.reliability, &s.quality.issues));
    RadarItem::Ok(RadarOk {
        symbol: asset.symbol.clone(),
        kind: asset.kind,
        signal: signal.map(|g| RadarSignal { action: g.action, score: g.score, confidence: g.confidence }),
        reliability: s.reliability.clone(),
        agreeing: s.agreeing,
        sources: s.sources.len(),
        sparkline: s.candles[s.candles.len().saturating_sub(48)..].iter().map(|c| c.close).collect(),
        last_close: s.candles.last().map(|c| c.close),
    })
}

// ---------- "Can I buy now?" ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertOk {
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    pub price: Option<f64>,
    pub as_of: i64,
    #[serde(flatten)]
    pub alert: BuyAlert,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum AlertItem {
    Ok(AlertOk),
    Err { symbol: String, kind: Kind, name: String, error: String },
}

fn shock_of(l: ShockLevel) -> AlertShock {
    match l {
        ShockLevel::Calm => AlertShock::Calm,
        ShockLevel::Agitated => AlertShock::Agitated,
        ShockLevel::Shock => AlertShock::Shock,
    }
}
fn trend_of(t: Trend) -> AlertTrend {
    match t {
        Trend::Up => AlertTrend::Up,
        Trend::Down => AlertTrend::Down,
        Trend::Range => AlertTrend::Range,
    }
}
fn macro_of(l: MacroLevel) -> AlertMacro {
    match l {
        MacroLevel::Calm => AlertMacro::Calm,
        MacroLevel::Tense => AlertMacro::Tense,
        MacroLevel::High => AlertMacro::High,
    }
}

/// "Can I buy now?" for each asset: the rule of the notifications (iPhone, Apple Watch, Android) and of the brief.
pub async fn alerts_for(assets: &[Asset]) -> Vec<AlertItem> {
    join_all(assets.iter().map(|a| async move {
        let (r, z, g) = tokio::join!(radar_item(a, Interval::H4), zones(&a.symbol, a.kind), guard_for(&a.symbol, a.kind));
        let (z, g) = (z.ok(), g.ok());
        if let (RadarItem::Err(e), None) = (&r, &z) {
            return AlertItem::Err { symbol: a.symbol.clone(), kind: a.kind, name: a.name.clone(), error: e.error.clone() };
        }
        let ok = match &r {
            RadarItem::Ok(o) => Some(o),
            RadarItem::Err(_) => None,
        };
        let price = z.as_ref().and_then(|z| z.price).or_else(|| ok.and_then(|o| o.last_close));
        let alert = buy_alert(&AlertInput {
            symbol: a.symbol.clone(),
            name: a.name.clone(),
            price,
            signal: ok.and_then(|o| o.signal.as_ref()).map(|s| AlertSignal { action: s.action, confidence: s.confidence }),
            reliability: ok.map(|o| o.reliability.level),
            zones: z.as_ref().map(|z| z.zones.iter().map(|x| AlertZone::from(&x.zone)).collect()).unwrap_or_default(),
            shock: g.as_ref().map(|g| shock_of(g.result.shock.level)),
            trend: g.as_ref().map(|g| trend_of(g.result.regime.trend)),
            macro_level: z.as_ref().and_then(|z| z.macro_ctx.as_ref()).map(|m| macro_of(m.report.level)),
        });
        AlertItem::Ok(AlertOk { symbol: a.symbol.clone(), kind: a.kind, name: a.name.clone(), price, as_of: now_ms(), alert })
    }))
    .await
}

// ---------- News ----------

#[derive(Debug, Clone, Serialize)]
pub struct SourceCount {
    pub name: String,
    pub ok: bool,
    pub count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsReport {
    pub as_of: i64,
    pub items: Vec<NewsItem>,
    pub top: Vec<String>,
    pub digest: NewsDigest,
    pub sources: Vec<SourceCount>,
}

/// News section of these assets (cached 5 minutes per list of assets).
pub async fn news_report(assets: &[Asset]) -> Result<Arc<NewsReport>> {
    let list = assets.to_vec();
    cached(&format!("newsReport:{}", quotes_key(assets)), 300_000, || async move {
        let fetched = crate::news::fetch_news(&list).await;
        let now = now_ms();
        let mut by_source: IndexMap<String, SourceCount> = IndexMap::new();
        for r in &fetched.results {
            let name = r.feed.name.clone();
            let prev = by_source.get(&name);
            let entry = SourceCount {
                name: name.clone(),
                ok: prev.is_some_and(|p| p.ok) || r.ok,
                count: prev.map_or(0, |p| p.count) + r.items.len(),
                error: if r.ok { None } else { prev.and_then(|p| p.error.clone()).or_else(|| r.error.clone()) },
            };
            by_source.insert(name, entry);
        }
        let feeds: Vec<FeedItems> =
            fetched.results.iter().map(|r| FeedItems { category: r.feed.category, fallback: r.feed.fallback, items: &r.items }).collect();
        let items = aggregate(&feeds, &fetched.watch, now, MAX_AGE_MS);
        Ok::<_, Error>(NewsReport {
            as_of: now,
            top: top_stories(&items, 5).into_iter().map(|i| i.id).collect(),
            digest: news_digest(&items, now),
            items,
            sources: by_source.into_values().collect(),
        })
    })
    .await
}

// ---------- Search ----------

pub fn to_item(e: &UniverseEntry, kind: Kind) -> Value {
    let mut o = serde_json::json!({ "symbol": e.0, "name": e.1, "kind": kind, "rank": if e.2 != 0 { Value::from(e.2) } else { Value::Null } });
    match kind {
        Kind::Crypto => o["exchanges"] = Value::from(e.3),
        Kind::Stock => o["etf"] = Value::from(e.3 == 1),
    }
    o
}

/// Search across the full universe (every crypto, every US-listed stock / ETF): exact symbols first, then the largest.
pub async fn search_assets(q: &str, limit: usize) -> Vec<Value> {
    let (c, s) = tokio::join!(crypto_universe(), stock_universe());
    let c = c.unwrap_or_default();
    let s = s.unwrap_or_default();
    search_all(&c, &s, q, limit).into_iter().map(|h| to_item(h.e, h.kind)).collect()
}

// ---------- Sentiment ----------

#[derive(Debug, Clone, Serialize)]
pub struct FearGreed {
    pub value: f64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Social {
    pub bullish_percent: Option<f64>,
    pub sample: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sentiment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fear_greed: Option<FearGreed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub social: Option<Social>,
}

pub async fn sentiment(symbol: &str, kind: Kind) -> Sentiment {
    let fng = async {
        if kind != Kind::Crypto {
            return None;
        }
        cached("fng", 600_000, || async {
            let d = crate::http::get_json("https://api.alternative.me/fng/?limit=1").await?;
            let v = crate::jsval::number(d.pointer("/data/0/value"));
            let label = if v < 25.0 {
                "Peur extrême"
            } else if v < 45.0 {
                "Peur"
            } else if v <= 55.0 {
                "Neutre"
            } else if v <= 75.0 {
                "Avidité"
            } else {
                "Avidité extrême"
            };
            Ok::<_, Error>(FearGreed { value: v, label: label.into() })
        })
        .await
        .ok()
        .map(|v| (*v).clone())
    };
    let s = symbol.to_string();
    let key = format!("st:{}:{symbol}", kind.as_str());
    let social = cached(&key, 300_000, || async move {
        let id = if kind == Kind::Crypto { format!("{s}.X") } else { s };
        let d = crate::http::get_json(&format!("https://api.stocktwits.com/api/2/streams/symbol/{}.json", crate::jsval::encode_uri_component(&id)))
            .await?;
        let tags: Vec<String> = d
            .get("messages")
            .and_then(Value::as_array)
            .map(|m| {
                m.iter()
                    .filter_map(|m| m.pointer("/entities/sentiment/basic").and_then(Value::as_str).filter(|t| !t.is_empty()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let bull = tags.iter().filter(|t| *t == "Bullish").count();
        Ok::<_, Error>(Social {
            bullish_percent: if tags.is_empty() { None } else { Some(bull as f64 / tags.len() as f64 * 100.0) },
            sample: tags.len(),
        })
    });
    let (fear_greed, social) = tokio::join!(fng, social);
    Sentiment { fear_greed, social: social.ok().map(|v| (*v).clone()) }
}

/// `movers` input: the last daily close of each asset (`kind:symbol` → close).
pub type Closes = HashMap<String, Option<f64>>;

// ---------- Decision ----------

/// One line of the user's portfolio (`weights=SYM:kind:pct`): used for one computation, never stored nor logged.
#[derive(Debug, Clone, PartialEq)]
pub struct Weight {
    pub symbol: String,
    pub kind: Kind,
    pub pct: f64,
}

/// Name of an asset from the universe (its symbol when unknown).
async fn asset_name(symbol: &str, kind: Kind) -> String {
    let list = universe(kind).await.ok();
    list.as_ref().and_then(|l| l.iter().find(|e| e.0 == symbol).map(|e| e.1.clone())).unwrap_or_else(|| make_asset(symbol, kind, None).name)
}

/// The user's exposure inputs: every line's long daily history and the factor's (Bitcoin, or the S&P 500 via SPY).
async fn exposure_input(kind: Kind, weights: &[Weight]) -> Option<ExposureInput> {
    if weights.is_empty() {
        return None;
    }
    let (fs, factor) = match kind {
        Kind::Crypto => ("BTC", "Bitcoin"),
        Kind::Stock => ("SPY", "S&P 500"),
    };
    let (f, lines) = tokio::join!(long(fs, kind), join_all(weights.iter().map(|w| async move { (w, long(&w.symbol, w.kind).await.ok()) })));
    let f = f.ok()?;
    Some(ExposureInput {
        factor: factor.into(),
        factor_symbol: fs.into(),
        factor_daily: f.candles.clone(),
        holdings: lines
            .into_iter()
            .filter_map(|(w, h)| h.map(|h| HoldingSeries { symbol: w.symbol.clone(), kind: w.kind, weight: w.pct, daily: h.candles.clone() }))
            .collect(),
    })
}

/// Benchmarks of the technical structure and the market regime: Bitcoin for a crypto; the S&P 500 (SPY) then the
/// Nasdaq-100 (QQQ, the Nasdaq Composite itself is not in the candle sources) for a stock. Long daily histories,
/// cached one hour like every `long`; one that fails is left out.
async fn benchmarks(kind: Kind) -> Vec<Benchmark> {
    let list: &[(&str, &str)] = match kind {
        Kind::Crypto => &[("BTC", "Bitcoin (BTC)")],
        Kind::Stock => &[("SPY", "S&P 500 (SPY)"), ("QQQ", "Nasdaq-100 (QQQ)")],
    };
    join_all(list.iter().map(|(symbol, name)| async move {
        long(symbol, kind).await.ok().map(|h| Benchmark { name: (*name).into(), symbol: (*symbol).into(), kind, daily: h.candles.clone() })
    }))
    .await
    .into_iter()
    .flatten()
    .collect()
}

/// Market regime (risk-on / risk-off / neutre) of `/api/macro`: the macro stress and the S&P 500's trend (its daily
/// closes from the macro series).
pub async fn macro_regime(report: &MacroReport) -> Option<MarketRegime> {
    let series = crate::macro_data::macro_series().await.ok();
    let closes: Vec<f64> = series.as_ref().and_then(|s| s.spx.as_ref()).map(|p| p.iter().map(|x| x.close).collect()).unwrap_or_default();
    market_regime(Some(report), "S&P 500", &closes)
}

/// Decision for one asset (`/api/decision`). Every input is cached on its own (candles, zones, guard, macro, news,
/// extras); the personal parts (average cost, weights) are only used for this answer, never in a cache key.
pub async fn decision_for(symbol: &str, kind: Kind, cost: Option<f64>, weights: &[Weight], score_weights: Option<ScoreWeights>) -> Result<Decision> {
    let name = asset_name(symbol, kind).await;
    // The calendar is slow when cold (≈ 2 s per Nasdaq day): past 5 s the decision goes without it and says so, the
    // fetches keep filling the cache for the next call. A calendar with a failed source counts as not loaded.
    let events =
        async { tokio::time::timeout(std::time::Duration::from_secs(5), crate::calendar::upcoming_for(symbol, kind, 7)).await.ok().flatten() };
    let (h1, h4, d, hist, z, g, news, expo, bench, events) = tokio::join!(
        snap(symbol, kind, Interval::H1),
        snap(symbol, kind, Interval::H4),
        snap(symbol, kind, Interval::D1),
        long(symbol, kind),
        zones(symbol, kind),
        guard_for(symbol, kind),
        crate::guard::news(symbol, kind, &name),
        exposure_input(kind, weights),
        benchmarks(kind),
        events,
    );
    let (h4, d) = (h4?, d?);
    let (h1, hist, z, g) = (h1.ok(), hist.ok(), z.ok(), g.ok());
    let price = z.as_ref().and_then(|z| z.price);
    let long_candles: Vec<Candle> = hist.as_ref().map(|h| h.candles.clone()).unwrap_or_default();
    let ex = {
        let s = symbol.to_string();
        let daily = if long_candles.len() > d.candles.len() { long_candles.clone() } else { d.candles.clone() };
        cached(&format!("extras:{}:{symbol}", kind.as_str()), 600_000, || async move { Ok::<_, Error>(extras(&s, kind, price, &daily).await) })
            .await
            .ok()
    };
    let (fundamentals, liquidity, track) = ex.map(|e| (*e).clone()).unwrap_or((None, None, None));
    let reliability = if h4.reliability.score < d.reliability.score { h4.reliability.clone() } else { d.reliability.clone() };
    let mut issues = d.quality.issues.clone();
    issues.extend(h4.quality.issues.iter().filter(|i| !d.quality.issues.contains(i)).cloned());
    let macro_ctx = z.as_ref().and_then(|z| z.macro_ctx.as_ref());
    let inputs = g.as_ref().map(|g| &g.inputs);
    let input = DecisionInput {
        symbol,
        kind,
        name: &name,
        now: now_ms(),
        price,
        h1: h1.as_ref().map(|s| s.candles.as_slice()).unwrap_or(&[]),
        h4: &h4.candles,
        daily: &d.candles,
        long: &long_candles,
        reliability,
        quality_issues: issues,
        agreeing: d.agreeing,
        sources: d.sources.len(),
        zone_evidence: z.as_ref().map(|z| z.zones.iter().filter_map(|x| x.zone.evidence.map(|e| (x.zone.horizon, e))).collect()).unwrap_or_default(),
        guard: g.as_ref().map(|g| &g.result),
        macro_report: macro_ctx.map(|m| &m.report),
        macro_evidence: macro_ctx.and_then(|m| m.evidence),
        market: MarketInputs {
            fear_greed: inputs.and_then(|i| i.fear_greed),
            funding_rate: inputs.and_then(|i| i.funding_rate),
            social_bullish: inputs.and_then(|i| i.social_bullish),
            social_sample: inputs.map_or(0.0, |i| i.social_sample),
            news: Some(news),
        },
        fundamentals,
        liquidity,
        track,
        cost,
        exposure: expo,
        benchmarks: bench,
        score_weights,
        events,
    };
    Ok(decide(&input))
}

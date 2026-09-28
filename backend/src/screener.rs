//! Stock and crypto selection (`web/server/screener.ts`).
//! Stage 1: every asset is scored on its candles (one source, fast) and ranked.
//! Stage 2: the best candidates are checked with the full multi-source consensus, the market guard and their trend;
//! a candidate that fails a check is set aside or put on watch, with the reason.
use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use futures::StreamExt;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::cache::cached;
use crate::engine::backtest::{TrackRecord, backtest_default, track_record};
use crate::engine::reliability::ReliabilityLevel;
use crate::engine::screener::{
    ByCriterion, CRITERIA, CandleInterval, Criterion, Horizon, Market, RankRule, RawFactors, Scored, Spec, Validation, align_series, explain,
    factors_at, is_pegged, pick, roles, score_universe, span, spec, validate,
};
use crate::http::{Error, Result, err, get_json_with};
use crate::js::now_ms;
use crate::jsval::encode_uri_component;
use crate::market::{parse, parse_stock};
use crate::stocks_extra::parse as extra;
use crate::types::{Candle, DAY_MS, Kind};
use crate::universe::crypto_universe;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

async fn get(url: &str) -> Result<Value> {
    get_json_with(url, &[("User-Agent", UA)], Duration::from_secs(12)).await
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    pub symbol: String,
    pub name: String,
    pub sector: String,
    pub market_cap: f64,
}

static SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+(Common Stock|Ordinary Shares|American Depositary Shares?|Class [A-C] (Common|Ordinary|Capital) Stock|Capital Stock|Common Shares).*$").unwrap()
});
static CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(Class|Series) [A-C]\b").unwrap());
static TICKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z][A-Z0-9-]{0,6}$").unwrap());

/// Company names without the listing suffix ("Apple Inc. Common Stock" → "Apple Inc.").
pub fn clean_name(s: &str) -> String {
    crate::engine::news::js_trim(&SUFFIX.replace(s, "")).to_string()
}

fn js_string(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(v) => crate::jsval::to_string(Some(v)),
    }
}

/// Largest US-listed companies, one share class per company (GOOGL kept, GOOG dropped).
pub fn parse_listed(d: &Value, size: usize) -> Vec<Listed> {
    let rows = d.pointer("/data/rows").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut rows: Vec<Listed> = rows
        .iter()
        .map(|r| {
            let symbol = crate::engine::news::js_trim(&crate::jsval::to_string(r.get("symbol"))).replace(['.', '/'], "-");
            let sector = js_string(r.get("sector"));
            let cap = crate::jsval::string_to_number(&js_string(r.get("marketCap")).replace(',', ""));
            Listed {
                symbol,
                name: clean_name(&js_string(r.get("name"))),
                sector: if sector.is_empty() { "Autre".into() } else { sector },
                market_cap: if cap.is_nan() { 0.0 } else { cap },
            }
        })
        .filter(|r| r.market_cap > 0.0 && TICKER.is_match(&r.symbol))
        .collect();
    rows.sort_by(|a, b| b.market_cap.partial_cmp(&a.market_cap).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for r in rows {
        let company: String =
            CLASS.replace_all(&r.name, "").to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect();
        if !seen.insert(company) {
            continue;
        }
        out.push(r);
        if out.len() >= size {
            break;
        }
    }
    out
}

pub fn sector_fr(s: &str) -> String {
    match s {
        "Technology" => "Technologie",
        "Finance" => "Finance",
        "Health Care" => "Santé",
        "Consumer Discretionary" => "Consommation",
        "Industrials" => "Industrie",
        "Energy" => "Énergie",
        "Telecommunications" => "Télécoms",
        "Consumer Staples" => "Biens de base",
        "Basic Materials" => "Matériaux",
        "Real Estate" => "Immobilier",
        "Utilities" => "Services publics",
        "Miscellaneous" => "Divers",
        "Autre" => "Autre",
        other => other,
    }
    .to_string()
}

pub async fn listed() -> Result<Arc<Vec<Listed>>> {
    cached("screener:listed", 12 * 3_600_000, || async {
        Ok(parse_listed(&get("https://api.nasdaq.com/api/screener/stocks?tableonly=true&download=true").await?, 150))
    })
    .await
}

/// Daily candles for selection: Finviz (10 years in one call), Yahoo as fallback; ≈ 10 years kept (2 600 sessions).
pub async fn daily_for(symbol: &str) -> Result<Arc<Vec<Candle>>> {
    let s = symbol.to_string();
    cached(&format!("screener:daily:{symbol}"), 3 * 3_600_000, || async move {
        let id = encode_uri_component(&s);
        if let Ok(d) = get(&format!("https://finviz.com/api/quote.ashx?instrument=stock&ticker={id}&timeframe=d")).await {
            if let Ok(c) = extra::finviz(&d) {
                if c.len() > 300 {
                    return Ok(last(c, 2600));
                }
            }
        }
        let c = parse_stock::yahoo(&get(&format!("https://query1.finance.yahoo.com/v8/finance/chart/{id}?interval=1d&range=10y")).await?)?;
        Ok(last(c, 2600))
    })
    .await
}

fn last(mut c: Vec<Candle>, n: usize) -> Vec<Candle> {
    if c.len() > n {
        c.drain(..c.len() - n);
    }
    c
}

// ---------- Cryptos ----------

/// Pegged or wrapped tokens: they follow a currency, gold or another coin, there is nothing to choose there.
static STABLE: LazyLock<HashSet<&str>> = LazyLock::new(|| {
    "USDT USDC DAI FDUSD TUSD USDE USDS PYUSD USDD BUSD FRAX USD1 RLUSD EURC USDP GUSD LUSD SUSD USDX USDG USD0 BFUSD USDTB EUROC AEUR XUSD USDQ USDF GHO CRVUSD DOLA MIM USDM USDB USDA USDAI FXUSD EUSD USTB BUIDL OUSG USYC USDZ AUSD"
        .split(' ')
        .collect()
});
static WRAPPED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(W(BTC|ETH|BNB|SOL|STETH|EETH|TRX|AVAX)|STETH|WSTETH|WEETH|CBBTC|CBETH|RETH|METH|EZETH|RSETH|SOLVBTC|LBTC|BTCB|TBTC|PAXG|XAUT|JITOSOL|MSOL|BNSOL|SUSDE|SAVAX|STSOL|OSETH|SWETH|ETHX|CMETH|LSETH|FBTC|UNIBTC|PUMPBTC|ENZOBTC|CLBTC|BBTC)$").unwrap()
});

pub async fn crypto_listed() -> Result<Arc<Vec<Listed>>> {
    cached("screener:crypto:listed", 6 * 3_600_000, || async {
        let u = crypto_universe().await?;
        let mut list: Vec<_> = u.iter().filter(|e| e.2 > 0 && !STABLE.contains(e.0.as_str()) && !WRAPPED.is_match(&e.0)).collect();
        list.sort_by_key(|e| e.2);
        Ok(list.into_iter().map(|e| Listed { symbol: e.0.clone(), name: e.1.clone(), sector: "Crypto".into(), market_cap: 0.0 }).collect())
    })
    .await
}

/// Daily crypto candles (≈ 1 000 days): Gate, then MEXC, then Kraken.
pub async fn crypto_daily_for(base: &str) -> Result<Arc<Vec<Candle>>> {
    let b = base.to_string();
    cached(&format!("screener:cdaily:{base}"), 3 * 3_600_000, || async move {
        let kraken = if b == "BTC" { "XBT".to_string() } else { b.clone() };
        let urls = [
            (
                format!("https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair={b}_USDT&interval=1d&limit=1000"),
                parse::gate as fn(&Value) -> Result<Vec<Candle>>,
            ),
            (format!("https://api.mexc.com/api/v3/klines?symbol={b}USDT&interval=1d&limit=1000"), parse::binance),
            (format!("https://api.kraken.com/0/public/OHLC?pair={kraken}USD&interval=1440"), parse::kraken),
        ];
        for (url, parser) in urls {
            let Ok(d) = get(&url).await else { continue };
            let Ok(c) = parser(&d) else { continue };
            // The current day is still forming: only closed days.
            let now = now_ms();
            let mut c: Vec<Candle> = c.into_iter().filter(|x| x.close > 0.0 && x.time.saturating_add(DAY_MS) <= now).collect();
            c.sort_by_key(|x| x.time);
            if c.len() > 300 {
                return Ok(c);
            }
        }
        err("historique indisponible")
    })
    .await
}

// ---------- Intraday candles ----------

fn interval_ms(i: CandleInterval) -> i64 {
    i.minutes() * 60_000
}

/// Only closed candles (the current one is still moving).
fn closed(c: Vec<Candle>, ms: i64) -> Vec<Candle> {
    let now = now_ms();
    let mut c: Vec<Candle> = c.into_iter().filter(|x| x.close > 0.0 && x.time.saturating_add(ms) <= now).collect();
    c.sort_by_key(|x| x.time);
    c
}

/// Stock intraday candles (Yahoo): recent window for the ranking, 60 days for the replay.
pub async fn stock_intraday(symbol: &str, interval: CandleInterval, replay: bool) -> Result<Arc<Vec<Candle>>> {
    let range = if replay {
        "60d"
    } else if interval == CandleInterval::M5 {
        "5d"
    } else {
        "1mo"
    };
    let ttl = if replay { 6 * 3_600_000 } else { interval.ttl() as i64 };
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval={}&range={range}&includePrePost=false",
        encode_uri_component(symbol),
        interval.as_str()
    );
    cached(&format!("screener:si:{symbol}:{}:{range}", interval.as_str()), ttl, || async move {
        Ok(closed(parse_stock::yahoo(&get(&url).await?)?, interval_ms(interval)))
    })
    .await
}

/// Crypto intraday candles (Gate, 1 000 candles: 3.5 days in 5 min, 10 days in 15 min, 3 weeks in 30 min).
pub async fn crypto_intraday(base: &str, interval: CandleInterval) -> Result<Arc<Vec<Candle>>> {
    let url = format!("https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair={base}_USDT&interval={}&limit=1000", interval.as_str());
    cached(&format!("screener:ci:{base}:{}", interval.as_str()), interval.ttl() as i64, || async move {
        Ok(closed(parse::gate(&get(&url).await?)?, interval_ms(interval)))
    })
    .await
}

async fn candles_for(market: Market, symbol: String, interval: CandleInterval, replay: bool) -> Result<Arc<Vec<Candle>>> {
    match (interval, market) {
        (CandleInterval::D1, Kind::Crypto) => crypto_daily_for(&symbol).await,
        (CandleInterval::D1, Kind::Stock) => daily_for(&symbol).await,
        (_, Kind::Crypto) => crypto_intraday(&symbol, interval).await,
        (_, Kind::Stock) => stock_intraday(&symbol, interval, replay).await,
    }
}

/// `mapLimit`: at most `limit` at a time, results in the order of the items.
async fn map_limit<T, R, F, Fut>(items: Vec<T>, limit: usize, f: F) -> Vec<R>
where
    F: Fn(T) -> Fut,
    Fut: Future<Output = R>,
{
    futures::stream::iter(items.into_iter().map(f)).buffered(limit.max(1)).collect().await
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

/// Buy now, or a limit order at the top of the buy zone when the price is a little above it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub entry: f64,
    pub limit: Option<f64>,
    pub stop: f64,
    pub target: f64,
    pub atr_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub rank: usize,
    pub symbol: String,
    pub name: String,
    pub sector: String,
    pub market_cap: f64,
    pub price: f64,
    pub scores: ByCriterion<f64>,
    pub why: ByCriterion<String>,
    pub action: String,
    pub zone_status: String,
    pub plan: Option<Plan>,
    pub track: Option<TrackRecord>,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Watched {
    #[serde(flatten)]
    pub candidate: Candidate,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetAside {
    pub symbol: String,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenResult {
    pub market: Market,
    pub horizon: Horizon,
    pub as_of: i64,
    pub scanned: usize,
    pub criteria: ByCriterion<&'static str>,
    pub roles: ByCriterion<String>,
    /// Which criterion ranks, and how (for the page).
    pub rank_by: Criterion,
    pub rank_rule: RankRule,
    pub hold_text: String,
    pub evidence: String,
    /// Stocks: the US session is closed, intraday rankings come from the last session.
    pub market_closed: bool,
    pub buy: Vec<Candidate>,
    pub watch: Vec<Watched>,
    pub set_aside: Vec<SetAside>,
    pub validation: Option<Validation>,
}

/// What stage 2 learns about a finalist (consensus, guard, daily candles).
#[derive(Debug, Clone)]
pub struct Verified {
    pub reliability: ReliabilityLevel,
    pub shock: String,
    pub reversal_down: bool,
    pub daily: Vec<Candle>,
}

#[derive(Clone)]
struct Row {
    s: Listed,
    f: RawFactors,
    sc: Scored,
    c: Arc<Vec<Candle>>,
}

/// Stage 1 on the whole universe (ranking), stage 2 on the best candidates (checks), then the final lists.
pub async fn screen<V, VF>(h: Horizon, verify: V, market: Market, top_n: usize, market_closed: bool) -> Result<ScreenResult>
where
    V: Fn(String, CandleInterval) -> VF,
    VF: Future<Output = Result<Verified>>,
{
    let crypto = market == Kind::Crypto;
    let sp: &'static Spec = spec(market, h);
    let list = if crypto { crypto_listed().await? } else { listed().await? };
    let universe: Vec<Listed> = list.iter().take(if crypto { 120 } else { 150 }).cloned().collect();
    let empty: Arc<Vec<Candle>> = Arc::new(Vec::new());
    let candles: Vec<Arc<Vec<Candle>>> = map_limit(universe.iter().map(|s| s.symbol.clone()).collect(), 10, |sym| {
        let empty = empty.clone();
        async move { candles_for(market, sym, sp.interval, false).await.unwrap_or(empty) }
    })
    .await;
    // Cryptos that barely move are pegged tokens the lists missed: nothing to choose there.
    let mut pegged: HashSet<String> = HashSet::new();
    let factors: Vec<Option<RawFactors>> = candles
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let f = if c.len() > 220 { factors_at(c, c.len() - 1, sp) } else { None };
            if let Some(f) = &f {
                if crypto && is_pegged(f.atr_pct, sp.interval) {
                    pegged.insert(universe[i].symbol.clone());
                    return None;
                }
            }
            f
        })
        .collect();
    let scored = score_universe(&factors, sp);
    let rows: Vec<Row> = universe
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match (&factors[i], &scored[i]) {
            (Some(f), Some(sc)) => Some(Row { s: s.clone(), f: f.clone(), sc: *sc, c: candles[i].clone() }),
            _ => None,
        })
        .collect();
    // A few more than needed: some will fail the checks.
    let finalists: Vec<Row> =
        pick(&rows, |x| Some(x.sc.total), |x| if crypto { None } else { Some(x.s.sector.clone()) }, top_n + 6).into_iter().cloned().collect();

    let verify = &verify;
    let checked: Vec<Candidate> = map_limit(finalists, 4, |row: Row| async move {
        let v = verify(row.s.symbol.clone(), sp.interval).await.ok();
        let bt = track_record(&backtest_default(match &v {
            Some(v) if !v.daily.is_empty() => &v.daily,
            _ => &row.c[row.c.len().saturating_sub(500)..],
        }));
        let f = &row.f;
        let checks = vec![
            Check {
                label: "Données recoupées".into(),
                ok: v.as_ref().is_some_and(|v| v.reliability != ReliabilityLevel::Low),
                detail: match &v {
                    Some(v) => format!(
                        "fiabilité {} sur les sources indépendantes",
                        match v.reliability {
                            ReliabilityLevel::High => "élevée",
                            ReliabilityLevel::Medium => "moyenne",
                            ReliabilityLevel::Low => "faible",
                        }
                    ),
                    None => "sources indisponibles".into(),
                },
            },
            Check {
                label: "Garde-fou marché".into(),
                ok: v.as_ref().is_some_and(|v| v.shock != "shock" && !v.reversal_down),
                detail: match &v {
                    None => "indisponible",
                    Some(v) if v.shock == "shock" => "marché en choc sur ce titre",
                    Some(v) if v.reversal_down => "risque de retournement à la baisse élevé",
                    Some(v) if v.shock == "agitated" => "agité : taille divisée par deux",
                    Some(_) => "conditions normales",
                }
                .into(),
            },
            Check {
                label: "Tendance de fond".into(),
                ok: f.trend >= 40.0,
                detail: (if f.trend >= 70.0 {
                    "haussière"
                } else if f.trend >= 40.0 {
                    "mitigée"
                } else {
                    "baissière"
                })
                .into(),
            },
        ];
        let atr_abs = f.atr_pct.map(|a| a / 100.0 * f.price);
        let entry = f.price;
        let zone_top = match (f.zone_status.as_str(), f.zone_distance) {
            ("above", Some(d)) if d <= 10.0 => Some(entry * (1.0 - d / 100.0)),
            _ => None,
        };
        // Stop and target from the price actually paid: the limit when there is one.
        let base = zone_top.unwrap_or(entry);
        // `atrAbs ? … : null` then `stop && …`: an ATR or a stop of 0 counts as absent, as in JavaScript.
        let stop = atr_abs.filter(|a| *a != 0.0 && !a.is_nan()).map(|a| base - sp.stop_atr * a);
        Candidate {
            rank: 0,
            symbol: row.s.symbol.clone(),
            name: row.s.name.clone(),
            sector: if crypto { "Crypto".into() } else { sector_fr(&row.s.sector) },
            market_cap: row.s.market_cap,
            price: f.price,
            scores: row.sc.scores,
            why: explain(f, &row.sc, sp, market),
            action: f.action.as_str().into(),
            zone_status: f.zone_status.as_str().into(),
            plan: match (stop.filter(|s| *s != 0.0 && !s.is_nan()), f.atr_pct) {
                (Some(stop), Some(atr_pct)) => Some(Plan { entry, limit: zone_top, stop, target: base + 2.0 * (base - stop), atr_pct }),
                _ => None,
            },
            track: if bt.trades > 0 { Some(bt) } else { None },
            checks,
        }
    })
    .await;

    let (mut buy, mut watch, mut set_aside) = (Vec::new(), Vec::new(), Vec::new());
    for cand in checked {
        let (data, guard, trend) = (&cand.checks[0], &cand.checks[1], &cand.checks[2]);
        // A short-term rebound ("reversal") buys what has just fallen: a bearish trend is expected there, not a warning.
        let trend_matters = sp.rank != RankRule::Reversal;
        if !data.ok {
            set_aside.push(SetAside { symbol: cand.symbol.clone(), name: cand.name.clone(), reason: format!("{} : {}", data.label, data.detail) });
        } else if !guard.ok || (trend_matters && !trend.ok) {
            let reason = if !guard.ok { format!("{} : {}", guard.label, guard.detail) } else { format!("{} {}", trend.label, trend.detail) };
            watch.push(Watched { candidate: cand, reason });
        } else if buy.len() < top_n {
            let rank = buy.len() + 1;
            buy.push(Candidate { rank, ..cand });
        }
    }

    let hist_universe = universe.clone();
    let ttl = if sp.interval == CandleInterval::D1 { 12 * 3_600_000 } else { 2 * 3_600_000 };
    let key = format!("screener:validation:{}:{}:{top_n}", market.as_str(), h.as_str());
    let pegged = Arc::new(pegged);
    let validation = cached(&key, ttl, move || async move {
        let hist: Vec<Arc<Vec<Candle>>> = if sp.interval == CandleInterval::D1 || crypto {
            candles
        } else {
            let empty: Arc<Vec<Candle>> = Arc::new(Vec::new());
            map_limit(hist_universe.iter().map(|s| s.symbol.clone()).collect(), 10, |sym| {
                let empty = empty.clone();
                async move { candles_for(market, sym, sp.interval, true).await.unwrap_or(empty) }
            })
            .await
        };
        let longest = hist.iter().map(|c| c.len()).max().unwrap_or(0);
        // Daily: stocks with ≈ 6 years (replay from 2020), cryptos with ≈ 2.5 years. Intraday: the full window.
        let min_len = if sp.interval == CandleInterval::D1 { if crypto { 900 } else { 1500 } } else { (longest as f64 * 0.8).floor() as usize };
        let usable: Vec<(&Listed, &Arc<Vec<Candle>>)> =
            hist_universe.iter().zip(hist.iter()).filter(|(s, c)| c.len() > min_len && !pegged.contains(&s.symbol)).collect();
        let btc = if crypto { usable.iter().position(|(s, _)| s.symbol == "BTC") } else { None };
        let series: Vec<Vec<Candle>> = usable.iter().map(|(_, c)| c.to_vec()).collect();
        let sectors: Vec<String> = usable.iter().map(|(s, _)| s.sector.clone()).collect();
        Ok::<_, Error>(validate(
            &align_series(&series, sp.interval != CandleInterval::D1),
            sp,
            h,
            top_n,
            if crypto { None } else { Some(&sectors) },
            btc,
        ))
    })
    .await
    .ok()
    .and_then(|v| (*v).clone());

    Ok(ScreenResult {
        market,
        horizon: h,
        as_of: now_ms(),
        scanned: rows.len(),
        criteria: CRITERIA,
        roles: roles(sp, market),
        rank_by: sp.rank.criterion(),
        rank_rule: sp.rank,
        hold_text: span(sp.hold, sp.interval, market),
        // Measured once, when the ranking criterion was chosen (its period is in the text); the replay on today's
        // data is `validation`. Said explicitly so the two figures are never read as the same measure.
        evidence: format!("Mesuré au choix de la méthode — {}", sp.evidence),
        market_closed: !crypto && sp.interval != CandleInterval::D1 && market_closed,
        buy,
        watch,
        set_aside,
        validation,
    })
}

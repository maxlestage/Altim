//! Additional stock and ETF providers (no key), `web/server/stocks-extra.ts`: daily candles from Dow Jones
//! (WSJ / MarketWatch), AlphaQuery, Finviz, the Financial Times and eToro; live quotes from Fidelity, StockCharts,
//! TipRanks and Public.com.
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, Shared, join_all};
use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::http::{CLIENT, Error, Result, err};
use crate::js::now_ms;
use crate::jsval::{encode_uri_component as enc, slice_utf16};
pub use crate::quotes::SourceQuote as Quote;
use crate::types::{Candle, Interval};

const UA: [(&str, &str); 3] = [
    ("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15"),
    ("Accept", "application/json, text/html, */*"),
    ("Accept-Language", "en-US,en;q=0.9"),
];

/// GET (or POST with a JSON body) → text, the UA headers plus `headers`, 10 s.
async fn fetch_text(url: &str, headers: &[(&str, &str)], body: Option<String>) -> Result<String> {
    let mut req = match &body {
        Some(_) => CLIENT.post(url),
        None => CLIENT.get(url),
    }
    .timeout(Duration::from_secs(10));
    let mut all: Vec<(&str, &str)> = UA.to_vec();
    for (k, v) in headers {
        match all.iter_mut().find(|(x, _)| x.eq_ignore_ascii_case(k)) {
            Some(slot) => slot.1 = v,
            None => all.push((k, v)),
        }
    }
    for (k, v) in all {
        req = req.header(k, v);
    }
    if let Some(b) = body {
        req = req.body(b);
    }
    let res = req.send().await.map_err(crate::http::from_reqwest)?;
    if !res.status().is_success() {
        return err(format!("HTTP {}", res.status().as_u16()));
    }
    res.text().await.map_err(crate::http::from_reqwest)
}

fn json_parse(text: &str) -> Result<Value> {
    serde_json::from_str(text).map_err(|_| Error("JSON invalide".into()))
}

async fn fetch_json(url: &str, headers: &[(&str, &str)], body: Option<String>) -> Result<Value> {
    json_parse(&fetch_text(url, headers, body).await?)
}

fn dotted(s: &str) -> String {
    s.replace('-', ".")
}
/// Class shares written BRK/B (Fidelity, StockCharts).
fn slashed(s: &str) -> String {
    s.replace('-', "/")
}

/// Response parsers (formats checked on real responses, September 2026, see tests/stocks_extra.rs).
pub mod parse {
    use indexmap::IndexMap;
    use regex::Regex;
    use serde_json::Value;

    use super::{Quote, dotted};
    use crate::http::{Error, Result, err};
    use crate::jsval::{array, get, idx, is_num, need, nullish, number, slice_utf16, string_to_number, to_string, truthy, truthy_num};
    use crate::market::{ny_date_string, ny_open_checked};
    use crate::types::Candle;

    /// `Number(x)` (after dropping "$ ," from a string), kept when finite.
    pub fn n(v: Option<&Value>) -> Option<f64> {
        let x = match v {
            Some(Value::String(s)) => string_to_number(&s.replace(['$', ','], "")),
            _ => number(v),
        };
        x.is_finite().then_some(x)
    }
    fn valid(c: &Candle) -> bool {
        c.close > 0.0 && c.open > 0.0 && c.high >= c.low
    }
    /// "2026-09-25…" → 9:30 New York that day.
    pub fn from_iso(s: &str) -> Result<i64> {
        let parts: Vec<f64> = slice_utf16(s, 10).split('-').map(string_to_number).collect();
        let at = |i: usize| parts.get(i).copied().unwrap_or(f64::NAN);
        ny_open_checked(at(0), at(1), at(2))
    }
    /// Timestamp (ms) → its calendar date in New York → 9:30 that day.
    pub fn from_ny(ms: f64) -> Result<i64> {
        from_iso(&ny_date_string(ms)?)
    }
    fn or0(x: f64) -> f64 {
        if truthy_num(x) { x } else { 0.0 }
    }

    /// WSJ / MarketWatch timeseries: ticks (ms, midnight UTC of the session) + [open, high, low, last] per tick.
    pub fn wsj(d: &Value) -> Result<Vec<Candle>> {
        let d = Some(d);
        let ticks = get(get(d, "TimeInfo"), "Ticks").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]);
        let mut px: Option<&Value> = None;
        if let Some(series) = get(d, "Series").and_then(|v| v.as_array()) {
            for s in series {
                need(Some(s), "s.SeriesId")?;
                if get(Some(s), "SeriesId").and_then(|v| v.as_str()) == Some("s1") {
                    px = get(Some(s), "DataPoints");
                    break;
                }
            }
        }
        let mut out = Vec::new();
        for (i, t) in ticks.iter().enumerate() {
            let p = idx(px, i);
            let Some(p) = p.and_then(|p| p.as_array()) else { continue };
            if (0..p.len()).any(|k| p[k].is_null()) {
                continue;
            }
            let t = number(Some(t)).trunc();
            if !t.is_finite() || t.abs() > 8.64e15 {
                return Err(Error("Invalid Date".into()));
            }
            let v = |k: usize| number(p.get(k));
            let c = Candle { time: from_iso(&crate::js::iso(t as i64))?, open: v(0), high: v(1), low: v(2), close: v(3), volume: 0.0 };
            if valid(&c) {
                out.push(c);
            }
        }
        Ok(out)
    }

    const ALPHAQUERY: &str = "(d?.unadjusted ?? []).map((r) => ({ time: fromIso(String(r.x)), open: Number(r.open), high: Number(r.high), low: Number(r.low), close: Number(r.close), volume: Number(r.volume) || 0 }))";
    /// AlphaQuery: {"unadjusted": [{"x": "2026-09-25T00:00:00Z", open, high, low, close, volume}]}.
    pub fn alphaquery(d: &Value) -> Result<Vec<Candle>> {
        let list = get(Some(d), "unadjusted");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.unadjusted ?? [])", "map", ALPHAQUERY)? };
        let mut out = Vec::new();
        for r in list {
            need(Some(r), "r.x")?;
            let r = Some(r);
            let f = |k: &str| number(get(r, k));
            out.push(Candle {
                time: from_iso(&to_string(get(r, "x")))?,
                open: f("open"),
                high: f("high"),
                low: f("low"),
                close: f("close"),
                volume: or0(f("volume")),
            });
        }
        Ok(out.into_iter().filter(valid).collect())
    }

    const FINVIZ: &str = "(d?.date ?? []).map((t, i) => ({ time: fromNy(t * 1000), open: Number(d.open[i]), high: Number(d.high[i]), low: Number(d.low[i]), close: Number(d.close[i]), volume: Number(d.volume?.[i]) || 0 }))";
    /// Finviz chart API: parallel arrays; `date` in seconds during the New York session.
    pub fn finviz(d: &Value) -> Result<Vec<Candle>> {
        let d = Some(d);
        let dates = get(d, "date");
        let dates = if nullish(dates) { &[][..] } else { array(dates, "(d?.date ?? [])", "map", FINVIZ)? };
        let mut out = Vec::new();
        for (i, t) in dates.iter().enumerate() {
            let time = from_ny(number(Some(t)) * 1000.0)?;
            let col = |k: &str| -> Result<f64> {
                let a = get(d, k);
                need(a, &format!("d.{k}[i]"))?;
                Ok(number(idx(a, i)))
            };
            let (open, high, low, close) = (col("open")?, col("high")?, col("low")?, col("close")?);
            out.push(Candle { time, open, high, low, close, volume: or0(number(idx(get(d, "volume"), i))) });
        }
        Ok(out.into_iter().filter(valid).collect())
    }

    /// Financial Times chart API: Dates + Open/High/Low/Close series.
    pub fn ft(d: &Value) -> Result<Vec<Candle>> {
        let d = Some(d);
        let dates = get(d, "Dates").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]);
        let mut series: IndexMap<String, Option<&Value>> = IndexMap::new();
        let comps = get(idx(get(d, "Elements"), 0), "ComponentSeries").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]);
        for c in comps {
            need(Some(c), "c.Type")?;
            series.insert(to_string(get(Some(c), "Type")), get(Some(c), "Values"));
        }
        let at = |k: &str, i: usize| series.get(k).and_then(|v| idx(*v, i));
        let mut out = Vec::new();
        for (i, s) in dates.iter().enumerate() {
            let time = from_iso(&to_string(Some(s)))?;
            let close = at("Close", i);
            let c = Candle {
                time,
                open: number(at("Open", i)),
                high: number(at("High", i)),
                low: number(at("Low", i)),
                close: number(close),
                volume: 0.0,
            };
            if !nullish(close) && valid(&c) {
                out.push(c);
            }
        }
        Ok(out)
    }

    const FT_XID: &str = "(d?.data?.security ?? []).filter((s) => {\n      const [sym, ex] = String(s.symbol).split(\":\");\n      return sym === dotted(symbol) && us.test(ex ?? \"\");\n    })";
    /// FT search: the primary US listing of this exact symbol (NASDAQ, NYSE, NYSE Arca, Cboe BZX, NYSE American).
    pub fn ft_xid(d: &Value, symbol: &str) -> Result<Option<Value>> {
        let list = get(get(Some(d), "data"), "security");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.data?.security ?? [])", "filter", FT_XID)? };
        let mut us = Vec::new();
        for s in list {
            need(Some(s), "s.symbol")?;
            let sym = to_string(get(Some(s), "symbol"));
            let mut parts = sym.split(':');
            let (a, ex) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            if a == dotted(symbol) && matches!(ex, "NSQ" | "NYQ" | "PCQ" | "BTQ" | "ASQ" | "NMQ") {
                us.push(s);
            }
        }
        let pick = us.iter().find(|s| truthy(get(Some(s), "isPrimary"))).or(us.first());
        Ok(pick.and_then(|s| get(Some(s), "xid")).filter(|x| !x.is_null()).cloned())
    }

    const ETORO: &str = "(d?.Candles?.[0]?.Candles ?? []).map((r) => ({ time: fromIso(String(r.FromDate)), open: Number(r.Open), high: Number(r.High), low: Number(r.Low), close: Number(r.Close), volume: Number(r.Volume) || 0 }))";
    /// eToro daily candles (eToro's own prices, sessions dated at midnight UTC).
    pub fn etoro(d: &Value) -> Result<Vec<Candle>> {
        let list = get(idx(get(Some(d), "Candles"), 0), "Candles");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.Candles?.[0]?.Candles ?? [])", "map", ETORO)? };
        let mut out = Vec::new();
        for r in list {
            need(Some(r), "r.FromDate")?;
            let r = Some(r);
            let f = |k: &str| number(get(r, k));
            out.push(Candle {
                time: from_iso(&to_string(get(r, "FromDate")))?,
                open: f("Open"),
                high: f("High"),
                low: f("Low"),
                close: f("Close"),
                volume: or0(f("Volume")),
            });
        }
        Ok(out.into_iter().filter(valid).collect())
    }

    const ETORO_IDS: &str = "(d?.InstrumentDisplayDatas ?? []).filter((x) => x.InstrumentTypeID === 5 || x.InstrumentTypeID === 6)";
    /// eToro instruments: stocks (type 5) and ETFs (type 6) by symbol (class shares written BRK.B).
    pub fn etoro_ids(d: &Value) -> Result<IndexMap<String, f64>> {
        let list = get(Some(d), "InstrumentDisplayDatas");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.InstrumentDisplayDatas ?? [])", "filter", ETORO_IDS)? };
        let mut m = IndexMap::new();
        for x in list {
            need(Some(x), "x.InstrumentTypeID")?;
            let x = Some(x);
            let t = get(x, "InstrumentTypeID");
            if is_num(t, 5.0) || is_num(t, 6.0) {
                m.insert(to_string(get(x, "SymbolFull")), number(get(x, "InstrumentID")));
            }
        }
        Ok(m)
    }

    fn js_trim(s: &str) -> &str {
        s.trim_matches(|c: char| {
            matches!(
                c,
                '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                    ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
            )
        })
    }

    /// Fidelity (JSONP-like, wrapped in parentheses): LAST_PRICE and PCT_CHG_TODAY per symbol.
    pub fn fidelity(text: &str) -> Result<IndexMap<String, Quote>> {
        let t = js_trim(text);
        let t = t.strip_prefix('(').unwrap_or(t);
        let t = Regex::new(r"\)\s*;?$").unwrap().replace(t, "");
        let d = super::json_parse(&t)?;
        let mut m = IndexMap::new();
        if let Some(Value::Object(o)) = get(Some(&d), "QUOTES") {
            for (sym, q) in o {
                let price = n(get(Some(q), "LAST_PRICE"));
                if get(Some(q), "ERROR_CODE").and_then(|v| v.as_str()) == Some("0") {
                    if let Some(price) = price.filter(|p| truthy_num(*p)) {
                        m.insert(sym.replace(['.', '/'], "-"), Quote { price, change: n(get(Some(q), "PCT_CHG_TODAY")) });
                    }
                }
            }
        }
        Ok(m)
    }

    fn change(price: f64, prev: Option<f64>) -> Option<f64> {
        prev.filter(|p| truthy_num(*p)).map(|p| (price / p - 1.0) * 100.0)
    }

    /// StockCharts summary: last close and previous close.
    pub fn stockcharts(d: &Value) -> Result<Quote> {
        let price = n(get(Some(d), "close")).filter(|p| truthy_num(*p));
        let prev = n(get(Some(d), "lastClose"));
        let Some(price) = price else { return err("StockCharts : symbole inconnu") };
        Ok(Quote { price, change: change(price, prev) })
    }

    /// TipRanks: daily closes, the last one is the current price.
    pub fn tipranks(d: &Value) -> Result<Quote> {
        let list = get(Some(d), "prices");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.prices ?? [])", "map", "(d?.prices ?? []).map((x) => n(x.p))")? };
        let mut p = Vec::new();
        for x in list {
            need(Some(x), "x.p")?;
            if let Some(v) = n(get(Some(x), "p")).filter(|v| truthy_num(*v)) {
                p.push(v);
            }
        }
        if p.is_empty() {
            return err("TipRanks : aucun cours");
        }
        let price = p[p.len() - 1];
        let prev = if p.len() > 1 { Some(p[p.len() - 2]) } else { None };
        Ok(Quote { price, change: change(price, prev) })
    }

    /// Public.com stock page: the quote is embedded in the page data ("last" and "previousClose").
    pub fn public_com(html: &str, symbol: &str) -> Result<Quote> {
        // As the TypeScript: only the first "." of the symbol is escaped.
        let sym = dotted(symbol).replacen('.', "\\.", 1);
        let scope = match Regex::new(&format!(r#"(?i)"symbol"\s*:\s*"{sym}""#)).ok().and_then(|re| re.find(html)) {
            Some(m) => {
                // html.slice(i, i + 4000), in UTF-16 units.
                let rest = &html[m.start()..];
                slice_utf16(rest, 4000)
            }
            None => html,
        };
        let grab = |re: &str| Regex::new(re).unwrap().captures(scope).and_then(|c| c.get(1)).map(|m| Value::String(m.as_str().into()));
        let price = n(grab(r#""last"\s*:\s*"?([0-9.]+)"#).as_ref()).filter(|p| truthy_num(*p));
        let prev = n(grab(r#""previousClose"\s*:\s*"?([0-9.]+)"#).as_ref());
        let Some(price) = price else { return err("Public.com : cours introuvable") };
        Ok(Quote { price, change: change(price, prev) })
    }
}

// ---------- Identifier lookups (kept once found) ----------

type Memo = Mutex<HashMap<String, Shared<BoxFuture<'static, Result<Value>>>>>;
static MEMO: LazyLock<Memo> = LazyLock::new(Default::default);

/// One lookup per key, shared by concurrent callers; a failure is forgotten (retried next time).
async fn once<F>(key: String, f: F) -> Result<Value>
where
    F: std::future::Future<Output = Result<Value>> + Send + 'static,
{
    let fut = {
        let mut memo = MEMO.lock().unwrap();
        memo.entry(key.clone())
            .or_insert_with(|| {
                async move {
                    let r = f.await;
                    if r.is_err() {
                        MEMO.lock().unwrap().remove(&key);
                    }
                    r
                }
                .boxed()
                .shared()
            })
            .clone()
    };
    fut.await
}

/// Entitlement token of the MarketWatch charts (WSJ_TOKEN in the environment). Without it, the source is skipped.
fn wsj_token() -> String {
    std::env::var("WSJ_TOKEN").unwrap_or_default()
}

fn wsj_url(key: &str, time_frame: &str) -> String {
    let token = wsj_token();
    let q = json!({
        "Step": "P1D", "TimeFrame": time_frame, "EntitlementToken": token, "IncludeMockTick": false, "FilterNullSlots": true, "FilterClosedPoints": true,
        "IncludeClosedSlots": false, "IncludeOfficialClose": true, "InjectOpen": false, "ShowPreMarket": false, "ShowAfterHours": false, "UseExtendedTimeFrame": true,
        "WantPriorClose": false, "IncludeCurrentQuotes": false, "ResetTodaysAfterHoursPercentChange": false,
        // No volume series: the public entitlement refuses it ("Series kind 'Volume' is not known").
        "Series": [{ "Key": key, "Dialect": "Charting", "Kind": "Ticker", "SeriesId": "s1", "DataTypes": ["Open", "High", "Low", "Last"] }],
    });
    format!("https://api.wsj.net/api/michelangelo/timeseries/history?json={}&ckey={}", enc(&q.to_string()), slice_utf16(&token, 10))
}

async fn wsj_fetch(key: &str, time_frame: &str) -> Result<Value> {
    let token = wsj_token();
    fetch_json(&wsj_url(key, time_frame), &[("Dylan2010.EntitlementToken", &token)], None).await
}

/// Dow Jones keys depend on the exchange: tried in turn, the first one that answers is kept.
async fn wsj_key(symbol: &str) -> Result<String> {
    let s = dotted(symbol);
    let v = once(format!("wsj:{symbol}"), async move {
        for key in [
            format!("STOCK/US/XNAS/{s}"),
            format!("STOCK/US/XNYS/{s}"),
            format!("FUND/US/ARCX/{s}"),
            format!("FUND/US/XNAS/{s}"),
            format!("STOCK/US/XASE/{s}"),
            format!("FUND/US/BATS/{s}"),
            format!("STOCK/US/ARCX/{s}"),
        ] {
            if let Ok(d) = wsj_fetch(&key, "P5D").await {
                if parse::wsj(&d).is_ok_and(|c| !c.is_empty()) {
                    return Ok(Value::String(key));
                }
            }
        }
        err("non coté")
    })
    .await?;
    Ok(crate::jsval::to_string(Some(&v)))
}

async fn ft_xid(symbol: &str) -> Result<Value> {
    let symbol = symbol.to_string();
    once(format!("ft:{symbol}"), async move {
        let url = format!("https://markets.ft.com/data/searchapi/searchsecurities?query={}", enc(&dotted(&symbol)));
        match parse::ft_xid(&fetch_json(&url, &[], None).await?, &symbol)? {
            Some(x) if crate::jsval::truthy(Some(&x)) => Ok(x),
            _ => err("non coté"),
        }
    })
    .await
}

/// eToro's instrument list (≈ 12 MB): loaded once a day.
async fn etoro_id(symbol: &str) -> Result<Option<f64>> {
    type List = Shared<BoxFuture<'static, Result<Arc<IndexMap<String, f64>>>>>;
    static LIST: LazyLock<Mutex<Option<(i64, List)>>> = LazyLock::new(Default::default);
    let ids = {
        let mut list = LIST.lock().unwrap();
        let stale = list.as_ref().is_none_or(|(at, _)| now_ms() - at > 86_400_000);
        if stale {
            let at = now_ms();
            let fut = async move {
                let r = async {
                    Ok(Arc::new(parse::etoro_ids(
                        &fetch_json("https://api.etorostatic.com/sapi/instrumentsmetadata/V1.1/instruments", &[], None).await?,
                    )?))
                }
                .await;
                if r.is_err() {
                    let mut list = LIST.lock().unwrap();
                    if list.as_ref().is_some_and(|(t, _)| *t == at) {
                        *list = None;
                    }
                }
                r
            }
            .boxed()
            .shared();
            *list = Some((at, fut));
        }
        list.as_ref().unwrap().1.clone()
    };
    Ok(ids.await?.get(&dotted(symbol)).copied())
}

// ---------- Sources ----------

pub type CandleFetch = fn(String) -> BoxFuture<'static, Result<Vec<Candle>>>;

/// Daily candle provider (added to `market::STOCK_SOURCES`).
pub struct CandleSource {
    pub name: &'static str,
    pub supports: fn(Interval) -> bool,
    pub fetch: CandleFetch,
}

fn daily(i: Interval) -> bool {
    i == Interval::D1
}

pub static EXTRA_CANDLE_SOURCES: LazyLock<Vec<CandleSource>> = LazyLock::new(|| {
    vec![
        CandleSource {
            name: "WSJ / MarketWatch",
            supports: |i| daily(i) && !wsj_token().is_empty(),
            fetch: |s| async move { parse::wsj(&wsj_fetch(&wsj_key(&s).await?, "P5Y").await?) }.boxed(),
        },
        CandleSource {
            name: "AlphaQuery",
            supports: daily,
            fetch: |s| {
                async move {
                    parse::alphaquery(
                        &fetch_json(&format!("https://www.alphaquery.com/data/stock-price-chart?ticker={}", enc(&dotted(&s))), &[], None).await?,
                    )
                }
                .boxed()
            },
        },
        CandleSource {
            name: "Finviz",
            supports: daily,
            fetch: |s| {
                async move {
                    parse::finviz(
                        &fetch_json(&format!("https://finviz.com/api/quote.ashx?instrument=stock&ticker={}&timeframe=d", enc(&s)), &[], None).await?,
                    )
                }
                .boxed()
            },
        },
        CandleSource {
            name: "Financial Times",
            supports: daily,
            fetch: |s| {
                async move {
                    let xid = ft_xid(&s).await?;
                    let body = json!({
                        "days": 1100, "dataNormalized": false, "dataPeriod": "Day", "dataInterval": 1, "realtime": false, "yFormat": "0.###",
                        "timeServiceFormat": "JSON", "returnDateType": "ISO8601",
                        "elements": [{ "Type": "price", "Symbol": xid, "OverlayIndicators": [], "Params": {} }],
                    });
                    parse::ft(
                        &fetch_json("https://markets.ft.com/data/chartapi/series", &[("Content-Type", "application/json")], Some(body.to_string()))
                            .await?,
                    )
                }
                .boxed()
            },
        },
        CandleSource {
            name: "eToro",
            supports: daily,
            fetch: |s| {
                async move {
                    let id = etoro_id(&s).await?.filter(|x| crate::jsval::truthy_num(*x));
                    let Some(id) = id else { return err("non coté") };
                    parse::etoro(
                        &fetch_json(&format!("https://candle.etoro.com/candles/asc.json/OneDay/1000/{}", crate::js::number_to_string(id)), &[], None)
                            .await?,
                    )
                }
                .boxed()
            },
        },
    ]
});

pub type QuoteFetch = fn(Vec<String>) -> BoxFuture<'static, Result<IndexMap<String, Quote>>>;

/// Live quote provider for stocks (added to `quotes::QUOTE_SOURCES`).
pub struct QuoteSourceExtra {
    pub name: &'static str,
    pub fetch: QuoteFetch,
}

async fn each<F, Fut>(symbols: Vec<String>, one: F) -> Result<IndexMap<String, Quote>>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Quote>>,
{
    let done = join_all(symbols.into_iter().map(|s| one(s.clone()).map(move |r| (s, r)))).await;
    Ok(done.into_iter().filter_map(|(s, r)| r.ok().map(|q| (s, q))).collect())
}

pub static EXTRA_QUOTE_SOURCES: LazyLock<Vec<QuoteSourceExtra>> = LazyLock::new(|| {
    vec![
        QuoteSourceExtra {
            name: "Fidelity",
            fetch: |symbols| {
                async move {
                    if symbols.is_empty() {
                        return Ok(IndexMap::new());
                    }
                    let list = symbols.iter().map(|s| enc(&slashed(s))).collect::<Vec<_>>().join(",");
                    parse::fidelity(
                        &fetch_text(&format!("https://fastquote.fidelity.com/service/quote/json?productid=embeddedquotes&symbols={list}"), &[], None)
                            .await?,
                    )
                }
                .boxed()
            },
        },
        QuoteSourceExtra {
            name: "StockCharts",
            fetch: |symbols| {
                each(symbols, |s| async move {
                    parse::stockcharts(
                        &fetch_json(&format!("https://stockcharts.com/j-sum/sum?cmd=symsum&symbol={}", enc(&slashed(&s))), &[], None).await?,
                    )
                })
                .boxed()
            },
        },
        QuoteSourceExtra {
            name: "TipRanks",
            fetch: |symbols| {
                each(symbols, |s| async move {
                    parse::tipranks(&fetch_json(&format!("https://www.tipranks.com/api/stocks/getData/?name={}", enc(&dotted(&s))), &[], None).await?)
                })
                .boxed()
            },
        },
        QuoteSourceExtra {
            name: "Public.com",
            fetch: |symbols| {
                each(symbols, |s| async move {
                    let html = fetch_text(&format!("https://public.com/stocks/{}", enc(&dotted(&s).to_lowercase())), &[], None).await?;
                    parse::public_com(&html, &s)
                })
                .boxed()
            },
        },
    ]
});

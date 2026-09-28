//! Multi-source market data (`web/server/market.ts`): every source is queried in parallel, compared candle by candle
//! with the median, and the first agreeing one is served (its candles blended with the other agreeing sources).
use std::future::Future;
use std::sync::{Arc, LazyLock};

use chrono::{Datelike, Timelike};
use chrono_tz::America::New_York;
use futures::FutureExt;
use futures::future::{BoxFuture, join_all};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use crate::engine::reliability::{QualityReport, Reliability, assess_quality, reliability};
use crate::http::{Error, Result, err, get_json};
use crate::js::{iso_date, median, now_ms};
use crate::jsval::encode_uri_component as enc;
use crate::quotes::{QUOTE_SOURCES, QuoteSourceStatus, consensus_quotes, make_asset, webull_ticker_id};
use crate::stocks_extra::EXTRA_CANDLE_SOURCES;
use crate::types::{Candle, Interval, Kind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceStatus {
    pub name: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deviation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consensus {
    pub candles: Vec<Candle>,
    pub source: String,
    pub agreeing: usize,
    /// Fewer than half of the responding sources agree (always set by `consensus`).
    pub conflict: bool,
    pub sources: Vec<SourceStatus>,
}

/// `Consensus & { symbol, kind, interval, quality, reliability }` (same key order as the TypeScript spread).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub candles: Vec<Candle>,
    pub source: String,
    pub agreeing: usize,
    pub conflict: bool,
    pub sources: Vec<SourceStatus>,
    pub symbol: String,
    pub kind: Kind,
    pub interval: Interval,
    pub quality: QualityReport,
    pub reliability: Reliability,
}

// ---------- JavaScript time helpers ----------

/// Candle time of a JavaScript `NaN` (an unparsable date): never "closed", so `closed_only` drops it like
/// `Number.isFinite` / the NaN comparison do in the TypeScript.
pub const NAN_TIME: i64 = i64::MAX;

/// A JavaScript time value (ms) as a candle time.
pub fn ms(x: f64) -> i64 {
    if x.is_finite() { x as i64 } else { NAN_TIME }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `Date.UTC(y, m0, d, h, min)`, with its overflow rules (month 12 = next January, day 0 = last day of the
/// previous month, years 0–99 = 1900–1999). NaN when an argument is not finite or the date is out of range.
pub fn date_utc(y: f64, m0: f64, d: f64, h: f64, min: f64) -> f64 {
    if ![y, m0, d, h, min].iter().all(|x| x.is_finite()) {
        return f64::NAN;
    }
    let mut y = y.trunc();
    if (0.0..=99.0).contains(&y) {
        y += 1900.0;
    }
    let m0 = m0.trunc();
    let ym = y + (m0 / 12.0).floor();
    let mn = m0.rem_euclid(12.0);
    if ym.abs() > 400_000.0 || d.abs() > 1e9 || h.abs() > 1e12 || min.abs() > 1e14 {
        return f64::NAN;
    }
    let day = days_from_civil(ym as i64, mn as i64 + 1, 1) as f64 + d.trunc() - 1.0;
    let t = day * 86_400_000.0 + h.trunc() * 3_600_000.0 + min.trunc() * 60_000.0;
    if t.abs() > 8.64e15 { f64::NAN } else { t }
}

const NOT_FINITE: &str = "date value is not finite in DateTimeFormat format()";

fn ny(t: f64) -> Result<chrono::DateTime<chrono_tz::Tz>> {
    if !t.is_finite() {
        return err(NOT_FINITE);
    }
    chrono::DateTime::from_timestamp_millis(t as i64).map(|d| d.with_timezone(&New_York)).ok_or_else(|| Error(NOT_FINITE.into()))
}

/// `NY_DAY.format(t)` / `NY_DATE.format(t)` (en-CA: "2026-09-25"), New York calendar date of a time value.
pub fn ny_date_string(t: f64) -> Result<String> {
    let d = ny(t)?;
    Ok(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()))
}

/// `nyOpen` with JavaScript numbers: throws like `Intl.DateTimeFormat` on a non-finite date.
pub fn ny_open_checked(year: f64, month: f64, day: f64) -> Result<i64> {
    for utc_hour in [13.0, 14.0] {
        let t = date_utc(year, month - 1.0, day, utc_hour, 30.0);
        let local = ny(t)?;
        if local.hour() == 9 && local.minute() == 30 {
            return Ok(t as i64);
        }
    }
    Ok(ms(date_utc(year, month - 1.0, day, 13.0, 30.0)))
}

/// Opening of the New York session (9:30 local) for a date, US daylight saving time handled (`nyOpen`).
pub fn ny_open(year: i32, month: u32, day: u32) -> i64 {
    ny_open_checked(year as f64, month as f64, day as f64).unwrap_or(NAN_TIME)
}

/// `[y, m, d] = s.split(sep).map(Number)` then `nyOpen(y, m, d)`, positions given by `order` (indices of y, m, d).
fn ny_open_parts(s: &str, sep: char, order: [usize; 3]) -> Result<i64> {
    let parts: Vec<f64> = s.split(sep).map(crate::jsval::string_to_number).collect();
    let at = |i: usize| parts.get(i).copied().unwrap_or(f64::NAN);
    ny_open_checked(at(order[0]), at(order[1]), at(order[2]))
}

// ---------- Candle helpers ----------

/// `Math.max` / `Math.min` (NaN wins).
fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}
fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// Only closed candles, sorted chronologically (`closedOnly`).
pub fn closed_only(candles: Vec<Candle>, interval: Interval) -> Vec<Candle> {
    closed_only_at(candles, interval, now_ms())
}

pub fn closed_only_at(candles: Vec<Candle>, interval: Interval, now: i64) -> Vec<Candle> {
    let mut c: Vec<Candle> =
        candles.into_iter().filter(|c| c.time.saturating_add(interval.step()) <= now && c.close.is_finite() && c.close > 0.0).collect();
    c.sort_by_key(|c| c.time);
    c
}

/// Stock candle closed: the daily session lasts 6h30, not 24h (`stockClosed`).
pub fn stock_closed(candles: Vec<Candle>, interval: Interval) -> Vec<Candle> {
    stock_closed_at(candles, interval, now_ms())
}

pub fn stock_closed_at(candles: Vec<Candle>, interval: Interval, now: i64) -> Vec<Candle> {
    let duration = if interval == Interval::D1 { 23_400_000 } else { interval.step() };
    let mut c: Vec<Candle> = candles.into_iter().filter(|c| c.time.saturating_add(duration) <= now && c.close.is_finite() && c.close > 0.0).collect();
    c.sort_by_key(|c| c.time);
    c
}

/// Candles of `from` ms grouped into complete candles of `to` ms (UTC-aligned).
pub fn aggregate(candles: &[Candle], from: i64, to: i64) -> Vec<Candle> {
    let per = crate::js::round(to as f64 / from as f64) as usize;
    let mut sorted = candles.to_vec();
    sorted.sort_by_key(|c| c.time);
    let mut groups: IndexMap<i64, Vec<Candle>> = IndexMap::new();
    for c in sorted {
        let key = ((c.time as f64 / to as f64).floor() * to as f64) as i64;
        groups.entry(key).or_default().push(c);
    }
    groups
        .into_iter()
        .filter(|(_, items)| items.len() == per)
        .map(|(time, items)| Candle {
            time,
            open: items[0].open,
            high: items.iter().map(|c| c.high).fold(f64::NEG_INFINITY, jmax),
            low: items.iter().map(|c| c.low).fold(f64::INFINITY, jmin),
            close: items[items.len() - 1].close,
            volume: items.iter().fold(0.0, |a, c| a + c.volume),
        })
        .collect()
}

/// 1h → 4h for stocks: `per`-bar blocks inside each session (New York day).
pub fn aggregate_session(candles: &[Candle], per: usize) -> Vec<Candle> {
    let day = |t: i64| ny_date_string(if t == NAN_TIME { f64::NAN } else { t as f64 }).ok();
    let mut out = Vec::new();
    let mut bucket: Vec<Candle> = Vec::new();
    let flush = |bucket: &mut Vec<Candle>, out: &mut Vec<Candle>| {
        if bucket.is_empty() {
            return;
        }
        out.push(Candle {
            time: bucket[0].time,
            open: bucket[0].open,
            high: bucket.iter().map(|c| c.high).fold(f64::NEG_INFINITY, jmax),
            low: bucket.iter().map(|c| c.low).fold(f64::INFINITY, jmin),
            close: bucket[bucket.len() - 1].close,
            volume: bucket.iter().fold(0.0, |a, c| a + c.volume),
        });
        bucket.clear();
    };
    for c in candles {
        if !bucket.is_empty() && (bucket.len() == per || day(bucket[0].time) != day(c.time)) {
            flush(&mut bucket, &mut out);
        }
        bucket.push(*c);
    }
    flush(&mut bucket, &mut out);
    out
}

/// Median deviation (%) of each series from the per-timestamp median, over 20 shared candles.
pub fn deviations(series: &[Vec<Candle>]) -> Vec<f64> {
    let maps: Vec<IndexMap<i64, f64>> = series.iter().map(|s| s.iter().map(|c| (c.time, c.close)).collect()).collect();
    maps.iter()
        .enumerate()
        .map(|(i, m)| {
            let mut shared: Vec<i64> = m.keys().copied().filter(|t| maps.iter().enumerate().any(|(j, o)| j != i && o.contains_key(t))).collect();
            shared.sort();
            let shared = &shared[shared.len().saturating_sub(20)..];
            if shared.is_empty() {
                return f64::INFINITY;
            }
            let devs: Vec<f64> = shared
                .iter()
                .map(|t| {
                    let at: Vec<f64> = maps.iter().filter_map(|o| o.get(t).copied()).collect();
                    let r = median(&at);
                    (m[t] / r - 1.0).abs() * 100.0
                })
                .collect();
            median(&devs)
        })
        .collect()
}

/// Consensus candles: for each candle of the primary source, the median open / high / low / close of every agreeing
/// source that has that timestamp; the volume stays the primary's.
pub fn blend(primary: &[Candle], others: &[Vec<Candle>]) -> Vec<Candle> {
    if others.is_empty() {
        return primary.to_vec();
    }
    let maps: Vec<IndexMap<i64, Candle>> = others.iter().map(|o| o.iter().map(|c| (c.time, *c)).collect()).collect();
    primary
        .iter()
        .map(|c| {
            let mut same = vec![*c];
            same.extend(maps.iter().filter_map(|m| m.get(&c.time).copied()));
            if same.len() < 2 {
                return *c;
            }
            let pick = |f: fn(&Candle) -> f64| median(&same.iter().map(f).collect::<Vec<_>>());
            let open = pick(|x| x.open);
            let close = pick(|x| x.close);
            let high = jmax(jmax(pick(|x| x.high), open), close);
            let low = jmin(jmin(pick(|x| x.low), open), close);
            Candle { time: c.time, open, high, low, close, volume: c.volume }
        })
        .collect()
}

// ---------- Parsers ----------

/// Upstream candle formats (checked against real responses, see tests/market.rs). Errors carry the same messages as
/// the TypeScript (including JavaScriptCore's TypeErrors on a malformed response).
pub mod parse {
    use serde_json::Value;

    use super::ms;
    use crate::http::{Error, Result, err};
    use crate::jsval::{V, array, get, idx, is_num, is_str, need, nullish, number as num, to_string, truthy};
    use crate::types::Candle;

    fn c(time: f64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle { time: ms(time), open, high, low, close, volume }
    }
    fn at(r: &Value, i: usize) -> f64 {
        num(idx(Some(r), i))
    }
    fn f(r: &Value, k: &str) -> f64 {
        num(get(Some(r), k))
    }
    /// `o.data.map(row)` after checking each row is an object (`first` = first property read, as JSC names it).
    fn rows(v: V, expr: &str, call: &str, first: &str, row: impl Fn(&Value) -> Candle) -> Result<Vec<Candle>> {
        let list = array(v, expr, "map", call)?;
        list.iter().map(|r| need(Some(r), first).map(|_| row(r))).collect()
    }

    const BINANCE: &str =
        "d.map((r) => ({ time: num(r[0]), open: num(r[1]), high: num(r[2]), low: num(r[3]), close: num(r[4]), volume: num(r[5]) }))";
    pub fn binance(d: &Value) -> Result<Vec<Candle>> {
        rows(Some(d), "d", BINANCE, "r[0]", |r| c(at(r, 0), at(r, 1), at(r, 2), at(r, 3), at(r, 4), at(r, 5)))
    }

    const OKX: &str =
        "o.data.map((r) => ({ time: num(r[0]), open: num(r[1]), high: num(r[2]), low: num(r[3]), close: num(r[4]), volume: num(r[5]) }))";
    pub fn okx(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.code")?;
        if !is_str(get(Some(d), "code"), "0") {
            return err("OKX");
        }
        let mut out = rows(get(Some(d), "data"), "o.data", OKX, "r[0]", |r| c(at(r, 0), at(r, 1), at(r, 2), at(r, 3), at(r, 4), at(r, 5)))?;
        out.reverse();
        Ok(out)
    }

    const COINBASE: &str = "d.map((r) => ({ time: r[0] * 1000, low: r[1], high: r[2], open: r[3], close: r[4], volume: r[5] }))";
    pub fn coinbase(d: &Value) -> Result<Vec<Candle>> {
        let mut out = rows(Some(d), "d", COINBASE, "r[0]", |r| c(at(r, 0) * 1000.0, at(r, 3), at(r, 2), at(r, 1), at(r, 4), at(r, 5)))?;
        out.reverse();
        Ok(out)
    }

    const KRAKEN_EXPR: &str = "(Object.entries(o.result).find(([k]) => k !== \"last\")?.[1])";
    const KRAKEN: &str = "(Object.entries(o.result).find(([k]) => k !== \"last\")?.[1]).map((r) => ({ time: num(r[0]) * 1000, open: num(r[1]), high: num(r[2]), low: num(r[3]), close: num(r[4]), volume: num(r[6]) }))";
    /// `if (o.error?.length) throw new Error(o.error.join())`, shared with the Kraken quotes.
    pub(crate) fn kraken_error(error: V, expr: &str) -> Result<()> {
        match error {
            Some(Value::Array(a)) if !a.is_empty() => {
                Err(Error(a.iter().map(|x| if x.is_null() { String::new() } else { to_string(Some(x)) }).collect::<Vec<_>>().join(",")))
            }
            Some(Value::String(s)) if !s.is_empty() => {
                Err(Error(format!("{expr}.join is not a function. (In '{expr}.join()', '{expr}.join' is undefined)")))
            }
            _ => Ok(()),
        }
    }
    pub fn kraken(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.error")?;
        kraken_error(get(Some(d), "error"), "o.error")?;
        let result = get(Some(d), "result");
        if nullish(result) {
            return err("Object.entries requires that input parameter not be null or undefined");
        }
        let found: V = match result {
            Some(Value::Object(o)) => o.iter().find(|(k, _)| k.as_str() != "last").map(|(_, v)| v),
            Some(Value::Array(a)) => a.first(),
            Some(Value::String(s)) if !s.is_empty() => {
                return Err(Error(format!("{KRAKEN_EXPR}.map is not a function. (In '{KRAKEN}', '{KRAKEN_EXPR}.map' is undefined)")));
            }
            _ => None,
        };
        rows(found, KRAKEN_EXPR, KRAKEN, "r[0]", |r| c(at(r, 0) * 1000.0, at(r, 1), at(r, 2), at(r, 3), at(r, 4), at(r, 6)))
    }

    const KUCOIN: &str =
        "o.data.map((r) => ({ time: num(r[0]) * 1000, open: num(r[1]), close: num(r[2]), high: num(r[3]), low: num(r[4]), volume: num(r[5]) }))";
    pub fn kucoin(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.code")?;
        if !is_str(get(Some(d), "code"), "200000") {
            return err("KuCoin");
        }
        let mut out =
            rows(get(Some(d), "data"), "o.data", KUCOIN, "r[0]", |r| c(at(r, 0) * 1000.0, at(r, 1), at(r, 3), at(r, 4), at(r, 2), at(r, 5)))?;
        out.reverse();
        Ok(out)
    }

    const GATE: &str =
        "d.map((r) => ({ time: num(r[0]) * 1000, close: num(r[2]), high: num(r[3]), low: num(r[4]), open: num(r[5]), volume: num(r[6]) }))";
    pub fn gate(d: &Value) -> Result<Vec<Candle>> {
        rows(Some(d), "d", GATE, "r[0]", |r| c(at(r, 0) * 1000.0, at(r, 5), at(r, 3), at(r, 4), at(r, 2), at(r, 6)))
    }

    const BITSTAMP: &str = "(d.data?.ohlc ?? []).map((r) => ({\n    time: num(r.timestamp) * 1000,\n    open: num(r.open),\n    high: num(r.high),\n    low: num(r.low),\n    close: num(r.close),\n    volume: num(r.volume)\n  }))";
    pub fn bitstamp(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "d.data")?;
        let ohlc = get(get(Some(d), "data"), "ohlc");
        if nullish(ohlc) {
            return Ok(vec![]);
        }
        rows(ohlc, "(d.data?.ohlc ?? [])", BITSTAMP, "r.timestamp", |r| {
            c(f(r, "timestamp") * 1000.0, f(r, "open"), f(r, "high"), f(r, "low"), f(r, "close"), f(r, "volume"))
        })
    }

    const GEMINI: &str = "d.map((r) => ({ time: r[0], open: r[1], high: r[2], low: r[3], close: r[4], volume: r[5] }))";
    /// Gemini: [ms, open, high, low, close, volume], most recent first.
    pub fn gemini(d: &Value) -> Result<Vec<Candle>> {
        let mut out = rows(Some(d), "d", GEMINI, "r[0]", |r| c(at(r, 0), at(r, 1), at(r, 2), at(r, 3), at(r, 4), at(r, 5)))?;
        out.reverse();
        Ok(out)
    }

    const BITFINEX: &str = "d.map((r) => ({ time: r[0], open: r[1], close: r[2], high: r[3], low: r[4], volume: r[5] }))";
    /// Bitfinex: [ms, open, CLOSE, high, low, volume], most recent first.
    pub fn bitfinex(d: &Value) -> Result<Vec<Candle>> {
        let mut out = rows(Some(d), "d", BITFINEX, "r[0]", |r| c(at(r, 0), at(r, 1), at(r, 3), at(r, 4), at(r, 2), at(r, 5)))?;
        out.reverse();
        Ok(out)
    }

    const CRYPTOCOM: &str =
        "(o.result?.data ?? []).map((r) => ({ time: num(r.t), open: num(r.o), high: num(r.h), low: num(r.l), close: num(r.c), volume: num(r.v) }))";
    pub fn cryptocom(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.code")?;
        if !is_num(get(Some(d), "code"), 0.0) {
            return err("Crypto.com");
        }
        let data = get(get(Some(d), "result"), "data");
        if nullish(data) {
            return Ok(vec![]);
        }
        rows(data, "(o.result?.data ?? [])", CRYPTOCOM, "r.t", |r| c(f(r, "t"), f(r, "o"), f(r, "h"), f(r, "l"), f(r, "c"), f(r, "v")))
    }

    const BITGET: &str =
        "o.data.map((r) => ({ time: num(r[0]), open: num(r[1]), high: num(r[2]), low: num(r[3]), close: num(r[4]), volume: num(r[5]) }))";
    pub fn bitget(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.code")?;
        if !is_str(get(Some(d), "code"), "00000") {
            return err("Bitget");
        }
        rows(get(Some(d), "data"), "o.data", BITGET, "r[0]", |r| c(at(r, 0), at(r, 1), at(r, 2), at(r, 3), at(r, 4), at(r, 5)))
    }

    const HTX: &str = "o.data.map((r) => ({ time: r.id * 1000, open: r.open, high: r.high, low: r.low, close: r.close, volume: r.amount }))";
    /// HTX: {id (s), open, close, low, high, amount}, most recent first.
    pub fn htx(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.status")?;
        if !is_str(get(Some(d), "status"), "ok") {
            return err("HTX");
        }
        let mut out = rows(get(Some(d), "data"), "o.data", HTX, "r.id", |r| {
            c(f(r, "id") * 1000.0, f(r, "open"), f(r, "high"), f(r, "low"), f(r, "close"), f(r, "amount"))
        })?;
        out.reverse();
        Ok(out)
    }

    const POLONIEX: &str =
        "d.map((r) => ({ time: num(r[12]), open: num(r[2]), high: num(r[1]), low: num(r[0]), close: num(r[3]), volume: num(r[5]) }))";
    /// Poloniex: [low, high, open, close, amount, quantity, …, startTime (index 12), closeTime].
    pub fn poloniex(d: &Value) -> Result<Vec<Candle>> {
        rows(Some(d), "d", POLONIEX, "r[12]", |r| c(at(r, 12), at(r, 2), at(r, 1), at(r, 0), at(r, 3), at(r, 5)))
    }

    const HITBTC: &str = "d.map((r) => ({\n    time: Date.parse(r.timestamp),\n    open: num(r.open),\n    high: num(r.max),\n    low: num(r.min),\n    close: num(r.close),\n    volume: num(r.volume)\n  }))";
    /// HitBTC: {timestamp ISO, open, close, min, max, volume}, most recent first.
    pub fn hitbtc(d: &Value) -> Result<Vec<Candle>> {
        let mut out = rows(Some(d), "d", HITBTC, "r.timestamp", |r| {
            let t = get(Some(r), "timestamp");
            let time = if nullish(t) { f64::NAN } else { crate::js::parse_date(&to_string(t)).map(|x| x as f64).unwrap_or(f64::NAN) };
            c(time, f(r, "open"), f(r, "max"), f(r, "min"), f(r, "close"), f(r, "volume"))
        })?;
        out.sort_by_key(|c| c.time);
        Ok(out)
    }

    const WHITEBIT: &str =
        "o.result.map((r) => ({ time: num(r[0]) * 1000, open: num(r[1]), close: num(r[2]), high: num(r[3]), low: num(r[4]), volume: num(r[5]) }))";
    /// WhiteBIT: [time (s), open, close, high, low, base volume, quote volume].
    pub fn whitebit(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.success")?;
        if !truthy(get(Some(d), "success")) {
            return err("WhiteBIT");
        }
        rows(get(Some(d), "result"), "o.result", WHITEBIT, "r[0]", |r| c(at(r, 0) * 1000.0, at(r, 1), at(r, 3), at(r, 4), at(r, 2), at(r, 5)))
    }

    const COINEX: &str = "o.data.map((r) => ({ time: num(r.created_at), open: num(r.open), high: num(r.high), low: num(r.low), close: num(r.close), volume: num(r.volume) }))";
    pub fn coinex(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.code")?;
        if !is_num(get(Some(d), "code"), 0.0) {
            return err("CoinEx");
        }
        rows(get(Some(d), "data"), "o.data", COINEX, "r.created_at", |r| {
            c(f(r, "created_at"), f(r, "open"), f(r, "high"), f(r, "low"), f(r, "close"), f(r, "volume"))
        })
    }

    const XT: &str = "o.result.map((r) => ({ time: num(r.t), open: num(r.o), high: num(r.h), low: num(r.l), close: num(r.c), volume: num(r.q) }))";
    /// XT: {t (ms), o, c, h, l, q (base volume), v (quote volume)}, most recent first.
    pub fn xt(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.rc")?;
        if !is_num(get(Some(d), "rc"), 0.0) {
            return err("XT");
        }
        let mut out = rows(get(Some(d), "result"), "o.result", XT, "r.t", |r| c(f(r, "t"), f(r, "o"), f(r, "h"), f(r, "l"), f(r, "c"), f(r, "q")))?;
        out.reverse();
        Ok(out)
    }

    const WOOX: &str = "o.rows.map((r) => ({ time: r.start_timestamp, open: r.open, high: r.high, low: r.low, close: r.close, volume: r.volume }))";
    /// WOO X: {open, close, low, high, volume, start_timestamp}, most recent first.
    pub fn woox(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.success")?;
        if !truthy(get(Some(d), "success")) {
            return err("WOO X");
        }
        let mut out = rows(get(Some(d), "rows"), "o.rows", WOOX, "r.start_timestamp", |r| {
            c(f(r, "start_timestamp"), f(r, "open"), f(r, "high"), f(r, "low"), f(r, "close"), f(r, "volume"))
        })?;
        out.reverse();
        Ok(out)
    }

    const BINGX: &str = "o.data.map((r) => ({ time: r[0], open: r[1], high: r[2], low: r[3], close: r[4], volume: r[5] }))";
    /// BingX: [time (ms), open, high, low, close, volume, closeTime, quote volume], most recent first.
    pub fn bingx(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.code")?;
        if !is_num(get(Some(d), "code"), 0.0) {
            return err("BingX");
        }
        let mut out = rows(get(Some(d), "data"), "o.data", BINGX, "r[0]", |r| c(at(r, 0), at(r, 1), at(r, 2), at(r, 3), at(r, 4), at(r, 5)))?;
        out.reverse();
        Ok(out)
    }

    const LBANK: &str = "o.data.map((r) => ({ time: r[0] * 1000, open: r[1], high: r[2], low: r[3], close: r[4], volume: r[5] }))";
    /// LBank: [time (s), open, high, low, close, volume].
    pub fn lbank(d: &Value) -> Result<Vec<Candle>> {
        need(Some(d), "o.result")?;
        if to_string(get(Some(d), "result")) != "true" {
            return err("LBank");
        }
        rows(get(Some(d), "data"), "o.data", LBANK, "r[0]", |r| c(at(r, 0) * 1000.0, at(r, 1), at(r, 2), at(r, 3), at(r, 4), at(r, 5)))
    }
}

/// Stock daily / hourly candle formats, dated at the opening of the New York session.
pub mod parse_stock {
    use serde_json::Value;

    use super::{ms, ny_open_parts};
    use crate::http::{Error, Result};
    use crate::jsval::{array, get, idx, need, nullish, number, slice_utf16, string_to_number, to_string, truthy, truthy_num};
    use crate::types::Candle;

    fn or0(x: f64) -> f64 {
        if truthy_num(x) { x } else { 0.0 }
    }

    const YAHOO: &str = "(r.timestamp ?? []).flatMap((t, i) => {\n      const [o, h, l, c] = [q.open?.[i], q.high?.[i], q.low?.[i], q.close?.[i]];\n      if ([o, h, l, c].some((v) => v == null))\n        return [];\n      return [{ time: t * 1000, open: o, high: h, low: l, close: c, volume: q.volume?.[i] ?? 0 }];\n    })";
    pub fn yahoo(d: &Value) -> Result<Vec<Candle>> {
        let d = Some(d);
        let r = idx(get(get(d, "chart"), "result"), 0);
        if !truthy(r) {
            let desc = get(get(get(d, "chart"), "error"), "description");
            return Err(Error(if nullish(desc) { "Yahoo : aucune donnée".into() } else { to_string(desc) }));
        }
        let q = idx(get(get(r, "indicators"), "quote"), 0);
        let ts = get(r, "timestamp");
        let ts = if nullish(ts) { &[][..] } else { array(ts, "(r.timestamp ?? [])", "flatMap", YAHOO)? };
        let mut out = Vec::new();
        for (i, t) in ts.iter().enumerate() {
            let [o, h, l, c] = ["open", "high", "low", "close"].map(|k| idx(get(q, k), i));
            if [o, h, l, c].iter().any(|v| nullish(*v)) {
                continue;
            }
            let volume = idx(get(q, "volume"), i);
            let volume = if nullish(volume) { 0.0 } else { number(volume) };
            out.push(Candle { time: ms(number(Some(t)) * 1000.0), open: number(o), high: number(h), low: number(l), close: number(c), volume });
        }
        Ok(out)
    }

    const NASDAQ: &str = "rows.map((r) => {\n      const [m, day, y] = String(r.date).split(\"/\").map(Number);\n      return { time: nyOpen(y, m, day), open: num(r.open), high: num(r.high), low: num(r.low), close: num(r.close), volume: num(r.volume) || 0 };\n    })";
    pub fn nasdaq(d: &Value) -> Result<Vec<Candle>> {
        let rows = get(get(get(Some(d), "data"), "tradesTable"), "rows");
        let rows = if nullish(rows) { &[][..] } else { array(rows, "rows", "map", NASDAQ)? };
        let num = |v| string_to_number(&to_string(v).replace(['$', ','], ""));
        let mut out = rows
            .iter()
            .map(|r| {
                need(Some(r), "r.date")?;
                let r = Some(r);
                let time = ny_open_parts(&to_string(get(r, "date")), '/', [2, 0, 1])?;
                Ok(Candle {
                    time,
                    open: num(get(r, "open")),
                    high: num(get(r, "high")),
                    low: num(get(r, "low")),
                    close: num(get(r, "close")),
                    volume: or0(num(get(r, "volume"))),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        out.reverse();
        Ok(out)
    }

    const ROBINHOOD: &str = "(d?.historicals ?? []).filter((r) => r.session === \"reg\" && !r.interpolated)";
    /// Robinhood daily history: regular session only, dated at midnight UTC.
    pub fn robinhood(d: &Value) -> Result<Vec<Candle>> {
        let list = get(Some(d), "historicals");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.historicals ?? [])", "filter", ROBINHOOD)? };
        let mut out = Vec::new();
        for r in list {
            need(Some(r), "r.session")?;
            let r = Some(r);
            if get(r, "session").and_then(|v| v.as_str()) != Some("reg") || truthy(get(r, "interpolated")) {
                continue;
            }
            let date = to_string(get(r, "begins_at"));
            let time = ny_open_parts(slice_utf16(&date, 10), '-', [0, 1, 2])?;
            let n = |k: &str| number(get(r, k));
            out.push(Candle {
                time,
                open: n("open_price"),
                high: n("high_price"),
                low: n("low_price"),
                close: n("close_price"),
                volume: or0(n("volume")),
            });
        }
        Ok(out)
    }

    const STOCKANALYSIS: &str = "(d?.data ?? []).map((r) => {\n    const [y, m, day] = String(r.t).split(\"-\").map(Number);\n    return { time: nyOpen(y, m, day), open: Number(r.o), high: Number(r.h), low: Number(r.l), close: Number(r.c), volume: Number(r.v) || 0 };\n  })";
    /// StockAnalysis daily history: {"data": [{"t": "2026-09-25", "o", "h", "l", "c", "v"}]}, most recent first.
    pub fn stockanalysis(d: &Value) -> Result<Vec<Candle>> {
        let list = get(Some(d), "data");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.data ?? [])", "map", STOCKANALYSIS)? };
        let mut out = list
            .iter()
            .map(|r| {
                need(Some(r), "r.t")?;
                let r = Some(r);
                let time = ny_open_parts(&to_string(get(r, "t")), '-', [0, 1, 2])?;
                let n = |k: &str| number(get(r, k));
                Ok(Candle { time, open: n("o"), high: n("h"), low: n("l"), close: n("c"), volume: or0(n("v")) })
            })
            .collect::<Result<Vec<_>>>()?;
        out.reverse();
        Ok(out)
    }

    const WEBULL: &str = "(d?.[0]?.data ?? []).map((row) => {\n    const f = row.split(\",\").map(Number), [y, m, day] = new Date(f[0] * 1000).toISOString().slice(0, 10).split(\"-\").map(Number);\n    return { time: nyOpen(y, m, day), open: f[1], close: f[2], high: f[3], low: f[4], volume: f[6] || 0 };\n  })";
    /// Webull daily chart: "time (s, midnight New York),open,close,high,low,previousClose,volume,vwap", most recent first.
    pub fn webull(d: &Value) -> Result<Vec<Candle>> {
        let list = get(idx(Some(d), 0), "data");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.[0]?.data ?? [])", "map", WEBULL)? };
        let mut out = list
            .iter()
            .map(|row| {
                need(Some(row), "row.split")?;
                let Some(row) = row.as_str() else {
                    return Err(Error("row.split is not a function. (In 'row.split(\",\")', 'row.split' is undefined)".into()));
                };
                let f: Vec<f64> = row.split(',').map(string_to_number).collect();
                let at = |i: usize| f.get(i).copied().unwrap_or(f64::NAN);
                let t = (at(0) * 1000.0).trunc();
                if !t.is_finite() || t.abs() > 8.64e15 {
                    return Err(Error("Invalid Date".into()));
                }
                let iso = crate::js::iso(t as i64);
                let time = ny_open_parts(slice_utf16(&iso, 10), '-', [0, 1, 2])?;
                Ok(Candle { time, open: at(1), close: at(2), high: at(3), low: at(4), volume: or0(at(6)) })
            })
            .collect::<Result<Vec<_>>>()?;
        out.reverse();
        Ok(out)
    }

    /// Cboe daily history (since 2004): the last 800 sessions are enough.
    pub fn cboe(d: &Value) -> Result<Vec<Candle>> {
        let list = get(Some(d), "data");
        let list = match list {
            None | Some(Value::Null) => &[][..],
            Some(Value::Array(a)) => a.as_slice(),
            _ => {
                return Err(Error(
                    "(d?.data ?? []).slice is not a function. (In '(d?.data ?? []).slice(-800)', '(d?.data ?? []).slice' is undefined)".into(),
                ));
            }
        };
        list[list.len().saturating_sub(800)..]
            .iter()
            .map(|r| {
                need(Some(r), "r.date")?;
                let r = Some(r);
                let time = ny_open_parts(&to_string(get(r, "date")), '-', [0, 1, 2])?;
                let n = |k: &str| number(get(r, k));
                Ok(Candle { time, open: n("open"), high: n("high"), low: n("low"), close: n("close"), volume: or0(n("volume")) })
            })
            .collect()
    }
}

// ---------- Sources ----------

pub type CandleFuture = BoxFuture<'static, Result<Vec<Candle>>>;

/// A candle provider: `fetch(base, interval)`, optionally limited to some timeframes.
#[derive(Clone)]
pub struct Source {
    pub name: String,
    pub supports: Option<Arc<dyn Fn(Interval) -> bool + Send + Sync>>,
    pub fetch: Arc<dyn Fn(String, Interval) -> CandleFuture + Send + Sync>,
}

impl Source {
    pub fn new<F, Fut>(name: &str, fetch: F) -> Self
    where
        F: Fn(String, Interval) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Vec<Candle>>> + Send + 'static,
    {
        Source { name: name.into(), supports: None, fetch: Arc::new(move |b, i| fetch(b, i).boxed()) }
    }
    /// Limits the source to the timeframes `f` accepts.
    pub fn only(mut self, f: impl Fn(Interval) -> bool + Send + Sync + 'static) -> Self {
        self.supports = Some(Arc::new(f));
        self
    }
    pub fn supports(&self, i: Interval) -> bool {
        self.supports.as_ref().is_none_or(|f| f(i))
    }
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Source").field("name", &self.name).finish()
    }
}

fn pick(i: Interval, h1: &'static str, h4: &'static str, d1: &'static str) -> &'static str {
    match i {
        Interval::H1 => h1,
        Interval::H4 => h4,
        Interval::D1 => d1,
    }
}
fn lower(s: &str) -> String {
    s.to_lowercase()
}
fn kraken_pair(b: &str) -> &str {
    if b == "BTC" { "XBT" } else { b }
}
fn not_daily(i: Interval) -> bool {
    i != Interval::D1
}
fn daily(i: Interval) -> bool {
    i == Interval::D1
}

pub static SOURCES: LazyLock<Vec<Source>> = LazyLock::new(|| {
    vec![
        Source::new("Binance", |b, i| async move {
            parse::binance(&get_json(&format!("https://api.binance.com/api/v3/klines?symbol={b}USDT&interval={}&limit=500", i.as_str())).await?)
        }),
        Source::new("OKX", |b, i| async move {
            parse::okx(
                &get_json(&format!("https://www.okx.com/api/v5/market/candles?instId={b}-USDT&bar={}&limit=300", pick(i, "1H", "4H", "1Dutc")))
                    .await?,
            )
        }),
        Source::new("Coinbase", |b, i| async move {
            let c = parse::coinbase(
                &get_json(&format!(
                    "https://api.exchange.coinbase.com/products/{b}-USD/candles?granularity={}",
                    if i == Interval::D1 { 86400 } else { 3600 }
                ))
                .await?,
            )?;
            Ok(if i == Interval::H4 { aggregate(&c, Interval::H1.step(), Interval::H4.step()) } else { c })
        }),
        Source::new("Kraken", |b, i| async move {
            parse::kraken(
                &get_json(&format!("https://api.kraken.com/0/public/OHLC?pair={}USD&interval={}", kraken_pair(&b), i.step() / 60_000)).await?,
            )
        }),
        Source::new("KuCoin", |b, i| async move {
            let end = now_ms().div_euclid(1000);
            let t = pick(i, "1hour", "4hour", "1day");
            parse::kucoin(
                &get_json(&format!(
                    "https://api.kucoin.com/api/v1/market/candles?type={t}&symbol={b}-USDT&startAt={}&endAt={end}",
                    end - (i.step() / 1000) * 500
                ))
                .await?,
            )
        }),
        Source::new("Gate.io", |b, i| async move {
            parse::gate(
                &get_json(&format!("https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair={b}_USDT&interval={}&limit=500", i.as_str()))
                    .await?,
            )
        }),
        Source::new("Bitstamp", |b, i| async move {
            parse::bitstamp(&get_json(&format!("https://www.bitstamp.net/api/v2/ohlc/{}usd/?step={}&limit=500", lower(&b), i.step() / 1000)).await?)
        }),
        Source::new("Gemini", |b, i| async move {
            // No 4 h candles at Gemini: rebuilt from 1 h.
            let c = parse::gemini(
                &get_json(&format!("https://api.gemini.com/v2/candles/{}usd/{}", lower(&b), if i == Interval::D1 { "1day" } else { "1hr" })).await?,
            )?;
            Ok(if i == Interval::H4 { aggregate(&c, Interval::H1.step(), Interval::H4.step()) } else { c })
        }),
        Source::new("Bitfinex", |b, i| async move {
            let pair = if b.encode_utf16().count() > 3 { format!("t{b}:USD") } else { format!("t{b}USD") };
            let url = format!(
                "https://api-pub.bitfinex.com/v2/candles/trade:{}:{pair}/hist?limit={}",
                if i == Interval::D1 { "1D" } else { "1h" },
                if i == Interval::H4 { 2000 } else { 500 }
            );
            let c = parse::bitfinex(&get_json(&url).await?)?;
            Ok(if i == Interval::H4 { aggregate(&c, Interval::H1.step(), Interval::H4.step()) } else { c })
        }),
        Source::new("Crypto.com", |b, i| async move {
            parse::cryptocom(
                &get_json(&format!(
                    "https://api.crypto.com/exchange/v1/public/get-candlestick?instrument_name={b}_USDT&timeframe={}&count=300",
                    pick(i, "1h", "4h", "1D")
                ))
                .await?,
            )
        }),
        Source::new("Bitget", |b, i| async move {
            parse::bitget(
                &get_json(&format!(
                    "https://api.bitget.com/api/v2/spot/market/candles?symbol={b}USDT&granularity={}&limit=500",
                    pick(i, "1h", "4h", "1Dutc")
                ))
                .await?,
            )
        }),
        Source::new("MEXC", |b, i| async move {
            parse::binance(
                &get_json(&format!("https://api.mexc.com/api/v3/klines?symbol={b}USDT&interval={}&limit=500", pick(i, "60m", "4h", "1d"))).await?,
            )
        }),
        // HTX daily candles start at 16:00 UTC (UTC+8): hourly timeframes only.
        Source::new("HTX", |b, i| async move {
            parse::htx(
                &get_json(&format!(
                    "https://api.huobi.pro/market/history/kline?symbol={}usdt&period={}&size=500",
                    lower(&b),
                    if i == Interval::H1 { "60min" } else { "4hour" }
                ))
                .await?,
            )
        })
        .only(not_daily),
        Source::new("Binance.US", |b, i| async move {
            parse::binance(&get_json(&format!("https://api.binance.us/api/v3/klines?symbol={b}USDT&interval={}&limit=500", i.as_str())).await?)
        }),
        Source::new("Poloniex", |b, i| async move {
            parse::poloniex(
                &get_json(&format!("https://api.poloniex.com/markets/{b}_USDT/candles?interval={}&limit=500", pick(i, "HOUR_1", "HOUR_4", "DAY_1")))
                    .await?,
            )
        }),
        Source::new("HitBTC", |b, i| async move {
            parse::hitbtc(
                &get_json(&format!("https://api.hitbtc.com/api/3/public/candles/{b}USDT?period={}&limit=500", pick(i, "H1", "H4", "D1"))).await?,
            )
        }),
        Source::new("WhiteBIT", |b, i| async move {
            parse::whitebit(&get_json(&format!("https://whitebit.com/api/v1/public/kline?market={b}_USDT&interval={}&limit=500", i.as_str())).await?)
        }),
        Source::new("CoinEx", |b, i| async move {
            parse::coinex(
                &get_json(&format!("https://api.coinex.com/v2/spot/kline?market={b}USDT&period={}&limit=500", pick(i, "1hour", "4hour", "1day")))
                    .await?,
            )
        }),
        Source::new("XT", |b, i| async move {
            parse::xt(&get_json(&format!("https://sapi.xt.com/v4/public/kline?symbol={}_usdt&interval={}&limit=500", lower(&b), i.as_str())).await?)
        }),
        Source::new("WOO X", |b, i| async move {
            parse::woox(&get_json(&format!("https://api.woox.io/v1/public/kline?symbol=SPOT_{b}_USDT&type={}&limit=500", i.as_str())).await?)
        }),
        // BingX daily candles start at 16:00 UTC (UTC+8): intraday timeframes only.
        Source::new("BingX", |b, i| async move {
            parse::bingx(
                &get_json(&format!("https://open-api.bingx.com/openApi/spot/v2/market/kline?symbol={b}-USDT&interval={}&limit=500", i.as_str()))
                    .await?,
            )
        })
        .only(not_daily),
        // LBank daily candles start at 16:00 UTC (UTC+8): intraday timeframes only.
        Source::new("LBank", |b, i| async move {
            let since = (now_ms() - 500 * i.step()).div_euclid(1000);
            let url = format!(
                "https://api.lbkex.com/v2/kline.do?symbol={}_usdt&size=500&type={}&time={since}",
                lower(&b),
                if i == Interval::H1 { "hour1" } else { "hour4" }
            );
            parse::lbank(&get_json(&url).await?)
        })
        .only(not_daily),
    ]
});

/// Class shares: BRK-B (Yahoo) = BRK.B elsewhere.
fn dotted(s: &str) -> String {
    s.replace('-', ".")
}

pub static STOCK_SOURCES: LazyLock<Vec<Source>> = LazyLock::new(|| {
    let mut v = vec![
        Source::new("Yahoo Finance", |symbol, i| async move {
            let (interval, range) = match i {
                Interval::D1 => ("1d", "2y"),
                Interval::H4 => ("1h", "2y"),
                Interval::H1 => ("1h", "6mo"),
            };
            let url =
                format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval={interval}&range={range}&includePrePost=false", enc(&symbol));
            let c = parse_stock::yahoo(&get_json(&url).await?)?;
            Ok(if i == Interval::H4 { aggregate_session(&c, 4) } else { c })
        }),
        Source::new("Nasdaq", |symbol, _| async move {
            let from = iso_date(now_ms() - 800 * 86_400_000);
            for cls in ["stocks", "etf"] {
                let url = format!(
                    "https://api.nasdaq.com/api/quote/{}/historical?assetclass={cls}&fromdate={from}&todate={}&limit=9999",
                    enc(&dotted(&symbol)),
                    iso_date(now_ms())
                );
                let c = parse_stock::nasdaq(&get_json(&url).await?)?;
                if !c.is_empty() {
                    return Ok(c);
                }
            }
            err("symbole inconnu")
        })
        .only(daily),
        // Robinhood hourly bars start on the hour (Yahoo's at :30): daily only, to compare the same candles.
        Source::new("Robinhood", |symbol, _| async move {
            parse_stock::robinhood(
                &get_json(&format!(
                    "https://api.robinhood.com/marketdata/historicals/{}/?interval=day&span=5year&bounds=regular",
                    enc(&dotted(&symbol))
                ))
                .await?,
            )
        })
        .only(daily),
        Source::new("StockAnalysis", |symbol, _| async move {
            parse_stock::stockanalysis(
                &get_json(&format!("https://stockanalysis.com/api/symbol/s/{}/history?range=5Y&period=daily", enc(&dotted(&symbol).to_lowercase())))
                    .await?,
            )
        })
        .only(daily),
        Source::new("Webull", |symbol, _| async move {
            let id = webull_ticker_id(&symbol).await;
            let Some(id) = id.filter(|x| crate::jsval::truthy_num(*x)) else { return err("non coté") };
            let url =
                format!("https://quotes-gw.webullfintech.com/api/quote/charts/query?tickerIds={}&type=d1&count=800", crate::js::number_to_string(id));
            parse_stock::webull(&get_json(&url).await?)
        })
        .only(daily),
        Source::new("Cboe", |symbol, _| async move {
            parse_stock::cboe(
                &get_json(&format!("https://cdn.cboe.com/api/global/delayed_quotes/charts/historical/{}.json", enc(&dotted(&symbol)))).await?,
            )
        })
        .only(daily),
    ];
    // Daily candles from Dow Jones (WSJ / MarketWatch), AlphaQuery, Finviz, the Financial Times and eToro (stocks_extra).
    // Finviz serves 10 years: the last 1 300 sessions (≈ 5 years) are enough.
    for s in EXTRA_CANDLE_SOURCES.iter() {
        let fetch = s.fetch;
        v.push(
            Source::new(s.name, move |symbol, _| async move {
                let c = fetch(symbol).await?;
                Ok(c[c.len().saturating_sub(1300)..].to_vec())
            })
            .only(s.supports),
        );
    }
    v
});

/// Long daily history (≈ 3 years) for the long-term horizon: only sources that serve 700–1 000 days.
pub static LONG_SOURCES: LazyLock<Vec<Source>> = LazyLock::new(|| {
    vec![
        Source::new("Bitstamp", |b, _| async move {
            parse::bitstamp(&get_json(&format!("https://www.bitstamp.net/api/v2/ohlc/{}usd/?step=86400&limit=1000", lower(&b))).await?)
        }),
        Source::new("Binance", |b, _| async move {
            parse::binance(&get_json(&format!("https://api.binance.com/api/v3/klines?symbol={b}USDT&interval=1d&limit=1000")).await?)
        }),
        Source::new("Gate.io", |b, _| async move {
            parse::gate(&get_json(&format!("https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair={b}_USDT&interval=1d&limit=1000")).await?)
        }),
        Source::new("MEXC", |b, _| async move {
            parse::binance(&get_json(&format!("https://api.mexc.com/api/v3/klines?symbol={b}USDT&interval=1d&limit=1000")).await?)
        }),
        Source::new("Kraken", |b, _| async move {
            parse::kraken(&get_json(&format!("https://api.kraken.com/0/public/OHLC?pair={}USD&interval=1440", kraken_pair(&b))).await?)
        }),
    ]
});

// ---------- Consensus ----------

/// Filter applied to each source's candles (`closedOnly` for crypto, `stockClosed` for stocks).
pub type Closed = fn(Vec<Candle>, Interval) -> Vec<Candle>;

struct Fetched {
    name: String,
    candles: Option<Vec<Candle>>,
    error: Option<String>,
}

/// Queries the sources in waves until `target` of them answer (`None` = all, `Infinity` in the TypeScript), drops the
/// lagging ones, compares the others with the median and blends the agreeing ones.
pub async fn consensus(
    base: &str,
    interval: Interval,
    sources: &[Source],
    target: Option<usize>,
    tolerance: f64,
    closed: Closed,
) -> Result<Consensus> {
    let sources: Vec<&Source> = sources.iter().filter(|s| s.supports(interval)).collect();
    let mut results: Vec<Fetched> = Vec::new();
    let ok_count = |r: &[Fetched]| r.iter().filter(|x| x.candles.is_some()).count();
    let mut cursor = 0;
    while cursor < sources.len() && target.is_none_or(|t| ok_count(&results) < t) {
        let n = target.map_or(sources.len(), |t| t - ok_count(&results));
        let wave = &sources[cursor..(cursor + n).min(sources.len())];
        cursor += wave.len();
        let fetched = join_all(wave.iter().map(|s| {
            let fut = (s.fetch)(base.to_string(), interval);
            let name = s.name.clone();
            async move {
                match fut.await {
                    Ok(c) => {
                        let candles = closed(c, interval);
                        if candles.is_empty() {
                            Fetched { name, candles: None, error: Some("vide".into()) }
                        } else {
                            Fetched { name, candles: Some(candles), error: None }
                        }
                    }
                    Err(e) => Fetched { name, candles: None, error: Some(e.0) },
                }
            }
        }))
        .await;
        results.extend(fetched);
    }
    // A source whose last candle lags behind the others (inactive market, delisted pair) is discarded.
    let latest = results.iter().filter_map(|r| r.candles.as_ref().map(|c| c[c.len() - 1].time)).max();
    if let Some(latest) = latest {
        for r in results.iter_mut() {
            let last = r.candles.as_ref().map(|c| c[c.len() - 1].time);
            if let Some(last) = last {
                if (last as f64) < latest as f64 - 2.0 * interval.step() as f64 {
                    r.error = Some(format!("en retard (dernière bougie du {})", iso_date(last)));
                    r.candles = None;
                }
            }
        }
    }
    let ok: Vec<usize> = (0..results.len()).filter(|&i| results[i].candles.is_some()).collect();
    if ok.is_empty() {
        return err("Toutes les sources ont échoué");
    }
    let devs = if ok.len() > 1 { deviations(&ok.iter().map(|&i| results[i].candles.clone().unwrap()).collect::<Vec<_>>()) } else { vec![0.0] };
    // Positions (in `ok`) of the agreeing sources.
    let agreeing: Vec<usize> = (0..ok.len()).filter(|&k| devs[k] <= tolerance).collect();
    let primary = agreeing.iter().copied().find(|&k| results[ok[k]].candles.as_ref().unwrap().len() >= 60).or(agreeing.first().copied()).unwrap_or(0);
    let others: Vec<Vec<Candle>> = agreeing.iter().filter(|&&k| k != primary).map(|&k| results[ok[k]].candles.clone().unwrap()).collect();
    let candles = blend(results[ok[primary]].candles.as_ref().unwrap(), &others);
    let sources = results
        .iter()
        .enumerate()
        .map(|(i, r)| match ok.iter().position(|&j| j == i) {
            Some(k) => SourceStatus { name: r.name.clone(), ok: devs[k] <= tolerance, deviation: Some(devs[k]), error: None },
            None => SourceStatus { name: r.name.clone(), ok: false, deviation: None, error: r.error.clone() },
        })
        .collect();
    Ok(Consensus {
        candles,
        source: results[ok[primary]].name.clone(),
        agreeing: agreeing.len(),
        // Fewer than half of the responding sources agree: impossible to know which ones are right.
        conflict: ok.len() > 1 && agreeing.len() * 2 < ok.len(),
        sources,
    })
}

/// Long daily history (≈ 3 years) for the long-term horizon.
pub async fn long_daily(symbol: &str, kind: Kind) -> Result<Consensus> {
    if kind == Kind::Crypto {
        return consensus(symbol, Interval::D1, &LONG_SOURCES, None, 0.5, closed_only).await;
    }
    let order = ["Robinhood", "StockAnalysis", "WSJ / MarketWatch", "Finviz", "Nasdaq", "Webull", "Cboe", "AlphaQuery", "Financial Times", "eToro"];
    let sources: Vec<Source> = order.iter().filter_map(|n| STOCK_SOURCES.iter().find(|s| s.name == *n).cloned()).collect();
    consensus(symbol, Interval::D1, &sources, None, 1.0, stock_closed).await
}

/// Intraday stocks: Yahoo's last close cross-checked with the live price of the other providers
/// (tolerance: 2 % on 1 h, 3 % on 4 h).
pub fn cross_check(last: f64, quotes: &[QuoteSourceStatus], known: &[String], tolerance: f64) -> Vec<SourceStatus> {
    quotes
        .iter()
        .filter(|q| !known.contains(&q.name))
        .map(|q| match q.price.filter(|p| crate::jsval::truthy_num(*p)) {
            None => SourceStatus {
                name: format!("{} (cours)", q.name),
                ok: false,
                deviation: None,
                error: Some(q.error.clone().unwrap_or_else(|| "non coté".into())),
            },
            Some(price) => {
                let deviation = (price / last - 1.0).abs() * 100.0;
                SourceStatus { name: format!("{} (cours)", q.name), ok: deviation <= tolerance, deviation: Some(deviation), error: None }
            }
        })
        .collect()
}

/// Validated snapshot: multi-source candles + quality + reliability score.
pub async fn snapshot(symbol: &str, kind: Kind, interval: Interval) -> Result<Snapshot> {
    let intra_stock = kind == Kind::Stock && interval != Interval::D1;
    // Live prices are fetched at the same time as the candles (not after).
    let candles = async {
        match kind {
            Kind::Crypto => consensus(symbol, interval, &SOURCES, None, 0.5, closed_only).await,
            Kind::Stock => consensus(symbol, interval, &STOCK_SOURCES, None, 1.0, stock_closed).await,
        }
    };
    let quotes = async { if intra_stock { consensus_quotes(&[make_asset(symbol, Kind::Stock, None)], &QUOTE_SOURCES).await } else { vec![] } };
    let (c, quotes) = tokio::join!(candles, quotes);
    let mut c = c?;
    if intra_stock && !c.candles.is_empty() {
        let known: Vec<String> = c.sources.iter().map(|s| s.name.clone()).collect();
        let empty = vec![];
        let q = quotes.first().map(|q| &q.sources).unwrap_or(&empty);
        let extra = cross_check(c.candles[c.candles.len() - 1].close, q, &known, if interval == Interval::H1 { 2.0 } else { 3.0 });
        c.sources.extend(extra);
    }
    let quality = assess_quality(&c.candles, interval.step(), kind, now_ms());
    // Distinct providers that agree (a provider's candles and its live price count once).
    let independent: IndexSet<&str> = c.sources.iter().filter(|s| s.ok).map(|s| s.name.strip_suffix(" (cours)").unwrap_or(&s.name)).collect();
    let rel = reliability(quality.score, independent.len(), c.conflict);
    let agreeing = c.sources.iter().filter(|s| s.ok).count();
    Ok(Snapshot {
        candles: c.candles,
        source: c.source,
        agreeing,
        conflict: c.conflict,
        sources: c.sources,
        symbol: symbol.into(),
        kind,
        interval,
        quality,
        reliability: rel,
    })
}

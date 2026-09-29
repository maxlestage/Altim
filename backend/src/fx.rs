//! EUR/USD rate for the euro display: Yahoo Finance `EURUSD=X` (intraday), else the ECB daily reference rate, else
//! Frankfurter (ECB-based). Engines keep computing in dollars; only the French texts and the clients' display convert.
//! Never invented: without any valid rate, amounts stay in dollars with the "$" sign.
use std::cell::Cell;
use std::sync::{LazyLock, RwLock};
use std::time::Duration;

use chrono::{NaiveDate, TimeZone};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::engine::format::format_price;
use crate::http::{Error, Result, get_json_with, get_text_with};
use crate::js::now_ms;

/// A rate younger than this is served as is; older, a refresh is started.
pub const FRESH_MS: i64 = 10 * 60_000;
/// The last valid rate is kept this long when every source fails, then dropped (amounts back in dollars).
pub const KEEP_MS: i64 = 7 * 24 * 3_600_000;
/// At most one refresh attempt per minute while the sources fail.
const RETRY_MS: i64 = 60_000;
const TIMEOUT: Duration = Duration::from_secs(5);

pub const YAHOO_URL: &str = "https://query1.finance.yahoo.com/v8/finance/chart/EURUSD=X?interval=1d&range=5d";
pub const ECB_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
pub const FRANKFURTER_URL: &str = "https://api.frankfurter.dev/v1/latest?base=USD&symbols=EUR";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FxRate {
    /// Euros for 1 dollar.
    pub rate: f64,
    /// Dollars for 1 euro (the quoted EUR/USD).
    pub usd_per_eur: f64,
    /// Time of the quote (ms): Yahoo's last trade, or 14:10 Frankfurt time of the ECB reference day.
    pub time: i64,
    /// Short source name ("Yahoo Finance", "BCE", "Frankfurter (BCE)").
    pub source: String,
    /// When Altim read it (ms).
    pub fetched: i64,
}

impl FxRate {
    fn new(usd_per_eur: f64, time: i64, source: &str) -> Result<FxRate> {
        // EUR/USD has stayed between 0.8 and 1.6 since 1999: anything far outside is a parsing error.
        if !(usd_per_eur.is_finite() && (0.3..=3.0).contains(&usd_per_eur)) {
            return Err(Error(format!("{source} : taux invalide ({usd_per_eur})")));
        }
        Ok(FxRate { rate: 1.0 / usd_per_eur, usd_per_eur, time, source: source.into(), fetched: now_ms() })
    }
}

/// Pure parsers (tested offline on saved responses).
pub mod parse {
    use super::*;

    /// Yahoo chart of `EURUSD=X`: `meta.regularMarketPrice` (dollars per euro) at `meta.regularMarketTime` (s).
    pub fn yahoo(d: &Value) -> Result<FxRate> {
        let meta = &d["chart"]["result"][0]["meta"];
        if meta["symbol"].as_str() != Some("EURUSD=X") {
            return Err(Error("Yahoo : réponse sans EURUSD=X".into()));
        }
        let price = meta["regularMarketPrice"].as_f64().ok_or_else(|| Error("Yahoo : cours absent".into()))?;
        let time = meta["regularMarketTime"].as_i64().ok_or_else(|| Error("Yahoo : heure absente".into()))? * 1000;
        FxRate::new(price, time, "Yahoo Finance")
    }

    /// ECB reference rates are set at 14:10 Frankfurt time on the day they carry.
    fn ecb_time(date: &str) -> Option<i64> {
        let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        chrono_tz::Europe::Berlin.from_local_datetime(&d.and_hms_opt(14, 10, 0)?).single().map(|t| t.timestamp_millis())
    }

    static ECB_DAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<Cube\s+time=['"](\d{4}-\d{2}-\d{2})['"]"#).unwrap());
    static ECB_USD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"<Cube\s+currency=['"]USD['"]\s+rate=['"]([0-9.]+)['"]"#).unwrap());

    /// ECB `eurofxref-daily.xml`: `<Cube time='YYYY-MM-DD'>` then `<Cube currency='USD' rate='1.1355'/>` (dollars per euro).
    pub fn ecb(xml: &str) -> Result<FxRate> {
        let day = ECB_DAY.captures(xml).map(|c| c[1].to_string()).ok_or_else(|| Error("BCE : date absente".into()))?;
        let usd = ECB_USD.captures(xml).and_then(|c| c[1].parse::<f64>().ok()).ok_or_else(|| Error("BCE : taux USD absent".into()))?;
        FxRate::new(usd, ecb_time(&day).ok_or_else(|| Error("BCE : date illisible".into()))?, "BCE")
    }

    /// Frankfurter `latest?base=USD&symbols=EUR`: `{ base: "USD", date, rates: { EUR } }` (euros per dollar).
    pub fn frankfurter(d: &Value) -> Result<FxRate> {
        if d["base"].as_str() != Some("USD") {
            return Err(Error("Frankfurter : base inattendue".into()));
        }
        let eur = d["rates"]["EUR"].as_f64().filter(|v| *v > 0.0).ok_or_else(|| Error("Frankfurter : taux EUR absent".into()))?;
        let time = d["date"].as_str().and_then(ecb_time).ok_or_else(|| Error("Frankfurter : date absente".into()))?;
        FxRate::new(1.0 / eur, time, "Frankfurter (BCE)")
    }
}

static CURRENT: RwLock<Option<FxRate>> = RwLock::new(None);
/// Last refresh attempt (ms), held during a refresh so concurrent callers wait for it instead of querying again.
static REFRESH: LazyLock<tokio::sync::Mutex<(i64, Vec<String>)>> = LazyLock::new(|| tokio::sync::Mutex::new((0, Vec::new())));

tokio::task_local! {
    /// A request that asked for dollars (`cur=USD`): its texts ignore the rate.
    static FORCE_USD: bool;
}

/// Runs `f` with the texts in dollars when `usd` (a client showing dollars), else as usual.
pub async fn scope<F: std::future::Future>(usd: bool, f: F) -> F::Output {
    FORCE_USD.scope(usd, f).await
}

/// Whether the current request asked for dollars (to carry into a task spawned by the cache).
pub fn forced_usd() -> bool {
    FORCE_USD.try_with(|v| *v).unwrap_or(false)
}

thread_local! {
    /// Tests: Some(Some(rate)) forces a rate on this thread, Some(None) forces "no rate"; None reads the cache.
    static TEST_RATE: Cell<Option<Option<f64>>> = const { Cell::new(None) };
}

/// Tests only: forces the rate seen by `current()` and `money()` on this thread (None = no rate), keeping the tests
/// deterministic whatever the shared cache holds. `clear_test_rate` goes back to the cache.
pub fn set_test_rate(rate: Option<f64>) {
    TEST_RATE.with(|t| t.set(Some(rate)));
}

pub fn clear_test_rate() {
    TEST_RATE.with(|t| t.set(None));
}

/// Stores a rate read elsewhere (tests, warm-up).
pub fn store(r: FxRate) {
    *CURRENT.write().unwrap() = Some(r);
}

/// The cached rate, when younger than 7 days (no network).
pub fn current() -> Option<FxRate> {
    if let Some(forced) = TEST_RATE.with(|t| t.get()) {
        return forced.map(|rate| FxRate { rate, usd_per_eur: 1.0 / rate, time: 0, source: "test".into(), fetched: now_ms() });
    }
    if forced_usd() {
        return None;
    }
    CURRENT.read().unwrap().clone().filter(|r| now_ms() - r.fetched < KEEP_MS)
}

/// Queries the sources in order until one answers with a valid rate.
async fn fetch_rate() -> std::result::Result<FxRate, Vec<String>> {
    let mut errors = Vec::new();
    match get_json_with(YAHOO_URL, &[], TIMEOUT).await.and_then(|d| parse::yahoo(&d)) {
        Ok(r) => return Ok(r),
        Err(e) => errors.push(format!("Yahoo Finance : {e}")),
    }
    match get_text_with(ECB_URL, &[("Accept", "application/xml")], TIMEOUT).await.and_then(|x| parse::ecb(&x)) {
        Ok(r) => return Ok(r),
        Err(e) => errors.push(format!("BCE : {e}")),
    }
    match get_json_with(FRANKFURTER_URL, &[], TIMEOUT).await.and_then(|d| parse::frankfurter(&d)) {
        Ok(r) => return Ok(r),
        Err(e) => errors.push(format!("Frankfurter : {e}")),
    }
    Err(errors)
}

async fn refresh() {
    let mut last = REFRESH.lock().await;
    let now = now_ms();
    if current().is_some_and(|r| now - r.fetched < FRESH_MS) || now - last.0 < RETRY_MS {
        return;
    }
    last.0 = now;
    match fetch_rate().await {
        Ok(r) => {
            last.1.clear();
            store(r);
        }
        Err(e) => last.1 = e,
    }
}

/// The rate, refreshed when older than 10 minutes: in the background when a valid one is cached (the request never
/// waits), otherwise awaited (a few seconds at most per source).
pub async fn ensure() -> Option<FxRate> {
    if TEST_RATE.with(|t| t.get()).is_some() {
        return current();
    }
    match current() {
        Some(r) if now_ms() - r.fetched < FRESH_MS => Some(r),
        Some(r) => {
            tokio::spawn(refresh());
            Some(r)
        }
        None => {
            refresh().await;
            current()
        }
    }
}

/// `usd` converted for display: (value, symbol) in euros with a rate, else in dollars (never a made-up rate).
pub fn convert(usd: f64) -> (f64, &'static str) {
    match current() {
        Some(r) => (usd * r.rate, "€"),
        None => (usd, "$"),
    }
}

/// The one helper of the server texts: "212,40 €" (formatPrice digits), "212,40 $" without a rate.
pub fn money(usd: f64) -> String {
    money_with(usd, format_price)
}

/// Same with the caller's number format, applied to the converted value.
pub fn money_with(usd: f64, fmt: impl Fn(f64) -> String) -> String {
    let (v, sym) = convert(usd);
    format!("{} {sym}", fmt(v))
}

/// Unit suffix glued to a scale ("M€", "Md$"): `unit("M")`.
pub fn unit(scale: &str) -> String {
    format!("{scale}{}", convert(0.0).1)
}

/// Rate used by the texts of a response, as an additive `fx` field: `{ currency, rate, usdPerEur, time, source }`
/// (`currency` "USD" with null rate when no rate is known: the texts then show dollars).
pub fn info() -> Value {
    match current() {
        Some(r) => serde_json::json!({ "currency": "EUR", "rate": r.rate, "usdPerEur": r.usd_per_eur, "time": r.time, "source": r.source }),
        None => serde_json::json!({ "currency": "USD", "rate": null, "usdPerEur": null, "time": null, "source": null }),
    }
}

/// Part of a cache key: texts cached with one rate are not served with another.
pub fn cache_tag() -> String {
    current().map(|r| format!("eur{}", r.fetched)).unwrap_or_else(|| "usd".into())
}

/// `GET /api/fx` body: `{ base: "USD", quote: "EUR", rate, usdPerEur, time, source, fetchedAt, stale, error? }`; with no
/// valid rate, `rate` and the others are null and `error` lists what failed.
pub async fn report() -> Value {
    let r = ensure().await;
    let errors = REFRESH.try_lock().map(|g| g.1.clone()).unwrap_or_default();
    match r {
        Some(r) => serde_json::json!({
            "base": "USD", "quote": "EUR", "rate": r.rate, "usdPerEur": r.usd_per_eur, "time": r.time, "source": r.source,
            "fetchedAt": r.fetched, "stale": now_ms() - r.fetched >= FRESH_MS,
        }),
        None => serde_json::json!({
            "base": "USD", "quote": "EUR", "rate": null, "usdPerEur": null, "time": null, "source": null, "fetchedAt": null,
            "stale": true, "error": if errors.is_empty() { "taux indisponible".to_string() } else { format!("taux indisponible ({})", errors.join(" ; ")) },
        }),
    }
}

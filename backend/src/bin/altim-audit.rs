//! Data audit: compares what Altim shows with independent references it does not use for the same figure.
//!   cargo run --release --bin altim-audit -- [server]   (default http://localhost:3000: a local server started with
//!   `ALTIM_DEV_OPEN=1 cargo run --release --bin altim`, without login)
//! - Live prices: crypto vs CoinPaprika (not an Altim source), stocks vs Yahoo Finance (1 source out of ~12).
//! - Daily closes over a year (history, charts, simulator): crypto vs Coinbase daily candles (Altim's daily history
//!   comes from Bitstamp, Binance, Gate, MEXC, Kraken), stocks vs Yahoo daily chart (Altim: Robinhood, Nasdaq, WSJ…).
//!   CoinGecko was tried first and dropped: its daily points are off by 10–14 % on some days (LINK on 7 March 2026:
//!   10,16 $ where Coinbase, Kraken, Bitstamp and Gate all closed at 8,70 $).
//! - Change shown next to each price: crypto over 24 h (recomputed from Coinbase 5-minute candles), stocks since the
//!   previous close (Yahoo); the moves of "Point du jour" since the last daily close; the simulator on BTC.
//!
//! Prints a table and exits 1 when a figure is off by more than the tolerance.
use std::collections::HashMap;
use std::time::Duration;

use altim_core::engine::history::Close;
use altim_core::web::portfolio::dca::{ONCE, simulate_dca};
use serde_json::Value;

const UA: &str = "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 AltimAudit/1.0";
/// Symbol, Coinbase product (None: not listed there), CoinPaprika id.
const CRYPTO: [(&str, Option<&str>, &str); 9] = [
    ("BTC", Some("BTC-USD"), "btc-bitcoin"),
    ("ETH", Some("ETH-USD"), "eth-ethereum"),
    ("SOL", Some("SOL-USD"), "sol-solana"),
    ("BNB", None, "bnb-binance-coin"),
    ("XRP", Some("XRP-USD"), "xrp-xrp"),
    ("ADA", Some("ADA-USD"), "ada-cardano"),
    ("DOGE", Some("DOGE-USD"), "doge-dogecoin"),
    ("AVAX", Some("AVAX-USD"), "avax-avalanche"),
    ("LINK", Some("LINK-USD"), "link-chainlink"),
];
const STOCKS: [&str; 8] = ["AAPL", "NVDA", "MSFT", "AMZN", "GOOGL", "META", "TSLA", "SPY"];
const DAY: i64 = 86_400_000;

struct Row {
    check: String,
    asset: String,
    altim: String,
    reference: String,
    gap: f64,
    tolerance: f64,
}

/// `v.toFixed(2)` from 100, else `v.toPrecision(5)`.
fn fmt(v: f64) -> String {
    if v >= 100.0 || v == 0.0 || !v.is_finite() {
        return format!("{v:.2}");
    }
    let digits = (4 - v.abs().log10().floor() as i32).max(0) as usize;
    format!("{v:.digits$}")
}

/// `new Date(ms).toISOString()`.
fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)).unwrap_or_default()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct Audit {
    http: reqwest::Client,
    rows: Vec<Row>,
}

impl Audit {
    async fn json(&self, url: &str) -> Result<Value, String> {
        for _ in 0..3 {
            let r = self.http.get(url).timeout(Duration::from_secs(30)).send().await.map_err(|e| format!("{url} → {e}"))?;
            match r.status().as_u16() {
                200..=299 => return r.json().await.map_err(|e| format!("{url} → {e}")),
                429 => tokio::time::sleep(Duration::from_secs(15)).await,
                s => return Err(format!("{url} → HTTP {s}")),
            }
        }
        Err(format!("{url} → trop de requêtes"))
    }

    fn row(&mut self, check: &str, asset: &str, altim: String, reference: String, gap: f64, tolerance: f64) {
        self.rows.push(Row { check: check.into(), asset: asset.into(), altim, reference, gap, tolerance });
    }

    /// The relative gap of Altim's figure to the reference, in %.
    fn add(&mut self, check: &str, asset: &str, a: f64, reference: f64, tolerance: f64) {
        self.row(check, asset, fmt(a), fmt(reference), (a / reference - 1.0).abs() * 100.0, tolerance);
    }

    /// Coinbase daily closes of the last 370 days (300 candles at most per request), by UTC day.
    async fn coinbase_daily(&self, product: &str) -> Result<Vec<Close>, String> {
        let mut out = Vec::new();
        let end = now();
        let mut to = end;
        while to > end - 370 * DAY {
            let from = (end - 370 * DAY).max(to - 300 * DAY);
            let url = format!("https://api.exchange.coinbase.com/products/{product}/candles?granularity=86400&start={}&end={}", iso(from), iso(to));
            out.extend(candles(&self.json(&url).await?).iter().map(|c| ((c[0] * 1000.0) as i64, c[4])));
            tokio::time::sleep(Duration::from_millis(400)).await;
            to -= 300 * DAY;
        }
        out.sort_by_key(|c| c.0);
        Ok(out)
    }

    /// Median and worst gap of Altim's closes to the reference ones of the same UTC day.
    fn closes(&mut self, asset: &str, ours: &[Close], reference: &HashMap<i64, f64>, source: &str) {
        let mut gaps: Vec<f64> = ours
            .iter()
            .filter_map(|(t, c)| reference.get(&(t.div_euclid(DAY) * DAY)).filter(|r| **r != 0.0).map(|r| (c / r - 1.0).abs() * 100.0))
            .collect();
        gaps.sort_by(f64::total_cmp);
        let median = gaps.get(gaps.len() / 2).copied().unwrap_or(f64::NAN);
        let worst = gaps.last().copied().unwrap_or(f64::NAN);
        self.row(&format!("clôtures 1 an ({} j) médiane", gaps.len()), asset, format!("{} clôtures", ours.len()), source.into(), median, 0.3);
        self.row("clôtures 1 an pire écart", asset, String::new(), String::new(), worst, 2.0);
    }
}

/// Rows of numbers (`number[][]`).
fn candles(v: &Value) -> Vec<Vec<f64>> {
    v.as_array()
        .map(|a| a.iter().map(|c| c.as_array().map(|c| c.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default()).collect())
        .unwrap_or_default()
}

fn find<'a>(list: &'a Value, symbol: &str, kind: Option<&str>) -> Option<&'a Value> {
    list.as_array()?.iter().find(|x| x["symbol"] == symbol && kind.is_none_or(|k| x["kind"] == k))
}

fn yahoo_meta(chart: &Value, key: &str) -> Option<f64> {
    chart["chart"]["result"][0]["meta"][key].as_f64()
}

async fn run(server: &str) -> Result<Vec<Row>, String> {
    let http = reqwest::Client::builder().user_agent(UA).build().map_err(|e| e.to_string())?;
    let mut a = Audit { http, rows: Vec::new() };
    let assets = CRYPTO.iter().map(|(s, ..)| format!("{s}:crypto")).chain(STOCKS.iter().map(|s| format!("{s}:stock"))).collect::<Vec<_>>().join(",");

    // ---------- Live prices ----------
    let tickers = a.json(&format!("{server}/api/tickers?symbols={assets}")).await?;
    let paprika = a.json("https://api.coinpaprika.com/v1/tickers?limit=100").await?;
    for (sym, product, pap) in CRYPTO {
        let t = find(&tickers, sym, Some("crypto"));
        let reference = paprika.as_array().and_then(|l| l.iter().find(|p| p["id"] == pap)).map(|p| &p["quotes"]["USD"]);
        let (Some(t), Some(r)) = (t.and_then(|t| t["price"].as_f64()), reference.and_then(|r| r["price"].as_f64())) else {
            let (altim, reference) = (t.and_then(|t| t["price"].as_f64()).map(fmt), reference.and_then(|r| r["price"].as_f64()).map(fmt));
            a.row("prix", sym, altim.unwrap_or_else(|| "—".into()), reference.unwrap_or_else(|| "—".into()), f64::NAN, 1.0);
            continue;
        };
        // Different instants (a few seconds to minutes apart): 1 % tolerance.
        a.add("prix en direct", sym, t, r, 1.0);
        // 24 h change: recomputed from Coinbase 5-minute candles (CoinPaprika's own 24 h figure lags by up to a point).
        let change = find(&tickers, sym, Some("crypto")).and_then(|t| t["change"].as_f64());
        if let (Some(change), Some(product)) = (change, product) {
            // 5-minute candles (300 = 25 h): the price 24 h ago within 5 minutes.
            let c = candles(&a.json(&format!("https://api.exchange.coinbase.com/products/{product}/candles?granularity=300")).await?);
            if let Some(first) = c.first() {
                if let Some(ago) = c.iter().find(|h| h[0] <= first[0] - 24.0 * 3600.0) {
                    let ref_change = (first[4] / ago[4] - 1.0) * 100.0;
                    a.row("variation 24 h (points de %)", sym, format!("{change:.2}"), format!("{ref_change:.2}"), (change - ref_change).abs(), 0.5);
                }
            }
        }
    }
    let mut yahoo: HashMap<&str, Value> = HashMap::new();
    for s in STOCKS {
        let chart = a.json(&format!("https://query1.finance.yahoo.com/v8/finance/chart/{s}?range=1y&interval=1d")).await?;
        if let (Some(t), Some(r)) = (find(&tickers, s, Some("stock")).and_then(|t| t["price"].as_f64()), yahoo_meta(&chart, "regularMarketPrice")) {
            a.add("prix en direct", s, t, r, 0.5);
        }
        yahoo.insert(s, chart);
    }

    // ---------- Daily closes over a year ----------
    let hist = a.json(&format!("{server}/api/history?days=365&symbols={assets}")).await?;
    let closes_of = |sym: &str, kind: &str| -> Vec<Close> {
        let series = hist["series"].as_array().and_then(|l| l.iter().find(|s| s["symbol"] == sym && s["kind"] == kind));
        candles(series.map(|s| &s["closes"]).unwrap_or(&Value::Null)).iter().filter(|c| c.len() >= 2).map(|c| (c[0] as i64, c[1])).collect()
    };
    for (sym, product, _) in CRYPTO {
        let Some(product) = product else { continue };
        let reference: HashMap<i64, f64> = a.coinbase_daily(product).await?.into_iter().collect();
        a.closes(sym, &closes_of(sym, "crypto"), &reference, "Coinbase");
    }
    for s in STOCKS {
        let r = &yahoo[s]["chart"]["result"][0];
        let times = r["timestamp"].as_array().cloned().unwrap_or_default();
        let closes = r["indicators"]["quote"][0]["close"].as_array().cloned().unwrap_or_default();
        let reference: HashMap<i64, f64> = times
            .iter()
            .zip(closes.iter())
            .filter_map(|(t, c)| Some(((t.as_f64()? * 1000.0) as i64).div_euclid(DAY) * DAY).zip(c.as_f64()))
            .collect();
        a.closes(s, &closes_of(s, "stock"), &reference, "Yahoo");
        // Change since the previous close, as shown on the radar.
        let day = a.json(&format!("https://query1.finance.yahoo.com/v8/finance/chart/{s}?range=1d&interval=1d")).await?;
        let change = find(&tickers, s, Some("stock")).and_then(|t| t["change"].as_f64());
        if let (Some(change), Some(p), Some(prev)) = (change, yahoo_meta(&day, "regularMarketPrice"), yahoo_meta(&day, "chartPreviousClose")) {
            let ref_change = (p / prev - 1.0) * 100.0;
            a.row("variation affichée (points de %)", s, format!("{change:.2}"), format!("{ref_change:.2}"), (change - ref_change).abs(), 0.5);
        }
    }

    // ---------- Simulator ("Si j'avais investi") on BTC, replayed on the reference closes ----------
    {
        let ours = closes_of("BTC", "crypto");
        // Same days as Altim (its last closed day), so both replays buy on the same dates.
        let last = ours.last().map(|c| c.0).unwrap_or(0);
        let reference: Vec<Close> = a.coinbase_daily("BTC-USD").await?.into_iter().filter(|(t, _)| *t <= last).collect();
        let t = now();
        for (every, label) in [(30, "100 $/mois 1 an"), (0, "achat unique 1 an")] {
            let sim = |c: &[Close]| if every > 0 { simulate_dca(c, 100.0, every, 365, t) } else { simulate_dca(c, 1300.0, ONCE, 365, t) };
            match (sim(&ours), sim(&reference)) {
                (Some(x), Some(r)) => a.add(&format!("simulateur BTC {label} (valeur)"), "BTC", x.value, r.value, 0.5),
                _ => a.row(&format!("simulateur BTC {label} (valeur)"), "BTC", "—".into(), "—".into(), f64::NAN, 0.5),
            }
        }
    }

    // ---------- "Point du jour": moves since the last daily close ----------
    {
        let brief = a.json(&format!("{server}/api/brief?symbols=BTC:crypto,ETH:crypto,SOL:crypto")).await?;
        for (sym, product, _) in CRYPTO.iter().take(3) {
            let Some(product) = product else { continue };
            let start = now().div_euclid(DAY) * DAY - DAY;
            let today = candles(
                &a.json(&format!(
                    "https://api.exchange.coinbase.com/products/{product}/candles?granularity=86400&start={}&end={}",
                    iso(start),
                    iso(now())
                ))
                .await?,
            );
            let prev_close = today.iter().find(|c| (c[0] * 1000.0) as i64 == start).map(|c| c[4]);
            let m = find(&brief["movers"], sym, None).and_then(|m| m["change"].as_f64());
            let price = find(&tickers, sym, None).and_then(|t| t["price"].as_f64());
            if let (Some(m), Some(prev), Some(p)) = (m, prev_close, price) {
                let r = (p / prev - 1.0) * 100.0;
                a.row("point du jour : variation depuis la clôture", sym, format!("{m:.2}"), format!("{r:.2}"), (m - r).abs(), 0.3);
            }
        }
    }
    Ok(a.rows)
}

#[tokio::main]
async fn main() {
    let server = std::env::args().nth(1).unwrap_or_else(|| "http://localhost:3000".into());
    let rows = match run(server.trim_end_matches('/')).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("✘ {e}");
            std::process::exit(2);
        }
    };
    let ok = |r: &Row| r.gap <= r.tolerance;
    let table: Vec<[String; 7]> = rows
        .iter()
        .map(|r| {
            [
                r.check.clone(),
                r.asset.clone(),
                r.altim.clone(),
                r.reference.clone(),
                if r.gap.is_finite() { format!("{:.3} %", r.gap) } else { "—".into() },
                r.tolerance.to_string(),
                if ok(r) { "✔" } else { "✘" }.into(),
            ]
        })
        .collect();
    let head = ["contrôle", "actif", "altim", "référence", "écart", "tolérance", "ok"].map(String::from);
    let widths: Vec<usize> = (0..7).map(|i| table.iter().chain([&head]).map(|r| r[i].chars().count()).max().unwrap_or(0)).collect();
    for r in [&head].into_iter().chain(table.iter()) {
        let cells: Vec<String> = r.iter().zip(&widths).map(|(c, w)| format!("{c}{}", " ".repeat(w - c.chars().count()))).collect();
        println!("{}", cells.join(" │ "));
    }
    let bad = rows.iter().filter(|r| !ok(r)).count();
    if bad > 0 {
        println!("✘ {bad} écart(s) au-delà de la tolérance");
        std::process::exit(1);
    }
    println!("✔ Toutes les données sont dans la tolérance");
}

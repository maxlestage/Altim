//! Real data, run by the daily "Santé des sources" workflow: `cargo test --test sources_live -- --ignored`.
//! Every source must still answer in the expected format, so that an API change is caught before it skews a
//! signal (port of the former web/test/sources-live.test.ts).
use std::time::Duration;

use altim::live::{ChanIds, FEEDS};
use altim::market::{SOURCES, STOCK_SOURCES};
use altim::quotes::{QUOTE_SOURCES, make_asset};
use altim::types::{Interval, Kind};
use futures::{SinkExt, StreamExt, future::join_all};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

const INTERVALS: [Interval; 3] = [Interval::H1, Interval::H4, Interval::D1];

/// HTTP 451: the exchange refuses this country (Binance from the US); that is not an API change.
fn report(results: Vec<(String, Result<f64, String>)>) {
    let mut failed = Vec::new();
    for (label, r) in &results {
        match r {
            Err(e) if e.contains("HTTP 451") => println!("~ {label} : bloqué dans ce pays"),
            Err(e) => {
                println!("✕ {label} : {e}");
                failed.push(label.clone());
            }
            Ok(_) => {}
        }
    }
    println!("{}/{} sources OK", results.len() - failed.len(), results.len());
    assert!(failed.is_empty(), "sources en échec : {failed:?}");
}

#[tokio::test]
#[ignore]
async fn candles_every_source_every_interval() {
    let mut jobs = Vec::new();
    for (sources, base) in [(&*SOURCES, "BTC"), (&*STOCK_SOURCES, "AAPL")] {
        for s in sources.iter() {
            for i in INTERVALS.into_iter().filter(|i| s.supports(*i)) {
                let label = format!("{} {base} {}", s.name, i.as_str());
                let fut = (s.fetch)(base.to_string(), i);
                jobs.push(async move {
                    let r = match tokio::time::timeout(Duration::from_secs(60), fut).await {
                        Err(_) => Err("délai dépassé".to_string()),
                        Ok(Err(e)) => Err(e.0),
                        Ok(Ok(c)) if c.len() < 20 => Err(format!("{} bougies seulement", c.len())),
                        Ok(Ok(c)) => {
                            let last = c[c.len() - 1];
                            if last.close > 0.0 && last.high >= last.low { Ok(c.len() as f64) } else { Err("bougie invalide".into()) }
                        }
                    };
                    (label, r)
                });
            }
        }
    }
    report(join_all(jobs).await);
}

#[tokio::test]
#[ignore]
async fn quotes_every_source() {
    let assets = [make_asset("BTC", Kind::Crypto, None), make_asset("AAPL", Kind::Stock, None)];
    let results = join_all(QUOTE_SOURCES.iter().map(|s| {
        let mine: Vec<_> = assets.iter().filter(|a| a.kind == s.kind).cloned().collect();
        let sym = if s.kind == Kind::Crypto { "BTC" } else { "AAPL" };
        let fut = (s.fetch)(mine);
        async move {
            let r = match tokio::time::timeout(Duration::from_secs(60), fut).await {
                Err(_) => Err("délai dépassé".to_string()),
                Ok(Err(e)) => Err(e.0),
                Ok(Ok(q)) => q.get(sym).map(|x| x.price).filter(|p| *p > 0.0).ok_or_else(|| "aucun cours".to_string()),
            };
            (s.name.clone(), r)
        }
    }))
    .await;
    report(results);
}

#[tokio::test]
#[ignore]
async fn realtime_every_websocket_exchange() {
    let results = join_all(FEEDS.iter().map(|f| async move {
        let run = async {
            let (mut ws, _) = tokio_tungstenite::connect_async(f.url).await.map_err(|_| "connexion impossible".to_string())?;
            for m in (f.subscribe)(&["BTC".to_string()]) {
                let text = match m {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                ws.send(Message::text(text)).await.map_err(|e| e.to_string())?;
            }
            let mut chan = ChanIds(Vec::new());
            while let Some(msg) = ws.next().await {
                let Ok(Message::Text(t)) = msg else { continue };
                let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                if let Some(p) = (f.parse)(&v, &mut chan).into_iter().find(|p| p.base == "BTC") {
                    return Ok(p.price);
                }
            }
            Err("connexion fermée".to_string())
        };
        let r = tokio::time::timeout(Duration::from_secs(15), run).await.unwrap_or_else(|_| Err("aucun prix en 15 s".into()));
        (f.name.to_string(), r)
    }))
    .await;
    report(results);
}

#[tokio::test]
#[ignore]
async fn macro_series_five_years() {
    let results = join_all(altim::macro_data::MACRO_SYMBOLS.iter().map(|(k, s)| async move {
        let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=5y", altim::jsval::encode_uri_component(s));
        let r = match altim::http::get_json_with(&url, &[("User-Agent", "Mozilla/5.0")], Duration::from_secs(30)).await {
            Err(e) => Err(e.0),
            Ok(d) => match altim::market::parse_stock::yahoo(&d) {
                Err(e) => Err(e.0),
                Ok(c) if c.len() < 500 => Err(format!("{} séances seulement", c.len())),
                Ok(c) => Ok(c.len() as f64),
            },
        };
        (format!("{} ({s})", k.as_str()), r)
    }))
    .await;
    report(results);
}

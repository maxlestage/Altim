//! Live check against the real providers (network): `cargo test --test live_network -- --ignored --nocapture`.
//! Writes the Rust results to $ALTIM_LIVE_OUT (JSON) to compare with the TypeScript ones (parity/live-check.ts).
use std::time::Duration;

use altim::js::to_value;
use altim::live::LiveHub;
use altim::market::snapshot;
use altim::quotes::{ASSETS, QUOTE_SOURCES, consensus_quotes};
use altim::types::{Interval, Kind};
use serde_json::json;

#[tokio::test]
#[ignore]
async fn live_snapshot_and_quotes() {
    let (btc, aapl, aapl_h, quotes) = tokio::join!(
        snapshot("BTC", Kind::Crypto, Interval::H1),
        snapshot("AAPL", Kind::Stock, Interval::D1),
        snapshot("AAPL", Kind::Stock, Interval::H1),
        consensus_quotes(&ASSETS, &QUOTE_SOURCES),
    );
    let err = |e: altim::http::Error| json!({ "error": e.0 });
    let out = json!({
        "btc1h": btc.map(|s| to_value(&s)).unwrap_or_else(err),
        "aapl1d": aapl.map(|s| to_value(&s)).unwrap_or_else(err),
        "aapl1h": aapl_h.map(|s| to_value(&s)).unwrap_or_else(err),
        "quotes": to_value(&quotes),
    });
    if let Ok(path) = std::env::var("ALTIM_LIVE_OUT") {
        std::fs::write(path, out.to_string()).unwrap();
    }
    for k in ["btc1h", "aapl1d", "aapl1h"] {
        let s = &out[k];
        println!("{k}: source {} agreeing {} candles {} last {} reliability {}", s["source"], s["agreeing"], s["candles"].as_array().map_or(0, |c| c.len()), s["candles"].as_array().and_then(|c| c.last()).map_or(json!(null), |c| c["close"].clone()), s["reliability"]);
    }
    for q in quotes {
        println!("{} {} {:?} {}/{}", q.symbol, q.price, q.change, q.agreeing, q.total);
    }
}

/// Raw candles of every daily stock source (to compare provider by provider with the TypeScript).
#[tokio::test]
#[ignore]
async fn live_stock_sources_raw() {
    let mut out = serde_json::Map::new();
    let sources: Vec<_> = altim::market::STOCK_SOURCES.iter().filter(|s| s.supports(Interval::D1)).collect();
    let res = futures::future::join_all(sources.iter().map(|s| (s.fetch)("AAPL".into(), Interval::D1))).await;
    for (s, r) in sources.iter().zip(res) {
        out.insert(s.name.clone(), r.map(|c| to_value(&c)).unwrap_or_else(|e| json!({ "error": e.0 })));
    }
    if let Ok(path) = std::env::var("ALTIM_LIVE_OUT") {
        std::fs::write(path, serde_json::Value::Object(out).to_string()).unwrap();
    }
}

#[tokio::test]
#[ignore]
async fn live_hub_websockets() {
    let hub = LiveHub::new();
    let mut sub = hub.subscribe(&["crypto:BTC".into(), "crypto:ETH".into(), "stock:AAPL".into()]);
    let mut seen = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(t) = sub.recv().await {
            println!("{}", to_value(&t));
            seen.push(t);
            if seen.len() >= 12 {
                break;
            }
        }
    })
    .await;
    let max_sources = seen.iter().filter(|t| t.kind == Kind::Crypto).map(|t| t.total).max().unwrap_or(0);
    println!("ticks {} ; sources crypto max {max_sources}", seen.len());
    assert!(!seen.is_empty());
    hub.close();
}

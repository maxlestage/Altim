//! 4 d and 1 w candles (altim-core `candles`, market.rs `MULTI_DAY_*`), on real responses saved on 2026-10-03
//! (samples/intervals: MEXC BTCUSDT 1W and 1d, Yahoo AAPL 1wk over 10 years and 1d over 5 years). No upstream call.
use std::sync::Arc;

use altim::app::{AppState, router};
use altim::auth::Auth;
use altim::cache::cached;
use altim::engine::reliability::{assess_quality, reliability};
use altim::http::Error;
use altim::js::now_ms;
use altim::live::LiveHub;
use altim::market::{MULTI_DAY_SOURCES, MULTI_DAY_STOCK_SOURCES, Snapshot, Source, closed_only, consensus, from_daily, parse, parse_stock};
use altim::quotes::ConsensusQuote;
use altim::types::{Candle, DAY_MS, Interval, Kind};
use altim_core::candles::{WEEK_ANCHOR_MS, align, bucket};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

fn sample(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{}/tests/samples/intervals/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}
fn mexc(name: &str) -> Vec<Candle> {
    parse::binance(&sample(name)).unwrap()
}
fn yahoo(name: &str) -> Vec<Candle> {
    parse_stock::yahoo(&sample(name)).unwrap()
}

/// Closes of `a` and `b` on their shared bucket times (at least `min` of them), all within `tol` %.
fn same_closes(a: &[Candle], b: &[Candle], min: usize, tol: f64) {
    let shared: Vec<(f64, f64)> = a.iter().filter_map(|x| b.iter().find(|y| y.time == x.time).map(|y| (x.close, y.close))).collect();
    assert!(shared.len() >= min, "{} shared candles", shared.len());
    for (x, y) in shared {
        assert!((x / y - 1.0).abs() * 100.0 <= tol, "{x} vs {y}");
    }
}

#[test]
fn weeks_rebuilt_from_days_match_the_native_weeks() {
    // MEXC: 500 days grouped by Monday-anchored week = its own weekly candles.
    let native = align(&mexc("mexc-1w.json"), Interval::W1);
    let rebuilt = bucket(&mexc("mexc-1d.json"), Interval::W1);
    assert!(native.len() >= 460 && rebuilt.len() >= 70, "{} {}", native.len(), rebuilt.len());
    assert!(native.iter().chain(&rebuilt).all(|c| (c.time - WEEK_ANCHOR_MS) % (7 * DAY_MS) == 0));
    same_closes(&rebuilt, &native, 70, 0.0001);
    let (n, r) = (native.iter().find(|c| c.time == rebuilt[10].time).unwrap(), rebuilt[10]);
    assert!((n.open / r.open - 1.0).abs() < 1e-6 && (n.high / r.high - 1.0).abs() < 1e-6 && (n.low / r.low - 1.0).abs() < 1e-6);
    // Yahoo: weeks dated Monday 04:00 UTC plus a live point, aligned on Monday 00:00 UTC; 5 years of sessions agree.
    let native = align(&yahoo("yahoo-1wk.json"), Interval::W1);
    let rebuilt = bucket(&yahoo("yahoo-1d.json"), Interval::W1);
    assert!(native.len() >= 500 && rebuilt.len() >= 250, "{} {}", native.len(), rebuilt.len());
    assert!(native.windows(2).all(|w| w[1].time - w[0].time == 7 * DAY_MS));
    same_closes(&rebuilt, &native, 250, 0.01);
}

#[test]
fn four_day_buckets_from_real_days() {
    // Stocks: 5 years of sessions (weekends, holidays) → ≈ 456 buckets on the epoch grid, every one with 1 to 4 sessions.
    let days = yahoo("yahoo-1d.json");
    let b = bucket(&days, Interval::D4);
    assert!(b.len() >= 400, "{}", b.len());
    assert!(b.iter().all(|c| c.time % (4 * DAY_MS) == 0));
    assert!(b.windows(2).all(|w| w[1].time > w[0].time && w[1].time - w[0].time <= 8 * DAY_MS));
    let first = b[0].time;
    let inside: Vec<&Candle> = days.iter().filter(|c| c.time >= first && c.time < first + 4 * DAY_MS).collect();
    assert!((1..=4).contains(&inside.len()));
    assert_eq!((b[0].open, b[0].close), (inside[0].open, inside[inside.len() - 1].close));
    assert_eq!(b[0].volume, inside.iter().map(|c| c.volume).sum::<f64>());
    // Crypto: 500 days = 124 complete buckets + the cut first one dropped (+ the forming one, dropped as not closed).
    let b = closed_only(bucket(&mexc("mexc-1d.json"), Interval::D4), Interval::D4);
    assert!((120..=125).contains(&b.len()), "{}", b.len());
    assert!(b.last().unwrap().time + 4 * DAY_MS <= now_ms());
}

fn fixed(name: &str, c: Vec<Candle>, expect: Interval) -> Source {
    Source::new(name, move |_, i| {
        let c = c.clone();
        async move {
            assert_eq!(i, expect);
            Ok(c)
        }
    })
}

#[tokio::test]
async fn weekly_consensus_native_and_rebuilt_agree() {
    let sources = [
        fixed("MEXC", align(&mexc("mexc-1w.json"), Interval::W1), Interval::W1),
        from_daily(&fixed("MEXC jours", mexc("mexc-1d.json"), Interval::D1)),
    ];
    let r = consensus("BTC", Interval::W1, &sources, None, 0.5, closed_only).await.unwrap();
    assert_eq!((r.source.as_str(), r.agreeing, r.conflict), ("MEXC", 2, false));
    assert!(r.candles.len() >= 460);
}

#[test]
fn multi_day_source_lists() {
    let names = |v: &[Source]| v.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    // Deepest history first (it gives the candles): Bitstamp's days for 4 d, the native weeks for 1 w.
    assert_eq!(names(&MULTI_DAY_SOURCES[0]), ["Bitstamp", "Binance", "Gate.io", "MEXC", "Kraken"]);
    assert_eq!(names(&MULTI_DAY_SOURCES[1]), ["Binance", "MEXC", "Bitstamp", "Gate.io", "Kraken"]);
    assert_eq!(MULTI_DAY_STOCK_SOURCES[0].name, "Yahoo Finance");
    assert!(MULTI_DAY_STOCK_SOURCES.iter().all(|s| !s.supports(Interval::H4) && !s.supports(Interval::D1)));
    // Daily candles starting at 16:00 UTC (HTX, BingX, LBank) never build 4 d / 1 w buckets.
    for s in altim::market::SOURCES.iter().filter(|s| ["HTX", "BingX", "LBank"].contains(&s.name.as_str())) {
        assert!(!s.supports(Interval::D4) && !s.supports(Interval::W1) && s.supports(Interval::H4));
    }
}

/// A snapshot from offline candles, as `market::snapshot` builds it.
async fn seed(symbol: &str, interval: Interval, candles: Vec<Candle>) {
    let c = closed_only(candles, interval);
    let quality = assess_quality(&c, interval.step(), Kind::Crypto, now_ms());
    let rel = reliability(quality.score, 3, false);
    let s = Snapshot {
        candles: c,
        source: "MEXC".into(),
        agreeing: 3,
        conflict: false,
        sources: vec![],
        symbol: symbol.into(),
        kind: Kind::Crypto,
        interval,
        quality,
        reliability: rel,
    };
    cached(&format!("snap:crypto:{symbol}:{}", interval.as_str()), 3_600_000, || async move { Ok::<_, Error>(s) }).await.unwrap();
}

async fn get(path: &str) -> (StatusCode, Value) {
    let app = router(AppState::new(LiveHub::with(vec![], vec![], std::time::Duration::from_secs(3600))), Auth::new(None, false));
    let r = app.oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = r.status();
    (status, serde_json::from_slice(&to_bytes(r.into_body(), 1 << 22).await.unwrap()).unwrap())
}

#[tokio::test]
async fn radar_on_four_days_and_weeks() {
    seed("BTC", Interval::W1, align(&mexc("mexc-1w.json"), Interval::W1)).await;
    seed("BTC", Interval::D4, bucket(&mexc("mexc-1d.json"), Interval::D4)).await;
    let none: Arc<Vec<ConsensusQuote>> = cached("quotes:crypto:BTC", 3_600_000, || async { Ok::<_, Error>(vec![]) }).await.unwrap();
    assert!(none.is_empty());
    for iv in ["1w", "4d"] {
        let (s, v) = get(&format!("/api/radar?symbols=BTC:crypto&interval={iv}")).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let row = &v[0];
        assert_eq!(row["symbol"], "BTC");
        assert!(row["signal"]["action"].is_string(), "{iv}: {row}");
        assert!(row["sparkline"].as_array().is_some_and(|a| a.len() == 48));
    }
    let (s, v) = get("/api/radar?symbols=BTC:crypto&interval=2w").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"], "interval invalide (1h | 4h | 1d | 4d | 1w)");
}

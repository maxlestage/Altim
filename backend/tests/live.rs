//! Port of web/test/live.test.ts (the /api/live SSE route tests belong to the route itself).
use std::sync::{Arc, Mutex};
use std::time::Duration;

use altim::live::{ChanIds, FEEDS, LiveHub, LiveQuote, Market, Tick, live_consensus, us_market_open};
use altim::quotes::{QuoteMap, QuoteSource, SourceQuote};
use altim::types::Kind;
use indexmap::IndexMap;
use serde_json::{Value, json};

fn samples() -> Value {
    serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/test/live-samples.json")).unwrap()).unwrap()
}
fn feed(name: &str) -> &'static altim::live::FeedSpec {
    FEEDS.iter().find(|f| f.name == name).unwrap()
}

// Messages captured on the real streams (BTC), see web/test/live-samples.json.
#[test]
fn chaque_flux_decode() {
    let names: Vec<&str> = FEEDS.iter().map(|f| f.name).collect();
    assert_eq!(names, ["OKX", "Coinbase", "Kraken", "Bitfinex", "Bitget", "Gate.io", "Crypto.com"]);
    let expected = [
        ("OKX", 84595.2, (84595.2 / 84060.1 - 1.0) * 100.0),
        ("Coinbase", 84581.99, (84581.99 / 84146.86 - 1.0) * 100.0),
        ("Kraken", 84576.0, 0.63),
        ("Bitfinex", 84529.0, 0.418166),
        ("Bitget", 84608.29, (84608.29 / 84161.83 - 1.0) * 100.0),
        ("Gate.io", 84596.4, 0.6384),
        ("Crypto.com", 84600.03, 0.64),
    ];
    let s = samples();
    for (name, price, change) in expected {
        let mut ids = ChanIds::default();
        let parsed: Vec<_> = s[name].as_array().unwrap().iter().flat_map(|m| (feed(name).parse)(m, &mut ids)).collect();
        let p = &parsed[0];
        assert_eq!(p.base, "BTC", "{name}");
        assert_eq!(p.price, price, "{name}");
        assert!((p.change.unwrap() - change).abs() < 5e-7, "{name}");
    }
}

#[test]
fn flux_messages_sans_prix_abonnements() {
    for f in FEEDS.iter() {
        assert!((f.parse)(&json!({ "event": "subscribe" }), &mut ChanIds::default()).is_empty());
        assert!((f.parse)(&json!([1, "hb"]), &mut ChanIds::default()).is_empty());
        assert!(!(f.subscribe)(&["BTC".into(), "PEPE".into()]).is_empty());
    }
    // Bitfinex: 4+ letter symbols use the "t{BASE}:USD" form, and the channel id is remembered.
    assert_eq!((feed("Bitfinex").subscribe)(&["DOGE".into()]), vec![json!({ "event": "subscribe", "channel": "ticker", "symbol": "tDOGE:USD" })]);
    let mut ids = ChanIds::default();
    (feed("Bitfinex").parse)(&json!({ "event": "subscribed", "channel": "ticker", "chanId": 7, "symbol": "tDOGE:USD" }), &mut ids);
    assert_eq!(ids.get(&json!(7)).map(String::as_str), Some("DOGE"));
    // Crypto.com heartbeats must be answered, or the exchange closes the connection.
    let reply = feed("Crypto.com").reply.unwrap();
    assert_eq!(reply(&json!({ "id": 42, "method": "public/heartbeat" })), Some(json!({ "id": 42, "method": "public/respond-heartbeat" })));
    assert_eq!(reply(&json!({ "method": "subscribe" })), None);
}

#[test]
fn consensus_temps_reel() {
    let now = 1_000_000;
    let q: IndexMap<String, LiveQuote> = [
        ("A", 100.0, Some(1.0), now - 1000),
        ("B", 100.4, Some(2.0), now - 500),
        ("C", 99.8, None, now),
        ("D", 110.0, Some(9.0), now),       // 10 % away: ignored
        ("E", 50.0, Some(0.0), now - 700_000), // stale: ignored
    ]
    .into_iter()
    .map(|(n, price, change, time)| (n.to_string(), LiveQuote { price, change, time }))
    .collect();
    let c = live_consensus(&q, now, 600_000, 1.0).unwrap();
    assert_eq!(c.price, 100.0);
    assert_eq!(c.change, Some(1.5));
    assert_eq!(c.agreeing, 3);
    assert_eq!(c.total, 4);
    let mut s = c.sources.clone();
    s.sort();
    assert_eq!(s, ["A", "B", "C"]);
    assert_eq!(c.time, now);
    assert!(live_consensus(&IndexMap::new(), now, 600_000, 1.0).is_none());
}

#[test]
fn seance_americaine() {
    let t = |s: &str| altim::js::parse_date(s).unwrap();
    assert!(us_market_open(t("2026-09-28T14:00:00Z"))); // Monday 10:00 EDT
    assert!(!us_market_open(t("2026-09-28T13:29:00Z"))); // 9:29 EDT
    assert!(us_market_open(t("2026-09-28T19:59:00Z"))); // 15:59 EDT
    assert!(!us_market_open(t("2026-09-28T20:00:00Z"))); // 16:00 EDT
    assert!(!us_market_open(t("2026-09-27T15:00:00Z"))); // Sunday
    assert!(us_market_open(t("2026-12-01T14:31:00Z"))); // Tuesday 9:31 EST
    assert!(!us_market_open(t("2026-12-01T14:29:00Z")));
}

/// REST source whose prices the test sets; records the symbols asked.
fn fake_source(name: &str, kind: Kind, prices: &[(&str, f64)]) -> (QuoteSource, Arc<Mutex<Vec<Vec<String>>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let prices: Vec<(String, f64)> = prices.iter().map(|(s, p)| (s.to_string(), *p)).collect();
    let c = calls.clone();
    let source = QuoteSource::new(name, kind, move |assets| {
        c.lock().unwrap().push(assets.iter().map(|a| a.symbol.clone()).collect());
        let out: QuoteMap = assets
            .iter()
            .filter_map(|a| prices.iter().find(|(s, _)| *s == a.symbol).map(|(s, p)| (s.clone(), SourceQuote { price: *p, change: Some(1.0) })))
            .collect();
        async move { Ok(out) }
    });
    (source, calls)
}

async fn wait(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

fn drain(sub: &mut altim::live::Subscription, into: &mut Vec<Tick>) {
    use futures::FutureExt;
    while let Some(Some(t)) = sub.recv().now_or_never() {
        into.push(t);
    }
}

#[tokio::test]
async fn hub_abonnement_debit_limite() {
    let hub = LiveHub::with(vec![], vec![], Duration::from_millis(60_000));
    let key = vec!["crypto:BTC".to_string()];
    let mut sub = hub.subscribe(&key);
    let mut ticks = Vec::new();
    for i in 0..20 {
        hub.on_quote("crypto:BTC", "OKX", 100.0 + i as f64, Some(1.0));
    }
    wait(50).await;
    drain(&mut sub, &mut ticks);
    assert_eq!(ticks.len(), 1);
    assert_eq!(ticks[0].price, 119.0); // latest state, not the first one
    hub.on_quote("crypto:BTC", "OKX", 119.0, Some(1.0));
    wait(300).await;
    drain(&mut sub, &mut ticks);
    assert_eq!(ticks.len(), 1); // same price: nothing sent
    hub.on_quote("crypto:BTC", "OKX", 120.0, Some(1.0));
    wait(300).await;
    drain(&mut sub, &mut ticks);
    assert_eq!(ticks.len(), 2);
    let snap: Vec<f64> = hub.snapshot(&["crypto:BTC".into(), "crypto:ETH".into()]).iter().map(|t| t.price).collect();
    assert_eq!(snap, [120.0]);
    assert_eq!(hub.watched(), 1);
    drop(sub);
    assert_eq!(hub.watched(), 0);
    hub.on_quote("crypto:BTC", "OKX", 121.0, None);
    wait(300).await;
    assert_eq!(ticks.len(), 2);
    // JSON of a tick: same fields as the TypeScript (market absent for a crypto).
    let v = altim::js::to_value(&ticks[1]);
    assert_eq!(v.as_object().unwrap().keys().collect::<Vec<_>>(), ["symbol", "kind", "price", "change", "agreeing", "total", "sources", "time"]);
    hub.close();
}

#[tokio::test]
async fn hub_actions_interrogees_crypto_sans_flux() {
    let (stock, stock_calls) = fake_source("Robinhood", Kind::Stock, &[("AAPL", 200.0)]);
    let (crypto, crypto_calls) = fake_source("Binance", Kind::Crypto, &[("OBSCURE", 0.5), ("BTC", 100.0)]);
    let hub = LiveHub::with(vec![], vec![stock, crypto], Duration::from_millis(60_000));
    let mut sub = hub.subscribe(&["stock:AAPL".into(), "crypto:OBSCURE".into()]);
    wait(30).await;
    let mut ticks = Vec::new();
    drain(&mut sub, &mut ticks);
    let aapl = ticks.iter().find(|t| t.symbol == "AAPL").unwrap();
    assert_eq!((aapl.kind, aapl.price, aapl.sources.clone()), (Kind::Stock, 200.0, vec!["Robinhood".to_string()]));
    assert!(matches!(aapl.market, Some(Market::Open | Market::Closed)));
    let obscure = ticks.iter().find(|t| t.symbol == "OBSCURE").unwrap();
    assert_eq!((obscure.kind, obscure.price), (Kind::Crypto, 0.5));
    assert_eq!(*stock_calls.lock().unwrap(), [["AAPL"]]);
    assert_eq!(*crypto_calls.lock().unwrap(), [["OBSCURE"]]);
    // A crypto with a live WebSocket quote is not polled.
    let _btc = hub.subscribe(&["crypto:BTC".into()]);
    wait(30).await;
    hub.on_quote("crypto:BTC", "OKX", 100.0, None);
    hub.poll().await;
    assert_eq!(crypto_calls.lock().unwrap().last().unwrap(), &["OBSCURE"]);
    drop(sub);
    hub.close();
}

#[tokio::test]
async fn hub_stream_et_desabonnement() {
    use futures::StreamExt;
    let hub = LiveHub::with(vec![], vec![], Duration::from_millis(60_000));
    let _mine = hub.subscribe(&["crypto:ETH".into()]);
    hub.on_quote("crypto:ETH", "OKX", 3000.0, Some(2.0));
    wait(20).await;
    let keys = vec!["crypto:ETH".to_string(), "crypto:SOL".to_string()];
    let sub = hub.subscribe(&keys);
    let first = hub.snapshot(&keys);
    assert_eq!(first.iter().map(|t| t.price).collect::<Vec<_>>(), [3000.0]);
    let mut stream = futures::stream::iter(first).chain(sub);
    assert_eq!(stream.next().await.unwrap().price, 3000.0);
    hub.on_quote("crypto:ETH", "OKX", 3010.0, Some(2.3));
    let t = tokio::time::timeout(Duration::from_secs(2), stream.next()).await.unwrap().unwrap();
    assert_eq!((t.symbol.as_str(), t.kind, t.price, t.agreeing, t.total), ("ETH", Kind::Crypto, 3010.0, 1, 1));
    assert_eq!(hub.watched(), 2);
    drop(stream);
    // The client left: SOL is no longer watched (ETH keeps the test's own subscription).
    assert_eq!(hub.watched(), 1);
    hub.close();
}

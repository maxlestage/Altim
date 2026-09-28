//! Port of web/test/quotes.test.ts and web/test/stocks-extra.test.ts.
use altim::market::{STOCK_SOURCES, ny_open};
use altim::quotes::{ASSETS, QUOTE_SOURCES, QuoteMap, QuoteResult, SourceQuote, combine, parse};
use altim::stocks_extra::{EXTRA_CANDLE_SOURCES, EXTRA_QUOTE_SOURCES, parse as extra};
use altim::types::{Candle, Kind};
use serde_json::{Value, json};

fn close_to(a: f64, b: f64, digits: i32) -> bool {
    (a - b).abs() < 10f64.powi(-digits) / 2.0
}

// Excerpts of real responses (27/09/2026).
#[test]
fn parseurs_de_cours() {
    let b = parse::binance(&json!([{ "symbol": "BTCUSDT", "lastPrice": "84700.10", "priceChangePercent": "0.81" }])).unwrap();
    assert_eq!(b["BTC"], SourceQuote { price: 84700.1, change: Some(0.81) });
    let okx = parse::okx(
        &json!({ "code": "0", "data": [{ "instId": "BTC-USDT", "last": "84699.2", "open24h": "84005.8" }, { "instId": "BTC-USDC", "last": "1", "open24h": "1" }] }),
        &["BTC".into()],
    )
    .unwrap();
    assert_eq!(okx["BTC"].price, 84699.2);
    assert!(close_to(okx["BTC"].change.unwrap(), 0.8254, 3));
    let bases: Vec<String> = ["BTC", "SOL", "BNB"].map(String::from).to_vec();
    let kraken = parse::kraken(
        &json!({ "error": [], "result": { "XXBTZUSD": { "c": ["84687.40000", "0.1"] }, "SOLUSD": { "c": ["123.90000", "1"] } } }),
        &bases,
    )
    .unwrap();
    assert_eq!(kraken["BTC"].price, 84687.4);
    assert_eq!(kraken["SOL"].price, 123.9);
    assert!(!kraken.contains_key("BNB"));
    let bfx = parse::bitfinex(&json!([["tBTCUSD", 84623, 2.05, 84638, 1.41, 639, 0.00760741, 84636, 305.5, 84846, 83781]])).unwrap();
    assert_eq!(bfx["BTC"], SourceQuote { price: 84636.0, change: Some(0.760741) });
    assert_eq!(
        parse::nasdaq(&json!({ "data": { "primaryData": { "lastSalePrice": "$341.07", "percentageChange": "+1.53%" } } })).unwrap(),
        SourceQuote { price: 341.07, change: Some(1.53) }
    );
    assert_eq!(
        parse::cboe(&json!({ "success": true, "details": { "current_price": 224.9975, "price_change_percent": 0.2177 } })).unwrap(),
        SourceQuote { price: 224.9975, change: Some(0.2177) }
    );
    let y = parse::yahoo(&json!({ "chart": { "result": [{ "meta": { "regularMarketPrice": 341.07 }, "indicators": { "quote": [{ "close": [338.98, null, 335.92, 341.07] }] } }] } })).unwrap();
    assert_eq!(y.price, 341.07);
    assert!(close_to(y.change.unwrap(), 1.5331, 3)); // 341,07 / 335,92 − 1
}

fn q(price: f64, change: Option<f64>) -> Option<QuoteMap> {
    Some([("BTC".to_string(), SourceQuote { price, change })].into_iter().collect())
}
fn r(name: &str, kind: Kind, quotes: Option<QuoteMap>, error: Option<&str>) -> QuoteResult {
    QuoteResult { name: name.into(), kind, quotes, error: error.map(String::from) }
}

#[test]
fn consensus_mediane_divergente_panne() {
    let btc: Vec<_> = ASSETS.iter().filter(|a| a.symbol == "BTC").cloned().collect();
    let out = combine(
        &btc,
        &[
            r("A", Kind::Crypto, q(100.0, Some(1.0)), None),
            r("B", Kind::Crypto, q(100.1, Some(2.0)), None),
            r("C", Kind::Crypto, q(99.95, None), None),
            r("Faux", Kind::Crypto, q(110.0, Some(9.0)), None),
            r("Panne", Kind::Crypto, None, Some("HTTP 451")),
            r("Actions", Kind::Stock, q(1.0, None), None),
        ],
    );
    let r = &out[0];
    assert_eq!(r.agreeing, 3);
    assert_eq!(r.total, 5);
    assert_eq!(r.price, 100.0);
    assert_eq!(r.change, Some(1.5));
    assert!(!r.sources.iter().find(|s| s.name == "Faux").unwrap().ok);
    assert_eq!(r.sources.iter().find(|s| s.name == "Panne").unwrap().error.as_deref(), Some("HTTP 451"));
    // Same JSON as the TypeScript (field names decoded by the apps).
    let v = altim::js::to_value(r);
    assert_eq!(v["sources"][4], json!({ "name": "Panne", "ok": false, "error": "HTTP 451" }));
    assert_eq!(v["sources"][0], json!({ "name": "A", "ok": true, "price": 100 }));
}

#[test]
fn aucun_cours_actif_omis() {
    assert!(combine(&ASSETS[..1], &[r("A", Kind::Crypto, None, Some("x"))]).is_empty());
}

// ---------- stocks-extra.test.ts (AAPL, 24–25/09/2026) ----------

fn s() -> Value {
    serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/stocks-extra-samples.json")).unwrap()).unwrap()
}

#[test]
fn bougies_journalieres_nouvelles_sources() {
    let s = s();
    let (d24, d25) = (ny_open(2026, 9, 24), ny_open(2026, 9, 25));
    let all: [(&str, Vec<Candle>); 5] = [
        ("WSJ", extra::wsj(&s["wsj"]).unwrap()),
        ("AlphaQuery", extra::alphaquery(&s["alphaquery"]).unwrap()),
        ("Finviz", extra::finviz(&s["finviz"]).unwrap()),
        ("FT", extra::ft(&s["ft"]).unwrap()),
        ("eToro", extra::etoro(&s["etoro"]).unwrap()),
    ];
    for (name, candles) in &all {
        assert_eq!(candles.iter().map(|c| c.time).collect::<Vec<_>>(), [d24, d25], "{name}");
        assert!(candles[1].high >= candles[1].low, "{name}");
    }
    let w = all[0].1[1];
    assert_eq!((w.open, w.high, w.low, w.close), (336.04, 341.67, 334.53, 341.07));
    assert_eq!(all[1].1[1].close, 341.07);
    assert!(close_to(all[2].1[1].close, 341.07, 2));
    assert_eq!(all[3].1[1].close, 341.07);
    // eToro quotes its own prices: close to the market, not identical.
    assert!((all[4].1[1].close / 341.07 - 1.0).abs() < 0.005);
}

#[test]
fn identifiants_ft_etoro() {
    let s = s();
    assert_eq!(extra::ft_xid(&s["ftSearch"], "AAPL").unwrap(), Some(json!("36276")));
    assert_eq!(extra::ft_xid(&s["ftSearch"], "MSFT").unwrap(), None);
    let ids = extra::etoro_ids(&json!({ "InstrumentDisplayDatas": [
        { "InstrumentID": 1, "SymbolFull": "EURUSD", "InstrumentTypeID": 1 },
        { "InstrumentID": 1001, "SymbolFull": "AAPL", "InstrumentTypeID": 5 },
        { "InstrumentID": 3000, "SymbolFull": "SPY", "InstrumentTypeID": 6 },
        { "InstrumentID": 1118, "SymbolFull": "BRK.B", "InstrumentTypeID": 5 },
    ] }))
    .unwrap();
    assert_eq!(ids.keys().collect::<Vec<_>>(), ["AAPL", "SPY", "BRK.B"]);
}

#[test]
fn cours_fidelity_stockcharts_tipranks_public() {
    let s = s();
    let f = extra::fidelity(s["fidelity"].as_str().unwrap()).unwrap();
    assert_eq!(f["AAPL"], SourceQuote { price: 341.07, change: Some(1.53) });
    assert_eq!(f["BRK-B"].price, 505.48);
    assert!(!f.contains_key("XXXX"));
    let sc = extra::stockcharts(&s["stockcharts"]).unwrap();
    assert_eq!(sc.price, 341.07);
    assert!(close_to(sc.change.unwrap(), (341.07 / 335.92 - 1.0) * 100.0, 6));
    assert_eq!(extra::tipranks(&s["tipranks"]).unwrap(), SourceQuote { price: 341.07, change: Some((341.07 / 335.92 - 1.0) * 100.0) });
    assert_eq!(extra::public_com(s["publicCom"].as_str().unwrap(), "AAPL").unwrap().price, 341.07);
    assert!(extra::stockcharts(&json!({})).is_err());
    assert!(extra::tipranks(&json!({ "prices": [] })).is_err());
    assert!(extra::public_com("<html></html>", "AAPL").is_err());
}

#[test]
fn dix_sept_fournisseurs_actions() {
    assert!(EXTRA_CANDLE_SOURCES.iter().all(|x| STOCK_SOURCES.iter().any(|y| y.name == x.name)));
    assert!(EXTRA_QUOTE_SOURCES.iter().all(|x| QUOTE_SOURCES.iter().any(|y| y.name == x.name && y.kind == Kind::Stock)));
    let mut providers: Vec<&str> = STOCK_SOURCES
        .iter()
        .map(|x| x.name.as_str())
        .chain(QUOTE_SOURCES.iter().filter(|q| q.kind == Kind::Stock).map(|q| q.name.as_str()))
        .collect();
    providers.sort();
    providers.dedup();
    assert_eq!(providers.len(), 17);
}

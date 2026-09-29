//! EUR/USD rate: parsers on saved real responses (Yahoo, ECB, Frankfurter, 29/09/2026), the euro texts of the server
//! (rate forced per test thread, never the shared cache), live sources (ignored: network).
use altim::fx::{self, money, parse};
use serde_json::Value;

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/samples/fx/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn json(name: &str) -> Value {
    serde_json::from_str(&sample(name)).unwrap()
}

#[test]
fn yahoo_eurusd() {
    let r = parse::yahoo(&json("yahoo-eurusd.json")).unwrap();
    assert_eq!(r.usd_per_eur, 1.1343);
    assert!((r.rate - 1.0 / 1.1343).abs() < 1e-12);
    assert_eq!(r.time, 1_790_693_520_000);
    assert_eq!(r.source, "Yahoo Finance");
    // Another symbol, no price or an absurd one: refused, never a guess.
    let mut other = json("yahoo-eurusd.json");
    other["chart"]["result"][0]["meta"]["symbol"] = "GBPUSD=X".into();
    assert!(parse::yahoo(&other).is_err());
    let mut none = json("yahoo-eurusd.json");
    none["chart"]["result"][0]["meta"]["regularMarketPrice"] = Value::Null;
    assert!(parse::yahoo(&none).is_err());
    let mut absurd = json("yahoo-eurusd.json");
    absurd["chart"]["result"][0]["meta"]["regularMarketPrice"] = 113.43.into();
    assert!(parse::yahoo(&absurd).is_err());
}

#[test]
fn ecb_daily_reference_rate() {
    let r = parse::ecb(&sample("ecb-daily.xml")).unwrap();
    assert_eq!(r.usd_per_eur, 1.1355);
    assert_eq!(r.source, "BCE");
    // 2026-09-29 14:10 in Frankfurt (CEST, UTC+2) = 12:10 UTC.
    assert_eq!(altim::js::iso(r.time), "2026-09-29T12:10:00.000Z");
    assert!(parse::ecb("<Cube><Cube time='2026-09-29'><Cube currency='JPY' rate='178.41'/></Cube></Cube>").is_err());
}

#[test]
fn frankfurter_ecb_based() {
    let r = parse::frankfurter(&json("frankfurter-usd-eur.json")).unwrap();
    assert_eq!(r.rate, 0.88067);
    assert_eq!(r.source, "Frankfurter (BCE)");
    assert_eq!(altim::js::iso(r.time), "2026-09-29T12:10:00.000Z");
    assert!(parse::frankfurter(&serde_json::json!({ "base": "EUR", "date": "2026-09-29", "rates": { "USD": 1.13 } })).is_err());
}

#[test]
fn server_texts_in_euros_with_a_rate_in_dollars_without() {
    fx::set_test_rate(Some(0.88));
    assert_eq!(money(241.3636), "212,40 €");
    assert_eq!(money(0.5), "0,4400 €");
    assert_eq!(fx::unit("M"), "M€");
    assert_eq!(fx::info()["currency"], "EUR");
    assert_eq!(fx::info()["rate"], 0.88);
    // Calendar amounts (dividend per share, EPS consensus, IPO range) go through the same helper.
    let cal = |name: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(format!("{}/tests/samples/calendar/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
    };
    let d = altim::calendar::parse::dividends(&cal("nasdaq-dividends-2026-09-30.json")).unwrap();
    let agnc = d.iter().find(|e| e.symbol.as_deref() == Some("AGNC")).unwrap();
    assert_eq!(agnc.detail.as_deref(), Some("0,1056 € par action, versé le 09/10/2026"));
    let i = altim::calendar::parse::ipos(&cal("nasdaq-ipo-2026-09.json")).unwrap();
    let oura = i.iter().find(|e| e.symbol.as_deref() == Some("OURA")).unwrap();
    assert!(oura.detail.as_deref().unwrap().starts_with("Fourchette 35,20 – 38,72 €"), "{:?}", oura.detail);

    fx::set_test_rate(None);
    assert_eq!(money(212.4), "212,40 $");
    assert_eq!(fx::unit("Md"), "Md$");
    assert_eq!(fx::info()["currency"], "USD");
    assert!(fx::info()["rate"].is_null());
    fx::clear_test_rate();
}

/// A client showing dollars (`cur=USD`) gets texts in dollars even with a cached rate; the others get euros.
#[tokio::test]
async fn a_request_in_dollars_ignores_the_rate() {
    fx::store(fx::FxRate { rate: 0.88, usd_per_eur: 1.0 / 0.88, time: 0, source: "BCE".into(), fetched: altim::js::now_ms() });
    assert_eq!(money(100.0), "88,00 €");
    assert_eq!(fx::scope(true, async { money(100.0) }).await, "100,00 $");
    assert_eq!(fx::scope(true, async { fx::info()["currency"].clone() }).await, "USD");
    assert_eq!(fx::scope(false, async { money(100.0) }).await, "88,00 €");
    assert_eq!(fx::report().await["rate"], 0.88);
    assert_eq!(fx::report().await["source"], "BCE");
}

#[tokio::test]
#[ignore]
async fn live_rate() {
    for (name, r) in [
        ("Yahoo", altim::http::get_json(fx::YAHOO_URL).await.and_then(|d| parse::yahoo(&d))),
        ("BCE", altim::http::get_text_with(fx::ECB_URL, &[], std::time::Duration::from_secs(8)).await.and_then(|x| parse::ecb(&x))),
        ("Frankfurter", altim::http::get_json(fx::FRANKFURTER_URL).await.and_then(|d| parse::frankfurter(&d))),
    ] {
        let r = r.unwrap_or_else(|e| panic!("{name} : {e}"));
        println!("{name} : 1 $ = {} € ({})", r.rate, altim::js::iso(r.time));
        assert!((0.6..1.3).contains(&r.rate));
    }
    // The chain as the server runs it.
    let r = fx::ensure().await.expect("aucune source de taux");
    assert!((0.6..1.3).contains(&r.rate));
}

//! Sector exposure (`/api/sectors`): parameter validation, SEC SIC mapping and Nasdaq sectors on saved responses.
use altim::fundamentals::parse as sec;
use altim::sectors::{ETF_LABEL, classify};
use serde_json::Value;

fn sample(name: &str) -> Value {
    let path = format!("{}/tests/samples/sectors/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} absent"))).unwrap()
}

#[test]
fn sec_filers_operating_or_fund() {
    let nvda = sec::filer(&sample("sec-nvda-submissions.json"));
    let s = nvda.sector.as_ref().unwrap();
    assert_eq!((s.label.as_str(), s.sic.as_str(), s.sic_description.as_str()), ("Industrie", "3674", "Semiconductors & Related Devices"));
    assert!(!nvda.fund);
    let brk = sec::filer(&sample("sec-brk-submissions.json"));
    assert_eq!(brk.sector.unwrap().label, "Finance et immobilier");
    // SPY ("other") and QQQ ("investment"): no SIC code, not operating companies.
    for f in ["sec-spy-submissions.json", "sec-qqq-submissions.json"] {
        let x = sec::filer(&sample(f));
        assert!(x.fund && x.sector.is_none(), "{f}");
    }
    // An operating company without SIC code is not called a fund.
    let odd = sec::filer(&serde_json::json!({"sic": "", "entityType": "operating", "name": "X"}));
    assert!(!odd.fund && odd.sector.is_none());
}

#[test]
fn nasdaq_sector_first_then_sec_then_reason() {
    let rows = sec::screener(&sample("nasdaq-screener-rows.json"));
    let row = |s: &str| Ok(rows.iter().find(|r| r.symbol == s).cloned());
    // Rows without an activity (BRK/B) are left out of the screener: the SEC classifies it.
    assert!(rows.iter().all(|r| r.symbol != "BRK-B"));
    let nvda_sec = Ok(Some(sec::filer(&sample("sec-nvda-submissions.json"))));

    let nvda = classify("NVDA", Some(false), &nvda_sec, &row("NVDA"));
    assert_eq!((nvda.sector.as_deref(), nvda.classification), (Some("Technologie"), Some("nasdaq")));
    assert_eq!(nvda.nasdaq.as_ref().unwrap().industry, "Semiconductors");
    assert_eq!(nvda.sec.as_ref().unwrap().sic, "3674");
    assert!(nvda.source.unwrap().contains("Semiconductors") && nvda.reason.is_none());

    let brk = classify("BRK-B", Some(false), &Ok(Some(sec::filer(&sample("sec-brk-submissions.json")))), &row("BRK-B"));
    assert_eq!((brk.sector.as_deref(), brk.classification), (Some("Finance et immobilier"), Some("sec")));
    assert!(brk.source.unwrap().contains("6331"));

    // Nasdaq down: the SEC still answers.
    let x = classify("NVDA", None, &nvda_sec, &Err("HTTP 403".into()));
    assert_eq!(x.classification, Some("sec"));

    // ETFs: flag of the directory, or a SEC fund when the directory failed. Never a sector.
    let spy = classify("SPY", Some(true), &Ok(None), &row("SPY"));
    assert_eq!((spy.sector.as_deref(), spy.classification, spy.etf), (Some(ETF_LABEL), Some("etf"), true));
    let qqq = classify("QQQ", None, &Ok(Some(sec::filer(&sample("sec-qqq-submissions.json")))), &Ok(None));
    assert_eq!(qqq.classification, Some("etf"));
    assert!(qqq.source.unwrap().contains("INVESCO QQQ TRUST"));

    // Nothing: null with the reason of each source.
    let none = classify("ABCD", Some(false), &Ok(None), &Ok(None));
    assert_eq!((none.sector, none.classification), (None, None));
    assert!(none.reason.as_deref().unwrap().contains("aucun dépôt à la SEC"));
    let down = classify("ABCD", None, &Err("HTTP 403".into()), &Err("délai dépassé".into()));
    let r = down.reason.unwrap();
    assert!(r.contains("SEC EDGAR indisponible (HTTP 403)") && r.contains("screener Nasdaq indisponible (délai dépassé)"), "{r}");
}

/// `/api/sectors`: a bad parameter is a 400 with a French message, before any upstream call.
#[tokio::test]
async fn sectors_parameter_validation() {
    use altim::app::{AppState, router};
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    let app = || {
        router(AppState::new(altim::live::LiveHub::with(vec![], vec![], std::time::Duration::from_secs(3600))), altim::auth::Auth::new(None, false))
    };
    let many = (0..51).map(|i| format!("A{i}")).collect::<Vec<_>>().join(",");
    for (path, message) in [
        ("/api/sectors".to_string(), "symbols requis"),
        ("/api/sectors?symbols=".to_string(), "symbols requis"),
        ("/api/sectors?symbols=..%2Fx".to_string(), "symbole invalide"),
        ("/api/sectors?symbols=AAPL:bond".to_string(), "kind invalide"),
        (format!("/api/sectors?symbols={many}"), "symbols invalide"),
    ] {
        let r = app().oneshot(Request::get(&path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{path}");
        let b = String::from_utf8_lossy(&to_bytes(r.into_body(), 1 << 16).await.unwrap()).to_string();
        let v: Value = serde_json::from_str(&b).unwrap();
        assert!(v["error"].as_str().is_some_and(|e| e.starts_with(message)), "{path}: {b}");
    }
    // Only cryptos: nothing to classify, no upstream call.
    let r = app().oneshot(Request::get("/api/sectors?symbols=BTC:crypto").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let v: Value = serde_json::from_slice(&to_bytes(r.into_body(), 1 << 16).await.unwrap()).unwrap();
    assert_eq!(v["items"], serde_json::json!([]));
}

#[tokio::test]
#[ignore]
async fn sectors_live() {
    let r = altim::sectors::sectors(&["AAPL".into(), "BRK-B".into(), "SPY".into(), "QQQ".into()]).await;
    for s in &r.sources {
        println!("{} ok={} {:?}", s.name, s.ok, s.error);
    }
    for i in &r.items {
        println!("{} → {:?} ({:?}) {:?} {:?}", i.symbol, i.sector, i.classification, i.source, i.reason);
    }
    assert_eq!(r.items.iter().find(|i| i.symbol == "SPY").unwrap().classification, Some("etf"));
    assert!(r.items.iter().find(|i| i.symbol == "AAPL").unwrap().sector.is_some());
}

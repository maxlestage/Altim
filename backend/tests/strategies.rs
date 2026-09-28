//! `/api/strategies`: parameters checked before any upstream call; live run on real data (ignored by default).
use serde_json::Value;

#[tokio::test]
async fn strategies_parameter_validation() {
    use altim::app::{AppState, router};
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    for (path, message) in [
        ("/api/strategies", "symbole invalide"),
        ("/api/strategies?symbol=..%2Fx", "symbole invalide"),
        ("/api/strategies?symbol=BTC&kind=forex", "kind invalide"),
        ("/api/strategies?symbol=1ABC&kind=stock", "symbole invalide"),
        ("/api/strategies?symbol=ABCDEFGHIJKLMN", "symbole invalide"),
    ] {
        let app = router(
            AppState::new(altim::live::LiveHub::with(vec![], vec![], std::time::Duration::from_secs(3600))),
            altim::auth::Auth::new(None, false),
        );
        let r = app.oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{path}");
        let b = String::from_utf8_lossy(&to_bytes(r.into_body(), 1 << 16).await.unwrap()).to_string();
        let v: Value = serde_json::from_str(&b).unwrap();
        assert!(v["error"].as_str().is_some_and(|e| e.starts_with(message)), "{path}: {b}");
    }
}

#[tokio::test]
#[ignore]
async fn strategies_live() {
    for (symbol, kind) in [("BTC", altim::types::Kind::Crypto), ("AAPL", altim::types::Kind::Stock)] {
        let r = altim::app::strategies::strategies_for(symbol, kind).await.unwrap();
        println!("{symbol} {} ({})", r.period, r.source);
        for s in &r.strategies {
            match &s.metrics {
                Some(m) => println!(
                    "  {:<28} total {:>8.2} % dd {:>7.2} % trades {:>3} expo {:>5.1} % sharpe {:?}",
                    s.name, m.total_return, m.max_drawdown, m.trades, m.exposure, m.sharpe
                ),
                None => println!("  {:<28} {:?}", s.name, s.unavailable),
            }
        }
        assert_eq!(r.strategies.len(), 8);
        assert!(r.bars >= 60);
    }
}

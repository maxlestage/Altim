//! `/api/validation`: pooling math on hand-built results, the per-asset run on real saved histories (same numbers as
//! the decision's track record, no look-ahead), the pending / ready answers of the route, and the saved sample of a
//! real run (for the web and mobile screens). The live run is ignored by default.
mod common;

use std::sync::Arc;
use std::time::Duration;

use altim::engine::backtest::{Regime, regime_at};
use altim::engine::metrics::track;
use altim::engine::signal::sanitize;
use altim::engine::validation::{
    AssetClass, AssetResult, BASKET, Failure, MIN_TRADES, RegimeDays, TradeOutcome, ValidationReport, Verdict, aggregate, group, headline, median,
    pooled, run_asset,
};
use altim::types::Kind;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

fn outcome(net_return: f64, risk: Option<f64>, regime: Regime) -> TradeOutcome {
    TradeOutcome { net_return, risk, regime }
}

fn asset(symbol: &str, class: AssetClass, total: f64, hold: f64, dd: f64, sharpe: Option<f64>, outcomes: Vec<TradeOutcome>) -> AssetResult {
    AssetResult {
        symbol: symbol.into(),
        class: Some(class),
        kind: Some(if class == AssetClass::Stock { Kind::Stock } else { Kind::Crypto }),
        trades: outcomes.len(),
        total_return: total,
        buy_and_hold: hold,
        max_drawdown: dd,
        hold_max_drawdown: dd * 2.0,
        sharpe,
        beat_hold: total > hold,
        after_tax: altim::engine::validation::after_tax(total),
        hold_after_tax: altim::engine::validation::after_tax(hold),
        outcomes,
        ..Default::default()
    }
}

#[test]
fn pooled_trade_statistics_by_hand() {
    let o = [
        outcome(10.0, Some(5.0), Regime::Bull),
        outcome(-5.0, Some(5.0), Regime::Bull),
        outcome(5.0, None, Regime::Bear),
        outcome(-2.0, Some(2.0), Regime::Range),
    ];
    let p = pooled(&o);
    assert_eq!(p.trades, 4);
    assert_eq!(p.win_rate, Some(50.0));
    // Gains 15, losses 7.
    assert_eq!(p.profit_factor, Some(2.14));
    // Mean 2 % = 12,5 × 0,5 − 3,5 × 0,5.
    assert_eq!(p.expectancy, Some(2.0));
    // R: 10/5, −5/5, −2/2 (the trade without a risk is left out) → 0.
    assert_eq!(p.avg_r, Some(0.0));
    // Deviations 8, −7, 3, −4 → variance 138/3 = 46; t = 2 / (√46 / 2).
    assert_eq!(p.t_stat, Some(((2.0 / (46f64.sqrt() / 2.0)) * 100.0).round() / 100.0));
    assert_eq!(pooled(&[]).trades, 0);
    assert_eq!(pooled(&[outcome(1.0, None, Regime::Bull)]).profit_factor, None, "aucune perte");
    assert_eq!(median(&[5.0, f64::NAN, 1.0]), Some(3.0));
}

#[test]
fn groups_regimes_and_verdicts_by_hand() {
    // 40 trades alternately +2 % and −1 % in bull markets: mean 0,5 %, t ≈ 2,1 → edge; 3 losing trades in bear.
    let bull: Vec<TradeOutcome> = (0..40).map(|i| outcome(if i % 2 == 0 { 2.0 } else { -1.0 }, Some(1.0), Regime::Bull)).collect();
    let mut a = asset("AAPL", AssetClass::Stock, 30.0, 20.0, -10.0, Some(1.0), bull);
    a.regime_days = vec![
        RegimeDays { regime: Regime::Bull, days: 300, signal: 25.0, hold: 40.0 },
        RegimeDays { regime: Regime::Bear, days: 100, signal: -2.0, hold: -15.0 },
    ];
    let mut b = asset("BTC", AssetClass::Btc, -5.0, 50.0, -30.0, None, vec![outcome(-1.0, None, Regime::Bear); 3]);
    b.regime_days = vec![
        RegimeDays { regime: Regime::Bear, days: 10, signal: 5.0, hold: -8.0 },
        RegimeDays { regime: Regime::Crisis, days: 50, signal: -3.0, hold: -20.0 },
    ];
    let c = asset("SOL", AssetClass::Altcoin, 10.0, -20.0, -20.0, Some(0.5), vec![]);
    let all = [&a, &b, &c];
    let g = group("all", "Tous", &all);
    assert_eq!((g.assets, g.pooled.trades), (3, 43));
    assert_eq!(g.median_return, Some(10.0));
    assert_eq!(g.worst_return.as_ref().map(|w| (w.symbol.as_str(), w.value)), Some(("BTC", -5.0)));
    assert_eq!(g.median_hold, Some(20.0));
    assert_eq!(g.median_drawdown, Some(-20.0));
    assert_eq!(g.worst_drawdown.as_ref().map(|w| w.symbol.as_str()), Some("BTC"));
    assert_eq!(g.median_hold_drawdown, Some(-40.0));
    // Sharpe of the two assets that have one.
    assert_eq!(g.median_sharpe, Some(0.75));
    assert_eq!((g.beat_hold, g.beat_share), (2, Some(66.7)));
    assert_eq!(g.median_after_tax, Some(7.0));
    assert_eq!(g.median_hold_after_tax, Some(14.0));
    assert!(!g.low_sample);
    let bull = g.regimes.iter().find(|r| r.regime == Regime::Bull).unwrap();
    assert_eq!(bull.pooled.trades, 40);
    assert_eq!(bull.verdict, Verdict::Edge, "{:?}", bull.pooled.t_stat);
    assert!(bull.pooled.t_stat.unwrap() >= 2.0);
    assert_eq!((bull.assets, bull.beat_hold, bull.median_signal, bull.median_hold), (1, 0, Some(25.0), Some(40.0)));
    let bear = g.regimes.iter().find(|r| r.regime == Regime::Bear).unwrap();
    assert_eq!((bear.pooled.trades, bear.verdict, bear.low_sample), (3, Verdict::Insufficient, true));
    // BTC has only 10 bear days (< 20): only AAPL is compared.
    assert_eq!((bear.days, bear.assets, bear.beat_hold, bear.beat_share), (110, 1, 1, Some(100.0)));
    assert_eq!(g.regimes.iter().map(|r| r.regime).collect::<Vec<_>>(), [Regime::Bull, Regime::Bear, Regime::Range, Regime::Crisis]);
    // The 3 bear losses bring the pooled t under 2: not demonstrated overall.
    assert_eq!(g.verdict, Verdict::Unproven, "{:?}", g.pooled.t_stat);

    // Report: basket order (never by performance), classes in their order, failures listed.
    let fail = Failure {
        symbol: "ETH".into(),
        name: "Ethereum".into(),
        kind: Kind::Crypto,
        class: AssetClass::Eth,
        error: "historique indisponible".into(),
    };
    let r = aggregate(vec![c.clone(), b.clone(), a.clone()], vec![fail], 1, "test");
    assert_eq!(r.assets.iter().map(|a| a.symbol.as_str()).collect::<Vec<_>>(), ["AAPL", "BTC", "SOL"]);
    assert_eq!(r.classes.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["stock", "btc", "altcoin"]);
    assert_eq!(r.failures.len(), 1);
    assert_eq!(r.basket.len(), BASKET.len());
    assert_eq!(r.out_of_sample.status, "notVerifiable");
    assert!(r.headline.starts_with("Sur 3 actifs"), "{}", r.headline);
    assert!(r.headline.contains("sur 2 actifs"), "{}", r.headline);
    assert!(r.headline.contains("trop peu de trades en marché baissier (3)"), "{}", r.headline);

    // Too few trades overall: says so rather than concluding.
    let small = group("all", "Tous", &[&c]);
    assert_eq!(small.verdict, Verdict::Insufficient);
    assert!(small.low_sample && small.pooled.trades < MIN_TRADES);
    assert!(headline(&small, Some((3.0, 3.0))).contains("trop peu de trades (0)"));
    assert!(headline(&group("all", "Tous", &[]), None).starts_with("Aucun actif"));
}

fn basket(symbol: &str) -> &'static altim::engine::validation::BasketAsset {
    BASKET.iter().find(|b| b.symbol == symbol).unwrap()
}

/// On the real saved histories: the same numbers as the decision's track record, trades filed by regime, days
/// compounded back to the totals.
#[test]
fn run_asset_matches_the_decision_track() {
    let mut runs = Vec::new();
    for s in ["BTC", "ETH", "SOL", "DOGE", "AAPL", "NVDA", "SPY"] {
        let input = common::find(s, "long").unwrap();
        let b = basket(s);
        let r = run_asset(b, &input.candles, "test").unwrap();
        let t = track(&input.candles, b.kind).unwrap();
        assert_eq!(
            (r.trades, r.total_return, r.buy_and_hold, r.max_drawdown, r.sharpe),
            (t.trades, t.total_return, t.buy_and_hold, t.max_drawdown, t.sharpe)
        );
        assert_eq!((r.profit_factor, r.expectancy, r.avg_r), (t.profit_factor, t.details.expectancy, t.details.avg_r));
        assert_eq!(r.bars, t.details.tested_bars);
        assert_eq!(r.outcomes.len(), r.trades);
        for g in &t.details.regimes {
            assert_eq!(r.outcomes.iter().filter(|o| o.regime == g.regime).count(), g.trades, "{s} {:?}", g.regime);
        }
        // Every tested day but the first falls in one regime; compounding them gives back the totals.
        assert_eq!(r.regime_days.iter().map(|d| d.days).sum::<usize>(), r.bars - 1);
        let hold: f64 = r.regime_days.iter().map(|d| 1.0 + d.hold / 100.0).product();
        let c = sanitize(&input.candles);
        let first = c[altim::engine::backtest::WARMUP].close;
        let expected = c.last().unwrap().close / first;
        assert!((hold / expected - 1.0).abs() < 1e-3, "{s}: {hold} vs {expected}");
        assert!(r.hold_max_drawdown <= 0.0 && r.max_drawdown <= 0.0);
        runs.push(r);
    }
    let report = aggregate(runs.clone(), vec![], 0, "test");
    assert_eq!(report.overall.pooled.trades, runs.iter().map(|r| r.trades).sum::<usize>());
    assert_eq!(report.classes.iter().map(|g| g.assets).sum::<usize>(), 7);
    assert_eq!(report.overall.beat_hold, runs.iter().filter(|r| r.total_return > r.buy_and_hold).count());
    assert_eq!(report.overall.regimes.iter().map(|g| g.pooled.trades).sum::<usize>(), report.overall.pooled.trades);
}

/// No look-ahead: cutting the history at any day changes neither the regime of the days before it nor the trades
/// already closed then (their net return and regime).
#[test]
fn no_look_ahead() {
    for s in ["BTC", "AAPL"] {
        let input = common::find(s, "long").unwrap();
        let c = sanitize(&input.candles);
        for k in (0..c.len()).step_by(37) {
            assert_eq!(regime_at(&c[..=k], k), regime_at(&c, k), "{s} jour {k}");
        }
        let full = run_asset(basket(s), &input.candles, "t").unwrap();
        let cut = &input.candles[..input.candles.len() - 150];
        let part = run_asset(basket(s), cut, "t").unwrap();
        let t = altim::engine::metrics::track_run(cut, basket(s).kind, None).unwrap().1;
        let closed = t.result.trades.iter().filter(|x| x.exit_reason != "Fin du test").count();
        assert!(closed > 3, "{s}");
        for i in 0..closed {
            assert_eq!(part.outcomes[i], full.outcomes[i], "{s} trade {i}");
        }
    }
}

#[tokio::test]
async fn ready_or_pending_answers() {
    use altim::app::validation::ready_or_pending;
    let slow = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok::<_, altim::http::Error>(Arc::new(1u8))
    };
    let r = ready_or_pending(slow, Duration::from_millis(20)).await.unwrap();
    assert_eq!(r.status(), StatusCode::ACCEPTED);
    let b: Value = serde_json::from_slice(&to_bytes(r.into_body(), 1 << 16).await.unwrap()).unwrap();
    assert_eq!(b, serde_json::json!({ "pending": true }));
    let r = ready_or_pending(async { Ok::<_, altim::http::Error>(Arc::new(vec![1, 2])) }, Duration::from_secs(1)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(to_bytes(r.into_body(), 1 << 16).await.unwrap(), "[1,2]");
    let e = ready_or_pending(async { Err::<Arc<u8>, _>(altim::http::Error("panne".into())) }, Duration::from_secs(1)).await;
    assert!(e.is_err());
}

fn sample() -> ValidationReport {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/validation.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The saved real run follows the contract, and the route serves a cached report as is (never recomputed within 12 h).
#[tokio::test]
async fn route_serves_the_cached_report() {
    let s = sample();
    assert!(s.assets.len() + s.failures.len() == BASKET.len(), "{} + {}", s.assets.len(), s.failures.len());
    assert!(s.assets.len() >= 20, "au moins 20 actifs testés dans l'échantillon");
    assert!(!s.headline.is_empty() && !s.protections.is_empty() && !s.limits.is_empty());
    assert_eq!(s.overall.pooled.trades, s.assets.iter().map(|a| a.trades).sum::<usize>());
    assert_eq!(s.overall.regimes.iter().map(|g| g.pooled.trades).sum::<usize>(), s.overall.pooled.trades);
    let expected = serde_json::to_value(&s).unwrap();
    let seeded = s.clone();
    altim::cache::cached(
        altim::app::validation::CACHE_KEY,
        altim::app::validation::FRESH_MS,
        move || async move { Ok::<_, altim::http::Error>(seeded) },
    )
    .await
    .unwrap();
    let app = altim::app::router(
        altim::app::AppState::new(altim::live::LiveHub::with(vec![], vec![], Duration::from_secs(3600))),
        altim::auth::Auth::new(None, false),
    );
    let r = app.oneshot(Request::get("/api/validation").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b: Value = serde_json::from_slice(&to_bytes(r.into_body(), 1 << 24).await.unwrap()).unwrap();
    assert_eq!(b["headline"], expected["headline"]);
    assert_eq!(b["assets"].as_array().unwrap().len(), s.assets.len());
    assert_eq!(b["overall"]["trades"], expected["overall"]["trades"]);
}

/// Real run on the whole basket; `ALTIM_SAVE_SAMPLE=1` rewrites tests/samples/validation.json with it.
#[tokio::test]
#[ignore]
async fn validation_live() {
    let r = altim::app::validation::compute().await.unwrap();
    println!("{}", r.headline);
    for f in &r.failures {
        println!("  échec {} : {}", f.symbol, f.error);
    }
    for a in &r.assets {
        println!(
            "  {:<5} {:>3} trades  signal {:>8.2} %  détention {:>8.2} %  dd {:>7.2} %  sharpe {:?}  {}",
            a.symbol, a.trades, a.total_return, a.buy_and_hold, a.max_drawdown, a.sharpe, a.source
        );
    }
    for g in r.classes.iter().chain(std::iter::once(&r.overall)) {
        println!("{} : {} trades, {} ({:?})", g.label, g.pooled.trades, g.verdict_label, g.pooled.t_stat);
        for x in &g.regimes {
            println!(
                "   {} : {} trades {} ; jours {} ; bat la détention {}/{}",
                x.label, x.pooled.trades, x.verdict_label, x.days, x.beat_hold, x.assets
            );
        }
    }
    if std::env::var("ALTIM_SAVE_SAMPLE").is_ok_and(|v| v == "1") {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/validation.json");
        std::fs::write(path, serde_json::to_string_pretty(&altim::js::to_value(&r)).unwrap() + "\n").unwrap();
    }
    assert!(r.assets.len() >= BASKET.len() / 2);
}

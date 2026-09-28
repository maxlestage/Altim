//! Unit tests of `web/test/zones.test.ts` (Fibonacci part), `web/test/alerts.test.ts` and `web/test/selection.test.ts`
//! (engine part), ported.
use altim::engine::alerts::{AlertInput, AlertMacro, AlertShock, AlertSignal, AlertTrend, AlertZone, buy_alert};
use altim::engine::fibonacci::{
    Band, Horizon as ZH, Swing, Trend, ZoneStatus, fib_zone, fib_zones, level, swing_at, weekly, zone_evidence, zone_state,
};
use altim::engine::reliability::ReliabilityLevel;
use altim::engine::screener::{
    self, CRITERIA, CandleInterval, Criterion, Edge, HORIZON_LIST, Horizon, RankRule, SECTOR_CAP, Spec, align_series, factors_at, is_pegged, pick,
    ranks, roles, score_universe, span, to_horizon, validate,
};
use altim::engine::signal::{Action, Candle};
use altim::types::Kind;

const D: i64 = 86_400_000;

fn utc(y: i32, m: u32, d: u32) -> i64 {
    chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis()
}

fn close_to(a: f64, b: f64, digits: i32) {
    assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≠ {b}");
}

/// Candles following a path of closes (small wicks).
fn path(closes: &[f64], step: i64, t0: i64) -> Vec<Candle> {
    closes
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let o = if i > 0 { closes[i - 1] } else { c };
            Candle { time: t0 + i as i64 * step, open: o, high: c.max(o) * 1.002, low: c.min(o) * 0.998, close: c, volume: 1000.0 }
        })
        .collect()
}
fn p(closes: &[f64]) -> Vec<Candle> {
    path(closes, D, utc(2024, 1, 1))
}
fn lin(from: f64, to: f64, n: usize) -> Vec<f64> {
    (0..n).map(|i| from + ((to - from) * (i + 1) as f64) / n as f64).collect()
}
fn cat(parts: &[Vec<f64>]) -> Vec<f64> {
    parts.concat()
}

// ---------- zones.test.ts ----------

#[test]
fn weeks_monday_to_sunday() {
    let monday = utc(2026, 9, 21);
    let w = weekly(&path(&lin(100.0, 113.0, 14), D, monday));
    assert_eq!(w.len(), 2);
    assert_eq!(w[0].time, monday);
    close_to(w[0].close, 100.0 + 13.0 * 7.0 / 14.0, 9);
    assert!(w[1].high > w[0].high);
    assert_eq!(w[0].volume, 7000.0);
}

#[test]
fn swing_up_down_rebound() {
    let up = p(&cat(&[lin(100.0, 100.0, 30), lin(100.0, 150.0, 40), lin(150.0, 135.0, 10)]));
    let s = swing_at(&up, up.len() - 1, 120, 4.0, None).unwrap();
    assert_eq!(s.trend, Trend::Up);
    close_to(s.low, 100.0 * 0.998, 6);
    close_to(s.high, 150.0 * 1.002, 6);
    let down = p(&cat(&[lin(150.0, 150.0, 30), lin(150.0, 100.0, 40), lin(100.0, 105.0, 10)]));
    assert_eq!(swing_at(&down, down.len() - 1, 120, 4.0, None).unwrap().trend, Trend::Down);
    // Rebound of more than 38.2 % of the fall: the rebound is the current up move.
    let rebound = p(&cat(&[lin(150.0, 150.0, 30), lin(150.0, 100.0, 40), lin(100.0, 130.0, 30)]));
    let r = swing_at(&rebound, rebound.len() - 1, 150, 4.0, None).unwrap();
    assert_eq!(r.trend, Trend::Up);
    close_to(r.low, 100.0 * 0.998, 6);
    close_to(r.high, 130.0 * 1.002, 6);
    // A flat market: no move to trace levels on.
    assert!(swing_at(&p(&lin(100.0, 100.5, 80)), 79, 120, 4.0, None).is_none());
}

#[test]
fn levels_and_price_position() {
    let s = Swing { trend: Trend::Up, low: 100.0, high: 200.0, low_index: 0, high_index: 10, low_time: 0, high_time: 1 };
    close_to(level(&s, 0.382), 161.8, 9);
    close_to(level(&s, 0.618), 138.2, 9);
    assert_eq!(zone_state(&s, 170.0).status, ZoneStatus::Above);
    close_to(zone_state(&s, 170.0).distance.unwrap(), (1.0 - 161.8 / 170.0) * 100.0, 9);
    assert_eq!(zone_state(&s, 150.0).status, ZoneStatus::InZone);
    assert_eq!(zone_state(&s, 136.0).status, ZoneStatus::Golden);
    assert_eq!(zone_state(&s, 120.0).status, ZoneStatus::Deep);
    assert_eq!(zone_state(&s, 99.0).status, ZoneStatus::Broken);
    assert!(zone_state(&s, 150.0).text.contains("zone d'achat"));
}

#[test]
fn zone_of_a_horizon() {
    let c = p(&cat(&[lin(100.0, 100.0, 30), lin(100.0, 200.0, 50), lin(200.0, 150.0, 15)]));
    let z = fib_zone(&c, ZH::Medium, None, true);
    assert_eq!(z.status, ZoneStatus::InZone);
    let sw = z.swing.unwrap();
    close_to(z.zone.unwrap().to, level(&sw, 0.382), 9);
    close_to(z.golden.unwrap().from, level(&sw, 0.65), 9);
    assert_eq!(z.invalidation, Some(sw.low));
    assert_eq!(z.targets[0], sw.high);
    close_to(z.targets[2], sw.low + 1.618 * (sw.high - sw.low), 9);
    let down = fib_zone(&p(&cat(&[lin(200.0, 200.0, 30), lin(200.0, 100.0, 50), lin(100.0, 104.0, 10)])), ZH::Medium, None, true);
    assert_eq!(down.status, ZoneStatus::Downtrend);
    assert!(down.zone.is_none());
    assert_eq!(fib_zone(&p(&lin(100.0, 110.0, 10)), ZH::Short, None, true).status, ZoneStatus::None);
    let flat = fib_zone(&p(&lin(100.0, 100.5, 80)), ZH::Medium, None, true);
    assert_eq!(flat.text, "Pas de mouvement assez net pour tracer des niveaux.");
    // Three horizons; the long one uses the weekly candles of the long history.
    let zs = fib_zones(&c, &c, &c, None, true);
    assert_eq!(zs.iter().map(|x| x.horizon).collect::<Vec<_>>(), vec![ZH::Short, ZH::Medium, ZH::Long]);
    assert_eq!(zs[2].unit, "1 sem.");
}

#[test]
fn zone_history_without_look_ahead() {
    let mut closes = Vec::new();
    let mut px = 100.0;
    for _ in 0..8 {
        closes.extend(lin(px, px * 1.3, 25));
        px *= 1.3;
        closes.extend(lin(px, px * 0.86, 12));
        px *= 0.86;
    }
    let c = p(&closes);
    let e = zone_evidence(&c, ZH::Medium).unwrap();
    assert!(e.samples > 0.0);
    assert!(e.rate >= 0.0 && e.rate <= 100.0);
    close_to(e.lift, if e.base > 0.0 { e.rate / e.base } else { e.lift }, 9);
    let i = 150;
    let altered: Vec<Candle> = c
        .iter()
        .enumerate()
        .map(|(j, x)| if j > i { Candle { high: x.high * 3.0, low: x.low * 3.0, close: x.close * 3.0, open: x.open * 3.0, ..*x } } else { *x })
        .collect();
    assert_eq!(swing_at(&altered, i, 120, 4.0, None), swing_at(&c, i, 120, 4.0, None));
}

// ---------- alerts.test.ts ----------

fn zone(horizon: ZH, status: ZoneStatus, label: &str) -> AlertZone {
    AlertZone {
        horizon,
        label: label.into(),
        status,
        zone: Some(Band { from: 68_000.0, to: 76_000.0 }),
        golden: Some(Band { from: 68_000.0, to: 69_000.0 }),
        invalidation: Some(57_800.0),
    }
}

fn base() -> AlertInput {
    AlertInput {
        symbol: "BTC".into(),
        name: "Bitcoin".into(),
        price: Some(72_000.0),
        signal: Some(AlertSignal { action: Action::Hold, confidence: 40.0 }),
        reliability: Some(ReliabilityLevel::High),
        zones: vec![zone(ZH::Short, ZoneStatus::Above, "Court terme"), zone(ZH::Medium, ZoneStatus::Above, "Moyen terme")],
        shock: Some(AlertShock::Calm),
        trend: Some(AlertTrend::Up),
        macro_level: Some(AlertMacro::Calm),
    }
}

#[test]
fn alert_nothing_to_report() {
    let a = buy_alert(&base());
    assert!(!a.buy);
    assert_eq!(a.key, "");
    assert_eq!(a.title, "BTC : pas d'achat pour l'instant");
}

#[test]
fn alert_buy_signal_alone() {
    let a = buy_alert(&AlertInput { signal: Some(AlertSignal { action: Action::Buy, confidence: 55.0 }), ..base() });
    assert!(a.buy);
    assert!(!a.strong);
    assert_eq!(a.key, "signal");
    assert!(a.title.contains("achat possible"));
    assert!(a.body.contains("Signal ACHAT en 4 h (confiance 55 %)"));
}

#[test]
fn alert_zone_and_signal() {
    let a = buy_alert(&AlertInput {
        signal: Some(AlertSignal { action: Action::StrongBuy, confidence: 70.0 }),
        zones: vec![zone(ZH::Short, ZoneStatus::InZone, "Court terme"), zone(ZH::Medium, ZoneStatus::Golden, "Moyen terme")],
        ..base()
    });
    assert!(a.strong);
    assert_eq!(a.key, "signal+zone:medium");
    assert!(a.body.contains("zone d'or moyen terme"));
    assert!(a.body.contains("Invalidé sous 57"));
    assert!(a.title.contains("achat conseillé à 72"));
}

#[test]
fn alert_blockers() {
    let in_zone = AlertInput { zones: vec![zone(ZH::Medium, ZoneStatus::InZone, "Moyen terme")], ..base() };
    assert!(buy_alert(&in_zone).buy);
    assert!(!buy_alert(&AlertInput { reliability: Some(ReliabilityLevel::Low), ..in_zone.clone() }).buy);
    let shock = buy_alert(&AlertInput { shock: Some(AlertShock::Shock), ..in_zone.clone() });
    assert!(!shock.buy);
    assert!(shock.body.contains("risque de choc"));
    assert!(!buy_alert(&AlertInput { price: Some(50_000.0), ..in_zone }).buy);
}

#[test]
fn alert_cautions_do_not_block() {
    let a = buy_alert(&AlertInput {
        zones: vec![zone(ZH::Medium, ZoneStatus::InZone, "Moyen terme")],
        trend: Some(AlertTrend::Down),
        macro_level: Some(AlertMacro::High),
        ..base()
    });
    assert!(a.buy);
    assert!(a.body.contains("Tendance de fond baissière"));
    assert!(a.body.contains("Contexte macro très tendu"));
}

#[test]
fn alert_broken_or_downtrend_zone_is_no_reason() {
    let a = buy_alert(&AlertInput {
        zones: vec![zone(ZH::Medium, ZoneStatus::Broken, "Moyen terme"), zone(ZH::Long, ZoneStatus::Downtrend, "Long terme")],
        ..base()
    });
    assert!(!a.buy);
}

// ---------- selection.test.ts ----------

/// Daily candles growing at a constant daily rate (small wicks), same generator as the TypeScript test.
fn trend_series(n: usize, daily: f64, start: f64, seed: u32) -> Vec<Candle> {
    let mut s = seed;
    let mut px = start;
    let mut r = || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        s as f64 / 2f64.powi(32)
    };
    (0..n)
        .map(|i| {
            let o = px;
            px *= 1.0 + daily + (r() - 0.5) * 0.01;
            Candle { time: utc(2020, 1, 1) + i as i64 * D, open: o, high: o.max(px) * 1.003, low: o.min(px) * 0.997, close: px, volume: 1000.0 }
        })
        .collect()
}

#[test]
fn percentile_ranks() {
    assert_eq!(ranks(&[Some(10.0), Some(30.0), Some(20.0), None], true), vec![Some(0.0), Some(100.0), Some(50.0), None]);
    assert_eq!(ranks(&[Some(10.0), Some(30.0), Some(20.0)], false), vec![Some(100.0), Some(0.0), Some(50.0)]);
    assert_eq!(ranks(&[Some(5.0), Some(5.0), Some(5.0)], true), vec![Some(50.0); 3]);
}

#[test]
fn pick_best_three_per_sector() {
    let items: Vec<(&str, Option<f64>, &str)> = vec![
        ("A", Some(99.0), "Tech"),
        ("B", Some(98.0), "Tech"),
        ("C", Some(97.0), "Tech"),
        ("D", Some(96.0), "Tech"),
        ("E", Some(95.0), "Santé"),
        ("F", None, "Santé"),
        ("G", Some(10.0), "Énergie"),
    ];
    assert_eq!(SECTOR_CAP, 3);
    fn names<'a>(v: Vec<&(&'a str, Option<f64>, &str)>) -> Vec<&'a str> {
        v.into_iter().map(|x| x.0).collect()
    }
    assert_eq!(names(pick(&items, |x| x.1, |x| Some(x.2.to_string()), 5)), vec!["A", "B", "C", "E", "G"]);
    assert_eq!(names(pick(&items, |x| x.1, |_| None, 2)), vec!["A", "B"]);
}

fn m3() -> Spec {
    *screener::spec(Kind::Stock, Horizon::Mo3)
}
fn m6() -> Spec {
    *screener::spec(Kind::Stock, Horizon::Mo6)
}

#[test]
fn stock_factors_without_look_ahead() {
    let up = trend_series(600, 0.003, 100.0, 1);
    let f = factors_at(&up, 599, &m3()).unwrap();
    assert!(f.momentum.unwrap() > 30.0);
    assert_eq!(f.trend, 100.0);
    assert!(f.atr_pct.unwrap() > 0.0);
    let altered: Vec<Candle> = up
        .iter()
        .enumerate()
        .map(|(i, c)| if i > 450 { Candle { close: c.close * 0.5, open: c.open * 0.5, high: c.high * 0.5, low: c.low * 0.5, ..*c } } else { *c })
        .collect();
    assert_eq!(factors_at(&altered, 450, &m3()), factors_at(&up, 450, &m3()));
    assert!(factors_at(&up, 100, &m3()).is_none());
}

#[test]
fn ranking_rules() {
    let l = m6();
    let list: Vec<_> = [trend_series(600, 0.004, 100.0, 1), trend_series(600, 0.001, 100.0, 2), trend_series(600, -0.001, 100.0, 3)]
        .iter()
        .map(|c| factors_at(c, 599, &l))
        .collect();
    let totals = |spec: &Spec| score_universe(&list, spec).iter().map(|x| x.unwrap().total).collect::<Vec<_>>();
    assert_eq!(totals(&l), vec![100.0, 50.0, 0.0]);
    assert_eq!(totals(&Spec { rank: RankRule::Reversal, ..l }), vec![0.0, 50.0, 100.0]);
    assert_eq!(totals(&Spec { rank: RankRule::Signal, ..l }), list.iter().map(|f| f.as_ref().unwrap().signal).collect::<Vec<_>>());
    for x in score_universe(&list, &l) {
        for k in [Criterion::Signal, Criterion::Trend, Criterion::Momentum, Criterion::Zone, Criterion::Risk] {
            assert!(*x.unwrap().scores.get(k) >= 0.0);
            assert!(!CRITERIA.get(k).is_empty());
        }
    }
    assert_eq!(roles(&l, Kind::Stock).momentum, "classe les actions");
    assert!(roles(screener::spec(Kind::Crypto, Horizon::Mo6), Kind::Crypto).risk.contains("les plus calmes"));
    assert!(roles(screener::spec(Kind::Stock, Horizon::H1), Kind::Stock).momentum.contains("en baisse"));
    assert!(screener::spec(Kind::Stock, Horizon::Mo6).stop_atr > screener::spec(Kind::Stock, Horizon::D7).stop_atr);
}

#[test]
fn eight_durations_and_former_names() {
    assert_eq!(HORIZON_LIST.map(|h| h.as_str()), ["30m", "1h", "5h", "7d", "14d", "1m", "3m", "6m"]);
    for m in [Kind::Stock, Kind::Crypto] {
        for h in HORIZON_LIST {
            let sp = screener::spec(m, h);
            assert!(sp.hold > 0);
            assert!(sp.evidence.chars().count() > 20);
        }
    }
    assert_eq!(screener::spec(Kind::Stock, Horizon::M30).interval, CandleInterval::M5);
    assert_eq!(screener::spec(Kind::Crypto, Horizon::H5).interval, CandleInterval::M30);
    assert_eq!(span(6, CandleInterval::M5, Kind::Stock), "30 min");
    assert_eq!(span(10, CandleInterval::M30, Kind::Crypto), "5 h");
    assert_eq!(span(126, CandleInterval::D1, Kind::Stock), "6 mois");
    assert_eq!(span(7, CandleInterval::D1, Kind::Crypto), "7 jours");
    assert_eq!(span(3, CandleInterval::M30, Kind::Crypto), "1,5 h");
    assert_eq!(to_horizon("medium", Kind::Stock), Some(Horizon::Mo3));
    assert_eq!(to_horizon("medium", Kind::Crypto), Some(Horizon::Mo1));
    assert_eq!(to_horizon("30m", Kind::Crypto), Some(Horizon::M30));
    assert_eq!(to_horizon("2y", Kind::Stock), None);
}

#[test]
fn pegged_tokens() {
    assert!(is_pegged(Some(0.05), CandleInterval::D1));
    assert!(!is_pegged(Some(2.5), CandleInterval::D1));
    assert!(is_pegged(Some(0.01), CandleInterval::M5));
    assert!(!is_pegged(Some(0.3), CandleInterval::M5));
    assert!(!is_pegged(None, CandleInterval::D1));
}

#[test]
fn replay_edge() {
    let series = align_series(&(0..12).map(|k| trend_series(700, -0.002 + k as f64 * 0.0006, 100.0, k + 1)).collect::<Vec<_>>(), false);
    let m = Spec { hold: 21, step: 21, ..m3() };
    let v = validate(&series, &m, Horizon::Mo1, 3, None, None).unwrap();
    assert!(v.periods > 5);
    assert!(v.top > v.universe);
    assert_eq!(v.beat_rate, 100.0);
    assert_eq!(v.edge, Edge::Clear);
    assert_eq!(validate(&series, &Spec { cost: 0.5, ..m }, Horizon::Mo1, 3, None, None).unwrap().edge, Edge::None);
    assert_eq!(validate(&series, &Spec { rank: RankRule::Reversal, ..m }, Horizon::Mo1, 3, None, None).unwrap().edge, Edge::None);
    let with_btc = validate(&series, screener::spec(Kind::Crypto, Horizon::Mo1), Horizon::Mo1, 3, None, Some(0)).unwrap();
    assert_eq!(with_btc.hold, 30);
    assert!(with_btc.benchmark.is_some());
}

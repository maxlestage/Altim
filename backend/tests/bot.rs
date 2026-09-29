//! « Bot Altim » (`engine::bot`, `/api/bot`): features without look-ahead on real saved histories, labels at the
//! next open, the logistic regression (deterministic, training statistics only), the walk-forward's purge, the
//! out-of-sample evaluation by hand, today's view from the saved real report, and the route. The live run is
//! ignored by default (`ALTIM_SAVE_SAMPLE=1` rewrites tests/samples/bot.json).
mod common;

use std::time::Duration;

use altim::engine::bot::*;
use altim::engine::signal::sanitize;
use altim::engine::validation::{BASKET, Verdict};
use altim::types::{Candle, DAY_MS, Kind};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

fn long(symbol: &str) -> Vec<Candle> {
    sanitize(&common::find(symbol, "long").unwrap().candles)
}

/// Deterministic pseudo-random numbers in [0, 1) (no dependency, same sequence every run).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The features at the close of day t are the same whether the later candles exist, are cut or are rewritten.
#[test]
fn features_have_no_look_ahead() {
    for s in ["BTC", "AAPL", "SOL"] {
        let c = long(s);
        assert!(c.len() > WARMUP + 100, "{s}: {}", c.len());
        let full = Indicators::new(&c);
        for cut in [WARMUP, WARMUP + 57, c.len() / 2, c.len() - 25] {
            let part = &c[..=cut];
            let mut changed = c.clone();
            let mut rng = Lcg(cut as u64);
            for k in changed.iter_mut().skip(cut + 1) {
                let m = 0.3 + 3.0 * rng.next();
                k.open *= m;
                k.close *= m * 1.1;
                k.high = k.open.max(k.close) * 1.05;
                k.low = k.open.min(k.close) * 0.9;
                k.volume *= 10.0 * rng.next();
            }
            let (a, b) = (Indicators::new(part), Indicators::new(&changed));
            for i in WARMUP..=cut {
                let x = full.at(i).unwrap_or_else(|| panic!("{s} {i}"));
                assert_eq!(a.at(i), Some(x), "{s}: coupé à {cut}, jour {i}");
                assert_eq!(b.at(i), Some(x), "{s}: futur réécrit après {cut}, jour {i}");
            }
        }
        // Rows: one per candle after the warm-up; the label only once the exit candle exists.
        let rows = asset_rows(0, &c, Kind::Crypto);
        assert_eq!(rows.len(), c.len() - WARMUP);
        assert_eq!(rows.iter().filter(|r| r.fwd.is_none()).count(), HORIZON + 1);
        assert!(full.at(WARMUP - 1).is_none());
    }
}

#[test]
fn labels_start_at_the_next_open_with_the_costs() {
    let c = long("AAPL");
    let i = 400;
    let f = forward(&c, i, Kind::Stock).unwrap();
    assert_eq!((f.entry, f.exit, f.exit_time), (c[i + 1].open, c[i + 1 + HORIZON].open, c[i + 1 + HORIZON].time));
    assert!((f.gross - (c[i + 1 + HORIZON].open / c[i + 1].open - 1.0) * 100.0).abs() < 1e-9);
    assert!(f.net < f.gross && f.drawdown <= 0.0);
    assert!(forward(&c, c.len() - 1 - HORIZON, Kind::Stock).is_none());
    assert!(forward(&c, c.len() - 2 - HORIZON, Kind::Stock).is_some());
    // Round trip ≈ 0.2 % of fees + 0.1 % of slippage + the spread: stocks 0.31 %, cryptos 0.32 %.
    let (s, k) = (round_trip_cost(Kind::Stock), round_trip_cost(Kind::Crypto));
    assert!((0.3..0.32).contains(&s) && k > s && k < 0.33, "{s} {k}");
    // Break-even a hair above the cost (the cost is taken on the proceeds too).
    assert!(net_return(s + 0.01, Kind::Stock) > 0.0 && net_return(s - 0.01, Kind::Stock) < 0.0);
}

fn synthetic(n: usize, seed: u64) -> (Vec<Features>, Vec<bool>) {
    let mut rng = Lcg(seed);
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for _ in 0..n {
        let x: Features = std::array::from_fn(|j| (rng.next() - 0.5) * (1.0 + j as f64) + 10.0 * j as f64);
        // Depends on feature 0 only (plus noise).
        let y = (x[0] + (rng.next() - 0.5) * 0.4) > 0.0;
        xs.push(x);
        ys.push(y);
    }
    (xs, ys)
}

#[test]
fn logistic_regression_is_deterministic_and_uses_training_statistics_only() {
    let (xs, ys) = synthetic(2000, 7);
    let refs: Vec<&Features> = xs.iter().collect();
    let m = Model::fit(&refs, &ys);
    assert_eq!(m, Model::fit(&refs, &ys), "déterministe");
    assert!(m.coef[0] > 1.0, "{:?}", m.coef);
    for j in 1..N_FEATURES {
        assert!(m.coef[j].abs() < 0.25 * m.coef[0], "{j}: {:?}", m.coef);
    }
    let mut hi = xs[0];
    let mut lo = xs[0];
    hi[0] = 0.4;
    lo[0] = -0.4;
    assert!(m.prob(&hi) > 0.8 && m.prob(&lo) < 0.2);
    // Standardisation from the training rows only: the first half's statistics, not the whole set's.
    let half = Model::fit(&refs[..1000], &ys[..1000]);
    let mean0 = xs[..1000].iter().map(|x| x[0]).sum::<f64>() / 1000.0;
    assert!((half.mean[0] - mean0).abs() < 1e-12);
    assert_eq!(half.rows, 1000);
    assert!((half.base_rate - ys[..1000].iter().filter(|y| **y).count() as f64 / 1000.0).abs() < 1e-12);
    // A constant feature does not break the fit (standard deviation 1).
    let flat: Vec<Features> = xs
        .iter()
        .map(|x| {
            let mut y = *x;
            y[5] = 3.0;
            y
        })
        .collect();
    let m2 = Model::fit(&flat.iter().collect::<Vec<_>>(), &ys);
    assert_eq!(m2.sd[5], 1.0);
    assert!(m2.coef.iter().all(|c| c.is_finite()));
}

#[test]
fn actions_follow_both_thresholds() {
    assert_eq!(action(0.7, 0.3, 0.6, 0.5), BotAction::Buy);
    assert_eq!(action(0.4, 0.6, 0.6, 0.5), BotAction::Sell);
    assert_eq!(action(0.5, 0.4, 0.6, 0.5), BotAction::Wait);
    assert_eq!(action(0.7, 0.6, 0.6, 0.5), BotAction::Wait, "les deux : pas de signal");
}

/// Rows of `assets` synthetic assets over `days` days, label known 21 days later.
fn rows(assets: usize, days: usize, seed: u64) -> Vec<Row> {
    let mut rng = Lcg(seed);
    let mut out = Vec::new();
    for a in 0..assets {
        for d in 0..days {
            let x: Features = std::array::from_fn(|_| rng.next() - 0.5);
            let gross = (x[0] + rng.next() - 0.5) * 10.0;
            let time = d as i64 * DAY_MS;
            let fwd = (d + HORIZON + 1 < days).then(|| Forward {
                exit_time: (d + HORIZON + 1) as i64 * DAY_MS,
                entry: 100.0,
                exit: 100.0 * (1.0 + gross / 100.0),
                gross,
                net: net_return(gross, Kind::Stock),
                drawdown: gross.min(0.0) - 1.0,
                up: net_return(gross, Kind::Stock) > 0.0,
                down: gross < -round_trip_cost(Kind::Stock),
            });
            out.push(Row { asset: a, time, x, fwd });
        }
    }
    out
}

#[test]
fn walk_forward_purges_and_never_reads_the_future() {
    let r = rows(3, 1000, 11);
    let (blocks, preds) = walk_forward(&r);
    let first = MIN_TRAIN + HORIZON + PURGE;
    assert_eq!(blocks.len(), (1000 - first).div_ceil(RETRAIN_EVERY));
    assert_eq!(blocks[0].start, first as i64 * DAY_MS);
    for (k, b) in blocks.iter().enumerate() {
        assert!(b.trained, "{k}");
        // The purge: the last training label ends at least PURGE days before the test starts.
        assert_eq!(b.start - b.cutoff, PURGE as i64 * DAY_MS);
        let last_label = r.iter().filter_map(|x| x.fwd.map(|f| f.exit_time)).filter(|t| *t < b.cutoff).max().unwrap();
        assert!(b.start - last_label > PURGE as i64 * DAY_MS, "{k}");
        assert_eq!(b.train_rows, r.iter().filter(|x| x.fwd.is_some_and(|f| f.exit_time < b.cutoff)).count());
    }
    // No prediction before the first test day; one for every row after it.
    for (x, p) in r.iter().zip(&preds) {
        assert_eq!(p.is_some(), x.time >= blocks[0].start, "{}", x.time);
    }
    // Rewriting what was not known yet (labels ending after a block's cutoff, rows after the block) leaves the
    // block's predictions unchanged.
    let k = 1;
    let (cutoff, end) = (blocks[k].cutoff, blocks[k].end);
    let mut changed = r.clone();
    let mut rng = Lcg(99);
    for x in changed.iter_mut() {
        if let Some(f) = x.fwd.as_mut().filter(|f| f.exit_time >= cutoff) {
            f.gross = -f.gross;
            f.up = !f.up;
            f.down = !f.down;
        }
        if x.time >= end {
            x.x = std::array::from_fn(|_| rng.next() * 50.0);
        }
    }
    let (_, again) = walk_forward(&changed);
    for (i, x) in r.iter().enumerate() {
        if x.time >= blocks[k].start && x.time < end {
            assert_eq!(preds[i], again[i], "jour {}", x.time / DAY_MS);
        }
    }
    // Too little history: nothing is tested.
    let (b, p) = walk_forward(&rows(1, first, 3));
    assert!(b.is_empty() && p.iter().all(|x| x.is_none()));
}

fn pred(action: BotAction, block: usize) -> Option<Prediction> {
    Some(Prediction { up: 0.6, down: 0.4, threshold_up: 0.55, threshold_down: 0.45, base_up: 0.5, base_down: 0.4, action, block })
}

#[test]
fn evaluation_by_hand() {
    // One asset, 30 test days, each label ends 21 days later; net = gross here (hand-set), baseline = mean of all.
    let mut r: Vec<Row> = Vec::new();
    let mut p = Vec::new();
    for d in 0..30usize {
        let gross = if d % 2 == 0 { 4.0 } else { -2.0 };
        r.push(Row {
            asset: 0,
            time: d as i64 * DAY_MS,
            x: [0.0; N_FEATURES],
            fwd: Some(Forward {
                exit_time: (d + 21) as i64 * DAY_MS,
                entry: 100.0,
                exit: 100.0 + gross,
                gross,
                net: gross,
                drawdown: -1.0 - d as f64 % 2.0,
                up: gross > 0.0,
                down: gross < 0.0,
            }),
        });
        p.push(pred(
            match d {
                0 | 5 | 22 => BotAction::Buy,
                1 | 23 => BotAction::Sell,
                _ => BotAction::Wait,
            },
            0,
        ));
    }
    let (s, per) = evaluate(&r, &p);
    // Day 5 falls in the holding of day 0 (until day 21): ignored. Buys on days 0 and 22 (+4, +4).
    assert_eq!(s.buy_net, vec![4.0, 4.0]);
    // Baseline = mean of the 30 days = 1 %: excess 3 each.
    assert_eq!(s.buy_excess, vec![3.0, 3.0]);
    // Sell on day 1 (−2 followed: 3 points below a random day), day 23 is inside its 21 days? No: 23 ≥ 22.
    assert_eq!(s.sell_gross, vec![-2.0, -2.0]);
    assert_eq!(s.sell_avoided, vec![3.0, 3.0]);
    assert_eq!(s.wait_net.len(), 30 - 5);
    let a = &per[&0];
    assert_eq!((a.buys.len(), a.sells, a.labelled, a.test_rows), (2, 2, 30, 30));
    assert!((a.bot_return.unwrap() - (1.04f64 * 1.04 - 1.0) * 100.0).abs() < 1e-9);
    assert!((a.hold_return.unwrap() - ((100.0 - 2.0) / 100.0 - 1.0) * 100.0).abs() < 1e-9, "{:?}", a.hold_return);
    let st = stats(&s);
    assert_eq!(st.buy.signals, 2);
    assert_eq!((st.buy.mean_net, st.buy.baseline_net, st.buy.excess), (Some(4.0), Some(1.0), Some(3.0)));
    assert_eq!((st.sell.mean_after, st.sell.baseline_after, st.sell.avoided), (Some(-2.0), Some(1.0), Some(3.0)));
    assert_eq!(st.buy.verdict, Some(Verdict::Insufficient));
    assert_eq!(st.wait.share, Some(83.3));
    assert_eq!(st.sell.fall_rate, Some(100.0));

    // Verdicts with the validation's rule: 40 excesses alternating +2 / −1 (t ≈ 2.1) → edge; the mirror → negative.
    let mut e = Samples::default();
    for i in 0..40 {
        let x = if i % 2 == 0 { 2.0 } else { -1.0 };
        e.buy_net.push(x);
        e.buy_base.push(0.0);
        e.buy_excess.push(x);
        e.sell_gross.push(-x);
        e.sell_base.push(0.0);
        e.sell_avoided.push(-x);
    }
    let st = stats(&e);
    assert_eq!(st.buy.verdict, Some(Verdict::Edge));
    assert!(st.buy.t_stat.unwrap() >= 2.0);
    assert_eq!(st.sell.verdict, Some(Verdict::Negative));
}

#[test]
fn calibration_and_brier_skill() {
    let pairs = vec![(0.25, false, 0.5), (0.35, true, 0.5), (0.65, true, 0.5), (0.95, true, 0.5), (1.0, true, 0.5)];
    let b = calibration(&pairs);
    assert_eq!(b.len(), 6);
    assert_eq!(b.iter().map(|x| x.rows).sum::<usize>(), 5);
    assert_eq!((b[0].rows, b[0].realised), (1, Some(0.0)));
    assert_eq!((b[5].rows, b[5].predicted, b[5].realised), (2, Some(97.5), Some(100.0)), "1,0 compte dans le dernier");
    assert_eq!(b[3].rows, 0);
    assert!(b[3].predicted.is_none());
    assert!(brier_skill(&pairs).unwrap() > 0.0);
    assert!(brier_skill(&[(0.9, false, 0.5), (0.1, true, 0.5)]).unwrap() < 0.0);
    assert_eq!(brier_skill(&[]), None);
}

/// The whole pipeline on the real saved histories (7 assets): deterministic, figures that add up.
#[test]
fn run_on_saved_histories() {
    let make = || {
        ["BTC", "ETH", "SOL", "DOGE", "AAPL", "NVDA", "SPY"]
            .iter()
            .map(|s| History { asset: BASKET.iter().find(|b| b.symbol == *s).unwrap(), candles: long(s), source: "test".into() })
            .collect::<Vec<_>>()
    };
    let r = run(make(), vec![], 1, "test");
    assert_eq!(r, run(make(), vec![], 1, "test"), "déterministe");
    assert_eq!(r.assets.iter().map(|a| a.symbol.as_str()).collect::<Vec<_>>(), ["AAPL", "NVDA", "SPY", "BTC", "ETH", "SOL", "DOGE"]);
    assert_eq!(r.features.len(), N_FEATURES);
    assert!(!r.headline.is_empty() && !r.method.is_empty() && !r.limits.is_empty());
    for g in &r.groups {
        let s = &g.stats;
        assert!(s.labelled <= s.test_rows, "{:?}", g.id);
        if let (Some(m), Some(b), Some(e)) = (s.buy.mean_net, s.buy.baseline_net, s.buy.excess) {
            assert!((m - b - e).abs() < 0.02, "{m} {b} {e}");
        }
        if let (Some(m), Some(b), Some(a)) = (s.sell.mean_after, s.sell.baseline_after, s.sell.avoided) {
            assert!((b - m - a).abs() < 0.02, "{m} {b} {a}");
        }
        assert_eq!(s.calibration_up.iter().map(|b| b.rows).sum::<usize>(), s.labelled);
        assert!(s.buy.signals + s.sell.signals <= s.labelled);
        let m = g.model.as_ref().expect("modèle");
        assert!(m.pair().is_some());
        assert!((m.up.threshold - m.up.base_rate - THRESHOLD_MARGIN * 100.0).abs() < 1e-9);
    }
    assert_eq!(r.overall.buy.signals, r.groups.iter().map(|g| g.stats.buy.signals).sum::<usize>());
    assert_eq!(r.overall.sell.signals, r.groups.iter().map(|g| g.stats.sell.signals).sum::<usize>());
    for a in &r.assets {
        assert!(a.now.action.is_some(), "{}", a.symbol);
    }
    // A history too short to compute features is listed as a failure.
    let short = vec![History { asset: &BASKET[0], candles: long("AAPL")[..100].to_vec(), source: "t".into() }];
    let r = run(short, vec![], 1, "t");
    assert!(r.groups.is_empty() && r.failures.len() == 1 && r.failures[0].error.contains("trop court"));
    assert!(r.headline.starts_with("Aucun actif"));
}

pub fn sample() -> BotReport {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/bot.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Today's view from the saved real report: probabilities, the action of its thresholds, the reason it does not
/// count without an edge; and the pro / con line and nudge only on a side with an edge.
#[test]
fn view_from_the_saved_report() {
    let report = sample();
    let now = common::now();
    let none = bot_view(None, "BTC", Kind::Crypto, &long("BTC"), now);
    assert!(!none.available && none.text == NOT_COMPUTED && none.action.is_none() && none.in_basket);
    for (s, kind) in [("BTC", Kind::Crypto), ("SOL", Kind::Crypto), ("AAPL", Kind::Stock), ("NVDA", Kind::Stock)] {
        let v = bot_view(Some(&report), s, kind, &long(s), now);
        assert!(v.available, "{s}");
        let (up, down) = (v.up.unwrap(), v.down.unwrap());
        assert!((0.0..=100.0).contains(&up) && (0.0..=100.0).contains(&down));
        assert_eq!(v.action, Some(action(up, down, v.threshold_up.unwrap(), v.threshold_down.unwrap())), "{s}");
        assert!(v.contributions.len() <= 3 && !v.contributions.is_empty());
        let g = report.group(BotGroup::of(kind)).unwrap();
        let edge = match v.action.unwrap() {
            BotAction::Buy => g.stats.buy.verdict == Some(Verdict::Edge),
            BotAction::Sell => g.stats.sell.verdict == Some(Verdict::Edge),
            BotAction::Wait => false,
        };
        assert_eq!(v.counts, edge);
        if !edge {
            assert!(v.line(false).is_none() && v.nudge() == 0.0);
        }
        assert!(v.text.contains("probabilité de hausse"), "{}", v.text);
        // Same view from the candles known then: the last one after `now` is ignored.
        let mut more = long(s);
        let mut next = *more.last().unwrap();
        next.time = now + DAY_MS;
        next.close *= 2.0;
        next.high = next.close;
        more.push(next);
        assert_eq!(bot_view(Some(&report), s, kind, &more, now), v);
    }
    // Out of the basket: the group's model, said so.
    let pepe = bot_view(Some(&report), "PEPE", Kind::Crypto, &long("DOGE"), now);
    assert!(pepe.available && !pepe.in_basket);
    // Too short a history.
    let short = bot_view(Some(&report), "BTC", Kind::Crypto, &long("BTC")[..150], now);
    assert!(short.available && short.action.is_none() && short.text.contains("trop court"));

    // A group with an edge on both sides and a model that always says ACHETER (then VENDRE).
    let mut forced = report.clone();
    let g = forced.groups.iter_mut().find(|g| g.id == BotGroup::Crypto).unwrap();
    g.stats.buy.verdict = Some(Verdict::Edge);
    g.stats.sell.verdict = Some(Verdict::Edge);
    g.model.as_mut().unwrap().up.intercept = 20.0;
    g.model.as_mut().unwrap().down.intercept = -20.0;
    let v = bot_view(Some(&forced), "BTC", Kind::Crypto, &long("BTC"), now);
    assert_eq!((v.action, v.counts, v.nudge()), (Some(BotAction::Buy), true, NUDGE));
    let (pro, line) = v.line(false).unwrap();
    assert!(pro && line.starts_with("Le bot appris est favorable (probabilité de hausse 100 %"), "{line}");
    let g = forced.groups.iter_mut().find(|g| g.id == BotGroup::Crypto).unwrap();
    g.model.as_mut().unwrap().up.intercept = -20.0;
    g.model.as_mut().unwrap().down.intercept = 20.0;
    let v = bot_view(Some(&forced), "BTC", Kind::Crypto, &long("BTC"), now);
    assert_eq!((v.action, v.counts, v.nudge()), (Some(BotAction::Sell), true, -NUDGE));
    assert!(v.line(true).unwrap().1.starts_with("Le bot appris conseille de sortir"));
    assert!(!v.line(false).unwrap().0);
    // Edge on the buy side only: a VENDRE does not count.
    forced.groups.iter_mut().find(|g| g.id == BotGroup::Crypto).unwrap().stats.sell.verdict = Some(Verdict::Unproven);
    let v = bot_view(Some(&forced), "BTC", Kind::Crypto, &long("BTC"), now);
    assert!(!v.counts && v.note == NO_EDGE);
}

/// The saved real run follows the contract, and the route serves a cached report as is; the views read it.
#[tokio::test]
async fn routes_serve_the_cached_report() {
    let s = sample();
    assert_eq!(s.assets.len() + s.failures.len(), BASKET.len());
    assert_eq!(s.groups.iter().map(|g| g.id).collect::<Vec<_>>(), [BotGroup::Stock, BotGroup::Crypto]);
    assert!(s.headline.starts_with("Hors échantillon"));
    // The headline from the numbers only: no edge, an edge on one side, a negative side.
    assert!(headline(&s.groups).contains("ni sur les actions ni sur les cryptos : il ne compte pas"), "{}", headline(&s.groups));
    let mut g = s.groups.clone();
    g[1].stats.buy.verdict = Some(Verdict::Edge);
    g[1].stats.buy.t_stat = Some(2.31);
    g[0].stats.sell.verdict = Some(Verdict::Negative);
    g[0].stats.sell.t_stat = Some(-2.5);
    let h = headline(&g);
    assert!(h.starts_with("Hors échantillon, avantage observé pour les achats sur les cryptos (t = 2,3), à confirmer"), "{h}");
    assert!(h.ends_with("Attention : ses ventes sur les actions ont été suivies de hausses (t = −2,5)."), "{h}");
    let expected = serde_json::to_value(&s).unwrap();
    let app = altim::app::router(
        altim::app::AppState::new(altim::live::LiveHub::with(vec![], vec![], Duration::from_secs(3600))),
        altim::auth::Auth::new(None, false),
    );
    let seeded = s.clone();
    altim::cache::cached(altim::app::bot::CACHE_KEY, altim::app::bot::FRESH_MS, move || async move { Ok::<_, altim::http::Error>(seeded) })
        .await
        .unwrap();
    let r = app.clone().oneshot(Request::get("/api/bot").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let b: Value = serde_json::from_slice(&to_bytes(r.into_body(), 1 << 24).await.unwrap()).unwrap();
    assert_eq!(b["headline"], expected["headline"]);
    assert_eq!(b["groups"][0]["buy"]["signals"], expected["groups"][0]["buy"]["signals"]);
    assert_eq!(b["groups"][0]["model"]["up"]["weights"].as_array().unwrap().len(), N_FEATURES);
    let r = app.oneshot(Request::get("/api/bot/views?symbols=BAD!:crypto").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

/// Real run on the whole basket; `ALTIM_SAVE_SAMPLE=1` rewrites tests/samples/bot.json with it.
#[tokio::test]
#[ignore]
async fn bot_live() {
    let r = altim::app::bot::compute().await.unwrap();
    println!("{}", r.headline);
    for g in &r.groups {
        println!("{}", g.text);
    }
    for f in &r.failures {
        println!("  échec {} : {}", f.symbol, f.error);
    }
    if std::env::var("ALTIM_SAVE_SAMPLE").is_ok_and(|v| v == "1") {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/bot.json");
        std::fs::write(path, serde_json::to_string_pretty(&altim::js::to_value(&r)).unwrap() + "\n").unwrap();
    }
    assert!(r.assets.len() >= BASKET.len() / 2);
}

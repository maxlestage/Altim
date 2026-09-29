//! « Bot Altim » v2 (`engine::bot`, `engine::bot_trees`, `bot_history`, `/api/bot`): features without look-ahead
//! (the asset's and its market's) on real saved histories, labels at the next open, the three fitted candidates
//! (deterministic, training statistics only), the trend rule, the nested walk-forward's purges and choice, the
//! out-of-sample evaluation and the clustered t by hand, the long-history parsers on real saved responses, today's
//! view from the saved real reports (v2, and v1 still decoded), and the route. The live runs are ignored by default
//! (`ALTIM_SAVE_SAMPLE=1` rewrites tests/samples/bot.json).
mod common;

use std::time::Duration;

use altim::bot_history::{longer, merge_pages, next_end};
use altim::engine::bot::*;
use altim::engine::bot_trees::{self, Forest};
use altim::engine::signal::sanitize;
use altim::engine::validation::{BASKET, Verdict};
use altim::market::{parse, parse_stock};
use altim::types::{Candle, DAY_MS, Kind};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

fn long(symbol: &str) -> Vec<Candle> {
    sanitize(&common::find(symbol, "long").unwrap().candles)
}

fn market_of(symbol: &str) -> Vec<Candle> {
    if ["BTC", "ETH", "SOL", "DOGE"].contains(&symbol) { long("BTC") } else { long("SPY") }
}

/// Deterministic pseudo-random numbers in [0, 1) (no dependency, same sequence every run).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn scramble(c: &mut [Candle], from: usize, seed: u64) {
    let mut rng = Lcg(seed);
    for k in c.iter_mut().skip(from) {
        let m = 0.3 + 3.0 * rng.next();
        k.open *= m;
        k.close *= m * 1.1;
        k.high = k.open.max(k.close) * 1.05;
        k.low = k.open.min(k.close) * 0.9;
        k.volume *= 10.0 * rng.next();
    }
}

/// The features (and the trend rule's state) at the close of day t are the same whether the later candles of the
/// asset and of its market exist, are cut or are rewritten.
#[test]
fn features_have_no_look_ahead() {
    for s in ["BTC", "AAPL", "SOL", "SPY"] {
        let c = long(s);
        let m = market_of(s);
        assert!(c.len() > WARMUP + 100, "{s}: {}", c.len());
        let full = Indicators::new(&c, &m);
        let first = (WARMUP..c.len()).find(|i| full.at(*i).is_some()).unwrap();
        assert!(first < WARMUP + 50, "{s}: {first}");
        for cut in [first, first + 57, c.len() / 2, c.len() - 25] {
            let t = c[cut].time;
            let part = &c[..=cut];
            let mpart: Vec<Candle> = m.iter().filter(|x| x.time <= t).copied().collect();
            let mut changed = c.clone();
            scramble(&mut changed, cut + 1, cut as u64);
            let mut mchanged = m.clone();
            let mcut = m.iter().position(|x| x.time > t).unwrap_or(m.len());
            scramble(&mut mchanged, mcut, cut as u64 + 7);
            let (a, b) = (Indicators::new(part, &mpart), Indicators::new(&changed, &mchanged));
            for i in first..=cut {
                let x = full.at(i).unwrap_or_else(|| panic!("{s} {i}"));
                assert_eq!(a.at(i), Some(x), "{s}: coupé à {cut}, jour {i}");
                assert_eq!(b.at(i), Some(x), "{s}: futur réécrit après {cut}, jour {i}");
                assert_eq!((a.trend(i), b.trend(i)), (full.trend(i), full.trend(i)), "{s} {i}");
            }
        }
        // Rows: one per candle with features; the label only once the exit candle exists; one training row in 5.
        let rows = asset_rows(0, &c, &m, Kind::Crypto);
        assert_eq!(rows.len(), c.len() - first, "{s}");
        assert_eq!(rows.iter().filter(|r| r.fwd.is_none()).count(), HORIZON + 1);
        assert_eq!(rows.iter().filter(|r| r.day.is_none()).count(), 2);
        let train = rows.iter().filter(|r| r.train).count();
        assert!(train.abs_diff(rows.len() / TRAIN_STRIDE) <= 1, "{train}");
        assert!(full.at(WARMUP - 1).is_none());
    }
    // The market of a stock is the S&P 500: SPY's own relative strength is 0; AAPL's is its return minus SPY's.
    let spy = long("SPY");
    let ind = Indicators::new(&spy, &spy);
    let x = ind.at(spy.len() - 1).unwrap();
    assert_eq!((x[18], x[19], x[20]), (0.0, 0.0, 0.0));
    let aapl = long("AAPL");
    let ind = Indicators::new(&aapl, &spy);
    let i = aapl.len() - 1;
    let x = ind.at(i).unwrap();
    let r = |c: &[Candle], k: usize| (c[c.len() - 1].close / c[c.len() - 1 - k].close - 1.0) * 100.0;
    assert!((x[18] as f64 - (r(&aapl, 20) - r(&spy, 20))).abs() < 1e-3, "{}", x[18]);
    // 12-1 momentum and the trend rule by hand.
    let mom = (aapl[i - 21].close / aapl[i - 252].close - 1.0) * 100.0;
    assert!((x[16] as f64 - mom).abs() < 1e-3);
    let sma = aapl[i - 199..=i].iter().map(|c| c.close).sum::<f64>() / 200.0;
    let expect = match (aapl[i].close > sma, mom > 0.0) {
        (true, true) => 1,
        (false, false) => -1,
        _ => 0,
    };
    assert_eq!(ind.trend(i), expect);
    // Without enough market history: no row.
    assert!(Indicators::new(&aapl, &spy[spy.len() - 100..]).at(i).is_none());
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
    let (s, k) = (round_trip_cost(Kind::Stock), round_trip_cost(Kind::Crypto));
    assert!((0.3..0.32).contains(&s) && k > s && k < 0.33, "{s} {k}");
    assert!(net_return(s + 0.01, Kind::Stock) > 0.0 && net_return(s - 0.01, Kind::Stock) < 0.0);
    // The day-by-day return of a row: from the next open to the one after.
    let rows = asset_rows(0, &c, &long("SPY"), Kind::Stock);
    let r = rows.iter().find(|r| r.time == c[i].time).unwrap();
    assert!((r.day.unwrap() as f64 - (c[i + 2].open / c[i + 1].open - 1.0) * 100.0).abs() < 1e-4);
}

fn synthetic(n: usize, seed: u64) -> (Vec<Features>, Vec<bool>) {
    let mut rng = Lcg(seed);
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for _ in 0..n {
        let x: Features = std::array::from_fn(|j| ((rng.next() - 0.5) * (1.0 + j as f64) + 10.0 * j as f64) as f32);
        // Depends on feature 0 only (plus noise).
        let y = (x[0] as f64 + (rng.next() - 0.5) * 0.4) > 0.0;
        xs.push(x);
        ys.push(y);
    }
    (xs, ys)
}

#[test]
fn logistic_regression_is_deterministic_and_uses_training_statistics_only() {
    let (xs, ys) = synthetic(2000, 7);
    let refs: Vec<&[f32]> = xs.iter().map(|x| &x[..]).collect();
    let m = Model::fit(&refs, &ys, &ALL_COLS);
    assert_eq!(m, Model::fit(&refs, &ys, &ALL_COLS), "déterministe");
    assert!(m.coef[0] > 1.0, "{:?}", m.coef);
    for j in 1..N_FEATURES {
        assert!(m.coef[j].abs() < 0.25 * m.coef[0], "{j}: {:?}", m.coef);
    }
    let mut hi = xs[0];
    let mut lo = xs[0];
    hi[0] = 0.4;
    lo[0] = -0.4;
    assert!(m.prob(&hi) > 0.8 && m.prob(&lo) < 0.2);
    // The v1 candidate reads its 16 columns only.
    let v1 = Model::fit(&refs, &ys, &V1_COLS);
    assert_eq!(v1.coef.len(), N_V1);
    let mut moved = hi;
    moved[20] = 1e6;
    assert_eq!(v1.prob(&hi), v1.prob(&moved));
    // Standardisation from the training rows only: the first half's statistics, not the whole set's.
    let half = Model::fit(&refs[..1000], &ys[..1000], &ALL_COLS);
    let mean0 = xs[..1000].iter().map(|x| x[0] as f64).sum::<f64>() / 1000.0;
    assert!((half.mean[0] - mean0).abs() < 1e-9);
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
    let m2 = Model::fit(&flat.iter().map(|x| &x[..]).collect::<Vec<_>>(), &ys, &ALL_COLS);
    assert_eq!(m2.sd[5], 1.0);
    assert!(m2.coef.iter().all(|c| c.is_finite()));
    // Published and read back.
    assert_eq!(ModelOut::of(&m).model().unwrap().prob(&hi), m.prob(&hi));
}

#[test]
fn boosted_trees_are_deterministic_and_learn_a_threshold() {
    // The label is 1 above 0.3 on feature 17 (a new one), noise elsewhere.
    let mut rng = Lcg(3);
    let mut xs: Vec<Features> = Vec::new();
    let mut ys = Vec::new();
    for _ in 0..3000 {
        let x: Features = std::array::from_fn(|_| rng.next() as f32);
        ys.push(x[17] > 0.3 && rng.next() > 0.1);
        xs.push(x);
    }
    let refs: Vec<&[f32]> = xs.iter().map(|x| &x[..]).collect();
    let f = Forest::fit(&refs, &ys, &ALL_COLS);
    assert_eq!(f, Forest::fit(&refs, &ys, &ALL_COLS), "déterministe");
    assert_eq!(f.trees.len(), bot_trees::TREES);
    assert!(f.trees.iter().all(|t| t.len() <= 15));
    let mut hi = xs[0];
    let mut lo = xs[0];
    hi[17] = 0.9;
    lo[17] = 0.1;
    assert!(f.prob(&hi) > 0.8 && f.prob(&lo) < 0.1, "{} {}", f.prob(&hi), f.prob(&lo));
    // The root's first split is on feature 17, near 0.3; leaves hold at least MIN_LEAF rows (checked by count).
    let root = f.trees[0][0];
    assert_eq!(root.feature, 17);
    assert!((root.threshold - 0.3).abs() < 0.05, "{}", root.threshold);
    // Contributions add up to the margin minus the bias and the roots' values, the biggest on feature 17.
    let c = f.contributions(&hi, N_FEATURES);
    let roots: f64 = f.trees.iter().map(|t| t[0].value).sum();
    assert!((c.iter().sum::<f64>() - (f.margin(&hi) - f.bias - roots)).abs() < 1e-9);
    let top = c.iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap().0;
    assert_eq!(top, 17);
    // Bins from the training rows only: fitting on the first half never reads the second half's values.
    let mut changed = xs.clone();
    for x in changed.iter_mut().skip(1500) {
        x[17] = 99.0;
    }
    let a = Forest::fit(&refs[..1500], &ys[..1500], &ALL_COLS);
    let b = Forest::fit(&changed.iter().map(|x| &x[..]).collect::<Vec<_>>()[..1500], &ys[..1500], &ALL_COLS);
    assert_eq!(a, b);
    // Compact JSON (one array per node) read back identically.
    let json = serde_json::to_string(&f).unwrap();
    assert!(json.contains("\"trees\":[[["), "{}", &json[..200]);
    let back: Forest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.prob(&hi), f.prob(&hi));
    // Quantile edges: distinct, below the maximum.
    let mut v: Vec<f32> = (0..100).map(|i| (i % 3) as f32).collect();
    assert_eq!(bot_trees::edges(&mut v), vec![0.0, 1.0]);
}

#[test]
fn trend_rule_and_actions() {
    assert_eq!(action(0.7, 0.3, 0.6, 0.5), BotAction::Buy);
    assert_eq!(action(0.4, 0.6, 0.6, 0.5), BotAction::Sell);
    assert_eq!(action(0.5, 0.4, 0.6, 0.5), BotAction::Wait);
    assert_eq!(action(0.7, 0.6, 0.6, 0.5), BotAction::Wait, "les deux : pas de signal");
    assert_eq!((trend_action(1), trend_action(0), trend_action(-1)), (BotAction::Buy, BotAction::Wait, BotAction::Sell));
    // Frequencies per state, smoothed: state 1 → 3 of 4 rises → (3 + 1) / (4 + 2).
    let t = TrendSide::fit(&[1, 1, 1, 1, -1, -1, 0], &[true, true, true, false, false, false, true]);
    assert_eq!(t.probs, [1.0 / 4.0, 2.0 / 3.0, 4.0 / 6.0]);
    assert!((t.base_rate - 4.0 / 7.0).abs() < 1e-12);
}

/// Rows of `assets` synthetic assets over `days` days, label known 21 days later; the outcome follows feature 20
/// (a v2 feature) when `signal`.
fn rows(assets: usize, days: usize, seed: u64, signal: bool) -> Vec<Row> {
    let mut rng = Lcg(seed);
    let mut out = Vec::new();
    for a in 0..assets {
        for d in 0..days {
            let x: Features = std::array::from_fn(|_| (rng.next() - 0.5) as f32);
            let gross = ((if signal { x[20] as f64 * 3.0 } else { 0.0 }) + rng.next() - 0.5) * 10.0;
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
            let trend = if x[1] > 0.2 {
                1
            } else if x[1] < -0.2 {
                -1
            } else {
                0
            };
            out.push(Row { asset: a, time, x, trend, train: d % TRAIN_STRIDE == 0, fwd, day: Some((gross / 20.0) as f32) });
        }
    }
    out
}

#[test]
fn nested_walk_forward_purges_and_never_reads_the_future() {
    let r = rows(8, 1400, 11, true);
    let wf = walk_forward(&r, 1);
    let first = MIN_TRAIN + HORIZON + PURGE;
    assert_eq!(wf.blocks.len(), (1400 - first).div_ceil(RETRAIN_EVERY));
    assert_eq!(wf.blocks[0].start, first as i64 * DAY_MS);
    for (k, b) in wf.blocks.iter().enumerate() {
        assert!(b.trained, "{k}");
        assert_eq!(b.start - b.cutoff, PURGE as i64 * DAY_MS);
        let last_label = r.iter().filter_map(|x| x.fwd.map(|f| f.exit_time)).filter(|t| *t < b.cutoff).max().unwrap();
        assert!(b.start - last_label > PURGE as i64 * DAY_MS, "{k}");
        assert_eq!(b.train_rows, r.iter().filter(|x| x.train && x.fwd.is_some_and(|f| f.exit_time < b.cutoff)).count());
        // Every candidate scored on the inner validation; the chosen one has the lowest log-loss.
        assert_eq!(b.scores.len(), CANDIDATES.len());
        let best = b.scores.iter().filter_map(|s| s.log_loss.map(|l| (s.id, l))).min_by(|a, c| a.1.total_cmp(&c.1)).unwrap();
        assert_eq!(b.chosen, Some(best.0), "{k}");
        // The outcome follows a v2 feature: the v1 model (without it) is never chosen.
        assert_ne!(b.chosen, Some(Candidate::V1), "{k}");
    }
    // Same result on several threads.
    assert_eq!(walk_forward(&r, 4), wf);
    // No prediction before the first test day; one per candidate for every row after it; nested = the choice's.
    let nested = wf.nested();
    for (i, x) in r.iter().enumerate() {
        for c in CANDIDATES {
            assert_eq!(wf.preds[c.index()][i].is_some(), x.time >= wf.blocks[0].start, "{}", x.time);
        }
        if let Some(p) = nested[i] {
            let chosen = wf.blocks[p.block as usize].chosen.unwrap();
            assert_eq!(Some(p), wf.preds[chosen.index()][i]);
        }
    }
    // The trend rule's action comes from its state.
    for (x, p) in r.iter().zip(&wf.preds[Candidate::Trend.index()]) {
        if let Some(p) = p {
            assert_eq!(p.action, trend_action(x.trend));
        }
    }
    // Rewriting what was not known yet (labels ending after a block's cutoff, rows after the block) leaves the
    // block's choice, scores and predictions unchanged.
    let k = 1;
    let (cutoff, end) = (wf.blocks[k].cutoff, wf.blocks[k].end);
    let mut changed = r.clone();
    let mut rng = Lcg(99);
    for x in changed.iter_mut() {
        if let Some(f) = x.fwd.as_mut().filter(|f| f.exit_time >= cutoff) {
            f.gross = -f.gross;
            f.up = !f.up;
            f.down = !f.down;
        }
        if x.time >= end {
            x.x = std::array::from_fn(|_| (rng.next() * 50.0) as f32);
        }
    }
    let again = walk_forward(&changed, 1);
    assert_eq!(again.blocks[k].scores, wf.blocks[k].scores);
    assert_eq!(again.blocks[k].chosen, wf.blocks[k].chosen);
    for (i, x) in r.iter().enumerate() {
        if x.time >= wf.blocks[k].start && x.time < end {
            for c in CANDIDATES {
                assert_eq!(wf.preds[c.index()][i], again.preds[c.index()][i], "jour {}", x.time / DAY_MS);
            }
        }
    }
    // Nothing learnable: still a choice per block, and the inner validation window ends at the cutoff.
    let noise = rows(4, 1000, 5, false);
    let dates = timeline(&noise);
    let sel = select(&noise, &dates, 700);
    assert!(sel.chosen.is_some());
    assert!(sel.scores.iter().all(|s| s.log_loss.is_some()));
    // Too little history: nothing is tested.
    let small = walk_forward(&rows(1, first, 3, false), 1);
    assert!(small.blocks.is_empty() && small.preds.iter().all(|p| p.iter().all(|x| x.is_none())));
}

fn pred(action: BotAction, block: u32) -> Option<Prediction> {
    Some(Prediction { up: 0.6, down: 0.4, base_up: 0.5, base_down: 0.4, action, block })
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
            trend: 0,
            train: false,
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
            day: Some(if d < 10 { -1.0 } else { 1.0 }),
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
    let (s, per) = evaluate(&r, &p, |_| true, |_| 0.0);
    assert_eq!(s.buy_net, vec![4.0, 4.0]);
    assert_eq!(s.buy_excess, vec![3.0, 3.0]);
    assert_eq!((s.buy_time.clone(), s.buy_asset.clone()), (vec![0, 22 * DAY_MS], vec![0, 0]));
    assert_eq!(s.sell_gross, vec![-2.0, -2.0]);
    assert_eq!(s.sell_avoided, vec![3.0, 3.0]);
    assert_eq!(s.wait_net.len(), 30 - 5);
    let a = &per[&0];
    assert_eq!((a.buys.len(), a.sells, a.labelled, a.test_rows), (2, 2, 30, 30));
    assert!((a.bot_return.unwrap() - (1.04f64 * 1.04 - 1.0) * 100.0).abs() < 1e-9);
    // Exits: out for 20 days from day 1 (days 1-20, which include the 9 falling days), back in from day 21 (days
    // 21-22), out again from day 23 (the rest). Hold: 10 × −1 % then 20 × +1 %.
    let x = a.exit.unwrap();
    assert_eq!((x.days, x.out_days), (30, 27));
    let hold = 0.99f64.powi(10) * 1.01f64.powi(20);
    assert!((x.hold_return - (hold - 1.0) * 100.0).abs() < 1e-3, "{}", x.hold_return);
    assert!((x.bot_return - (0.99 * 1.01f64.powi(2) - 1.0) * 100.0).abs() < 1e-3, "{}", x.bot_return);
    assert!((x.hold_max_drawdown - (0.99f64.powi(10) - 1.0) * 100.0).abs() < 1e-3);
    assert!((x.bot_max_drawdown + 1.0).abs() < 1e-3);
    let st = stats(&s);
    assert_eq!(st.buy.signals, 2);
    assert_eq!((st.buy.mean_net, st.buy.baseline_net, st.buy.excess), (Some(4.0), Some(1.0), Some(3.0)));
    assert_eq!((st.sell.mean_after, st.sell.baseline_after, st.sell.avoided), (Some(-2.0), Some(1.0), Some(3.0)));
    assert_eq!(st.buy.verdict, Some(Verdict::Insufficient));
    assert_eq!(st.wait.share, Some(83.3));
    assert_eq!(st.sell.fall_rate, Some(100.0));
    assert_eq!(st.sell.exit.assets, 1);
    assert_eq!(st.sell.exit.out_share, Some(90.0));
    // The kept rows only: the first 10 days.
    let (s10, _) = evaluate(&r, &p, |x| x.time < 10 * DAY_MS, |_| 0.0);
    assert_eq!(s10.test_rows, 10);
    assert_eq!(s10.buy_net, vec![4.0]);

    // Verdicts with the validation's rule: 40 excesses alternating +2 / −1 on 40 different days (t ≈ 2.1) → edge;
    // the mirror → negative.
    let mut e = Samples::default();
    for i in 0..40 {
        let x = if i % 2 == 0 { 2.0 } else { -1.0 };
        e.buy_net.push(x);
        e.buy_base.push(0.0);
        e.buy_excess.push(x);
        e.buy_time.push(i as i64 * DAY_MS);
        e.buy_asset.push(i % 4);
        e.sell_gross.push(-x);
        e.sell_base.push(0.0);
        e.sell_avoided.push(-x);
        e.sell_time.push(i as i64 * DAY_MS);
        e.sell_asset.push(i % 4);
    }
    let st = stats(&e);
    assert_eq!(st.buy.verdict, Some(Verdict::Edge));
    assert!(st.buy.t_stat.unwrap() >= 2.0);
    assert_eq!(st.buy.t_stat, st.buy.clustered.by_date);
    assert_eq!(st.buy.clustered.dates, 40);
    assert_eq!(st.sell.verdict, Some(Verdict::Negative));
    // The same 40 signals on 2 days only: averaged by day first, t over 2 values → not an edge any more.
    let mut same = e.clone();
    same.buy_time = (0..40).map(|i| (i % 2) as i64 * DAY_MS).collect();
    let st = stats(&same);
    assert_eq!(st.buy.clustered.dates, 2);
    assert_ne!(st.buy.verdict, Some(Verdict::Edge));
    assert!(st.buy.clustered.per_signal.unwrap() >= 2.0);
}

#[test]
fn clustered_t_by_hand() {
    // Day 0: 1 and 3 (mean 2); day 1: 4; day 2: 0 → t of [2, 4, 0] = 2 / (2 / √3).
    let c = clustered(&[1.0, 3.0, 4.0, 0.0], &[0, 3_600_000, DAY_MS, 2 * DAY_MS], &[0, 1, 0, 1]);
    assert_eq!(c.dates, 3);
    assert!((c.by_date.unwrap() - (3f64).sqrt()).abs() < 0.01);
    // By asset: [2.5, 1.5] → mean 2, sd 0.707 → t = 4.
    assert_eq!((c.assets, c.by_asset), (2, Some(4.0)));
    assert_eq!(cluster_means(&[1.0, 3.0], &[5, 5]), vec![2.0]);
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

/// The whole pipeline on the real saved histories (7 assets, 2 of them as extra training assets): deterministic
/// (same result on one or several threads), figures that add up.
#[test]
fn run_on_saved_histories() {
    let make = || {
        ["BTC", "ETH", "SOL", "DOGE", "AAPL", "NVDA", "SPY"]
            .iter()
            .map(|s| History { asset: BASKET.iter().find(|b| b.symbol == *s).unwrap(), candles: long(s), source: "test".into(), extra: false })
            .chain(std::iter::once(History { asset: &EXTRA[0], candles: long("NVDA"), source: "test".into(), extra: true }))
            .collect::<Vec<_>>()
    };
    let r = run(make(), vec![], 1, "test", 1);
    assert_eq!(r, run(make(), vec![], 1, "test", 4), "déterministe");
    assert_eq!(r.version, 2);
    assert_eq!(r.assets.iter().map(|a| a.symbol.as_str()).collect::<Vec<_>>(), ["AAPL", "NVDA", "SPY", "BTC", "ETH", "SOL", "DOGE"]);
    assert_eq!(r.features.len(), N_FEATURES);
    assert!(!r.headline.is_empty() && !r.method.is_empty() && !r.limits.is_empty() && !r.changes.is_empty());
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
        assert_eq!(s.buy.t_stat, s.buy.clustered.by_date);
        let m = g.model.as_ref().expect("modèle");
        assert!(m.pair().is_some());
        assert!((m.up_model.threshold - m.up_model.base_rate - THRESHOLD_MARGIN * 100.0).abs() < 1e-9);
        assert_eq!(g.candidates.len(), CANDIDATES.len());
        assert_eq!(g.selection.len(), g.blocks);
        assert_eq!(g.candidates.iter().map(|c| c.chosen_blocks).sum::<usize>(), g.trained_blocks);
        // Every candidate is tested on the same days as the nested result (all of them fit in every block here).
        for c in &g.candidates {
            assert_eq!(c.stats.labelled, s.labelled, "{:?}", c.id);
        }
        let h = g.holdout.as_ref().unwrap();
        assert!(h.stats.labelled <= s.labelled && h.stats.test_rows > 0);
        assert!(g.universe.median_years.unwrap() > 2.0);
    }
    let stock = r.group(BotGroup::Stock).unwrap();
    assert_eq!((stock.universe.basket, stock.universe.extra), (3, 1));
    assert!(stock.extra.is_some());
    assert_eq!(stock.market, "S&P 500 (SPY)");
    assert_eq!(r.overall.buy.signals, r.groups.iter().map(|g| g.stats.buy.signals).sum::<usize>());
    assert_eq!(r.overall.sell.signals, r.groups.iter().map(|g| g.stats.sell.signals).sum::<usize>());
    for a in &r.assets {
        assert!(a.now.action.is_some(), "{}", a.symbol);
        assert!(a.years.unwrap() > 2.0);
    }
    // A history too short to compute features is listed as a failure (an extra one apart).
    let short = vec![
        History { asset: &BASKET[0], candles: long("AAPL")[..100].to_vec(), source: "t".into(), extra: false },
        History { asset: &EXTRA[0], candles: long("AAPL")[..100].to_vec(), source: "t".into(), extra: true },
    ];
    let r = run(short, vec![], 1, "t", 1);
    assert!(r.groups.is_empty() && r.failures.len() == 1 && r.failures[0].error.contains("trop court"));
    assert_eq!(r.extra_failures.len(), 1);
    assert!(r.headline.starts_with("Aucun actif"));
}

fn load(name: &str) -> BotReport {
    let path = format!("{}/tests/samples/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

pub fn sample() -> BotReport {
    load("bot.json")
}

/// Today's view from the saved real report: probabilities, the action of its thresholds (or of the trend rule),
/// the reason it does not count without an edge; and the pro / con line and nudge only on a side with an edge.
#[test]
fn view_from_the_saved_report() {
    let report = sample();
    let now = common::now();
    let none = bot_view(None, "BTC", Kind::Crypto, &long("BTC"), &[], now);
    assert!(!none.available && none.text == NOT_COMPUTED && none.action.is_none() && none.in_basket);
    for (s, kind) in [("BTC", Kind::Crypto), ("SOL", Kind::Crypto), ("AAPL", Kind::Stock), ("NVDA", Kind::Stock), ("SPY", Kind::Stock)] {
        let v = bot_view(Some(&report), s, kind, &long(s), &market_of(s), now);
        assert!(v.available, "{s}");
        let (up, down) = (v.up.unwrap(), v.down.unwrap());
        assert!((0.0..=100.0).contains(&up) && (0.0..=100.0).contains(&down));
        let g = report.group(BotGroup::of(kind)).unwrap();
        let chosen = g.model.as_ref().unwrap().candidate;
        assert_eq!(v.model, Some(chosen));
        if chosen != Candidate::Trend {
            assert_eq!(v.action, Some(action(up, down, v.threshold_up.unwrap(), v.threshold_down.unwrap())), "{s}");
        }
        assert!(v.contributions.len() <= 3 && !v.contributions.is_empty(), "{s}");
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
        // Same view from the candles known then: the last one after `now` is ignored (asset and market).
        let mut more = long(s);
        let mut next = *more.last().unwrap();
        next.time = now + DAY_MS;
        next.close *= 2.0;
        next.high = next.close;
        more.push(next);
        let mut m = market_of(s);
        m.push(Candle { time: now + DAY_MS, ..next });
        assert_eq!(bot_view(Some(&report), s, kind, &more, &m, now), v);
    }
    // Out of the basket: the group's model, said so.
    let pepe = bot_view(Some(&report), "PEPE", Kind::Crypto, &long("DOGE"), &long("BTC"), now);
    assert!(pepe.available && !pepe.in_basket);
    // Too short a history; no market.
    let short = bot_view(Some(&report), "BTC", Kind::Crypto, &long("BTC")[..150], &[], now);
    assert!(short.available && short.action.is_none() && short.text.contains("trop court"));
    let no_market = bot_view(Some(&report), "AAPL", Kind::Stock, &long("AAPL"), &[], now);
    assert!(no_market.action.is_none() && no_market.text.contains("marché"), "{}", no_market.text);

    // A group with an edge on both sides and a model that always says ACHETER (then VENDRE).
    let forced_side = |intercept: f64| LiveSide {
        base_rate: 50.0,
        threshold: 55.0,
        logit: Some(ModelOut {
            base_rate: 50.0,
            threshold: 55.0,
            intercept,
            weights: vec![Weight { id: "ret5".into(), coef: 0.0, mean: 0.0, sd: 1.0 }],
        }),
        ..LiveSide::default()
    };
    let mut forced = report.clone();
    let g = forced.groups.iter_mut().find(|g| g.id == BotGroup::Crypto).unwrap();
    g.stats.buy.verdict = Some(Verdict::Edge);
    g.stats.sell.verdict = Some(Verdict::Edge);
    let m = g.model.as_mut().unwrap();
    m.candidate = Candidate::Logit;
    m.up_model = forced_side(20.0);
    m.down_model = forced_side(-20.0);
    let v = bot_view(Some(&forced), "BTC", Kind::Crypto, &long("BTC"), &[], now);
    assert_eq!((v.action, v.counts, v.nudge()), (Some(BotAction::Buy), true, NUDGE));
    let (pro, line) = v.line(false).unwrap();
    assert!(pro && line.starts_with("Le bot appris est favorable (probabilité de hausse 100 %"), "{line}");
    let m = forced.groups.iter_mut().find(|g| g.id == BotGroup::Crypto).unwrap().model.as_mut().unwrap();
    m.up_model = forced_side(-20.0);
    m.down_model = forced_side(20.0);
    let v = bot_view(Some(&forced), "BTC", Kind::Crypto, &long("BTC"), &[], now);
    assert_eq!((v.action, v.counts, v.nudge()), (Some(BotAction::Sell), true, -NUDGE));
    assert!(v.line(true).unwrap().1.starts_with("Le bot appris conseille de sortir"));
    assert!(!v.line(false).unwrap().0);
    // Edge on the buy side only: a VENDRE does not count.
    forced.groups.iter_mut().find(|g| g.id == BotGroup::Crypto).unwrap().stats.sell.verdict = Some(Verdict::Unproven);
    let v = bot_view(Some(&forced), "BTC", Kind::Crypto, &long("BTC"), &[], now);
    assert!(!v.counts && v.note == NO_EDGE);
}

/// A v1 report (saved real run) still decodes, with the v2 fields empty, and its logistic weights still give a view.
#[test]
fn v1_report_still_decodes() {
    let v1 = load("bot-v1.json");
    assert_eq!(v1.version, 0);
    assert!(v1.changes.is_empty() && v1.timing.is_none());
    let g = v1.group(BotGroup::Stock).unwrap();
    assert!(g.candidates.is_empty() && g.holdout.is_none());
    let pair = g.model.as_ref().unwrap().pair().unwrap();
    assert_eq!(pair.candidate, Candidate::V1);
    let v = bot_view(Some(&v1), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), common::now());
    assert!(v.available && v.action.is_some());
}

#[test]
fn long_history_parsers_on_saved_responses() {
    let read = |f: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(format!("{}/tests/samples/bot-history/{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
    };
    // Bitstamp, two pages read backwards (`end`): the newer page first, then the one before its first candle.
    let p1 = parse::bitstamp(&read("bitstamp-btc-page1.json")).unwrap();
    let p2 = parse::bitstamp(&read("bitstamp-btc-page2.json")).unwrap();
    assert_eq!((p1.len(), p2.len()), (5, 5));
    assert_eq!(next_end(&p1, 5), Some(p1[0].time / 1000 - 1));
    assert_eq!(next_end(&p1, 1000), None, "page incomplète : début de la cotation");
    assert!(p2.last().unwrap().time < p1[0].time);
    let all = merge_pages(vec![p1.clone(), p2.clone(), p1.clone()]);
    assert_eq!(all.len(), 10);
    assert!(all.windows(2).all(|w| w[1].time - w[0].time == DAY_MS), "jours consécutifs");
    assert_eq!(all[0].close, 438.04);
    // A pair not listed then: empty page, no earlier page.
    let empty = parse::bitstamp(&read("bitstamp-trx-empty.json")).unwrap();
    assert!(empty.is_empty() && next_end(&empty, 1000).is_none());
    // Yahoo with period1 / period2: Coca-Cola, January 2005 (20 sessions).
    let ko = parse_stock::yahoo(&read("yahoo-ko-2005-01.json")).unwrap();
    assert!((19..=21).contains(&ko.len()), "{}", ko.len());
    assert!(ko.iter().all(|c| c.low <= c.close && c.close <= c.high && c.volume > 0.0));
    // The longer of two sources wins; both errors reported.
    let (c, src) = longer(("Bitstamp", Ok(p1.clone())), ("Yahoo Finance", Ok(all.clone()))).unwrap();
    assert_eq!((c.len(), src.as_str()), (10, "Yahoo Finance"));
    let (c, src) = longer(("Bitstamp", Ok(all.clone())), ("Yahoo Finance", Ok(all.clone()))).unwrap();
    assert_eq!((c.len(), src.as_str()), (10, "Bitstamp"));
    let e =
        longer(("Bitstamp", Err(altim::http::Error("HTTP 404".into()))), ("Yahoo Finance", Err(altim::http::Error("HTTP 429".into())))).unwrap_err();
    assert_eq!(e.0, "Bitstamp : HTTP 404 ; Yahoo Finance : HTTP 429");
}

/// The saved real run follows the contract, and the route serves a cached report as is; the views read it.
#[tokio::test]
async fn routes_serve_the_cached_report() {
    let s = sample();
    assert_eq!(s.version, 2);
    assert_eq!(s.assets.len() + s.failures.len(), BASKET.len());
    assert_eq!(s.groups.iter().map(|g| g.id).collect::<Vec<_>>(), [BotGroup::Stock, BotGroup::Crypto]);
    assert!(s.headline.starts_with("Hors échantillon"));
    assert_eq!(headline(&s.groups), s.headline);
    for g in &s.groups {
        assert_eq!(g.candidates.len(), CANDIDATES.len());
        assert!(g.universe.extra > 0 && g.data_years.is_some() && g.holdout.is_some());
    }
    // The headline from the numbers only: an edge on one side, a negative side.
    let mut g = s.groups.clone();
    for x in g.iter_mut() {
        x.stats.buy.verdict = Some(Verdict::Unproven);
        x.stats.sell.verdict = Some(Verdict::Unproven);
    }
    assert!(headline(&g).contains("ni sur les actions ni sur les cryptos : il ne compte pas"), "{}", headline(&g));
    assert!(!headline(&g).contains("Attention"));
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
    let b: Value = serde_json::from_slice(&to_bytes(r.into_body(), 1 << 25).await.unwrap()).unwrap();
    assert_eq!(b["headline"], expected["headline"]);
    assert_eq!(b["groups"][0]["buy"]["signals"], expected["groups"][0]["buy"]["signals"]);
    assert_eq!(b["groups"][0]["candidates"].as_array().unwrap().len(), CANDIDATES.len());
    let r = app.oneshot(Request::get("/api/bot/views?symbols=BAD!:crypto").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
}

/// Long histories, live: bitcoin on Bitstamp since 2011 (several pages), Coca-Cola on Yahoo over 20 years.
#[tokio::test]
#[ignore]
async fn bot_history_live() {
    let now = altim::js::now_ms();
    let btc = altim::bot_history::bitstamp_history("BTC", now).await.unwrap();
    println!("BTC Bitstamp {} bougies depuis {}", btc.len(), altim::js::iso_date(btc[0].time));
    assert!(btc.len() > 5000 && btc.windows(2).all(|w| w[1].time > w[0].time));
    let ko = altim::bot_history::stock_history("KO", now).await.unwrap();
    println!("KO Yahoo {} bougies depuis {}", ko.len(), altim::js::iso_date(ko[0].time));
    assert!(ko.len() > 4500);
    let (sol, src) = altim::bot_history::long_history("SOL", Kind::Crypto, now).await.unwrap();
    println!("SOL {} {} bougies", src, sol.len());
    assert!(sol.len() > 1500);
}

/// Real run on the whole universe; `ALTIM_SAVE_SAMPLE=1` rewrites tests/samples/bot.json with it.
#[tokio::test]
#[ignore]
async fn bot_live() {
    let r = altim::app::bot::compute().await.unwrap();
    println!("{}", r.headline);
    println!("{:?}", r.timing);
    for g in &r.groups {
        println!("{}", g.text);
        println!("  univers {:?}", g.universe);
        for c in &g.candidates {
            println!(
                "  {:?} choisi {}/{} : achats {} excès {:?} t {:?} ({:?}) ; ventes {} évité {:?} t {:?} ({:?}) ; Brier {:?}/{:?}",
                c.id,
                c.chosen_blocks,
                c.trained_blocks,
                c.stats.buy.signals,
                c.stats.buy.excess,
                c.stats.buy.t_stat,
                c.stats.buy.verdict,
                c.stats.sell.signals,
                c.stats.sell.avoided,
                c.stats.sell.t_stat,
                c.stats.sell.verdict,
                c.stats.brier_skill_up,
                c.stats.brier_skill_down
            );
        }
    }
    for f in r.failures.iter().chain(&r.extra_failures) {
        println!("  échec {} : {}", f.symbol, f.error);
    }
    if std::env::var("ALTIM_SAVE_SAMPLE").is_ok_and(|v| v == "1") {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/bot.json");
        std::fs::write(path, serde_json::to_string_pretty(&altim::js::to_value(&r)).unwrap() + "\n").unwrap();
    }
    assert!(r.assets.len() >= BASKET.len() / 2);
}

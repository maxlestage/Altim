//! « Bot Altim » v3 (`engine::bot_v3`): the corrected threshold, peers ranks and quintiles without look-ahead, the
//! long trees' early stopping, the economic choice, the peers evaluation by hand, the volatility-managed trend, the
//! forward test's split, the whole run on saved histories (deterministic, v2's selection unchanged), the decision
//! view, the saved real reports (v3, v2 and v1 still decoded). The synthetic benchmark (time and memory at the real
//! size) is ignored by default.
mod common;

use altim::engine::bot::*;
use altim::engine::bot_v3::{self, *};
use altim::engine::signal::sanitize;
use altim::engine::validation::{BASKET, Verdict};
use altim::types::{Candle, DAY_MS, Kind};

/// Deterministic pseudo-random numbers in [0, 1).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        let (a, b) = (self.next().max(1e-12), self.next());
        (-2.0 * a.ln()).sqrt() * (2.0 * std::f64::consts::PI * b).cos()
    }
}

/// A random walk of daily candles from `start` (ms), weekdays only for stocks.
fn walk(start: i64, end: i64, stock: bool, vol: f64, seed: u64) -> Vec<Candle> {
    let mut rng = Lcg(seed);
    let mut out = Vec::new();
    let mut p = 50.0 + 50.0 * rng.next();
    let mut t = start;
    while t < end {
        let wd = (t / DAY_MS + 4).rem_euclid(7); // 0 = Sunday
        if !stock || (1..=5).contains(&wd) {
            let open = p;
            p *= (vol * rng.normal() + 0.0002).exp();
            let (hi, lo) = (open.max(p) * (1.0 + 0.3 * vol * rng.next()), open.min(p) * (1.0 - 0.3 * vol * rng.next()));
            out.push(Candle {
                time: t + if stock { 14 * 3_600_000 } else { 0 },
                open,
                high: hi,
                low: lo,
                close: p,
                volume: 1e6 * (0.5 + rng.next()),
            });
        }
        t += DAY_MS;
    }
    out
}

/// Synthetic histories at the real size: every stock of the universe since 1990 (a few listed later), SPY since
/// 1993, cryptos since 2011-2020.
fn synthetic_universe(end: i64) -> Vec<History<'static>> {
    let y = |year: i64| (year - 1970) * 36525 * DAY_MS / 100;
    let mut out = Vec::new();
    for (k, a) in BASKET.iter().chain(EXTRA.iter()).enumerate() {
        let stock = a.kind == Kind::Stock;
        let start = match (stock, a.symbol) {
            (true, "SPY") => y(1993) + 29 * DAY_MS,
            (true, _) => y(1990 + (k % 7 == 0) as i64 * (k as i64 % 20)),
            (false, "BTC") => y(2011) + 200 * DAY_MS,
            (false, _) => y(2014 + (k as i64 % 7)),
        };
        let extra = BASKET.iter().position(|b| std::ptr::eq(b, a)).is_none();
        out.push(History {
            asset: a,
            candles: walk(start, end, stock, if stock { 0.018 } else { 0.045 }, 1000 + k as u64),
            source: "synthétique".into(),
            extra,
        });
    }
    out
}

fn rss(key: &str) -> f64 {
    altim::app::bot::memory_mb(key).unwrap_or(0.0)
}

/// Time and peak memory of the whole v3 run on one thread at the real size (synthetic prices: no result is read).
#[test]
#[ignore]
fn v3_synthetic_benchmark() {
    let end = 1_790_690_207_853;
    let h = synthetic_universe(end);
    println!("historiques : {} actifs, {} bougies ; RSS {} Mo", h.len(), h.iter().map(|x| x.candles.len()).sum::<usize>(), rss("VmRSS:"));
    let t = std::time::Instant::now();
    let threads = std::env::var("ALTIM_BOT_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let r = bot_v3::run(h, vec![], end, "synthétique", threads);
    let v3 = r.v3.as_ref().unwrap();
    println!(
        "calcul {:.0} s, pic {} Mo, lignes max {} ({} octets), blocs {}",
        t.elapsed().as_secs_f64(),
        rss("VmHWM:"),
        v3.compute.max_rows,
        v3.compute.row_bytes,
        v3.compute.blocks
    );
    for g in &v3.groups {
        for x in &g.horizons {
            println!("  {:?} {} j : arbres longs {:?} / {:?} / {:?}", g.id, x.horizon, x.rounds_up, x.rounds_down, x.rounds_peers);
        }
    }
}

fn long(symbol: &str) -> Vec<Candle> {
    sanitize(&common::find(symbol, "long").unwrap().candles)
}

fn scaled(c: &[Candle], k: f64) -> Vec<Candle> {
    c.iter().map(|x| Candle { open: x.open * k, high: x.high * k, low: x.low * k, close: x.close * k, ..*x }).collect()
}

#[test]
fn corrected_threshold() {
    let k = k_count();
    assert_eq!((k.v1, k.v2, k.v3, k.total), (4, 20, 90, 114));
    let t = t_required(k.total);
    assert!((t - 3.5157).abs() < 1e-3, "{t}");
    assert!((norm_quantile(0.975) - 1.959964).abs() < 1e-5);
    assert!((norm_quantile(0.025) + 1.959964).abs() < 1e-5);
    assert!((norm_quantile(1.0 - 0.025 / 4.0) - 2.4977).abs() < 1e-3);
    assert!((norm_quantile(1e-6) + 4.753424).abs() < 1e-4);
    assert_eq!(verdict_at(40, Some(3.4), Some(1.0), t), Verdict::Unproven);
    assert_eq!(verdict_at(40, Some(3.6), Some(1.0), t), Verdict::Edge);
    assert_eq!(verdict_at(40, Some(3.6), Some(-1.0), t), Verdict::Unproven, "t sans gain moyen positif");
    assert_eq!(verdict_at(40, Some(-3.6), Some(-1.0), t), Verdict::Negative);
    assert_eq!(verdict_at(29, Some(9.0), Some(1.0), t), Verdict::Insufficient);
    assert_eq!(verdict_at(40, Some(2.1), Some(1.0), 2.0), Verdict::Edge);
    // Labels: v2's text with t ≥ 2, the corrected one otherwise.
    assert_eq!(buy_verdict_label_t(Verdict::Edge, 2.0), buy_verdict_label(Verdict::Edge));
    assert_eq!(sell_verdict_label_t(Verdict::Negative, 2.0), sell_verdict_label(Verdict::Negative));
    assert_eq!(buy_verdict_label_t(Verdict::Edge, t), "Mieux qu'une entrée au hasard (t ≥ 3,5 par jour, seuil corrigé), à confirmer");
}

#[test]
fn peers_ranks_labels_and_quintiles() {
    assert_eq!(pct_ranks(&[3.0, 1.0, 2.0, 2.0, 5.0]), vec![0.75, 0.0, 0.375, 0.375, 1.0]);
    assert_eq!(pct_ranks(&[7.0]), vec![0.5]);
    assert_eq!(pct_rank_among(2.0, &[1.0, 3.0]), 0.5);
    assert_eq!(pct_rank_among(9.0, &[1.0, 3.0]), 1.0);
    assert_eq!(pct_rank_among(3.0, &[1.0, 3.0]), 0.75);
    // 6 assets on day 0 (labelled), 3 on day 1 (not ranked), 5 on day 2 with 4 labels (ranked, no median).
    let row = |asset: usize, day: i64, v: f32, gross: Option<f64>| Row {
        asset,
        time: day * DAY_MS,
        x: std::array::from_fn(|j| v + j as f32),
        trend: 0,
        train: true,
        fwd: gross.map(|g| Forward {
            exit_time: (day + 21) * DAY_MS + asset as i64,
            entry: 100.0,
            exit: 100.0 + g,
            gross: g,
            net: g - 0.3,
            drawdown: 0.0,
            up: g > 0.3,
            down: g < -0.3,
        }),
        day: None,
    };
    let mut rows = Vec::new();
    for a in 0..6 {
        rows.push(row(a, 0, a as f32 * 10.0, Some(a as f64)));
        if a < 3 {
            rows.push(row(a, 1, 1.0, Some(1.0)));
        }
        if a < 5 {
            rows.push(row(a, 2, -(a as f32), (a < 4).then_some(a as f64)));
        }
    }
    rows.sort_by_key(|r| (r.asset, r.time));
    let before = rows.clone();
    let xs = to_peers(&mut rows);
    for (i, r) in rows.iter().enumerate() {
        let d = r.time / DAY_MS;
        assert_eq!(xs[i].ranked, d != 1, "jour {d}");
        if d == 1 {
            assert_eq!(r.x, before[i].x, "jour sans assez de pairs : mesures brutes");
        }
        if d == 0 {
            // Ranks of the asset-specific columns; regime dummies and market columns stay raw.
            assert_eq!(r.x[0], r.asset as f32 / 5.0);
            assert_eq!(r.x[16], r.asset as f32 / 5.0);
            assert_eq!(r.x[12], before[i].x[12]);
            assert_eq!(r.x[23], before[i].x[23]);
            // Median of 0..5 = 2.5: assets 3, 4, 5 beat it; known when the last peer's label ends.
            let l = xs[i].label.unwrap();
            assert_eq!(l.beat, r.asset >= 3);
            assert_eq!(l.med_net, 2.2);
            assert_eq!(l.known, 21 * DAY_MS + 5);
        } else {
            assert!(xs[i].label.is_none(), "moins de 5 résultats ce jour-là");
        }
        if d == 2 {
            // Ranked on the day's 5 values only (lower value for a higher asset).
            assert_eq!(r.x[0], (4 - r.asset) as f32 / 4.0);
        }
    }
    // No look-ahead: another day's rows never change a day's peers features.
    let mut changed = before.clone();
    for r in changed.iter_mut().filter(|r| r.time == 2 * DAY_MS) {
        r.x = [99.0; 24];
    }
    let xs2 = to_peers(&mut changed);
    for (i, r) in changed.iter().enumerate().filter(|(_, r)| r.time == 0) {
        assert_eq!(r.x, rows[i].x);
        assert_eq!(xs2[i].label, xs[i].label);
    }
    // Quintiles: 11 assets → ⌊11/5⌋ = 2 each side; ties by the universe order; under 5 assets: ATTENDRE.
    let items: Vec<(usize, i64, usize, f64)> = (0..11).map(|a| (a, 0, a, if a < 3 { 5.0 } else { a as f64 / 100.0 })).collect();
    let q = quintile_actions(&items);
    assert_eq!((q[&0], q[&1], q[&2]), (BotAction::Buy, BotAction::Buy, BotAction::Wait));
    assert_eq!((q[&3], q[&4], q[&5]), (BotAction::Sell, BotAction::Sell, BotAction::Wait));
    let few: Vec<(usize, i64, usize, f64)> = (0..4).map(|a| (a, 1, a, a as f64)).collect();
    assert!(quintile_actions(&few).values().all(|a| *a == BotAction::Wait));
    // The cut-offs of a day for an asset outside the universe.
    assert_eq!(peers_action(0.9, Some(0.8), Some(0.2)), Some(BotAction::Buy));
    assert_eq!(peers_action(0.2, Some(0.8), Some(0.2)), Some(BotAction::Sell));
    assert_eq!(peers_action(0.5, Some(0.8), Some(0.2)), Some(BotAction::Wait));
    assert_eq!(peers_action(0.5, None, Some(0.2)), None);
    let reference: Vec<(String, Features)> = (0..4).map(|k| (format!("S{k}"), [k as f32; 24])).collect();
    let f = peers_features(&[1.5; 24], &reference, "X");
    assert_eq!((f[0], f[12]), (0.5, 1.5));
    // Its own entry is replaced, not counted twice.
    let f = peers_features(&[3.0; 24], &reference, "S3");
    assert_eq!(f[0], 1.0);
}

fn synthetic_rows(assets: usize, days: usize, seed: u64, signal: bool) -> Vec<Row> {
    let mut rng = Lcg(seed);
    let mut out = Vec::new();
    for a in 0..assets {
        for d in 0..days {
            let x: Features = std::array::from_fn(|_| (rng.next() - 0.5) as f32);
            let gross = ((if signal { x[20] as f64 * 3.0 } else { 0.0 }) + rng.next() - 0.5) * 10.0;
            let fwd = (d + 21 < days).then(|| Forward {
                exit_time: (d + 21) as i64 * DAY_MS,
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
            out.push(Row { asset: a, time: d as i64 * DAY_MS, x, trend, train: d % TRAIN_STRIDE == 0, fwd, day: Some((gross / 20.0) as f32) });
        }
    }
    out
}

#[test]
fn long_trees_stop_early_without_reading_the_validation() {
    let mut rng = Lcg(5);
    let mk = |n: usize, rng: &mut Lcg| -> (Vec<Features>, Vec<bool>) {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for _ in 0..n {
            let x: Features = std::array::from_fn(|_| rng.next() as f32);
            ys.push((x[3] > 0.5) ^ (rng.next() < 0.3));
            xs.push(x);
        }
        (xs, ys)
    };
    let (tx, ty) = mk(3000, &mut rng);
    let (vx, vy) = mk(1000, &mut rng);
    let t: Vec<&[f32]> = tx.iter().map(|x| &x[..]).collect();
    let v: Vec<&[f32]> = vx.iter().map(|x| &x[..]).collect();
    let (f, stop) = altim::engine::bot_trees::Forest::fit_params(&t, &ty, &ALL_COLS, &altim::engine::bot_trees::LONG, Some((&v, &vy)));
    let stop = stop.unwrap();
    assert!(stop.rounds >= 1 && stop.rounds < 1000, "{stop:?}");
    assert_eq!(stop.computed, stop.rounds + altim::engine::bot_trees::PATIENCE, "arrêt 50 arbres après le meilleur");
    assert_eq!(f.trees.len(), stop.rounds);
    assert!(f.trees.iter().all(|x| x.len() <= 31), "profondeur 4 au plus");
    // The validation rows only choose the count: the same trees as a fit with that many rounds and no validation.
    let p = altim::engine::bot_trees::Params { trees: stop.rounds, ..altim::engine::bot_trees::LONG };
    assert_eq!(altim::engine::bot_trees::Forest::fit_params(&t, &ty, &ALL_COLS, &p, None).0, f);
    // v2's fit is unchanged: 100 trees, no stop.
    let v2 = altim::engine::bot_trees::Forest::fit(&t, &ty, &ALL_COLS);
    assert_eq!(v2.trees.len(), 100);
    assert_eq!(altim::engine::bot_trees::Forest::fit_params(&t, &ty, &ALL_COLS, &altim::engine::bot_trees::V2, None).0, v2);
    // Row cap: an even stride.
    let many: Vec<usize> = (0..100_001).collect();
    let c = cap_rows(&many);
    assert!(c.len() <= MAX_TREE_ROWS && c.len() > MAX_TREE_ROWS / 2, "{}", c.len());
    assert_eq!((c[0], c[1] - c[0]), (0, 3));
    assert_eq!(cap_rows(&many[..10]).len(), 10);
}

#[test]
fn economic_choice_and_signals_by_hand() {
    let s = |id, score: Option<f64>, signals| EconScore { id, score, signals };
    let all = |_: V3Candidate| true;
    use V3Candidate::*;
    assert_eq!(
        choose(&[s(Trend, Some(0.1), 40), s(Logit, Some(0.3), 40), s(Trees, Some(0.5), 20)], all, Trend),
        Some(Logit),
        "20 signaux : pas éligible"
    );
    assert_eq!(choose(&[s(Trend, Some(0.3), 40), s(Logit, Some(0.3), 40)], all, Trend), Some(Trend), "égalité : le plus simple");
    assert_eq!(choose(&[s(Trend, None, 0), s(Logit, Some(-0.3), 40)], all, Trend), Some(Logit), "même négatif, le meilleur éligible");
    assert_eq!(choose(&[s(Logit, Some(0.3), 10)], all, XsMomentum), Some(XsMomentum), "aucun éligible : la règle");
    assert_eq!(choose(&[s(Logit, Some(0.3), 40)], |c| c != Logit, Trend), Some(Trend), "non ajusté sur toute la fenêtre");
    // A signals: non-overlapping per asset, excess vs the same asset's mean in the window.
    let rows = synthetic_rows(2, 60, 3, false);
    let w: Vec<&Row> = rows.iter().filter(|r| r.fwd.is_some()).collect();
    let sig = a_signals(&w, |_| BotAction::Buy);
    // Days 0, 21 (label ends day 21: the next signal can start then) per asset: 39 labelled days → 0, 21.
    assert_eq!(sig.len(), 4);
    let base0 = w.iter().filter(|r| r.asset == 0).map(|r| r.fwd.unwrap().net).sum::<f64>() / 39.0;
    assert!((sig[0].1 - (rows[0].fwd.unwrap().net - base0)).abs() < 1e-9);
    assert_eq!((sig[0].0, sig[1].0), (0, 21));
    assert!(a_signals(&w, |_| BotAction::Wait).is_empty());
}

#[test]
fn peers_evaluation_by_hand() {
    // 5 assets, one labelled day: asset k makes k² % (right-skewed), net = k² − 0.3 %; median net 3.7 %, mean 5.7 %.
    let mut rows = Vec::new();
    for a in 0..5usize {
        rows.push(Row {
            asset: a,
            time: 0,
            x: [a as f32; 24],
            trend: 0,
            train: false,
            fwd: Some(Forward {
                exit_time: 21 * DAY_MS,
                entry: 100.0,
                exit: 100.0 + (a * a) as f64,
                gross: (a * a) as f64,
                net: (a * a) as f64 - 0.3,
                drawdown: 0.0,
                up: true,
                down: false,
            }),
            day: None,
        });
    }
    let xs = to_peers(&mut rows);
    let items: Vec<(usize, i64, usize, f64)> = (0..5).map(|i| (i, 0, i, rows[i].x[16] as f64)).collect();
    let q = quintile_actions(&items);
    let preds: Vec<Option<Prediction>> =
        (0..5).map(|i| Some(Prediction { up: 0.0, down: 0.0, base_up: 0.0, base_down: 0.0, action: q[&i], block: 0 })).collect();
    let s = evaluate_peers(&rows, &xs, &preds, |_| true, false);
    // Buy the best (asset 4: 15.7 − 3.7 = +12), sell the worst (asset 0: switching to the median gives 3.7 − 0).
    assert!((s.buy_excess[0] - 12.0).abs() < 1e-5, "{:?}", s.buy_excess);
    assert!((s.sell_avoided[0] - 3.7).abs() < 1e-5);
    // Against the mean (the control): 15.7 − 5.7 = +10 and 5.7 − 0; a random pick beats the median on average
    // (every asset's net minus the median: 0 − 3.7, 0.7 − 3.7, … sums to 5 × (5.7 − 3.7) = +10 > 0), not the mean.
    let m = evaluate_peers(&rows, &xs, &preds, |_| true, true);
    assert!((m.buy_excess[0] - 10.0).abs() < 1e-5 && (m.sell_avoided[0] - 5.7).abs() < 1e-5);
    let l = xs[0].label.unwrap();
    assert!((l.med_net - 3.7).abs() < 1e-6 && (l.mean_net - 5.7).abs() < 1e-6);
    assert_eq!(s.sell_fell, vec![true]);
    assert_eq!(s.wait_net.len(), 3);
    assert!(s.up_pairs.is_empty(), "la règle n'a pas de probabilité");
    let st = ConfigStats::of(&s, 3.5, (Some(0), Some(0)), false);
    assert_eq!((st.buy.signals, st.buy.excess, st.buy.beat_share), (1, Some(12.0), Some(100.0)));
    assert_eq!(st.buy.verdict, Some(Verdict::Insufficient));
    assert!(st.exit.is_none() && st.buy_vs_mean.is_none());
    let both = st.with_mean(ConfigStats::of(&m, 3.5, (Some(0), Some(0)), false));
    assert_eq!(both.buy_vs_mean.as_ref().unwrap().excess, Some(10.0));
    // Proven only when both references say so.
    let mut p = both.clone();
    p.buy.verdict = Some(Verdict::Edge);
    assert!(!p.proven(false));
    p.buy_vs_mean.as_mut().unwrap().verdict = Some(Verdict::Edge);
    assert!(p.proven(false) && !p.proven(true));
}

#[test]
fn walk_forwards_purge_and_keep_v2_unchanged() {
    let r = synthetic_rows(6, 1500, 11, true);
    let aw = a_walk(&r, 20, 1);
    // v2's selection inside v3 is v2's walk-forward, unchanged.
    let v2 = walk_forward(&r, 1);
    assert_eq!(aw.blocks, v2.blocks);
    assert_eq!(aw.v2_nested(&r, 3), v2.nested());
    for c in CANDIDATES {
        assert_eq!(aw.slot(&r, 2, c.index()), v2.preds[c.index()], "{c:?}");
    }
    assert_eq!(aw.wf(&r).preds[..4], v2.preds[..]);
    assert_eq!(a_walk(&r, 20, 4), aw, "même résultat sur plusieurs fils");
    // v3's choice: the eligible best score (or the rule), and only fitted candidates.
    for (k, ch) in aw.choices.iter().enumerate() {
        let c = ch.chosen.unwrap();
        let eligible: Vec<&EconScore> = ch.scores.iter().filter(|s| s.score.is_some() && s.signals >= MIN_VAL_SIGNALS).collect();
        if let Some(best) = eligible.iter().max_by(|a, b| a.score.unwrap().total_cmp(&b.score.unwrap())) {
            assert!((ch.scores.iter().find(|s| s.id == c).unwrap().score.unwrap() - best.score.unwrap()).abs() < 1e-9, "{k}");
        }
        assert!(aw.slot(&r, 1, c.a_slot().unwrap()).iter().flatten().any(|p| p.block == k as u32));
    }
    // The long trees need two windows before the block: fitted from the third block on here.
    assert!(aw.choices[0].rounds.0.is_none() && aw.choices.iter().skip(2).all(|c| c.rounds.0.is_some() && c.rounds.1.is_some()));
    // Nothing known after a block's cutoff (labels ending later, rows after the block) changes its choice or predictions.
    let k = 2;
    let (cutoff, end) = (aw.blocks[k].cutoff, aw.blocks[k].end);
    let mut changed = r.clone();
    let mut rng = Lcg(99);
    for x in changed.iter_mut() {
        if let Some(f) = x.fwd.as_mut().filter(|f| f.exit_time >= cutoff) {
            f.gross = -f.gross;
            f.net = -f.net;
            f.up = !f.up;
            f.down = !f.down;
        }
        if x.time >= end {
            x.x = std::array::from_fn(|_| (rng.next() * 50.0) as f32);
        }
    }
    let again = a_walk(&changed, 20, 1);
    assert_eq!(again.choices[k], aw.choices[k]);
    for slot in 0..A_CANDS.len() {
        let (a, b) = (again.slot(&changed, 1, slot), aw.slot(&r, 1, slot));
        for (i, x) in r.iter().enumerate() {
            if x.time >= aw.blocks[k].start && x.time < end {
                assert_eq!(a[i], b[i]);
            }
        }
    }
    // Horizon 60: first block after 504 + 60 + 60 dates, v2's selection with a 60-day purge.
    let a60 = a_walk(&r, 60, 1);
    assert_eq!(a60.blocks[0].start, first_test(60) as i64 * DAY_MS);
    assert_eq!(a60.blocks[0].start - a60.blocks[0].cutoff, 60 * DAY_MS);
    // Family B on peers rows: same blocks, the same determinism and the same purge.
    let mut p = r.clone();
    let xs = to_peers(&mut p);
    let bw = b_walk(&p, &xs, 20, 1);
    assert_eq!(bw.starts, aw.blocks.iter().map(|b| b.start).collect::<Vec<_>>());
    assert_eq!(b_walk(&p, &xs, 20, 3), bw);
    assert!(bw.choices.iter().all(|c| c.chosen.is_some()));
    // Each day, 6 assets → one ACHETER and one VENDRE per candidate.
    let mom = bw.slot(&p, &xs, 2, 0);
    let per_day = mom.iter().flatten().filter(|x| x.action == BotAction::Buy).count();
    assert_eq!(per_day, mom.iter().flatten().count() / 6);
    let nested = bw.nested(&p, &xs, 1);
    for (i, x) in nested.iter().enumerate() {
        if let Some(x) = x {
            let c = bw.choices[x.block as usize].chosen.unwrap();
            assert_eq!(Some(*x), bw.slot(&p, &xs, 1, c.b_slot().unwrap())[i]);
            break;
        }
    }
    let mut pc = changed.clone();
    let xsc = to_peers(&mut pc);
    let bagain = b_walk(&pc, &xsc, 20, 1);
    assert_eq!(bagain.choices[k], bw.choices[k]);
}

#[test]
fn volatility_managed_trend_by_hand() {
    let c = long("AAPL");
    let from = c[600].time;
    let d = vol_managed_days(&c, Kind::Stock, from);
    assert_eq!(d.len(), c.len() - 600 - 2);
    for (k, x) in d.iter().enumerate() {
        let i = 600 + k;
        assert!([0.0, 0.25, 0.5, 0.75, 1.0].contains(&x.3), "{}", x.3);
        let sma = c[i - 199..=i].iter().map(|x| x.close).sum::<f64>() / 200.0;
        if c[i].close <= sma {
            assert_eq!(x.3, 0.0);
        }
        assert!((x.1 - (c[i + 2].open / c[i + 1].open - 1.0)).abs() < 1e-12);
        // Managed return: exposure × the day's return − the cost of a change.
        let prev = if k == 0 { None } else { Some(d[k - 1].3) };
        if prev == Some(x.3) {
            assert!((x.2 - x.3 * x.1).abs() < 1e-12);
        }
    }
    // No look-ahead: cutting the future leaves the past days unchanged.
    let cut = vol_managed_days(&c[..900], Kind::Stock, from);
    assert_eq!(&d[..cut.len()], &cut[..]);
    let s = series_stats(&[0.01, -0.02, 0.03], &[1.0, 1.0, 1.0], 252.0);
    assert_eq!(s.days, 3);
    assert!((s.max_drawdown.unwrap() + 2.0).abs() < 1e-9);
    assert!((s.total_return.unwrap() - ((1.01 * 0.98 * 1.03 - 1.0) * 100.0)).abs() < 0.01);
    assert!(s.sharpe.unwrap() > 0.0);
}

/// The labels of another horizon from the prices kept are those of rows built at that horizon (the candles can go).
#[test]
fn relabel_matches_rows_built_at_the_horizon() {
    let mut h = saved_universe();
    let _ = prepare(&mut h, vec![]);
    let kinds: Vec<Kind> = h.iter().map(|x| x.asset.kind).collect();
    for g in [BotGroup::Stock, BotGroup::Crypto] {
        let mut d = group_rows(g, &h, 20, None).unwrap();
        let prices: std::collections::HashMap<usize, Prices> = d.spans.keys().map(|k| (*k, Prices::of(&h[*k].candles))).collect();
        let built = group_rows(g, &h, 60, None).unwrap();
        relabel(&mut d.rows, &prices, &kinds, 60);
        assert_eq!(d.rows, built.rows, "{g:?}");
        relabel(&mut d.rows, &prices, &kinds, 20);
        assert_eq!(d.rows, group_rows(g, &h, 20, None).unwrap().rows);
        assert!(d.asset_years.values().all(|(y, f)| y.is_some() && f.is_some()));
    }
}

fn saved_universe() -> Vec<History<'static>> {
    let b = |s: &str| BASKET.iter().find(|x| x.symbol == s).unwrap();
    let e = |s: &str| EXTRA.iter().find(|x| x.symbol == s).unwrap();
    let mut v: Vec<History<'static>> = ["BTC", "ETH", "SOL", "DOGE", "AAPL", "NVDA", "SPY"]
        .iter()
        .map(|s| History { asset: b(s), candles: long(s), source: "test".into(), extra: false })
        .collect();
    v.push(History { asset: e("ORCL"), candles: scaled(&long("NVDA"), 0.7), source: "test".into(), extra: true });
    v.push(History { asset: e("CSCO"), candles: scaled(&long("AAPL"), 1.3), source: "test".into(), extra: true });
    v.push(History { asset: e("XLM"), candles: scaled(&long("ETH"), 0.01), source: "test".into(), extra: true });
    v
}

/// The whole v3 on the real saved histories (5 stocks, 5 cryptos, 3 of them extra): deterministic, the contract's
/// shape, v2's selection in the v2-shaped fields at the corrected threshold, the decision view.
#[test]
fn run_on_saved_histories() {
    let now = common::now();
    let r = bot_v3::run(saved_universe(), vec![], now, "test", 2);
    let v3 = r.v3.as_ref().unwrap();
    assert_eq!(r.version, 3);
    assert_eq!(v3.prereg_date, "2026-09-30");
    assert_eq!(v3.k.total, 114);
    assert!((v3.t_required - 3.5157).abs() < 1e-3);
    assert!(v3.headline.starts_with("Bot v3 : "), "{}", v3.headline);
    assert_eq!(r.headline, v3.headline);
    assert!(v3.forward_headline.starts_with("Aucun signal daté après le 30/09/2026"), "{}", v3.forward_headline);
    assert_eq!(v3.candidates.len(), 8);
    assert_eq!(v3.groups.iter().map(|g| g.id).collect::<Vec<_>>(), [BotGroup::Stock, BotGroup::Crypto]);
    for g in &v3.groups {
        assert_eq!(g.horizons.iter().map(|h| h.horizon).collect::<Vec<_>>(), [20, 60]);
        assert!(g.peers_from.is_some(), "{:?}", g.id);
        let vm = g.vol_managed.as_ref().unwrap();
        assert!(vm.assets > 0 && vm.hold.days > 0 && vm.managed.sharpe.is_some() && !vm.text.is_empty());
        for h in &g.horizons {
            let ids: Vec<&str> = h.configs.iter().map(|c| c.id.as_str()).collect();
            assert_eq!(ids, ["absolute", "peers", "v2", "trend", "v1", "logit", "trees", "treesLong", "xsMomentum", "xsLogit", "xsTrees"]);
            assert_eq!(h.configs.iter().filter(|c| c.headline).count(), 2);
            assert_eq!(h.selection.len(), h.blocks);
            for c in &h.configs {
                // Everything is dated before the forward test here; the extra universe only for the nested ones.
                assert_eq!(c.forward.test_rows, 0, "{}", c.id);
                assert_eq!(c.extra.is_some(), c.nested, "{}", c.id);
                assert!(c.main.buy.signals + c.main.sell.signals <= c.main.labelled.max(1) * 2);
                if let (Some(m), Some(b), Some(e)) = (c.main.buy.mean, c.main.buy.baseline, c.main.buy.excess) {
                    assert!((m - b - e).abs() < 0.02, "{} {m} {b} {e}", c.id);
                }
                for s in [&c.main.buy, &c.main.sell] {
                    if s.signals >= 30 && s.t.is_some_and(|t| t.abs() < 3.5) {
                        assert_ne!(s.verdict, Some(Verdict::Edge));
                    }
                }
            }
            let chosen: usize = h.configs.iter().filter(|c| c.family == Family::Absolute && c.candidate.is_some()).map(|c| c.chosen_blocks).sum();
            assert_eq!(chosen, h.selection.iter().filter(|b| b.absolute.is_some()).count());
        }
    }
    // The v2-shaped fields: v2's four candidates, the verdict labels of the corrected threshold.
    for g in &r.groups {
        assert_eq!(g.candidates.len(), CANDIDATES.len());
        assert!(g.model.is_some());
    }
    for a in &r.assets {
        let n = a.v3.as_ref().unwrap_or_else(|| panic!("{}", a.symbol));
        assert!(n.absolute20.is_some() && n.absolute60.is_some(), "{}", a.symbol);
        assert!(n.peers20.is_some(), "{}", a.symbol);
    }
    // Deterministic whatever the threads.
    let again = bot_v3::run(saved_universe(), vec![], now, "test", 1);
    assert_eq!(again.v3, r.v3);
    assert_eq!(again.groups, r.groups);
    // JSON: the live models stay in memory; the rest reads back.
    let json = serde_json::to_string(&r).unwrap();
    let back: BotReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back.v3, r.v3);
    assert!(back.v3_live.is_none());

    // Today's view: the 4 headline configurations; none counts without an edge at the corrected threshold.
    let m = |s: &str| if ["BTC", "ETH", "SOL", "DOGE"].contains(&s) { long("BTC") } else { long("SPY") };
    for (s, kind) in [("AAPL", Kind::Stock), ("BTC", Kind::Crypto), ("PEPE", Kind::Crypto)] {
        let candles = if s == "PEPE" { long("DOGE") } else { long(s) };
        let v = bot_view(Some(&r), s, kind, &candles, &m(s), now);
        let v3v = v.v3.as_ref().unwrap();
        assert!(v3v.available);
        assert_eq!(v3v.signals.len(), 4, "{s}");
        let any_edge = v3v.signals.iter().any(|x| x.counts);
        assert_eq!(v.counts, any_edge);
        if !any_edge {
            assert!(v.line(true).is_none() && v.nudge() == 0.0);
            assert!(v.note.starts_with("Aucun avantage démontré au seuil corrigé"), "{}", v.note);
        }
    }
    // A report read back from JSON has no live models: said, never counted.
    let v = bot_view(Some(&back), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now);
    assert!(!v.v3.as_ref().unwrap().available && !v.counts && v.nudge() == 0.0);

    // Forced: an edge on the peers' buy side at 20 days → counts +3 when today's peers action is ACHETER; a
    // forward test with ≥ 30 signals and a negative excess contradicts it.
    let mut forced = r.clone();
    let stock_now = bot_view(Some(&r), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now).v3.unwrap();
    let peers20 = stock_now.signals.iter().find(|x| x.family == Family::Peers && x.horizon == 20).unwrap().action.unwrap();
    {
        let v3 = forced.v3.as_mut().unwrap();
        let c = v3.groups[0].horizons[0].configs.iter_mut().find(|c| c.id == "peers").unwrap();
        let s = if peers20 == BotAction::Sell { &mut c.main.sell } else { &mut c.main.buy };
        s.verdict = Some(Verdict::Edge);
        // Proven against the median only: does not count (the control against the mean must agree).
    }
    let v = bot_view(Some(&forced), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now);
    assert!(!v.counts, "prouvé face à la médiane seulement : ne compte pas");
    {
        let v3 = forced.v3.as_mut().unwrap();
        let c = v3.groups[0].horizons[0].configs.iter_mut().find(|c| c.id == "peers").unwrap();
        let m = if peers20 == BotAction::Sell { c.main.sell_vs_mean.as_mut() } else { c.main.buy_vs_mean.as_mut() };
        m.unwrap().verdict = Some(Verdict::Edge);
    }
    let v = bot_view(Some(&forced), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now);
    match peers20 {
        BotAction::Buy => {
            assert!(v.counts && v.nudge() == bot_v3::NUDGE);
            assert!(v.line(false).unwrap().0);
        }
        BotAction::Sell => {
            assert!(v.counts && v.nudge() == -bot_v3::NUDGE);
            assert!(v.line(false).is_none(), "VENDRE entre pairs : ligne contre seulement pour qui détient");
            assert!(v.line(true).unwrap().1.contains("alléger"));
        }
        BotAction::Wait => assert!(!v.counts),
    }
    if peers20 != BotAction::Wait {
        let v3 = forced.v3.as_mut().unwrap();
        let c = v3.groups[0].horizons[0].configs.iter_mut().find(|c| c.id == "peers").unwrap();
        let f = if peers20 == BotAction::Sell { &mut c.forward.sell } else { &mut c.forward.buy };
        f.signals = 30;
        f.excess = Some(-0.1);
        let v = bot_view(Some(&forced), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now);
        assert!(!v.counts && v.nudge() == 0.0);
        assert!(v.v3.unwrap().signals.iter().any(|x| x.contradicted));
    }
}

fn load(name: &str) -> BotReport {
    let path = format!("{}/tests/samples/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The saved real v3 report follows the contract; the v2 and v1 ones still decode (no `v3`).
#[test]
fn saved_reports_decode() {
    let r = load("bot.json");
    assert_eq!(r.version, 3);
    let v3 = r.v3.as_ref().unwrap();
    assert_eq!((v3.prereg_date.as_str(), v3.k.total), ("2026-09-30", 114));
    assert_eq!(v3.groups.len(), 2);
    assert_eq!(v3.after_prereg.len(), AFTER_PREREG.len());
    for g in &v3.groups {
        assert_eq!(g.horizons.len(), 2);
        for h in &g.horizons {
            assert_eq!(h.configs.len(), 11);
            for c in &h.configs {
                // Family B carries the control against the group's mean; a side counts only when both agree.
                assert_eq!(c.main.buy_vs_mean.is_some(), c.family == Family::Peers, "{}", c.id);
                if c.main.proven(false) {
                    assert!(c.main.buy_vs_mean.as_ref().is_none_or(|m| m.t.unwrap() >= v3.t_required));
                }
            }
        }
    }
    assert_eq!(bot_v3::headline(&v3.groups, t_required(v3.k.total), v3.k.total), v3.headline);
    assert_eq!(r.assets.len() + r.failures.len(), BASKET.len());
    let t = r.timing.as_ref().unwrap();
    assert!(t.peak_rss_mb.is_some());
    let v2 = load("bot-v2.json");
    assert_eq!(v2.version, 2);
    assert!(v2.v3.is_none() && v2.assets.iter().all(|a| a.v3.is_none()));
    let v = bot_view(Some(&v2), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), common::now());
    assert!(v.available && v.v3.is_none());
    let v1 = load("bot-v1.json");
    assert_eq!(v1.version, 0);
    assert!(v1.v3.is_none());
}

/// Real run on the whole universe (v3 and v2's selection); `ALTIM_SAVE_SAMPLE=1` rewrites tests/samples/bot.json with
/// it. `ALTIM_BOT_THREADS=1` measures one core.
#[tokio::test]
#[ignore]
async fn bot_live() {
    let r = altim::app::bot::compute().await.unwrap();
    let v3 = r.v3.as_ref().unwrap();
    println!("{}", v3.headline);
    println!("{}", v3.forward_headline);
    println!("{:?} ; K = {:?}, t requis {:.3}", r.timing, v3.k, v3.t_required);
    println!("{:?}", v3.compute);
    let side =
        |s: &SideStats| format!("{} sig. exces {:?} t {:?} (actif {:?}) {:?}/{:?}", s.signals, s.excess, s.t, s.t_by_asset, s.verdict, s.raw_verdict);
    for g in &v3.groups {
        println!("== {} ; univers {:?} ; pairs depuis {:?}", g.label, g.universe, g.peers_from.map(altim::js::iso_date));
        for h in &g.horizons {
            println!(
                "  -- {} jours : test {:?} → {:?}, {} blocs ; arbres {:?} {:?} {:?}",
                h.horizon,
                h.test_from.map(altim::js::iso_date),
                h.test_to.map(altim::js::iso_date),
                h.blocks,
                h.rounds_up,
                h.rounds_down,
                h.rounds_peers
            );
            for c in &h.configs {
                println!(
                    "    {:<11} choisi {:>2}/{:>2} | achats {} | ventes {} | détention {:?} vs {:?} ({}/{})",
                    c.id,
                    c.chosen_blocks,
                    c.trained_blocks,
                    side(&c.main.buy),
                    side(&c.main.sell),
                    c.main.median_bot_return,
                    c.main.median_hold_return,
                    c.main.beat_hold,
                    c.main.hold_assets
                );
            }
        }
        if let Some(v) = &g.vol_managed {
            println!("  vol : {}", v.text);
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

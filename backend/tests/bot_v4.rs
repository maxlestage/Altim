//! « Bots sélectifs » (`engine::bot_v4`): Wilson interval and quantiles by hand, the gate chosen on the inner
//! validation only (silent when it does not beat the base rate or has under 30 signals), the verdict and status rules
//! (précis, en attente, contredit), the corrected threshold with K = 130, the whole run on saved histories
//! (deterministic, v3 unchanged), the decision view (counts only when proven and confirmed; ± 3 with v3, 0 when they
//! disagree) and the saved real report.
mod common;

use std::sync::Arc;

use altim::engine::bot::*;
use altim::engine::bot_v3;
use altim::engine::bot_v4::{self, *};
use altim::engine::signal::sanitize;
use altim::engine::validation::BASKET;
use altim::types::{Candle, Kind};

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

#[test]
fn wilson_quantiles_and_corrected_threshold() {
    let (lo, hi) = wilson(8, 10, 1.96).unwrap();
    assert!(close(lo, 0.490157, 1e-5) && close(hi, 0.943319, 1e-5), "{lo} {hi}");
    let (lo, hi) = wilson(60, 100, 1.96).unwrap();
    assert!(close(lo, 0.502001, 1e-5) && close(hi, 0.690600, 1e-5));
    assert_eq!(wilson(0, 5, 1.96).unwrap().0, 0.0);
    assert!(wilson(0, 0, 1.96).is_none());
    let v: Vec<f64> = (1..=100).map(|x| x as f64).collect();
    assert_eq!(upper_quantile(&v, 0.10), Some(91.0));
    assert_eq!(upper_quantile(&v, 0.01), Some(100.0));
    assert_eq!(upper_quantile(&[], 0.1), None);
    let k = k_count();
    assert_eq!((k.v1, k.v2, k.v3, k.v4, k.total), (4, 20, 90, 16, 130));
    assert!(close(t_required(k.total), 3.5504, 1e-3));
    assert_eq!(bot_v3::k_count().total + k.v4, k.total, "v3's own K (114) plus the 16 bots");
    assert_eq!(short_label("stock-20-rise"), "Hausse 20 j");
    assert_eq!(short_label("crypto-60-bottom"), "Bas du classement 60 j");
    assert_eq!(bot_id(BotGroup::Crypto, 60, Side::Top), "crypto-60-top");
}

/// W0 rows: one asset per row (no overlap), the two scores equal to `s`, a hit when `hit(s)`.
fn val(scores: &[f64], hit: impl Fn(f64) -> bool) -> Vec<ValRow> {
    scores.iter().enumerate().map(|(i, s)| ValRow { asset: i, time: 0, exit: 1, hit: hit(*s), s: [*s, *s] }).collect()
}

#[test]
fn gate_chosen_on_the_inner_validation_only() {
    // 2 000 rows; the top 10 % hit 80 % of the time, the rest 40 %: the gate opens at a level of the grid.
    let scores: Vec<f64> = (0..2000).map(|i| i as f64 / 2000.0).collect();
    let rows: Vec<ValRow> = scores
        .iter()
        .enumerate()
        .map(|(i, s)| ValRow { asset: i, time: 0, exit: 1, hit: if *s >= 0.9 { i % 5 != 0 } else { i % 5 < 2 }, s: [*s, *s] })
        .collect();
    let full: Vec<[f64; 2]> = scores.iter().map(|s| [s * 2.0, s * 3.0]).collect();
    let g = choose_gate(&rows, &full);
    assert_eq!(g.level, Some(0.10), "{g:?}");
    // Thresholds from the full models' own scores on W0 (no label): the 90th percentile of each.
    assert!(close(g.thresholds[0], 1.8, 1e-9) && close(g.thresholds[1], 2.7, 1e-9), "{g:?}");
    assert!(g.val_lower.unwrap() > g.val_base.unwrap());
    assert!(g.speaks([1.9, 2.8]) && !g.speaks([1.9, 2.0]) && !g.speaks([1.0, 2.8]));
    // The two models must agree: one extreme alone gives no signal.
    let disagree: Vec<ValRow> = rows.iter().map(|r| ValRow { s: [r.s[0], 1.0 - r.s[0]], ..*r }).collect();
    assert_eq!(choose_gate(&disagree, &full).level, None);
    // No extreme bucket beating the base rate: silent for the block.
    let flat = val(&scores, |s| ((s * 2000.0) as usize).is_multiple_of(2));
    let g = choose_gate(&flat, &full);
    assert_eq!(g.level, None);
    assert!(!g.speaks([9.0, 9.0]));
    // Fewer than 30 signals at every level: silent.
    let small: Vec<f64> = (0..200).map(|i| i as f64).collect();
    assert_eq!(choose_gate(&val(&small, |s| s > 150.0), &vec![[0.0, 0.0]; 200]).level, None);
    // Non-overlapping signals on W0: the same asset cannot signal again before its exit.
    let same: Vec<ValRow> = (0..2000).map(|i| ValRow { asset: 0, time: i, exit: i + 1000, hit: true, s: [1.0, 1.0] }).collect();
    assert_eq!(choose_gate(&same, &vec![[1.0, 1.0]; 2000]).val_signals, 0);
}

fn tally(n: usize, hits: usize, random: f64, excess: f64, labelled: usize, base_hits: usize) -> Tally {
    let sigs = (0..n)
        .map(|i| Sig {
            time: i as i64 * 86_400_000,
            asset: i % 7,
            hit: i < hits,
            random,
            excess: excess + if i % 2 == 0 { 0.5 } else { -0.5 },
            excess_median: None,
            prob: 0.7,
        })
        .collect();
    Tally { sigs, test_rows: labelled * 2, labelled, base_hits, spoken: n * 3, first: Some(0), last: Some(4 * 365 * 86_400_000) }
}

#[test]
fn precision_verdict_and_status() {
    let req = t_required(130);
    // 200 signals, 150 hits (75 %), random 50 %, base 50 %: precise.
    let s = PrecisionStats::of(&tally(200, 150, 0.5, 1.0, 10_000, 5_000), req);
    assert_eq!((s.signals, s.hits, s.precision), (200, 150, Some(75.0)));
    assert_eq!((s.base_rate, s.random_rate, s.reference), (Some(50.0), Some(50.0), Some(50.0)));
    assert!(s.wilson_low.unwrap() > 68.0 && s.wilson_high.unwrap() < 81.0);
    assert_eq!(s.lift, Some(25.0));
    assert!(s.t.unwrap() > req);
    assert_eq!(s.verdict, Precision::Precise);
    assert!(close(s.per_year.unwrap(), 50.0, 0.1));
    assert_eq!(s.coverage, Some(3.0));
    assert_eq!(s.mean_prob, Some(70.0));
    // The same precision but a base rate of 74 %: the lower bound does not beat it.
    let s = PrecisionStats::of(&tally(200, 150, 0.5, 1.0, 10_000, 7_400), req);
    assert_eq!(s.reference, Some(74.0));
    assert_eq!(s.verdict, Precision::NotProven);
    // Precise hits but a negative mean excess: not proven (being right on small moves is not enough).
    let s = PrecisionStats::of(&tally(200, 150, 0.5, -1.0, 10_000, 5_000), req);
    assert_eq!(s.verdict, Precision::NotProven);
    // Under 30 signals: insufficient; far below chance: contrary.
    assert_eq!(PrecisionStats::of(&tally(29, 29, 0.5, 1.0, 100, 50), req).verdict, Precision::Insufficient);
    assert_eq!(PrecisionStats::of(&tally(300, 30, 0.5, 1.0, 10_000, 5_000), req).verdict, Precision::Contrary);
    // Status: needs the forward test (≥ 30 signals, precision ≥ reference, excess ≥ 0).
    let main = PrecisionStats::of(&tally(200, 150, 0.5, 1.0, 10_000, 5_000), req);
    let empty = PrecisionStats::of(&Tally::default(), req);
    assert_eq!(Status::of(&main, &empty), Status::Pending);
    assert_eq!(Status::of(&main, &PrecisionStats::of(&tally(30, 20, 0.5, 0.5, 300, 150), req)), Status::Proven);
    assert_eq!(Status::of(&main, &PrecisionStats::of(&tally(30, 10, 0.5, 0.5, 300, 150), req)), Status::Contradicted);
    assert_eq!(Status::of(&empty, &main), Status::NotProven);
    // v3 and v4 together: ± 3 at most, 0 when they disagree.
    assert_eq!(combined_nudge(3.0, 3.0), 3.0);
    assert_eq!(combined_nudge(0.0, -3.0), -3.0);
    assert_eq!(combined_nudge(3.0, -3.0), 0.0);
    assert_eq!(combined_nudge(0.0, 0.0), 0.0);
}

fn long(symbol: &str) -> Vec<Candle> {
    sanitize(&common::find(symbol, "long").unwrap().candles)
}

fn scaled(c: &[Candle], k: f64) -> Vec<Candle> {
    c.iter().map(|x| Candle { open: x.open * k, high: x.high * k, low: x.low * k, close: x.close * k, ..*x }).collect()
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

/// The whole v4 on the real saved histories: its shape, every bot judged at K = 130, deterministic, read back from
/// JSON; the decision view counts only a proven bot that speaks.
#[test]
fn run_on_saved_histories() {
    let now = common::now();
    let r = bot_v3::run(saved_universe(), vec![], now, "test", 2);
    let v4 = r.v4.as_ref().unwrap();
    assert_eq!((v4.version, v4.prereg_date.as_str(), v4.forward_from), (4, "2026-10-01", 1_790_899_200_000));
    assert_eq!(v4.k.total, 130);
    assert!(close(v4.t_required, 3.5504, 1e-3));
    let ids: Vec<&str> = v4.bots.iter().map(|b| b.id.as_str()).collect();
    let mut want = Vec::new();
    for g in ["stock", "crypto"] {
        for h in [20, 60] {
            for s in ["rise", "fall", "top", "bottom"] {
                want.push(format!("{g}-{h}-{s}"));
            }
        }
    }
    assert_eq!(ids, want.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    assert!(v4.headline.starts_with("Bots sélectifs : "), "{}", v4.headline);
    assert!(v4.forward_headline.starts_with("Aucun signal daté du 02/10/2026"), "{}", v4.forward_headline);
    for b in &v4.bots {
        let s = &b.main;
        // Everything is dated before the forward test here.
        assert_eq!(b.forward.test_rows, 0, "{}", b.id);
        assert!(s.signals <= s.labelled && s.hits <= s.signals, "{}", b.id);
        assert!(b.open_blocks <= b.blocks && b.levels.iter().map(|l| l.blocks).sum::<usize>() == b.open_blocks, "{}", b.id);
        if s.signals > 0 {
            assert!(s.wilson_low.unwrap() <= s.precision.unwrap() && s.precision.unwrap() <= s.wilson_high.unwrap(), "{}", b.id);
            assert_eq!(s.excess_median.is_some(), b.side.peers(), "{}", b.id);
        }
        assert!(s.coverage.unwrap_or(0.0) <= 100.0, "{} {:?}", b.id, s.coverage);
        if s.verdict == Precision::Precise {
            assert!(s.t.unwrap() >= v4.t_required && s.wilson_low.unwrap() >= s.reference.unwrap());
        }
        // No forward signal yet: never « prouvé ».
        assert_ne!(b.status, Status::Proven, "{}", b.id);
        assert_eq!(b.status_label, b.status.label());
        assert!(!b.text.is_empty());
    }
    // Basket rows carry today's speaking bots (ids known).
    for a in &r.assets {
        let n = a.v4.as_ref().unwrap_or_else(|| panic!("{}", a.symbol));
        assert!(n.bots.iter().all(|id| v4.bot(id).is_some_and(|b| b.today.contains(&a.symbol))), "{}", a.symbol);
    }
    // Deterministic whatever the threads; v3 unchanged by v4 (the same run).
    let again = bot_v3::run(saved_universe(), vec![], now, "test", 1);
    assert_eq!(again.v4, r.v4);
    assert_eq!(again.v3, r.v3);
    // JSON: the live models stay in memory; the rest reads back.
    let json = serde_json::to_string(&r).unwrap();
    let back: BotReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back.v4, r.v4);
    assert!(back.v4_live.is_none());
    let v = bot_view(Some(&back), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now);
    let w = v.v4.as_ref().unwrap();
    assert!(!w.available && !w.counts && w.note.contains("rapport relu depuis un fichier"));

    // Today's view: the group's 8 bots; none counts while not proven.
    for (s, kind, m) in [("AAPL", Kind::Stock, "SPY"), ("BTC", Kind::Crypto, "BTC")] {
        let v = bot_view(Some(&r), s, kind, &long(s), &long(m), now);
        let w = v.v4.as_ref().unwrap();
        assert!(w.available);
        assert_eq!(w.signals.len(), 8, "{s}");
        assert!(!w.counts && w.nudge == 0.0);
        for x in &w.signals {
            assert_eq!(x.action.is_some(), x.probability.is_some(), "{s} {}", x.id);
            assert!(x.text.contains(if x.action.is_some() { x.side.avis() } else { "pas d'avis" }) || x.text.contains("pas de modèle"), "{}", x.text);
        }
    }

    // Forced: the stocks' 20-day rise gate open for everything (thresholds 0) → AAPL gets its avis; proven but
    // without forward signals it waits; proven and confirmed it counts (+ 3), never more with v3.
    let mut forced = r.clone();
    let mut live = (**forced.v4_live.as_ref().unwrap()).clone();
    let lh = live.groups.iter_mut().find(|g| g.group == BotGroup::Stock).unwrap().horizons.iter_mut().find(|h| h.horizon == 20).unwrap();
    if lh.a.is_none() {
        return; // too little saved history for the models: nothing more to force
    }
    lh.a_gates[0] = Gate { level: Some(0.1), thresholds: [0.0, 0.0], ..Gate::default() };
    forced.v4_live = Some(Arc::new(live));
    let set = |f: &mut BotReport, status: Status| {
        let b = f.v4.as_mut().unwrap().bots.iter_mut().find(|b| b.id == "stock-20-rise").unwrap();
        b.status = status;
    };
    set(&mut forced, Status::Pending);
    let v = bot_view(Some(&forced), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now);
    let w = v.v4.as_ref().unwrap();
    let rise = w.signals.iter().find(|s| s.id == "stock-20-rise").unwrap();
    assert_eq!(rise.action, Some(BotAction::Buy));
    assert!(!w.counts && w.note.contains("ne comptera qu'après 30 signaux"), "{}", w.note);
    set(&mut forced, Status::Proven);
    let v = bot_view(Some(&forced), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now);
    let w = v.v4.as_ref().unwrap();
    assert!(w.counts && w.nudge == bot_v4::NUDGE);
    let v3_counts = v.v3.as_ref().is_some_and(|x| x.counts);
    if !v3_counts {
        assert!(v.counts && v.nudge() == bot_v4::NUDGE);
        let (pro, text) = v.line(false).unwrap();
        assert!(pro && text.starts_with("Bot sélectif favorable"), "{text}");
    }
    // A proven sell bot speaking at the same time: they cancel out.
    let mut live = (**forced.v4_live.as_ref().unwrap()).clone();
    let lh = live.groups.iter_mut().find(|g| g.group == BotGroup::Stock).unwrap().horizons.iter_mut().find(|h| h.horizon == 20).unwrap();
    lh.a_gates[1] = Gate { level: Some(0.1), thresholds: [0.0, 0.0], ..Gate::default() };
    forced.v4_live = Some(Arc::new(live));
    forced.v4.as_mut().unwrap().bots.iter_mut().find(|b| b.id == "stock-20-fall").unwrap().status = Status::Proven;
    let v = bot_view(Some(&forced), "AAPL", Kind::Stock, &long("AAPL"), &long("SPY"), now);
    assert!(!v.v4.as_ref().unwrap().counts && v.nudge() == if v3_counts { v.v3.as_ref().unwrap().nudge } else { 0.0 });
}

/// The saved real report of 01/10/2026 (`bot-v4.json`: v3 recomputed and v4) carries v4 (16 bots, K = 130) and its
/// texts follow from its numbers.
#[test]
fn saved_report_has_v4() {
    let path = format!("{}/tests/samples/bot-v4.json", env!("CARGO_MANIFEST_DIR"));
    let r: BotReport = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let v4 = r.v4.as_ref().expect("v4 dans le rapport réel");
    assert_eq!((v4.prereg_date.as_str(), v4.k.total, v4.bots.len()), ("2026-10-01", 130, 16));
    assert_eq!(v4.after_prereg.len(), AFTER_PREREG.len());
    assert_eq!(bot_v4::headline(&v4.bots, t_required(v4.k.total), v4.k.total), v4.headline);
    for b in &v4.bots {
        if b.status != Status::NotProven {
            assert_eq!(b.main.verdict, Precision::Precise);
            assert!(b.main.t.unwrap() >= v4.t_required);
        }
    }
    assert!(r.assets.iter().all(|a| a.v4.is_some()));
}

//! « Bots sélectifs » (Bot Altim v4): a few specialised bots that aim at PRECISION, not coverage. Each one speaks
//! rarely — only when two fitted models agree on an extreme score — and otherwise says « pas d'avis ». Pure: computed
//! inside v3's run (`bot_v3::run`), from v3's rows, blocks and fitted models.
//!
//! PRE-REGISTRATION — fixed on 2026-10-01 (`PREREG_DATE`), written and committed before any v4 figure was computed
//! on real data, and not to be changed after seeing results (a change made afterwards must be listed in
//! `AFTER_PREREG` and shown). v1 – v3 were seen before this was written (v3, 2026-09-30: one fragile edge, the stocks'
//! peers buys at 20 days, t = 3.65 by date vs 3.52 required, 0.6 by asset, pending its forward test; every other
//! headline side not proven at the corrected threshold).
//!
//! 1. Data, universe, features, labels, blocks, purge, inner windows: v3's, unchanged and in the same run (basket of
//!    34 assets for the result, v2's 91 extra assets for training, reported apart; horizons H ∈ {20, 60}; retraining
//!    every 252 dates; inner validation W0 = the last 252 dates of each training window; inner training T0 = the
//!    training rows whose label ended H dates before W0).
//! 2. The bots (fixed list, `BOTS`, 16 in all): for each group (stocks and ETFs; cryptos) and horizon (20, 60):
//!    - « Hausse » (ACHETER): hit = the asset's net return over H (next open to the open H candles later, v2's
//!      costs) is positive (v2's `up` label);
//!    - « Baisse » (VENDRE): hit = its gross return over H is below minus the round-trip cost (v2's `down` label);
//!    - « Haut du classement » (ACHETER entre pairs): hit = its gross return over H beats the median gross return of
//!      its group's universe assets labelled that day (v3's peers label, ≥ 5 assets);
//!    - « Bas du classement » (VENDRE entre pairs, for a holder: lighten): hit = it does not beat that median.
//! 3. Scores and agreement filter: two of v3's fitted candidates per family, as v3 fits them (same rows, same early
//!    stopping): A (Hausse / Baisse) the logistic regression on 24 features (`Logit`) and the long boosted trees
//!    (`TreesLong`), each one's probability of `up` (Hausse) or `down` (Baisse); B (classement) the peers logistic
//!    regression (`XsLogit`) and the peers long trees (`XsTrees`), each one's probability p of beating the median
//!    (Haut: p; Bas: 1 − p). A bot speaks on an asset-day only when BOTH models' scores are at or above their own
//!    threshold (they must agree on an extreme); the probability shown is the mean of the two. A block where either
//!    model is missing (e.g. the first blocks, without early stopping window): the bot is silent.
//! 4. Thresholds, chosen on the inner validation only, never on the test: a fixed grid of levels q ∈ {10 %, 5 %,
//!    2.5 %, 1 %} (`LEVELS`). At level q, a model's threshold is the (1 − q) quantile of its scores over every row
//!    (basket and extra assets) of W0. On W0, with the two models fitted on T0 (v3's inner fits): signals = the rows
//!    where both are at or above their level-q thresholds, label known before the block's cutoff, non-overlapping per
//!    asset (as the test); precision = hits ÷ signals; Wilson 95 % lower bound (z = 1.96). Eligible: ≥
//!    `MIN_VAL_SIGNALS` = 30 W0 signals. Chosen level: the highest Wilson lower bound (ties within 1e-9: the less
//!    selective, earlier in the grid). The bot speaks in the block only when that lower bound exceeds W0's base rate
//!    (hits ÷ every labelled W0 row); otherwise (or with no eligible level) it is silent for the whole block. In the
//!    test block, the models fitted on the whole training window (v3's) take as thresholds the (1 − q) quantile of
//!    their own scores over W0's rows (features only, no label). « Now » (avis du jour): the same at the last date,
//!    with v3's « now » fits.
//! 5. Evaluation per bot, basket assets, signals dated before `FORWARD_FROM` (the result); extra universe apart;
//!    forward test apart (signals dated from 2026-10-02, the only truly fresh test). Signals non-overlapping per
//!    asset (the next one after the previous exit).
//!    - Precision = hits ÷ signals, with its Wilson 95 % interval.
//!    - Base rate = hits ÷ every labelled basket test row of the same group, horizon and period where the bot had
//!      models (silent or not). Random entry = for each signal, the hit share of the same asset's labelled rows in the
//!      same test block (A) or of the day's labelled universe rows (B), averaged over the signals. Reference = the
//!      higher of the two.
//!    - t = hit (1 / 0) − its random-entry share, clustered by date (mean per date, then t over the dates; by asset
//!      and per signal shown).
//!    - Mean net excess vs a random entry, points of %: A Hausse: net − the asset's mean net in the block; A Baisse:
//!      the asset's mean gross in the block − gross (what leaving avoided vs a random day); B Haut: net − the day's
//!      equal-weight mean net, and net − the day's median net; B Bas: the day's mean net − gross, and the day's median
//!      net − gross. Its t by date shown.
//!    - Signals per year = signals ÷ the years from the first to the last labelled basket test date (main period);
//!      coverage = asset-days the bot spoke on (before the overlap thinning) ÷ basket test rows with a prediction.
//!    - Calibration of the top bucket: mean shown probability of the signals vs their precision.
//!    - Verdict « précis »: ≥ 30 signals AND Wilson lower bound > the reference AND t ≥ the required t AND every
//!      mean net excess > 0 (B: against the mean and the median). « insuffisant » under 30 signals; « contraire » when
//!      t ≤ − required; « non prouvé » otherwise. The raw t ≥ 2 rule is shown next to it, for comparison only.
//! 6. Multiple testing: K counts every configuration × side evaluated since v1: v1 4, v2 20, v3 90, v4 16 (one per
//!    bot) → K = 130; required t = z(1 − 0.025 ÷ K) ≈ 3.55 (Bonferroni, two-sided 5 %). v3's own figures and verdicts
//!    stay as pre-registered (K = 114).
//! 7. Forward test: every signal dated from 2026-10-02 00:00 UTC (`FORWARD_FROM`), basket assets, by the same frozen
//!    walk-forward (deterministic, point-in-time, nothing stored); same statistics.
//! 8. Decision: a bot counts only when « précis » on the past AND confirmed by the forward test: ≥ 30 forward signals,
//!    forward precision ≥ the forward reference and every forward mean excess ≥ 0; until then it is shown as « en
//!    attente ». A counting bot whose avis today is ACHETER pushes the confidence by + 3 at most, VENDRE by − 3; never
//!    more than ± 3 together with v3 (0 when counting signals disagree); never the verdict or a veto. « Bas du
//!    classement » becomes a con line only for a holder.
//! 9. Compute: inside v3's background run, one thread by default, reusing v3's fitted models (only the inner windows
//!    are scored again); peak memory < 350 MB; time and memory measured and reported.
//! 10. Report: additive JSON (`v4` in the bot report, in each basket asset row and in the decision's bot view); v1 –
//!    v3 fields unchanged.

// The pre-registration above is kept as written (its list layout included).
#![allow(clippy::doc_overindented_list_items, clippy::doc_lazy_continuation)]

/// Version of the report.
pub const VERSION: u32 = 4;
/// Date of the pre-registration (the forward test starts the day after).
pub const PREREG_DATE: &str = "2026-10-01";
/// 2026-10-02 00:00 UTC (ms): signals dated from then on form the forward test.
pub const FORWARD_FROM: i64 = 1_790_899_200_000;
/// Share of W0's rows above a model's threshold, most to least selective last (fractions).
pub const LEVELS: [f64; 4] = [0.10, 0.05, 0.025, 0.01];
/// Signals needed on the inner validation for a level to be eligible.
pub const MIN_VAL_SIGNALS: usize = 30;
/// Wilson interval at 95 %.
pub const WILSON_Z: f64 = 1.96;
/// Configurations × sides evaluated in v1, v2, v3 and v4.
pub const K_V1: usize = 4;
pub const K_V2: usize = 20;
pub const K_V3: usize = 90;
pub const K_V4: usize = 16;
/// Changes made after the pre-registration, listed in the report (never hidden).
pub const AFTER_PREREG: [&str; 0] = [];

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::bot::{self, BotAction, BotGroup, Features, Pair, Prediction, Row, clustered, day_of, mean, round_to};
use super::bot_v3::{self, PeerModel, XsRow, norm_quantile, peers_features};
use super::validation::{MIN_TRADES, T_EDGE};
use crate::js::fr;
use crate::types::DAY_MS;

/// Family-wise error rate of the corrected threshold (two-sided).
pub const ALPHA: f64 = bot_v3::ALPHA;
/// Confidence change at most, v3 and v4 together.
pub const NUDGE: f64 = bot::NUDGE;

// ---------- Bots ----------

/// What a bot predicts (see the module doc, 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    #[default]
    Rise,
    Fall,
    Top,
    Bottom,
}

pub const SIDES: [Side; 4] = [Side::Rise, Side::Fall, Side::Top, Side::Bottom];

impl Side {
    pub fn peers(self) -> bool {
        matches!(self, Side::Top | Side::Bottom)
    }

    /// Index among the two bots of a family (0: buy side, 1: sell side).
    pub fn slot(self) -> usize {
        match self {
            Side::Rise | Side::Top => 0,
            Side::Fall | Side::Bottom => 1,
        }
    }

    pub fn action(self) -> BotAction {
        if self.slot() == 0 { BotAction::Buy } else { BotAction::Sell }
    }

    pub fn id(self) -> &'static str {
        match self {
            Side::Rise => "rise",
            Side::Fall => "fall",
            Side::Top => "top",
            Side::Bottom => "bottom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Side::Rise => "Hausse",
            Side::Fall => "Baisse",
            Side::Top => "Haut du classement",
            Side::Bottom => "Bas du classement",
        }
    }

    /// What a hit means, in French.
    pub fn hit_text(self, h: usize) -> String {
        match self {
            Side::Rise => format!("l'actif finit en hausse après {h} jours, frais payés"),
            Side::Fall => format!("l'actif baisse de plus que les frais d'un aller-retour en {h} jours"),
            Side::Top => format!("l'actif fait mieux que la médiane de son groupe sur {h} jours"),
            Side::Bottom => format!("l'actif fait moins bien que la médiane de son groupe sur {h} jours"),
        }
    }

    /// The avis when it speaks.
    pub fn avis(self) -> &'static str {
        match self {
            Side::Rise => "ACHETER (hausse attendue)",
            Side::Fall => "VENDRE (baisse attendue)",
            Side::Top => "ACHETER (dans le haut de son groupe)",
            Side::Bottom => "ALLÉGER (dans le bas de son groupe)",
        }
    }
}

/// A bot's id: "stock-20-rise".
pub fn bot_id(g: BotGroup, h: usize, side: Side) -> String {
    format!("{}-{h}-{}", g.id(), side.id())
}

/// « Hausse 20 j » from a bot's id (the id itself when unknown).
pub fn short_label(id: &str) -> String {
    let mut parts = id.splitn(3, '-').skip(1);
    let (Some(h), Some(side)) = (parts.next(), parts.next()) else { return id.to_string() };
    match SIDES.iter().find(|s| s.id() == side) {
        Some(s) => format!("{} {h} j", s.label()),
        None => id.to_string(),
    }
}

// ---------- Small statistics ----------

/// Wilson interval (fractions) of `hits` out of `n` at `z`; None without signals.
pub fn wilson(hits: usize, n: usize, z: f64) -> Option<(f64, f64)> {
    if n == 0 {
        return None;
    }
    let nf = n as f64;
    let p = hits as f64 / nf;
    let z2 = z * z;
    let den = 1.0 + z2 / nf;
    let centre = (p + z2 / (2.0 * nf)) / den;
    let half = z * (p * (1.0 - p) / nf + z2 / (4.0 * nf * nf)).sqrt() / den;
    Some(((centre - half).max(0.0), (centre + half).min(1.0)))
}

/// Value at or above which a share `q` of `v` lies (nearest rank: the ⌊(1 − q)·n⌋-th smallest); None when empty.
pub fn upper_quantile(v: &[f64], q: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let k = (((1.0 - q) * s.len() as f64).floor() as usize).min(s.len() - 1);
    Some(s[k])
}

/// Configurations × sides evaluated since v1 (see the module doc, 6).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct K4 {
    pub v1: usize,
    pub v2: usize,
    pub v3: usize,
    pub v4: usize,
    pub total: usize,
}

pub fn k_count() -> K4 {
    let v4 = bot::GROUPS.len() * bot_v3::HORIZONS.len() * SIDES.len();
    K4 { v1: K_V1, v2: K_V2, v3: K_V3, v4, total: K_V1 + K_V2 + K_V3 + v4 }
}

pub fn t_required(k: usize) -> f64 {
    norm_quantile(1.0 - ALPHA / 2.0 / k.max(1) as f64)
}

/// A score as the test sees it (predictions are kept as f32): thresholds and test compare the same values.
fn f32ed(x: f64) -> f64 {
    x as f32 as f64
}

// ---------- Gates: thresholds chosen on the inner validation ----------

/// A bot's gate in one block (or « now »): the chosen level and the two full models' thresholds; `level` None: silent.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Gate {
    pub level: Option<f64>,
    pub thresholds: [f64; 2],
    /// W0 figures of the chosen level (or of the best one when silent): signals, precision, Wilson lower bound, base.
    pub val_signals: usize,
    pub val_precision: Option<f64>,
    pub val_lower: Option<f64>,
    pub val_base: Option<f64>,
}

impl Gate {
    pub fn speaks(&self, s: [f64; 2]) -> bool {
        self.level.is_some() && s[0] >= self.thresholds[0] && s[1] >= self.thresholds[1]
    }
}

/// One labelled row of the inner validation W0 (rows in (asset, time) order): the two inner models' scores.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValRow {
    pub asset: usize,
    pub time: i64,
    pub exit: i64,
    pub hit: bool,
    pub s: [f64; 2],
}

/// Non-overlapping signals of W0 at thresholds `t`: (signals, hits).
fn val_signals(val: &[ValRow], t: [f64; 2]) -> (usize, usize) {
    let mut next: HashMap<usize, i64> = HashMap::new();
    let (mut n, mut k) = (0, 0);
    for r in val {
        if r.s[0] >= t[0] && r.s[1] >= t[1] && r.time >= *next.get(&r.asset).unwrap_or(&i64::MIN) {
            next.insert(r.asset, r.exit);
            n += 1;
            k += r.hit as usize;
        }
    }
    (n, k)
}

/// The gate of a block (see the module doc, 4): `val` the labelled W0 rows with the inner models' scores, `full` the
/// full models' scores on the same rows.
pub fn choose_gate(val: &[ValRow], full: &[[f64; 2]]) -> Gate {
    if val.is_empty() || full.len() != val.len() {
        return Gate::default();
    }
    let base = val.iter().filter(|r| r.hit).count() as f64 / val.len() as f64;
    let col = |v: &[[f64; 2]], j: usize| v.iter().map(|s| s[j]).collect::<Vec<f64>>();
    let inner: Vec<[f64; 2]> = val.iter().map(|r| r.s).collect();
    let (i0, i1) = (col(&inner, 0), col(&inner, 1));
    let mut best: Option<(f64, usize, usize, f64)> = None;
    for q in LEVELS {
        let (Some(a), Some(b)) = (upper_quantile(&i0, q), upper_quantile(&i1, q)) else { continue };
        let (n, k) = val_signals(val, [a, b]);
        if n < MIN_VAL_SIGNALS {
            continue;
        }
        let lower = wilson(k, n, WILSON_Z).map_or(0.0, |w| w.0);
        if best.is_none_or(|b| lower > b.3 + 1e-9) {
            best = Some((q, n, k, lower));
        }
    }
    let Some((q, n, k, lower)) = best else {
        return Gate { val_base: Some(base), ..Gate::default() };
    };
    let figures = Gate { val_signals: n, val_precision: Some(k as f64 / n as f64), val_lower: Some(lower), val_base: Some(base), ..Gate::default() };
    if lower <= base {
        return figures;
    }
    match (upper_quantile(&col(full, 0), q), upper_quantile(&col(full, 1), q)) {
        (Some(a), Some(b)) => Gate { level: Some(q), thresholds: [a, b], ..figures },
        _ => figures,
    }
}

/// A family-A bot's two scores at a row (`Rise`: the probabilities of a rise; `Fall`: of a fall).
pub fn a_scores(m: [&Pair; 2], x: &[f32], trend: i8, side: Side) -> [f64; 2] {
    let p = |q: &Pair| f32ed(if side == Side::Rise { q.up.prob(x, trend) } else { q.down.prob(x, trend) });
    [p(m[0]), p(m[1])]
}

/// A family-B bot's two scores (`Top`: p of beating the median; `Bottom`: 1 − p).
pub fn b_scores(m: [&PeerModel; 2], x: &[f32], side: Side) -> [f64; 2] {
    let p = |q: &PeerModel| {
        let s = f32ed(q.score(x));
        if side == Side::Top { s } else { 1.0 - s }
    };
    [p(m[0]), p(m[1])]
}

/// Family A's gates (Rise, Fall) at a retraining: W0 = rows from `val_from`, labels ended before `cutoff`; `inner`
/// fitted on T0, `full` on the whole training window (logistic, long trees). Silent without all four models.
pub fn a_gates(rows: &[Row], val_from: i64, cutoff: i64, inner: [Option<&Pair>; 2], full: [Option<&Pair>; 2]) -> [Gate; 2] {
    let (Some(i0), Some(i1), Some(f0), Some(f1)) = (inner[0], inner[1], full[0], full[1]) else { return [Gate::default(); 2] };
    let val: Vec<&Row> = rows.iter().filter(|r| r.time >= val_from && r.fwd.is_some_and(|f| f.exit_time < cutoff)).collect();
    [Side::Rise, Side::Fall].map(|side| {
        let vr: Vec<ValRow> = val
            .iter()
            .map(|r| {
                let f = r.fwd.unwrap();
                ValRow {
                    asset: r.asset,
                    time: r.time,
                    exit: f.exit_time,
                    hit: if side == Side::Rise { f.up } else { f.down },
                    s: a_scores([i0, i1], &r.x, r.trend, side),
                }
            })
            .collect();
        let fs: Vec<[f64; 2]> = val.iter().map(|r| a_scores([f0, f1], &r.x, r.trend, side)).collect();
        choose_gate(&vr, &fs)
    })
}

/// Family B's gates (Top, Bottom): as `a_gates` on the ranked rows with a peers label known before `cutoff`.
pub fn b_gates(rows: &[Row], xs: &[XsRow], val_from: i64, cutoff: i64, inner: [Option<&PeerModel>; 2], full: [Option<&PeerModel>; 2]) -> [Gate; 2] {
    let (Some(i0), Some(i1), Some(f0), Some(f1)) = (inner[0], inner[1], full[0], full[1]) else { return [Gate::default(); 2] };
    let val: Vec<usize> = (0..rows.len())
        .filter(|&i| rows[i].time >= val_from && xs[i].ranked && rows[i].fwd.is_some() && xs[i].label.is_some_and(|l| l.known < cutoff))
        .collect();
    [Side::Top, Side::Bottom].map(|side| {
        let vr: Vec<ValRow> = val
            .iter()
            .map(|&i| {
                let (r, l) = (&rows[i], xs[i].label.unwrap());
                ValRow {
                    asset: r.asset,
                    time: r.time,
                    exit: r.fwd.unwrap().exit_time,
                    hit: l.beat == (side == Side::Top),
                    s: b_scores([i0, i1], &r.x, side),
                }
            })
            .collect();
        let fs: Vec<[f64; 2]> = val.iter().map(|&i| b_scores([f0, f1], &rows[i].x, side)).collect();
        choose_gate(&vr, &fs)
    })
}

// ---------- Evaluation ----------

/// One non-overlapping signal of the test: hit, the random entry's hit share, the excesses (points of %) and the
/// shown probability.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sig {
    pub time: i64,
    pub asset: usize,
    pub hit: bool,
    pub random: f64,
    pub excess: f64,
    pub excess_median: Option<f64>,
    pub prob: f64,
}

/// What a period of the test gives: the signals and the counts behind the base rate and coverage.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tally {
    pub sigs: Vec<Sig>,
    pub test_rows: usize,
    pub labelled: usize,
    pub base_hits: usize,
    pub spoken: usize,
    pub first: Option<i64>,
    pub last: Option<i64>,
}

impl Tally {
    fn row(&mut self, time: i64, labelled: Option<bool>, spoke: bool) {
        self.test_rows += 1;
        self.spoken += spoke as usize;
        if let Some(h) = labelled {
            self.labelled += 1;
            self.base_hits += h as usize;
            self.first = Some(self.first.map_or(time, |f| f.min(time)));
            self.last = Some(self.last.map_or(time, |f| f.max(time)));
        }
    }
}

/// Family A's test (`p` = the logistic and long trees' out-of-sample predictions, `gates` per block).
pub fn evaluate_a(rows: &[Row], p: [&[Option<Prediction>]; 2], gates: &[[Gate; 2]], side: Side, keep: &dyn Fn(&Row) -> bool) -> Tally {
    let score = |q: &Prediction| if side == Side::Rise { q.up as f64 } else { q.down as f64 };
    let hit_of = |f: &bot::Forward| if side == Side::Rise { f.up } else { f.down };
    let pair = |i: usize| Some((p[0][i]?, p[1][i]?));
    // The random entry: the same asset in the same block (hits, net, gross).
    let mut agg: HashMap<(usize, u32), (usize, f64, f64, usize)> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        if let (Some((a, _)), Some(f)) = (pair(i), r.fwd)
            && keep(r)
        {
            let e = agg.entry((r.asset, a.block)).or_default();
            e.0 += hit_of(&f) as usize;
            e.1 += f.net;
            e.2 += f.gross;
            e.3 += 1;
        }
    }
    let mut t = Tally::default();
    let mut next: HashMap<usize, i64> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let Some((a, b)) = pair(i) else { continue };
        if !keep(r) {
            continue;
        }
        let s = [score(&a), score(&b)];
        let spoke = gates.get(a.block as usize).is_some_and(|g| g[side.slot()].speaks(s));
        t.row(r.time, r.fwd.map(|f| hit_of(&f)), spoke);
        let Some(f) = r.fwd else { continue };
        if spoke && r.time >= *next.get(&r.asset).unwrap_or(&i64::MIN) {
            next.insert(r.asset, f.exit_time);
            let (k, net, gross, n) = agg[&(r.asset, a.block)];
            let n = n as f64;
            let excess = if side == Side::Rise { f.net - net / n } else { gross / n - f.gross };
            t.sigs.push(Sig {
                time: r.time,
                asset: r.asset,
                hit: hit_of(&f),
                random: k as f64 / n,
                excess,
                excess_median: None,
                prob: (s[0] + s[1]) / 2.0,
            });
        }
    }
    t
}

/// Family B's test (`p` = the peers logistic and long trees' predictions, `up` = their score).
pub fn evaluate_b(rows: &[Row], xs: &[XsRow], p: [&[Option<Prediction>]; 2], gates: &[[Gate; 2]], side: Side, keep: &dyn Fn(&Row) -> bool) -> Tally {
    let top = side == Side::Top;
    let score = |q: &Prediction| if top { q.up as f64 } else { 1.0 - q.up as f64 };
    // The random entry: the day's labelled universe rows (share beating / not beating the median).
    let mut days: HashMap<i64, (usize, usize)> = HashMap::new();
    for (r, x) in rows.iter().zip(xs) {
        if let Some(l) = x.label {
            let e = days.entry(day_of(r.time)).or_default();
            e.0 += (l.beat == top) as usize;
            e.1 += 1;
        }
    }
    let mut t = Tally::default();
    let mut next: HashMap<usize, i64> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let (Some(a), Some(b)) = (p[0][i], p[1][i]) else { continue };
        if !keep(r) {
            continue;
        }
        let s = [score(&a), score(&b)];
        let spoke = gates.get(a.block as usize).is_some_and(|g| g[side.slot()].speaks(s));
        let label = r.fwd.and(xs[i].label);
        t.row(r.time, label.map(|l| l.beat == top), spoke);
        let (Some(f), Some(l)) = (r.fwd, label) else { continue };
        if spoke && r.time >= *next.get(&r.asset).unwrap_or(&i64::MIN) {
            next.insert(r.asset, f.exit_time);
            let (k, n) = days[&day_of(r.time)];
            let (mean_net, med_net) = (l.mean_net as f64, l.med_net as f64);
            let (excess, excess_median) = if top { (f.net - mean_net, f.net - med_net) } else { (mean_net - f.gross, med_net - f.gross) };
            t.sigs.push(Sig {
                time: r.time,
                asset: r.asset,
                hit: l.beat == top,
                random: k as f64 / n as f64,
                excess,
                excess_median: Some(excess_median),
                prob: (s[0] + s[1]) / 2.0,
            });
        }
    }
    t
}

/// A bot's verdict on a period (see the module doc, 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Precision {
    #[default]
    Insufficient,
    Precise,
    NotProven,
    Contrary,
}

impl Precision {
    pub fn label(self) -> &'static str {
        match self {
            Precision::Insufficient => "insuffisant",
            Precision::Precise => "précis",
            Precision::NotProven => "non prouvé",
            Precision::Contrary => "contraire",
        }
    }
}

/// A bot's figures on one period. Shares and rates in %, excesses in points of %.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PrecisionStats {
    pub signals: usize,
    pub hits: usize,
    pub precision: Option<f64>,
    pub wilson_low: Option<f64>,
    pub wilson_high: Option<f64>,
    /// Hits among every labelled test row (base rate) and the random entry's share (same asset and block / same day).
    pub base_rate: Option<f64>,
    pub random_rate: Option<f64>,
    /// The higher of the two: what the lower Wilson bound must beat.
    pub reference: Option<f64>,
    /// Precision − reference.
    pub lift: Option<f64>,
    /// t of hit − random entry, by date (the verdict's), by asset and per signal.
    pub t: Option<f64>,
    pub dates: usize,
    pub t_by_asset: Option<f64>,
    pub assets: usize,
    pub t_per_signal: Option<f64>,
    /// Mean net excess vs a random entry (family B: vs the group's equal-weight mean) and its t by date; family B
    /// also vs the group's median.
    pub excess: Option<f64>,
    pub excess_t: Option<f64>,
    pub excess_median: Option<f64>,
    pub excess_median_t: Option<f64>,
    pub per_year: Option<f64>,
    pub coverage: Option<f64>,
    /// Calibration of the top bucket: mean shown probability of the signals (%), to compare with the precision.
    pub mean_prob: Option<f64>,
    pub test_rows: usize,
    pub labelled: usize,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub years: Option<f64>,
    pub verdict: Precision,
    /// The same rule with t ≥ 2 (for comparison only).
    pub raw_verdict: Precision,
}

fn pct(x: Option<f64>) -> Option<f64> {
    x.map(|v| round_to(v * 100.0, 1))
}

fn verdict_of(s: &PrecisionStats, t_req: f64) -> Precision {
    if s.signals < MIN_TRADES {
        return Precision::Insufficient;
    }
    if s.t.is_some_and(|t| t <= -t_req) {
        return Precision::Contrary;
    }
    let beats = matches!((s.wilson_low, s.reference), (Some(l), Some(r)) if l > r);
    let pays = s.excess.is_some_and(|e| e > 0.0) && s.excess_median.is_none_or(|e| e > 0.0);
    if beats && pays && s.t.is_some_and(|t| t >= t_req) { Precision::Precise } else { Precision::NotProven }
}

impl PrecisionStats {
    pub fn of(t: &Tally, t_req: f64) -> PrecisionStats {
        let n = t.sigs.len();
        let hits = t.sigs.iter().filter(|s| s.hit).count();
        let w = wilson(hits, n, WILSON_Z);
        let base = (t.labelled > 0).then(|| t.base_hits as f64 / t.labelled as f64);
        let random = mean(&t.sigs.iter().map(|s| s.random).collect::<Vec<_>>());
        let reference = match (base, random) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let precision = (n > 0).then(|| hits as f64 / n as f64);
        let times: Vec<i64> = t.sigs.iter().map(|s| s.time).collect();
        let assets: Vec<usize> = t.sigs.iter().map(|s| s.asset).collect();
        let c = clustered(&t.sigs.iter().map(|s| s.hit as u8 as f64 - s.random).collect::<Vec<_>>(), &times, &assets);
        let ex: Vec<f64> = t.sigs.iter().map(|s| s.excess).collect();
        let exm: Vec<f64> = t.sigs.iter().filter_map(|s| s.excess_median).collect();
        let years = match (t.first, t.last) {
            (Some(a), Some(b)) if b > a => Some((b - a) as f64 / (365.25 * DAY_MS as f64)),
            _ => None,
        };
        let mut s = PrecisionStats {
            signals: n,
            hits,
            precision: pct(precision),
            wilson_low: pct(w.map(|w| w.0)),
            wilson_high: pct(w.map(|w| w.1)),
            base_rate: pct(base),
            random_rate: pct(random),
            reference: pct(reference),
            lift: match (precision, reference) {
                (Some(p), Some(r)) => Some(round_to((p - r) * 100.0, 1)),
                _ => None,
            },
            t: c.by_date,
            dates: c.dates,
            t_by_asset: c.by_asset,
            assets: c.assets,
            t_per_signal: c.per_signal,
            excess: mean(&ex).map(|v| round_to(v, 2)),
            excess_t: clustered(&ex, &times, &assets).by_date,
            excess_median: mean(&exm).map(|v| round_to(v, 2)),
            excess_median_t: if exm.is_empty() { None } else { clustered(&exm, &times, &assets).by_date },
            per_year: years.filter(|y| *y >= 0.25).map(|y| round_to(n as f64 / y, 1)),
            coverage: (t.test_rows > 0).then(|| round_to(t.spoken as f64 / t.test_rows as f64 * 100.0, 2)),
            mean_prob: pct(mean(&t.sigs.iter().map(|s| s.prob).collect::<Vec<_>>())),
            test_rows: t.test_rows,
            labelled: t.labelled,
            from: t.first,
            to: t.last,
            years: years.map(|y| round_to(y, 1)),
            verdict: Precision::Insufficient,
            raw_verdict: Precision::Insufficient,
        };
        // The verdict compares the unrounded Wilson bound and reference.
        let exact = PrecisionStats { wilson_low: w.map(|w| w.0), reference, ..s.clone() };
        s.verdict = verdict_of(&exact, t_req);
        s.raw_verdict = verdict_of(&exact, T_EDGE);
        s
    }

    /// The forward test confirms (see the module doc, 8).
    pub fn confirms(&self) -> bool {
        self.signals >= MIN_TRADES
            && matches!((self.precision, self.reference), (Some(p), Some(r)) if p >= r)
            && self.excess.is_some_and(|e| e >= 0.0)
            && self.excess_median.is_none_or(|e| e >= 0.0)
    }
}

/// Where a bot stands: « prouvé » (précis and confirmed by the forward test; only then it counts), « en attente »
/// (précis, fewer than 30 forward signals), « contredit » (précis, the forward test says otherwise), « non prouvé ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Proven,
    Pending,
    Contradicted,
    #[default]
    NotProven,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Proven => "prouvé",
            Status::Pending => "en attente",
            Status::Contradicted => "contredit",
            Status::NotProven => "non prouvé",
        }
    }

    pub fn of(main: &PrecisionStats, forward: &PrecisionStats) -> Status {
        if main.verdict != Precision::Precise {
            Status::NotProven
        } else if forward.signals < MIN_TRADES {
            Status::Pending
        } else if forward.confirms() {
            Status::Proven
        } else {
            Status::Contradicted
        }
    }
}

/// Blocks at one level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LevelCount {
    /// Share of W0's rows above each model's threshold (%).
    pub level: f64,
    pub blocks: usize,
}

/// One selective bot: what it predicts, how its gates were chosen, its out-of-sample figures and today's avis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SelectiveBot {
    pub id: String,
    pub group: BotGroup,
    pub horizon: usize,
    pub side: Side,
    pub label: String,
    /// "buy" or "sell" when it speaks.
    pub action: BotAction,
    pub hit: String,
    pub models: String,
    /// Test blocks, those where it could speak (gate open), and the levels chosen.
    pub blocks: usize,
    pub open_blocks: usize,
    pub levels: Vec<LevelCount>,
    /// Basket, signals dated before the forward test (the result); extra universe; forward test.
    pub main: PrecisionStats,
    pub extra: PrecisionStats,
    pub forward: PrecisionStats,
    pub status: Status,
    pub status_label: String,
    /// Today: the level of the gate (%; None: silent), the basket assets it speaks on and the day (ms).
    pub today_level: Option<f64>,
    pub today: Vec<String>,
    pub today_time: Option<i64>,
    pub text: String,
}

impl SelectiveBot {
    pub fn new(g: BotGroup, h: usize, side: Side, gates: &[[Gate; 2]]) -> SelectiveBot {
        let mut levels: Vec<LevelCount> = Vec::new();
        for q in LEVELS {
            let n = gates.iter().filter(|x| x[side.slot()].level == Some(q)).count();
            if n > 0 {
                levels.push(LevelCount { level: round_to(q * 100.0, 1), blocks: n });
            }
        }
        SelectiveBot {
            id: bot_id(g, h, side),
            group: g,
            horizon: h,
            side,
            label: format!("{} à {h} jours ({})", side.label(), g.short()),
            action: side.action(),
            hit: side.hit_text(h),
            models: if side.peers() {
                "Régression logistique et arbres boostés longs entre pairs (v3), qui doivent être d'accord".into()
            } else {
                "Régression logistique 24 mesures et arbres boostés longs (v3), qui doivent être d'accord".into()
            },
            blocks: gates.len(),
            open_blocks: gates.iter().filter(|x| x[side.slot()].level.is_some()).count(),
            levels,
            ..SelectiveBot::default()
        }
    }

    /// Fills the figures and the texts.
    pub fn with_stats(mut self, main: PrecisionStats, extra: PrecisionStats, forward: PrecisionStats) -> SelectiveBot {
        self.status = Status::of(&main, &forward);
        self.status_label = self.status.label().into();
        self.main = main;
        self.extra = extra;
        self.forward = forward;
        self.text = bot_text(&self);
        self
    }
}

fn num(x: Option<f64>, d: usize) -> String {
    x.map(|v| format!("{}{}", if v < 0.0 { "−" } else { "" }, fr(v.abs(), 0, d))).unwrap_or_else(|| "—".into())
}

fn pc(x: Option<f64>) -> String {
    x.map(|v| format!("{} %", fr(v, 0, 0))).unwrap_or_else(|| "—".into())
}

/// « 62 % de réussite (57 % à 67 %) sur 340 signaux, contre 55 % au hasard ».
pub fn precision_text(s: &PrecisionStats) -> String {
    if s.signals == 0 {
        return "aucun signal".into();
    }
    format!(
        "{} de réussite ({} à {}) sur {} signa{}, contre {} au hasard",
        pc(s.precision),
        pc(s.wilson_low),
        pc(s.wilson_high),
        s.signals,
        if s.signals > 1 { "ux" } else { "l" },
        pc(s.reference)
    )
}

fn bot_text(b: &SelectiveBot) -> String {
    let s = &b.main;
    let rhythm = match s.per_year {
        Some(y) => format!("{} signaux par an", num(Some(y), 1)),
        None => "rythme non calculable".into(),
    };
    let verdict = match b.status {
        Status::Proven => "précis, confirmé sur l'avenir".to_string(),
        Status::Pending => format!("précis sur le passé, en attente de {MIN_TRADES} signaux sur l'avenir ({} à ce jour)", b.forward.signals),
        Status::Contradicted => "précis sur le passé mais contredit par le test sur l'avenir".into(),
        Status::NotProven => match s.verdict {
            Precision::Insufficient => format!("trop peu de signaux pour juger (moins de {MIN_TRADES})"),
            Precision::Contrary => "moins bon que le hasard".into(),
            _ => "non prouvé".into(),
        },
    };
    format!("{} ; {rhythm} ; écart moyen {} ; {} — {verdict}.", precision_text(s), bot::points(s.excess), bot::t_text(s.t))
}

// ---------- Live models and today's avis ----------

/// The models and gates « now » for one group and horizon.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct V4LiveH {
    pub horizon: usize,
    pub a: Option<[Pair; 2]>,
    pub a_gates: [Gate; 2],
    pub b: Option<[PeerModel; 2]>,
    pub b_gates: [Gate; 2],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct V4LiveGroup {
    pub group: BotGroup,
    pub horizons: Vec<V4LiveH>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct V4Live {
    pub groups: Vec<V4LiveGroup>,
}

impl V4LiveH {
    /// Whether the bot `side` speaks at `x` (family B: at its peers features), with the shown probability; None
    /// without a model.
    pub fn avis(&self, side: Side, x: &Features, trend: i8, peers_x: Option<&Features>) -> Option<(bool, f64)> {
        let (s, gate) = if side.peers() {
            let m = self.b.as_ref()?;
            (b_scores([&m[0], &m[1]], peers_x?, side), self.b_gates[side.slot()])
        } else {
            let m = self.a.as_ref()?;
            (a_scores([&m[0], &m[1]], x, trend, side), self.a_gates[side.slot()])
        };
        Some((gate.speaks(s), (s[0] + s[1]) / 2.0))
    }
}

/// The bots speaking today on a basket asset (ids).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V4AssetNow {
    pub time: Option<i64>,
    pub bots: Vec<String>,
}

// ---------- Report ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V4Parameters {
    /// Levels of the grid (% of W0's rows above each model's threshold).
    pub levels: Vec<f64>,
    pub min_val_signals: usize,
    pub wilson_z: f64,
    pub min_signals: usize,
    pub nudge: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V4Report {
    pub version: u32,
    pub prereg_date: String,
    pub forward_from: i64,
    pub after_prereg: Vec<String>,
    pub k: K4,
    pub alpha: f64,
    pub t_required: f64,
    pub headline: String,
    pub forward_headline: String,
    /// Stocks then cryptos; 20 then 60 days; rise, fall, top, bottom.
    pub bots: Vec<SelectiveBot>,
    pub method: Vec<String>,
    pub limits: Vec<String>,
    pub parameters: V4Parameters,
}

impl V4Report {
    pub fn bot(&self, id: &str) -> Option<&SelectiveBot> {
        self.bots.iter().find(|b| b.id == id)
    }
}

fn frdate(iso: &str) -> String {
    iso.split('-').rev().collect::<Vec<_>>().join("/")
}

/// The verdict in plain French, from the numbers only.
pub fn headline(bots: &[SelectiveBot], t_req: f64, k: usize) -> String {
    if bots.is_empty() {
        return "Bots sélectifs : aucun bot n'a pu être entraîné.".into();
    }
    let list = |st: Status| {
        bots.iter().filter(|b| b.status == st).map(|b| format!("{} ({})", b.label.to_lowercase(), precision_text(&b.main))).collect::<Vec<_>>()
    };
    let (proven, pending, contradicted) = (list(Status::Proven), list(Status::Pending), list(Status::Contradicted));
    let mut out = if !proven.is_empty() {
        format!(
            "Bots sélectifs : précision démontrée et confirmée sur l'avenir pour {} ; seuls ceux-là comptent, un peu, dans les décisions.",
            proven.join(" ; ")
        )
    } else if !pending.is_empty() {
        format!(
            "Bots sélectifs : précision démontrée sur le passé (borne basse de l'intervalle au-dessus du hasard, t ≥ {}) pour {}, en attente de {MIN_TRADES} signaux sur l'avenir ; d'ici là ils ne comptent pas.",
            fr(t_req, 0, 2),
            pending.join(" ; ")
        )
    } else {
        format!(
            "Bots sélectifs : aucun des {} bots n'a démontré une précision supérieure au hasard au seuil corrigé (t ≥ {}, {k} tests comptés) ; ils sont affichés « non prouvés » et ne pèsent pas dans les décisions.",
            bots.len(),
            fr(t_req, 0, 2)
        )
    };
    if !proven.is_empty() && !pending.is_empty() {
        out.push_str(&format!(" En attente de confirmation : {}.", pending.join(" ; ")));
    }
    if !contradicted.is_empty() {
        out.push_str(&format!(" Contredits par le test sur l'avenir : {}.", contradicted.join(" ; ")));
    }
    let raw: Vec<String> =
        bots.iter().filter(|b| b.status == Status::NotProven && b.main.raw_verdict == Precision::Precise).map(|b| b.label.to_lowercase()).collect();
    if !raw.is_empty() {
        out.push_str(&format!(" Au seuil simple t ≥ 2 seulement (insuffisant après tant d'essais) : {}.", raw.join(" ; ")));
    }
    out
}

/// The forward test so far, in plain French.
pub fn forward_headline(bots: &[SelectiveBot]) -> String {
    let parts: Vec<String> =
        bots.iter().filter(|b| b.forward.signals > 0).map(|b| format!("{} : {}", b.label.to_lowercase(), precision_text(&b.forward))).collect();
    if parts.is_empty() {
        "Aucun signal daté du 02/10/2026 ou après dont l'issue est connue : le test sur l'avenir commence (il faut 20 à 60 jours de bourse pour juger un signal).".into()
    } else {
        format!("Depuis le 02/10/2026 : {}.", parts.join(" ; "))
    }
}

pub fn method(t_req: f64, k: &K4) -> Vec<String> {
    vec![
        "But : la précision, pas la couverture. Chaque bot parle rarement, seulement quand deux modèles sont d'accord sur un score extrême ; sinon il n'a « pas d'avis ».".into(),
        "16 bots fixés d'avance : actions et cryptos × 20 et 60 jours × hausse, baisse, haut et bas du classement entre pairs.".into(),
        format!(
            "Seuil choisi à chaque réentraînement sur la dernière année de l'entraînement seulement (jamais sur le test) : parmi les niveaux {} % des scores les plus extrêmes, celui qui maximise la borne basse de l'intervalle de Wilson, avec au moins {MIN_VAL_SIGNALS} signaux ; le bot se tait sur tout le bloc si cette borne ne dépasse pas le taux de base.",
            LEVELS.iter().map(|q| fr(q * 100.0, 0, 1)).collect::<Vec<_>>().join(" / ")
        ),
        "Précision = part des signaux qui réussissent, avec son intervalle de Wilson à 95 % ; comparée au taux de base et à une entrée au hasard (même actif et même bloc, ou même jour entre pairs) ; la plus haute des deux sert de référence.".into(),
        format!(
            "« Précis » : au moins {MIN_TRADES} signaux, borne basse de l'intervalle au-dessus de la référence, t par jour ≥ {} (seuil corrigé pour {} tests comptés depuis la v1 : {} en v1, {} en v2, {} en v3, {} en v4) et écart moyen positif face au hasard ; sinon « non prouvé ».",
            fr(t_req, 0, 2),
            k.total,
            k.v1,
            k.v2,
            k.v3,
            k.v4
        ),
        format!(
            "Dans une décision : un bot ne compte qu'une fois précis sur le passé et confirmé par au moins {MIN_TRADES} signaux sur l'avenir ; au plus {} points de confiance avec la v3, jamais contre un veto ni le verdict.",
            fr(NUDGE, 0, 0)
        ),
        format!("Protocole pré-enregistré le {} ; les signaux datés du 02/10/2026 ou après forment le test sur l'avenir.", frdate(PREREG_DATE)),
    ]
}

pub fn limits() -> Vec<String> {
    vec![
        "L'intervalle de Wilson suppose des signaux indépendants ; or plusieurs actifs signalent souvent le même jour : le t compté par jour est là pour ça, l'intervalle reste optimiste.".into(),
        "Un bot très sélectif donne peu de signaux : son intervalle est large, et un vrai petit avantage peut ne pas être prouvé.".into(),
        "Mêmes données que la v3 : biais du survivant (grands actifs d'aujourd'hui), coûts supposés, cours non ajustés des dividendes.".into(),
        "Réussir souvent ne suffit pas : une baisse annoncée peut être juste mais petite ; l'écart moyen face au hasard est montré à côté.".into(),
        "Déjà regardé : les résultats v1 à v3 étaient connus quand ce protocole a été écrit ; le seuil corrigé et le test sur l'avenir sont là pour ça.".into(),
    ]
}

// ---------- Decision view ----------

/// One bot today for one asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V4Signal {
    pub id: String,
    pub label: String,
    pub horizon: usize,
    pub side: Side,
    /// The bot's action when it speaks; None: « pas d'avis ».
    pub action: Option<BotAction>,
    /// Mean of the two models' probabilities (%), when it speaks.
    pub probability: Option<f64>,
    pub precision: Option<f64>,
    pub wilson_low: Option<f64>,
    pub wilson_high: Option<f64>,
    pub reference: Option<f64>,
    pub per_year: Option<f64>,
    pub status: Status,
    pub counts: bool,
    pub text: String,
}

/// v4 in a decision: the group's 8 bots today; `counts` when a proven one speaks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V4View {
    pub available: bool,
    pub signals: Vec<V4Signal>,
    pub counts: bool,
    pub nudge: f64,
    pub t_required: f64,
    pub note: String,
    pub pro: Option<String>,
    pub con: Option<String>,
    pub con_held: Option<String>,
}

impl V4View {
    /// The pro or con line of a decision, when it counts: (is_pro, text).
    pub fn line(&self, held: bool) -> Option<(bool, String)> {
        if !self.counts {
            return None;
        }
        if let Some(p) = &self.pro {
            return Some((true, p.clone()));
        }
        if let Some(c) = &self.con {
            return Some((false, c.clone()));
        }
        if held { self.con_held.clone().map(|c| (false, c)) } else { None }
    }
}

/// v4's view of an asset from its features at the last closed daily candle; None without a v4 report.
pub fn view(report: &bot::BotReport, group: BotGroup, symbol: &str, x: &Features, trend: i8) -> Option<V4View> {
    let r = report.v4.as_ref()?;
    let Some(live) = report.v4_live.as_ref().and_then(|l| l.groups.iter().find(|g| g.group == group)) else {
        let why = if report.v4_live.is_none() { "rapport relu depuis un fichier" } else { "pas de modèle pour ce groupe" };
        return Some(V4View {
            t_required: r.t_required,
            note: format!("Bots sélectifs indisponibles ({why}) : ils ne comptent pas."),
            ..V4View::default()
        });
    };
    let v3_live = report.v3_live.as_ref().and_then(|l| l.groups.iter().find(|g| g.group == group));
    let mut v = V4View { available: true, t_required: r.t_required, ..V4View::default() };
    let (mut buy, mut sell) = (false, false);
    let mut pending: Option<String> = None;
    for lh in &live.horizons {
        // The peers features against the universe's last day (v3's reference), as v3's view.
        let reference = v3_live.and_then(|g| g.horizons.iter().find(|x| x.horizon == lh.horizon)).map(|x| &x.reference);
        let px = reference.filter(|r| r.len() + 1 >= bot_v3::MIN_XS).map(|r| peers_features(x, r, symbol));
        for side in SIDES {
            let Some(b) = r.bot(&bot_id(group, lh.horizon, side)) else { continue };
            let avis = lh.avis(side, x, trend, px.as_ref());
            let speaks = avis.is_some_and(|a| a.0);
            let counts = speaks && b.status == Status::Proven;
            let what = match avis {
                None => "pas de modèle".to_string(),
                Some((true, _)) => side.avis().to_string(),
                Some((false, _)) => "pas d'avis".into(),
            };
            let base = format!("{} : {what}", b.label);
            if speaks && b.status == Status::Pending {
                pending.get_or_insert_with(|| {
                    format!(
                        "Bot sélectif {} : précis sur le passé ({}) mais il ne comptera qu'après {MIN_TRADES} signaux sur l'avenir qui le confirment ({} à ce jour).",
                        b.label.to_lowercase(),
                        precision_text(&b.main),
                        b.forward.signals
                    )
                });
            }
            if counts {
                let p = precision_text(&b.main);
                match side {
                    Side::Rise | Side::Top => {
                        buy = true;
                        v.pro.get_or_insert_with(|| format!("Bot sélectif favorable ({base}) : {p}, confirmé sur l'avenir"));
                    }
                    Side::Fall => {
                        sell = true;
                        v.con.get_or_insert_with(|| format!("Bot sélectif défavorable ({base}) : {p}, confirmé sur l'avenir"));
                    }
                    Side::Bottom => {
                        sell = true;
                        v.con_held.get_or_insert_with(|| {
                            format!("Bot sélectif : {base} ; alléger au profit d'actifs mieux classés peut se discuter ({p})")
                        });
                    }
                }
            }
            v.signals.push(V4Signal {
                id: b.id.clone(),
                label: b.label.clone(),
                horizon: lh.horizon,
                side,
                action: speaks.then_some(side.action()),
                probability: avis.filter(|a| a.0).map(|a| round_to(a.1 * 100.0, 1)),
                precision: b.main.precision,
                wilson_low: b.main.wilson_low,
                wilson_high: b.main.wilson_high,
                reference: b.main.reference,
                per_year: b.main.per_year,
                status: b.status,
                counts,
                text: format!("{base} — {} ({}).", precision_text(&b.main), b.status.label()),
            });
        }
    }
    if buy && sell {
        (v.pro, v.con, v.con_held) = (None, None, None);
    }
    v.counts = buy != sell;
    v.nudge = match (buy, sell) {
        (true, false) => NUDGE,
        (false, true) => -NUDGE,
        _ => 0.0,
    };
    let speaking = v.signals.iter().filter(|s| s.action.is_some()).count();
    v.note = if v.counts {
        format!(
            "Un bot sélectif prouvé et confirmé sur l'avenir donne un avis aujourd'hui : il compte un peu (au plus {} points), jamais contre un veto.",
            fr(NUDGE, 0, 0)
        )
    } else if buy && sell {
        "Bots sélectifs prouvés contradictoires aujourd'hui : ils ne comptent pas.".into()
    } else if let Some(p) = pending {
        p
    } else if speaking == 0 {
        "Bots sélectifs : pas d'avis aujourd'hui (aucun score assez extrême) ; ils ne comptent pas.".into()
    } else {
        format!(
            "Bots sélectifs : {speaking} avis aujourd'hui, mais aucun n'a de précision démontrée au seuil corrigé (t ≥ {}) et confirmée sur l'avenir : ils ne comptent pas.",
            fr(r.t_required, 0, 2)
        )
    };
    Some(v)
}

/// v3 and v4 together in a decision (see the module doc, 8): ± `NUDGE` at most, 0 when counting signals disagree.
pub fn combined_nudge(v3: f64, v4: f64) -> f64 {
    if v3 * v4 < 0.0 { 0.0 } else { (v3 + v4).signum() * if v3 + v4 == 0.0 { 0.0 } else { NUDGE } }
}

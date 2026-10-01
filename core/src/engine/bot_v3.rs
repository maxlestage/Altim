//! « Bot Altim » v3: longer data, better-founded targets, trees trained longer with early stopping, a stricter bar
//! and a genuine forward test. Pure: the route fetches the histories, this module computes.
//!
//! PRE-REGISTRATION — fixed on 2026-09-30 (`PREREG_DATE`), written before any v3 figure was computed on real data,
//! and not to be changed after seeing results (a change made afterwards must be listed in `AFTER_PREREG` and shown).
//! v1 (2026-09-29, no edge) and v2 (2026-09-29, no out-of-sample edge; stocks nested: buys t by date −2.4, sells
//! −4.5; cryptos inconclusive; holding beat it everywhere; its log-loss choice favoured the trend rule) were seen
//! before this was written: hence the corrected threshold and the forward test.
//!
//! 1. Data. Stocks and ETFs: Yahoo Finance daily candles from 1990-01-01 (or the first quote) to now (`bot_history`,
//!    `STOCK_FROM_S`), split-adjusted, not dividend-adjusted. The stocks' market is still SPY (quoted since
//!    1993-01-29): a stock feature row needs 200 SPY sessions, so stock rows start in late 1993. Cryptos as v2
//!    (Bitstamp since listing or Yahoo, the longer kept). Universe unchanged: the validation's 34-asset basket (the
//!    headline test) and v2's `EXTRA` (91 assets, training, reported apart) — 125 large assets of today: survivorship
//!    bias, a stated limit. Features (v2's 24) and labels (bought at the next open, sold at the open H candles later,
//!    v2's costs) point-in-time as in v2.
//! 2. Horizons H ∈ {20, 60} trading days (`HORIZONS`), everything computed and reported separately for each. Purge =
//!    H dates; one training row in 5 days per asset; retraining every 252 dates; ≥ 504 dates of training before the
//!    first test block; inner validation = the last 252 dates of each training window (v2's rules).
//! 3. Families and candidates (fixed list, `A_CANDS`, `B_CANDS`):
//!    A « absolu » (v2's targets: net rise over H above the round-trip cost / fall beyond it; actions as v2): the trend
//!      rule (not fitted), v1's logistic regression (16 features), the logistic regression on 24, v2's boosted trees
//!      (100 trees, depth 3, shrinkage 0.1) and `TreesLong`: boosted trees trained longer — up to 1 000 rounds, depth
//!      ≤ 4, shrinkage 0.05, leaves ≥ 200 rows, 32 quantile bins, λ = 1 — whose number of rounds is chosen by early
//!      stopping (patience 50 rounds on the validation log-loss, `bot_trees::LONG`).
//!    B « classement entre pairs » (cross-sectional): the label is whether the asset's gross return over H beats the
//!      median gross return of its group's universe assets labelled that day (stocks vs stocks, cryptos vs cryptos,
//!      basket and extra together, ≥ `MIN_XS` = 5 of them). Features: each day, the 17 asset-specific features become
//!      percentile ranks (0-1) among the group's universe assets having a feature row that day (≥ 5); the 4 regime
//!      dummies and the 3 market features are left raw. Candidates: `XsMomentum` (rule, not fitted: score = 12-1
//!      month momentum, Jegadeesh-Titman), `XsLogit` (logistic regression on the 24, v2's L2) and `XsTrees` (as
//!      `TreesLong`). Each day, among the assets with a score (≥ 5): ACHETER for the top quintile (the ⌊n/5⌋ highest
//!      scores), VENDRE for the bottom quintile (for a holder: lighten or leave in favour of better-ranked assets —
//!      never a short sale), ATTENDRE otherwise; ties broken by the fixed universe order.
//!    Baseline (a): v2's nested selection (log-loss, candidates v1 / logit / trees / trend), unchanged code path
//!      (`bot::select_h`), at both horizons.
//!    Rule baselines not fitted (d): `XsMomentum` above, the trend rule above, and a volatility-managed trend
//!      (Moreira-Muir style): exposure = 1[close > 200-day simple average] × min(1, σ*² ÷ σ²), σ = standard deviation of
//!      the last 20 daily log returns, σ* = median of the asset's σ from its first feature day up to that day
//!      (point-in-time), rounded down to 0, ¼, ½, ¾ or 1 (never leveraged), applied from the next open, each change of
//!      exposure paying |Δ| × half the round-trip cost. Compared with holding the same asset over the same days: return,
//!      volatility and Sharpe annualised (√252 stocks, √365 cryptos, no risk-free rate), max drawdown; per basket
//!      asset (medians, counts) and for an equal-weight portfolio of the basket assets (daily mean).
//! 4. Early stopping, never on the test and never on the window that chooses: at each retraining, W0 = the inner
//!    validation (last 252 dates of the training window, labels known before the cutoff), T0 = the training rows whose
//!    label ended H dates before W0; W1 = the 252 dates before T0's cutoff (labels known before it), T1 = the rows
//!    whose label ended H dates before W1. `TreesLong` / `XsTrees` are fitted on T1 with early stopping on W1 → R
//!    rounds; refitted with R rounds on T0 (scored on W0) and on the whole training window (the test block). Not
//!    fitted in a block without W1 / T1 (the first ones). Training rows of these trees are capped at `MAX_TREE_ROWS`
//!    (an even stride over the training rows, deterministic) for time and memory.
//! 5. Selection (economic, fixed ahead), per family at each retraining: every candidate fitted on T0 (rules: none) is
//!    scored on W0 by the mean excess of its ACHETER signals (non-overlapping per asset, as the test) over its
//!    baseline — A: a random entry on the same asset in W0 (the mean net return of its W0 days); B: the group's median
//!    net return that day — averaged by date first (a day's signals count once), then over the dates. Eligible with
//!    ≥ `MIN_VAL_SIGNALS` = 30 signals; the highest wins; ties (1e-9) go to the simpler one in the list order
//!    (A: trend, v1, logit, trees, trees long; B: momentum, logit, trees); none eligible: the rule. The chosen one is
//!    refitted on the whole training window and predicts the next block. « Now »: the same with every known label.
//! 6. Evaluation of each configuration (a family's nested selection — the headline; each candidate alone, for
//!    information only; the v2 baseline) per group and horizon: basket assets, signals dated up to 2026-09-30
//!    included; extra universe apart; same days for all. A as v2 (baseline: random entry on the same asset and block;
//!    VENDRE: what holding made next vs a random day; H days out after each VENDRE for the holding comparison). B:
//!    ACHETER excess = net return − the group's median net return that day; VENDRE benefit = the group's median net
//!    return − the asset's gross return (leaving it for a median asset, round trip paid). Non-overlapping signals per
//!    asset and side. t by date (the verdict's), by asset and per signal shown.
//! 7. Multiple testing: K = configurations × sides evaluated in v1 (1 model × 2 groups × 2 sides = 4), v2 (nested + 4
//!    candidates = 5 × 2 × 2 = 20) and v3 (per group and horizon: v2 baseline + 2 nested + 8 candidates = 11, × 2
//!    sides → 88; plus the volatility-managed trend × 2 groups → 90): K = 114 (`k_count`). Required t = z(1 − 0.025 ÷
//!    K) (Bonferroni, two-sided 5 %) ≈ 3.52. Verdict: « avantage » only with ≥ 30 signals, a positive mean and t by
//!    date ≥ the required t; « négatif » when t ≤ −required; « insuffisant » under 30; otherwise « non démontré ». The
//!    raw t ≥ 2 verdict is shown next to it for comparison only.
//! 8. Forward test: every signal dated after 2026-09-30 (`FORWARD_FROM`), basket assets, computed by the same frozen
//!    walk-forward (deterministic and point-in-time: nothing stored), reported apart as « Depuis le 30/09/2026 (test
//!    sur l'avenir) »; it starts empty and grows as outcomes mature; same statistics and required t. The only truly
//!    fresh test.
//! 9. Decision: only the 4 headline configurations (A and B nested × H 20 / 60) can count, on a side whose verdict is
//!    « avantage » at the corrected threshold and not contradicted by the forward test (≥ 30 forward signals with a
//!    mean excess ≤ 0). Confidence ± 3 at most (0 when a counting buy and a counting sell disagree), never the verdict
//!    or a veto. A B VENDRE becomes a con line only for a holder.
//! 10. Compute: may run minutes on one core, in the background; peak memory < 350 MB (f32 features, one group and
//!    horizon at a time, tree rows capped). Time and memory measured and reported. The v2-shaped fields of the report
//!    hold the v2 baseline at H = 20 (older clients read a v2 report), judged at the corrected threshold, dated up to
//!    2026-09-30.

// The pre-registration above is kept as written (its list layout included).
#![allow(clippy::doc_overindented_list_items, clippy::doc_lazy_continuation)]

/// Version of the report.
pub const VERSION: u32 = 3;
/// Date of the pre-registration (the forward test starts the day after).
pub const PREREG_DATE: &str = "2026-09-30";
/// 2026-10-01 00:00 UTC (ms): signals dated from then on form the forward test.
pub const FORWARD_FROM: i64 = 1_790_812_800_000;
/// Label horizons (trading days / daily candles), reported separately.
pub const HORIZONS: [usize; 2] = [20, 60];
/// Assets needed on a day for a cross-sectional rank, median or quintile.
pub const MIN_XS: usize = 5;
/// Signals needed on the inner validation for a candidate to be chosen.
pub const MIN_VAL_SIGNALS: usize = 30;
/// Training rows of the long trees, at most (even stride).
pub const MAX_TREE_ROWS: usize = 40_000;
/// Configurations × sides evaluated in v1 and v2.
pub const K_V1: usize = 4;
pub const K_V2: usize = 20;
/// Family-wise error rate of the corrected threshold (two-sided).
pub const ALPHA: f64 = 0.05;
/// Changes made after the pre-registration, listed in the report (never hidden).
pub const AFTER_PREREG: [&str; 2] = [
    "Contrôle ajouté le 29/09/2026 après le premier calcul réel : le classement entre pairs est aussi jugé face à la moyenne à parts égales du groupe, et pas seulement face à sa médiane. Les rendements étant asymétriques (quelques fortes hausses), la moyenne d'un groupe d'actifs pris au hasard dépasse sa médiane : face à la médiane, n'importe quelle sélection paraît gagner à l'achat et perdre à la vente. Un avantage entre pairs ne compte désormais dans les décisions que s'il tient face aux deux (règle plus stricte que celle pré-enregistrée).",
    "Règle ajoutée le 30/09/2026 après le premier calcul réel (seul avantage restant, achats entre pairs sur les actions à 20 jours : t = 3,65 par jour contre 3,52 exigé, mais 0,6 par actif, donc fragile) : un côté prouvé sur le passé ne compte dans les décisions qu'une fois que le test sur l'avenir a au moins 30 signaux et un écart moyen non négatif (face aux deux références pour le classement entre pairs). D'ici là il est seulement affiché. Règle plus stricte que celle pré-enregistrée.",
];

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::bot::{
    self, ALL_COLS, BotAction, BotAssetRow, BotGroup, BotGroupStat, BotReport, BotStats, Candidate, ExitStats, Features, GROUPS, History, INNER_VAL,
    MIN_TRAIN, MIN_TRAIN_ROWS, Model, Pair, Prediction, RETRAIN_EVERY, Row, Samples, Selection, SideModel, WalkForward, day_of, evaluate_h,
    group_rows, mean, points, r2, round_to, round_trip_cost, select_h, share, signed, stats_t, t_text, timeline, trend_action,
};
use super::bot_trees::{self, Forest, Stop};
use super::bot_v4;
use super::signal::sanitize;
use super::validation::{BASKET, Failure, MIN_TRADES, T_EDGE, Verdict, median};
use crate::js::fr;
use crate::types::{Candle, DAY_MS, Kind};

/// Stocks' daily history asked from (1990-01-01, s).
pub const STOCK_FROM_S: i64 = 631_152_000;
/// Blocks computed at the same time, at most (same result on one thread): each thread holds its own training copies.
pub const MAX_THREADS: usize = 2;
/// Threads by default: one. Measured on the real run: peak 301 MB on one thread, 351 MB on two (the dyno has 512 MB
/// for the whole server); `ALTIM_BOT_THREADS=2` allows two.
pub const DEFAULT_THREADS: usize = 1;
/// Confidence change at most (v2's).
pub const NUDGE: f64 = bot::NUDGE;

// ---------- Candidates and families ----------

/// The fixed candidates of v3 (see the module doc): family A first, then B, each in the order of simplicity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum V3Candidate {
    #[default]
    Trend,
    V1,
    Logit,
    Trees,
    TreesLong,
    XsMomentum,
    XsLogit,
    XsTrees,
}

pub const A_CANDS: [V3Candidate; 5] = [V3Candidate::Trend, V3Candidate::V1, V3Candidate::Logit, V3Candidate::Trees, V3Candidate::TreesLong];
pub const B_CANDS: [V3Candidate; 3] = [V3Candidate::XsMomentum, V3Candidate::XsLogit, V3Candidate::XsTrees];

/// A family of targets: « absolu » (rise / fall of the asset), « entre pairs » (vs its group's median), or v2's
/// selection (baseline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    #[default]
    Absolute,
    Peers,
    V2,
}

impl Family {
    pub fn label(self) -> &'static str {
        match self {
            Family::Absolute => "Hausse ou baisse de l'actif",
            Family::Peers => "Classement entre pairs",
            Family::V2 => "Sélection v2 (log-loss)",
        }
    }
}

impl V3Candidate {
    pub fn family(self) -> Family {
        if B_CANDS.contains(&self) { Family::Peers } else { Family::Absolute }
    }

    /// Slot of an A candidate in the walk-forward's predictions (v2's `CANDIDATES` order, then the long trees).
    pub fn a_slot(self) -> Option<usize> {
        match self {
            V3Candidate::V1 => Some(0),
            V3Candidate::Logit => Some(1),
            V3Candidate::Trees => Some(2),
            V3Candidate::Trend => Some(3),
            V3Candidate::TreesLong => Some(4),
            _ => None,
        }
    }

    pub fn b_slot(self) -> Option<usize> {
        B_CANDS.iter().position(|c| *c == self)
    }

    pub fn id(self) -> &'static str {
        match self {
            V3Candidate::Trend => "trend",
            V3Candidate::V1 => "v1",
            V3Candidate::Logit => "logit",
            V3Candidate::Trees => "trees",
            V3Candidate::TreesLong => "treesLong",
            V3Candidate::XsMomentum => "xsMomentum",
            V3Candidate::XsLogit => "xsLogit",
            V3Candidate::XsTrees => "xsTrees",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            V3Candidate::Trend => "Règle de tendance",
            V3Candidate::V1 => "Régression logistique v1",
            V3Candidate::Logit => "Régression logistique 24 mesures",
            V3Candidate::Trees => "Arbres boostés v2",
            V3Candidate::TreesLong => "Arbres boostés longs",
            V3Candidate::XsMomentum => "Momentum 12-1 entre pairs",
            V3Candidate::XsLogit => "Régression logistique entre pairs",
            V3Candidate::XsTrees => "Arbres boostés longs entre pairs",
        }
    }

    pub fn description(self) -> String {
        let l = bot_trees::LONG;
        let long = format!(
            "jusqu'à {} arbres (profondeur {} au plus, pas de {}), nombre d'arbres fixé par arrêt anticipé sur une fenêtre de validation interne, jamais sur le test",
            l.trees,
            l.depth,
            fr(l.shrinkage, 0, 2)
        );
        match self {
            V3Candidate::Trend => "Règle publiée, sans apprentissage : ACHETER au-dessus de la moyenne 200 jours avec un momentum 12-1 mois positif, VENDRE sous la moyenne avec un momentum négatif.".into(),
            V3Candidate::V1 => "Le modèle de la v1 : régression logistique sur les 16 mesures d'origine.".into(),
            V3Candidate::Logit => "Régression logistique sur les 24 mesures de la v2.".into(),
            V3Candidate::Trees => "Les arbres de la v2 : 100 arbres de profondeur 3, pas de 0,1.".into(),
            V3Candidate::TreesLong => format!("Arbres entraînés plus longtemps : {long}."),
            V3Candidate::XsMomentum => "Règle publiée (Jegadeesh et Titman), sans apprentissage : chaque jour, ACHETER les 20 % d'actifs du groupe au meilleur momentum 12-1 mois, VENDRE les 20 % au plus faible.".into(),
            V3Candidate::XsLogit => "Régression logistique qui estime la probabilité de faire mieux que la médiane du groupe, sur les mesures classées entre actifs du jour.".into(),
            V3Candidate::XsTrees => format!("Même cible entre pairs, {long}."),
        }
    }
}

/// A peers model: the rule's score (12-1 momentum, ranked), or a fitted probability of beating the median.
#[derive(Debug, Clone, PartialEq)]
pub enum PeerModel {
    Momentum,
    Logit(Model),
    Trees(Forest),
}

impl PeerModel {
    pub fn score(&self, x: &[f32]) -> f64 {
        match self {
            PeerModel::Momentum => x[16] as f64,
            PeerModel::Logit(m) => m.prob(x),
            PeerModel::Trees(f) => f.prob(x),
        }
    }

    /// Training share of the label (none for the rule).
    pub fn base_rate(&self) -> Option<f64> {
        match self {
            PeerModel::Momentum => None,
            PeerModel::Logit(m) => Some(m.base_rate),
            PeerModel::Trees(f) => Some(f.base_rate),
        }
    }
}

// ---------- Multiple testing ----------

/// Configurations × sides evaluated (see the module doc, 7).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct KCount {
    pub v1: usize,
    pub v2: usize,
    pub v3: usize,
    pub total: usize,
}

pub fn k_count() -> KCount {
    let per = 1 + 2 + A_CANDS.len() + B_CANDS.len();
    let v3 = GROUPS.len() * HORIZONS.len() * per * 2 + GROUPS.len();
    KCount { v1: K_V1, v2: K_V2, v3, total: K_V1 + K_V2 + v3 }
}

/// Standard normal quantile (Acklam's rational approximation, relative error < 1.2e-9).
#[allow(clippy::excessive_precision)] // the published coefficients, as written
pub fn norm_quantile(p: f64) -> f64 {
    const A: [f64; 6] =
        [-3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02, 1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00];
    const B: [f64; 5] = [-5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02, 6.680131188771972e+01, -1.328068155288572e+01];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00, 3.754408661907416e+00];
    let p = p.clamp(1e-300, 1.0 - 1e-16);
    let low = 0.02425;
    if p < low {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5]) / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - low {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        -norm_quantile(1.0 - p)
    }
}

/// |t| needed for an edge after `k` tests (Bonferroni, two-sided `ALPHA`).
pub fn t_required(k: usize) -> f64 {
    norm_quantile(1.0 - ALPHA / 2.0 / k.max(1) as f64)
}

// ---------- Small helpers ----------

/// `f(0..n)` on up to `threads` threads, in order (same result on one).
pub fn par_map<T: Send>(n: usize, threads: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = threads.clamp(1, MAX_THREADS);
    if threads == 1 || n < 2 {
        return (0..n).map(f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if k >= n {
                        break;
                    }
                    let r = f(k);
                    done.lock().unwrap().push((k, r));
                }
            });
        }
    });
    let mut out = done.into_inner().unwrap();
    out.sort_by_key(|(k, _)| *k);
    out.into_iter().map(|(_, r)| r).collect()
}

/// At most `MAX_TREE_ROWS` rows, an even stride over them (deterministic).
pub fn cap_rows<T: Copy>(rows: &[T]) -> Vec<T> {
    if rows.len() <= MAX_TREE_ROWS {
        return rows.to_vec();
    }
    let step = rows.len().div_ceil(MAX_TREE_ROWS);
    rows.iter().step_by(step).copied().collect()
}

fn both(ys: &[bool]) -> bool {
    ys.iter().any(|y| *y) && ys.iter().any(|y| !*y)
}

/// Early stopping of the long trees on `t` (training) and `w` (validation) with the labels `label`: the rounds, or
/// None when either set is too small or one-sided.
pub fn es_rounds(t: &[(&[f32], bool)], w: &[(&[f32], bool)]) -> Option<Stop> {
    let t = cap_rows(t);
    let (tx, ty): (Vec<&[f32]>, Vec<bool>) = t.into_iter().unzip();
    let (wx, wy): (Vec<&[f32]>, Vec<bool>) = w.iter().copied().unzip();
    if tx.len() < MIN_TRAIN_ROWS || wx.is_empty() || !both(&ty) {
        return None;
    }
    Forest::fit_params(&tx, &ty, &ALL_COLS, &bot_trees::LONG, Some((&wx, &wy))).1
}

/// Long trees with a fixed number of rounds on capped rows; None under `MIN_TRAIN_ROWS` or one-sided.
pub fn long_trees(t: &[(&[f32], bool)], rounds: usize) -> Option<Forest> {
    let t = cap_rows(t);
    let (tx, ty): (Vec<&[f32]>, Vec<bool>) = t.into_iter().unzip();
    if tx.len() < MIN_TRAIN_ROWS || !both(&ty) || rounds == 0 {
        return None;
    }
    Some(Forest::fit_params(&tx, &ty, &ALL_COLS, &bot_trees::Params { trees: rounds, ..bot_trees::LONG }, None).0)
}

/// The inner windows of a retraining whose cutoff is the date index `c` (see the module doc, 4): the validation's
/// first date index and the training cutoff index before it (labels ended before it); None without room.
pub fn inner_window(c: usize, h: usize, n_dates: usize) -> Option<(usize, usize)> {
    let v = c.min(n_dates).checked_sub(h + INNER_VAL).filter(|v| *v >= h)?;
    Some((v, v - h))
}

/// A candidate's score on a validation window: its ACHETER signals' excesses (day, excess) averaged by day first,
/// then over the days; with the signal count.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EconScore {
    pub id: V3Candidate,
    /// Mean excess (points of %) of the ACHETER signals, by date; None when the candidate could not be fitted or
    /// scored.
    pub score: Option<f64>,
    pub signals: usize,
}

fn econ(id: V3Candidate, signals: &[(i64, f64)]) -> EconScore {
    let values: Vec<f64> = signals.iter().map(|s| s.1).collect();
    let days: Vec<i64> = signals.iter().map(|s| s.0).collect();
    EconScore { id, score: mean(&bot::cluster_means(&values, &days)), signals: signals.len() }
}

/// The highest eligible score (≥ `MIN_VAL_SIGNALS` signals, candidate available); ties go to the earlier one;
/// none eligible: `fallback` (the family's rule).
pub fn choose(scores: &[EconScore], available: impl Fn(V3Candidate) -> bool, fallback: V3Candidate) -> Option<V3Candidate> {
    let mut best: Option<(V3Candidate, f64)> = None;
    for s in scores {
        if let Some(v) = s.score.filter(|_| s.signals >= MIN_VAL_SIGNALS && available(s.id))
            && best.is_none_or(|(_, b)| v > b + 1e-9)
        {
            best = Some((s.id, v));
        }
    }
    best.map(|b| b.0).or_else(|| available(fallback).then_some(fallback))
}

/// Non-overlapping ACHETER signals of family A on a window (rows of one or more assets in (asset, time) order, all
/// labelled): (day, net − the mean net of the same asset's rows in the window).
pub fn a_signals(window: &[&Row], action: impl Fn(&Row) -> BotAction) -> Vec<(i64, f64)> {
    let mut base: HashMap<usize, (f64, usize)> = HashMap::new();
    for r in window {
        if let Some(f) = r.fwd {
            let e = base.entry(r.asset).or_default();
            e.0 += f.net;
            e.1 += 1;
        }
    }
    let mut next: HashMap<usize, i64> = HashMap::new();
    let mut out = Vec::new();
    for r in window {
        let Some(f) = r.fwd else { continue };
        if action(r) == BotAction::Buy && r.time >= *next.get(&r.asset).unwrap_or(&i64::MIN) {
            next.insert(r.asset, f.exit_time);
            let (s, n) = base[&r.asset];
            out.push((day_of(r.time), f.net - s / n as f64));
        }
    }
    out
}

// ---------- Peers: ranks, labels, quintiles ----------

/// Asset-specific features turned into ranks among peers (the regime dummies and market features stay raw).
pub const XS_RANKED: [usize; 17] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 16, 17, 18, 19, 20];

/// A row's peers information: its features were ranked (≥ `MIN_XS` assets that day), and its label against the
/// median when ≥ `MIN_XS` assets were labelled that day.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct XsRow {
    pub ranked: bool,
    pub label: Option<XsLabel>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XsLabel {
    /// Gross return over H above the day's median.
    pub beat: bool,
    /// The day's median net return (%).
    pub med_net: f32,
    /// The day's equal-weight mean net return (%) (the control added after the first real run, see `AFTER_PREREG`).
    pub mean_net: f32,
    /// Last exit among the peers used: the label is only known then.
    pub known: i64,
}

/// Percentile ranks (0-1, ties averaged) of `values`.
pub fn pct_ranks(values: &[f32]) -> Vec<f32> {
    let n = values.len();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|a, b| values[*a].total_cmp(&values[*b]));
    let mut out = vec![0.5f32; n];
    if n < 2 {
        return out;
    }
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && values[idx[j + 1]] == values[idx[i]] {
            j += 1;
        }
        let r = (i + j) as f32 / 2.0 / (n - 1) as f32;
        for k in &idx[i..=j] {
            out[*k] = r;
        }
        i = j + 1;
    }
    out
}

/// Rank of `v` among `reference` plus itself (0-1, ties averaged), as `pct_ranks` would give it.
pub fn pct_rank_among(v: f32, reference: &[f32]) -> f32 {
    let below = reference.iter().filter(|x| **x < v).count() as f32;
    let equal = reference.iter().filter(|x| **x == v).count() as f32;
    let n = reference.len() as f32 + 1.0;
    if n < 2.0 { 0.5 } else { (below + equal / 2.0) / (n - 1.0) }
}

fn median32(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

/// The rows of each day (indices), days in order.
pub fn by_day(rows: &[Row]) -> BTreeMap<i64, Vec<u32>> {
    let mut m: BTreeMap<i64, Vec<u32>> = BTreeMap::new();
    for (i, r) in rows.iter().enumerate() {
        m.entry(day_of(r.time)).or_default().push(i as u32);
    }
    m
}

/// Turns the group's rows into peers rows in place: the ranks of `XS_RANKED` among the rows of the same day (≥
/// `MIN_XS` of them; true for those rows). Reads only the rows of that day (features at that close).
pub fn rank_peers(rows: &mut [Row]) -> Vec<bool> {
    let mut out = vec![false; rows.len()];
    for idx in by_day(rows).values().filter(|idx| idx.len() >= MIN_XS) {
        for &j in &XS_RANKED {
            let v: Vec<f32> = idx.iter().map(|i| rows[*i as usize].x[j]).collect();
            for (k, r) in pct_ranks(&v).into_iter().enumerate() {
                rows[idx[k] as usize].x[j] = r;
            }
        }
        for i in idx {
            out[*i as usize] = true;
        }
    }
    out
}

/// The peers labels of the rows' current labels: against the median of the same day's labelled rows (≥ `MIN_XS`),
/// known when the last of them is.
pub fn peer_labels(rows: &[Row]) -> Vec<Option<XsLabel>> {
    let mut out = vec![None; rows.len()];
    for idx in by_day(rows).values() {
        let labelled: Vec<u32> = idx.iter().copied().filter(|i| rows[*i as usize].fwd.is_some()).collect();
        if labelled.len() >= MIN_XS {
            let mut gross: Vec<f64> = labelled.iter().map(|i| rows[*i as usize].fwd.unwrap().gross).collect();
            let mut net: Vec<f64> = labelled.iter().map(|i| rows[*i as usize].fwd.unwrap().net).collect();
            let known = labelled.iter().map(|i| rows[*i as usize].fwd.unwrap().exit_time).max().unwrap_or(i64::MAX);
            let avg = net.iter().sum::<f64>() / net.len() as f64;
            let (mg, mn) = (median32(&mut gross), median32(&mut net));
            for i in &labelled {
                let f = rows[*i as usize].fwd.unwrap();
                out[*i as usize] = Some(XsLabel { beat: f.gross > mg, med_net: mn as f32, mean_net: avg as f32, known });
            }
        }
    }
    out
}

/// `rank_peers` then `peer_labels`.
pub fn to_peers(rows: &mut [Row]) -> Vec<XsRow> {
    let ranked = rank_peers(rows);
    peer_labels(rows).into_iter().zip(ranked).map(|(label, ranked)| XsRow { ranked, label }).collect()
}

/// What an asset's labels need of its (sanitized) candles: time, open and low, the same values.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prices {
    pub time: Vec<i64>,
    pub open: Vec<f64>,
    pub low: Vec<f64>,
}

impl Prices {
    pub fn of(raw: &[Candle]) -> Prices {
        let c = sanitize(raw);
        Prices { time: c.iter().map(|x| x.time).collect(), open: c.iter().map(|x| x.open).collect(), low: c.iter().map(|x| x.low).collect() }
    }

    /// `bot::forward_h` of the candle `i`, from these prices (same computation).
    pub fn forward(&self, i: usize, kind: Kind, h: usize) -> Option<bot::Forward> {
        let (e, x) = (i + 1, i + 1 + h);
        if x >= self.time.len() || self.open[e] <= 0.0 {
            return None;
        }
        let gross = (self.open[x] / self.open[e] - 1.0) * 100.0;
        let low = self.low[e..x].iter().fold(self.open[e], |a, k| a.min(*k));
        let net = bot::net_return(gross, kind);
        Some(bot::Forward {
            exit_time: self.time[x],
            entry: self.open[e],
            exit: self.open[x],
            gross,
            net,
            drawdown: (low / self.open[e] - 1.0) * 100.0,
            up: net > 0.0,
            down: gross < -round_trip_cost(kind),
        })
    }
}

/// The rows' labels over `h` candles (features unchanged): what `group_rows` at `h` would give.
pub fn relabel(rows: &mut [Row], prices: &HashMap<usize, Prices>, kinds: &[Kind], h: usize) {
    for r in rows.iter_mut() {
        let p = &prices[&r.asset];
        r.fwd = p.time.binary_search(&r.time).ok().and_then(|i| p.forward(i, kinds[r.asset], h));
    }
}

/// Actions of scored rows by day: (row, day, asset, score) → ACHETER for the ⌊n/5⌋ best of a day, VENDRE for the
/// ⌊n/5⌋ worst (order: score descending, then the universe order), ATTENDRE otherwise or under `MIN_XS` rows.
pub fn quintile_actions(items: &[(usize, i64, usize, f64)]) -> HashMap<usize, BotAction> {
    let mut days: BTreeMap<i64, Vec<(usize, usize, f64)>> = BTreeMap::new();
    for (row, day, asset, score) in items {
        days.entry(*day).or_default().push((*row, *asset, *score));
    }
    let mut out = HashMap::with_capacity(items.len());
    for (_, mut v) in days {
        v.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.1.cmp(&b.1)));
        let n = v.len();
        let k = if n >= MIN_XS { n / 5 } else { 0 };
        for (pos, (row, _, _)) in v.iter().enumerate() {
            let a = if pos < k {
                BotAction::Buy
            } else if pos >= n - k {
                BotAction::Sell
            } else {
                BotAction::Wait
            };
            out.insert(*row, a);
        }
    }
    out
}

/// Non-overlapping ACHETER signals of family B on a window of labelled rows: (day, net − the day's median net).
fn b_signals(rows: &[Row], xs: &[XsRow], window: &[usize], actions: &HashMap<usize, BotAction>) -> Vec<(i64, f64)> {
    let mut next: HashMap<usize, i64> = HashMap::new();
    let mut out = Vec::new();
    for &i in window {
        let r = &rows[i];
        let (Some(f), Some(l)) = (r.fwd, xs[i].label) else { continue };
        if actions.get(&i) == Some(&BotAction::Buy) && r.time >= *next.get(&r.asset).unwrap_or(&i64::MIN) {
            next.insert(r.asset, f.exit_time);
            out.push((day_of(r.time), f.net - l.med_net as f64));
        }
    }
    out
}

// ---------- Family A: fits and walk-forward ----------

/// Long trees for both sides (hausse, baisse) with their rounds; None when one side cannot be fitted.
fn long_pair(train: &[&Row], rounds: (usize, usize)) -> Option<Pair> {
    let up: Vec<(&[f32], bool)> = train.iter().filter_map(|r| r.fwd.map(|f| (&r.x[..], f.up))).collect();
    let down: Vec<(&[f32], bool)> = train.iter().filter_map(|r| r.fwd.map(|f| (&r.x[..], f.down))).collect();
    Some(Pair { candidate: Candidate::Trees, up: SideModel::Trees(long_trees(&up, rounds.0)?), down: SideModel::Trees(long_trees(&down, rounds.1)?) })
}

/// Family A at one retraining (cutoff date index `ci`): v2's selection (`select_h`), the long trees, the economic
/// scores on the inner validation and the v3 choice; `pairs[a_slot]` are fitted on the whole training window.
pub struct AFit {
    pub v2: Selection,
    pub pairs: Vec<Option<Pair>>,
    pub scores: Vec<EconScore>,
    pub chosen: Option<V3Candidate>,
    pub rounds: (Option<usize>, Option<usize>),
    /// v4's gates (rise, fall) of the block.
    pub v4: [bot_v4::Gate; 2],
}

pub fn a_fit(rows: &[Row], dates: &[i64], ci: usize, h: usize) -> AFit {
    let v2 = select_h(rows, dates, ci, h);
    let cutoff = dates.get(ci).copied().unwrap_or(i64::MAX);
    let known = |r: &Row, t: i64| r.fwd.is_some_and(|f| f.exit_time < t);
    let mut rounds = (None, None);
    let mut long_full = None;
    let mut long_inner = None;
    let w0 = inner_window(ci, h, dates.len());
    if let Some((v0, c1)) = w0
        && let Some((v1, c2)) = inner_window(c1, h, dates.len())
    {
        let (w1_start, t1_cut, t0_cut) = (dates[v1], dates[c2], dates[c1]);
        let pairs_of = |f: fn(&bot::Forward) -> bool, w: bool| -> Vec<(&[f32], bool)> {
            rows.iter()
                .filter(|r| if w { r.time >= w1_start && known(r, t0_cut) } else { r.train && known(r, t1_cut) })
                .map(|r| (&r.x[..], f(&r.fwd.unwrap())))
                .collect()
        };
        let up = es_rounds(&pairs_of(|f| f.up, false), &pairs_of(|f| f.up, true)).map(|s| s.rounds);
        let down = es_rounds(&pairs_of(|f| f.down, false), &pairs_of(|f| f.down, true)).map(|s| s.rounds);
        rounds = (up, down);
        if let (Some(u), Some(d)) = (up, down) {
            let t0: Vec<&Row> = rows.iter().filter(|r| r.train && known(r, t0_cut)).collect();
            long_inner = long_pair(&t0, (u, d));
            let full: Vec<&Row> = rows.iter().filter(|r| r.train && known(r, cutoff)).collect();
            long_full = long_pair(&full, (u, d));
        }
        let _ = v0;
    }
    // v4: the selective bots' thresholds from W0 (logistic and long trees, inner and full fits).
    let v4 = match w0 {
        Some((v0, _)) => bot_v4::a_gates(
            rows,
            dates[v0],
            cutoff,
            [v2.inner.get(Candidate::Logit.index()).and_then(|p| p.as_ref()), long_inner.as_ref()],
            [v2.pairs[Candidate::Logit.index()].as_ref(), long_full.as_ref()],
        ),
        None => [bot_v4::Gate::default(); 2],
    };
    let mut pairs: Vec<Option<Pair>> = v2.pairs.clone();
    pairs.push(long_full);
    let scores: Vec<EconScore> = match w0 {
        Some((v0, _)) => {
            let val: Vec<&Row> = rows.iter().filter(|r| r.time >= dates[v0] && known(r, cutoff)).collect();
            A_CANDS
                .iter()
                .map(|c| {
                    let inner = match c {
                        V3Candidate::TreesLong => long_inner.as_ref(),
                        _ => v2.inner.get(c.a_slot().unwrap()).and_then(|p| p.as_ref()),
                    };
                    match (c, inner) {
                        (V3Candidate::Trend, _) => econ(*c, &a_signals(&val, |r| trend_action(r.trend))),
                        (_, Some(p)) => econ(*c, &a_signals(&val, |r| p.predict(&r.x, r.trend).action)),
                        _ => EconScore { id: *c, score: None, signals: 0 },
                    }
                })
                .collect()
        }
        None => A_CANDS.iter().map(|c| EconScore { id: *c, score: None, signals: 0 }).collect(),
    };
    let chosen = choose(&scores, |c| pairs[c.a_slot().unwrap()].is_some(), V3Candidate::Trend);
    AFit { v2, pairs, scores, chosen, rounds, v4 }
}

/// v3's choice at one family-A block.
#[derive(Debug, Clone, PartialEq)]
pub struct AChoice {
    pub chosen: Option<V3Candidate>,
    pub scores: Vec<EconScore>,
    pub rounds: (Option<usize>, Option<usize>),
    pub v4: [bot_v4::Gate; 2],
}

/// Family A's walk-forward at horizon `h`: per block, v2's block (its choice and log-loss scores), v3's choice and
/// the models fitted on the whole training window (slots in v2's `CANDIDATES` order, then the long trees).
/// Predictions are made on demand (`preds`): one vector at a time in memory.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AWalk {
    pub blocks: Vec<bot::Block>,
    pub choices: Vec<AChoice>,
    pub pairs: Vec<Vec<Option<Pair>>>,
}

/// Index of the block whose test period holds `t` (blocks sorted, [start, end)).
fn block_at(blocks: &[(i64, i64)], t: i64) -> Option<usize> {
    let k = blocks.partition_point(|b| b.0 <= t).checked_sub(1)?;
    (t < blocks[k].1).then_some(k)
}

/// `f` over the rows in chunks on up to `threads` threads, in order.
fn per_row<T: Send + Clone>(rows: &[Row], threads: usize, f: impl Fn(usize, &Row) -> T + Sync) -> Vec<T> {
    const CHUNK: usize = 50_000;
    let parts = par_map(rows.len().div_ceil(CHUNK), threads, |c| {
        let (a, b) = (c * CHUNK, ((c + 1) * CHUNK).min(rows.len()));
        (a..b).map(|i| f(i, &rows[i])).collect::<Vec<T>>()
    });
    parts.concat()
}

impl AWalk {
    fn spans(&self) -> Vec<(i64, i64)> {
        self.blocks.iter().map(|b| (b.start, b.end)).collect()
    }

    /// Out-of-sample predictions with the slot `pick(block)` of each block (None: no prediction in that block).
    pub fn preds(&self, rows: &[Row], threads: usize, pick: impl Fn(usize) -> Option<usize> + Sync) -> Vec<Option<Prediction>> {
        let spans = self.spans();
        per_row(rows, threads, |_, r| {
            let k = block_at(&spans, r.time)?;
            let p = self.pairs[k][pick(k)?].as_ref()?;
            Some(Prediction { block: k as u32, ..p.predict(&r.x, r.trend) })
        })
    }

    /// One candidate everywhere.
    pub fn slot(&self, rows: &[Row], threads: usize, slot: usize) -> Vec<Option<Prediction>> {
        self.preds(rows, threads, |_| Some(slot))
    }

    /// v2's selection (log-loss).
    pub fn v2_nested(&self, rows: &[Row], threads: usize) -> Vec<Option<Prediction>> {
        self.preds(rows, threads, |k| self.blocks[k].chosen.map(|c| c.index()))
    }

    /// v3's selection (economic criterion).
    pub fn nested(&self, rows: &[Row], threads: usize) -> Vec<Option<Prediction>> {
        self.preds(rows, threads, |k| self.choices[k].chosen.and_then(|c| c.a_slot()))
    }

    /// v2's shape with every candidate's predictions (tests; memory heavy).
    pub fn wf(&self, rows: &[Row]) -> WalkForward {
        WalkForward { blocks: self.blocks.clone(), preds: (0..A_CANDS.len()).map(|c| self.slot(rows, 1, c)).collect() }
    }
}

/// First test date index at horizon `h` (training, label horizon and purge).
pub fn first_test(h: usize) -> usize {
    MIN_TRAIN + h + h
}

pub fn a_walk(rows: &[Row], h: usize, threads: usize) -> AWalk {
    let dates = timeline(rows);
    let starts: Vec<usize> = (first_test(h)..dates.len()).step_by(RETRAIN_EVERY).collect();
    let results = par_map(starts.len(), threads, |k| {
        let s = starts[k];
        let fit = a_fit(rows, &dates, s - h, h);
        release_memory();
        let block = bot::Block {
            start: dates[s],
            end: dates.get(s + RETRAIN_EVERY).copied().unwrap_or(i64::MAX),
            cutoff: dates[s - h],
            train_rows: fit.v2.train_rows,
            trained: fit.v2.chosen.is_some(),
            chosen: fit.v2.chosen,
            scores: fit.v2.scores,
        };
        (block, AChoice { chosen: fit.chosen, scores: fit.scores, rounds: fit.rounds, v4: fit.v4 }, fit.pairs)
    });
    let mut out = AWalk::default();
    for (block, choice, pairs) in results {
        out.blocks.push(block);
        out.choices.push(choice);
        out.pairs.push(pairs);
    }
    out
}

// ---------- Family B: fits and walk-forward ----------

/// Family B at one retraining: the three peers models fitted on the whole training window, the economic scores on
/// the inner validation and the choice.
pub struct BFit {
    pub models: Vec<Option<PeerModel>>,
    pub scores: Vec<EconScore>,
    pub chosen: Option<V3Candidate>,
    pub rounds: Option<usize>,
    pub train_rows: usize,
    /// v4's gates (top, bottom) of the block.
    pub v4: [bot_v4::Gate; 2],
}

/// Scores of rows by a model, ranked by day into actions.
fn rank_actions(rows: &[Row], idx: &[usize], m: &PeerModel) -> HashMap<usize, BotAction> {
    let items: Vec<(usize, i64, usize, f64)> = idx.iter().map(|&i| (i, day_of(rows[i].time), rows[i].asset, m.score(&rows[i].x))).collect();
    quintile_actions(&items)
}

pub fn b_fit(rows: &[Row], xs: &[XsRow], dates: &[i64], ci: usize, h: usize) -> BFit {
    let cutoff = dates.get(ci).copied().unwrap_or(i64::MAX);
    let known = |i: usize, t: i64| xs[i].ranked && xs[i].label.is_some_and(|l| l.known < t);
    let train_of = |t: i64| -> Vec<(&[f32], bool)> {
        (0..rows.len()).filter(|&i| rows[i].train && known(i, t)).map(|i| (&rows[i].x[..], xs[i].label.unwrap().beat)).collect()
    };
    let logit = |t: &[(&[f32], bool)]| -> Option<PeerModel> {
        let (x, y): (Vec<&[f32]>, Vec<bool>) = t.iter().copied().unzip();
        (x.len() >= MIN_TRAIN_ROWS && both(&y)).then(|| PeerModel::Logit(Model::fit(&x, &y, &ALL_COLS)))
    };
    let full = train_of(cutoff);
    let mut models: Vec<Option<PeerModel>> = vec![Some(PeerModel::Momentum), logit(&full), None];
    let mut inner: Vec<Option<PeerModel>> = vec![Some(PeerModel::Momentum), None, None];
    let mut rounds = None;
    let w0 = inner_window(ci, h, dates.len());
    if let Some((_, c1)) = w0 {
        let t0 = train_of(dates[c1]);
        inner[1] = logit(&t0);
        if let Some((v1, c2)) = inner_window(c1, h, dates.len()) {
            let w1: Vec<(&[f32], bool)> = (0..rows.len())
                .filter(|&i| rows[i].time >= dates[v1] && known(i, dates[c1]))
                .map(|i| (&rows[i].x[..], xs[i].label.unwrap().beat))
                .collect();
            rounds = es_rounds(&train_of(dates[c2]), &w1).map(|s| s.rounds);
            if let Some(r) = rounds {
                inner[2] = long_trees(&t0, r).map(PeerModel::Trees);
                models[2] = long_trees(&full, r).map(PeerModel::Trees);
            }
        }
    }
    let scores: Vec<EconScore> = match w0 {
        Some((v0, _)) => {
            let val: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].time >= dates[v0] && known(i, cutoff)).collect();
            B_CANDS
                .iter()
                .zip(&inner)
                .map(|(c, m)| match m {
                    Some(m) => econ(*c, &b_signals(rows, xs, &val, &rank_actions(rows, &val, m))),
                    None => EconScore { id: *c, score: None, signals: 0 },
                })
                .collect()
        }
        None => B_CANDS.iter().map(|c| EconScore { id: *c, score: None, signals: 0 }).collect(),
    };
    let chosen = choose(&scores, |c| models[c.b_slot().unwrap()].is_some(), V3Candidate::XsMomentum);
    let v4 = match w0 {
        Some((v0, _)) => {
            bot_v4::b_gates(rows, xs, dates[v0], cutoff, [inner[1].as_ref(), inner[2].as_ref()], [models[1].as_ref(), models[2].as_ref()])
        }
        None => [bot_v4::Gate::default(); 2],
    };
    BFit { models, scores, chosen, rounds, train_rows: full.len(), v4 }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BChoice {
    pub chosen: Option<V3Candidate>,
    pub scores: Vec<EconScore>,
    pub rounds: Option<usize>,
    pub train_rows: usize,
    pub v4: [bot_v4::Gate; 2],
}

/// Family B's walk-forward: per block (same blocks as family A) the choice and the peers models fitted on the whole
/// training window; predictions on demand (`up` = the score, `base_up` = the training base rate or 0 for the rule,
/// `action` from the day's quintiles).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BWalk {
    pub choices: Vec<BChoice>,
    pub starts: Vec<i64>,
    pub ends: Vec<i64>,
    pub models: Vec<Vec<Option<PeerModel>>>,
}

impl BWalk {
    /// Out-of-sample predictions with the model `pick(block)` of each block, ranked by day.
    pub fn preds(&self, rows: &[Row], xs: &[XsRow], threads: usize, pick: impl Fn(usize) -> Option<usize> + Sync) -> Vec<Option<Prediction>> {
        let spans: Vec<(i64, i64)> = self.starts.iter().copied().zip(self.ends.iter().copied()).collect();
        let parts = par_map(spans.len(), threads, |k| {
            let Some(m) = pick(k).and_then(|c| self.models[k][c].as_ref()) else { return Vec::new() };
            let idx: Vec<usize> = (0..rows.len()).filter(|&i| xs[i].ranked && rows[i].time >= spans[k].0 && rows[i].time < spans[k].1).collect();
            let actions = rank_actions(rows, &idx, m);
            let base = m.base_rate().unwrap_or(0.0) as f32;
            idx.iter()
                .map(|&i| {
                    (i, Prediction { up: m.score(&rows[i].x) as f32, down: 0.0, base_up: base, base_down: 0.0, action: actions[&i], block: k as u32 })
                })
                .collect::<Vec<_>>()
        });
        let mut out = vec![None; rows.len()];
        for list in parts {
            for (i, p) in list {
                out[i] = Some(p);
            }
        }
        out
    }

    pub fn slot(&self, rows: &[Row], xs: &[XsRow], threads: usize, slot: usize) -> Vec<Option<Prediction>> {
        self.preds(rows, xs, threads, |_| Some(slot))
    }

    pub fn nested(&self, rows: &[Row], xs: &[XsRow], threads: usize) -> Vec<Option<Prediction>> {
        self.preds(rows, xs, threads, |k| self.choices[k].chosen.and_then(|c| c.b_slot()))
    }
}

pub fn b_walk(rows: &[Row], xs: &[XsRow], h: usize, threads: usize) -> BWalk {
    let dates = timeline(rows);
    let starts: Vec<usize> = (first_test(h)..dates.len()).step_by(RETRAIN_EVERY).collect();
    let results = par_map(starts.len(), threads, |k| {
        let s = starts[k];
        let fit = b_fit(rows, xs, &dates, s - h, h);
        release_memory();
        let end = dates.get(s + RETRAIN_EVERY).copied().unwrap_or(i64::MAX);
        (dates[s], end, BChoice { chosen: fit.chosen, scores: fit.scores, rounds: fit.rounds, train_rows: fit.train_rows, v4: fit.v4 }, fit.models)
    });
    let mut out = BWalk::default();
    for (start, end, choice, models) in results {
        out.starts.push(start);
        out.ends.push(end);
        out.choices.push(choice);
        out.models.push(models);
    }
    out
}

// ---------- Evaluation ----------

/// Out-of-sample samples of family B: ACHETER excess = net − the day's median net; VENDRE benefit = the day's
/// median net − the asset's gross (switching out); non-overlapping per asset and side; buys compounded vs holding.
/// `vs_mean`: the day's equal-weight mean net instead of its median (the control of `AFTER_PREREG`).
pub fn evaluate_peers(rows: &[Row], xs: &[XsRow], preds: &[Option<Prediction>], keep: impl Fn(&Row) -> bool, vs_mean: bool) -> Samples {
    let mut s = Samples::default();
    let mut next_buy: HashMap<usize, i64> = HashMap::new();
    let mut next_sell: HashMap<usize, i64> = HashMap::new();
    let mut span: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
    let mut buys: HashMap<usize, Vec<f64>> = HashMap::new();
    for (i, (r, p)) in rows.iter().zip(preds).enumerate() {
        let Some(p) = p else { continue };
        if !keep(r) {
            continue;
        }
        s.test_rows += 1;
        let (Some(f), Some(l)) = (r.fwd, xs[i].label) else { continue };
        let med = if vs_mean { l.mean_net } else { l.med_net } as f64;
        let sp = span.entry(r.asset).or_insert((f.entry, f.exit));
        sp.1 = f.exit;
        s.all_net.push(f.net);
        s.all_gross.push(f.gross);
        s.all_drawdown.push(f.drawdown);
        if p.base_up > 0.0 {
            s.up_pairs.push((p.up as f64, l.beat, p.base_up as f64));
        }
        match p.action {
            BotAction::Buy if r.time >= *next_buy.get(&r.asset).unwrap_or(&i64::MIN) => {
                next_buy.insert(r.asset, f.exit_time);
                s.buy_net.push(f.net);
                s.buy_base.push(med);
                s.buy_excess.push(f.net - med);
                s.buy_time.push(r.time);
                s.buy_asset.push(r.asset);
                buys.entry(r.asset).or_default().push(f.net);
            }
            BotAction::Sell if r.time >= *next_sell.get(&r.asset).unwrap_or(&i64::MIN) => {
                next_sell.insert(r.asset, f.exit_time);
                s.sell_gross.push(f.gross);
                s.sell_base.push(med);
                s.sell_avoided.push(med - f.gross);
                s.sell_drawdown.push(f.drawdown);
                s.sell_base_drawdown.push(f.drawdown);
                s.sell_fell.push(!l.beat);
                s.sell_time.push(r.time);
                s.sell_asset.push(r.asset);
            }
            BotAction::Wait => s.wait_net.push(f.net),
            _ => {}
        }
    }
    for (a, (entry, exit)) in span {
        let bot = (buys.get(&a).map_or(1.0, |v| v.iter().map(|x| 1.0 + x / 100.0).product::<f64>()) - 1.0) * 100.0;
        s.per_asset.push((bot, (exit / entry - 1.0) * 100.0));
    }
    s
}

/// One side of a configuration, compact: signals, mean, matched baseline, excess (buy: mean − baseline; sell:
/// baseline − mean), t by date (the verdict's), by asset and per signal, share of signals beating the baseline,
/// verdict at the corrected threshold and at the raw t ≥ 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SideStats {
    pub signals: usize,
    pub mean: Option<f64>,
    pub baseline: Option<f64>,
    pub excess: Option<f64>,
    pub t: Option<f64>,
    pub dates: usize,
    pub t_by_asset: Option<f64>,
    pub assets: usize,
    pub t_per_signal: Option<f64>,
    pub beat_share: Option<f64>,
    pub verdict: Option<Verdict>,
    pub raw_verdict: Option<Verdict>,
}

/// A configuration's out-of-sample figures (compact). Returns in %, excesses in points of %.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigStats {
    pub test_rows: usize,
    pub labelled: usize,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub buy: SideStats,
    pub sell: SideStats,
    /// Share of the labelled test days on ATTENDRE, %.
    pub wait_share: Option<f64>,
    /// ACHETER signals compounded vs holding over the same test days, per asset (medians, assets beating it).
    pub median_bot_return: Option<f64>,
    pub median_hold_return: Option<f64>,
    pub beat_hold: usize,
    pub hold_assets: usize,
    /// Family A: holding vs leaving H days at each VENDRE (per asset, medians).
    pub exit: Option<ExitStats>,
    pub brier_skill_up: Option<f64>,
    pub brier_skill_down: Option<f64>,
    /// Family B: the same sides against the group's equal-weight mean (control added after the first real run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy_vs_mean: Option<SideStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell_vs_mean: Option<SideStats>,
}

impl ConfigStats {
    pub fn of(s: &Samples, t_req: f64, span: (Option<i64>, Option<i64>), with_exit: bool) -> ConfigStats {
        let st: BotStats = stats_t(s, t_req);
        let side = |n: usize, m: Option<f64>, b: Option<f64>, e: Option<f64>, c: &bot::Clustered, values: &[f64], v: Option<Verdict>| SideStats {
            signals: n,
            mean: m,
            baseline: b,
            excess: e,
            t: c.by_date,
            dates: c.dates,
            t_by_asset: c.by_asset,
            assets: c.assets,
            t_per_signal: c.per_signal,
            beat_share: share(values.iter().filter(|x| **x > 0.0).count(), values.len()),
            verdict: v,
            raw_verdict: Some(bot::verdict_at(n, c.by_date, mean(values), T_EDGE)),
        };
        ConfigStats {
            test_rows: st.test_rows,
            labelled: st.labelled,
            from: span.0,
            to: span.1,
            buy: side(st.buy.signals, st.buy.mean_net, st.buy.baseline_net, st.buy.excess, &st.buy.clustered, &s.buy_excess, st.buy.verdict),
            sell: side(
                st.sell.signals,
                st.sell.mean_after,
                st.sell.baseline_after,
                st.sell.avoided,
                &st.sell.clustered,
                &s.sell_avoided,
                st.sell.verdict,
            ),
            wait_share: st.wait.share,
            median_bot_return: st.buy.median_bot_return,
            median_hold_return: st.buy.median_hold_return,
            beat_hold: st.buy.beat_hold,
            hold_assets: st.buy.assets,
            exit: (with_exit && st.sell.exit.assets > 0).then_some(st.sell.exit),
            brier_skill_up: st.brier_skill_up,
            brier_skill_down: st.brier_skill_down,
            buy_vs_mean: None,
            sell_vs_mean: None,
        }
    }

    /// With the control against the mean (family B).
    pub fn with_mean(self, vs_mean: ConfigStats) -> ConfigStats {
        ConfigStats { buy_vs_mean: Some(vs_mean.buy), sell_vs_mean: Some(vs_mean.sell), ..self }
    }

    /// A side's verdict at the corrected threshold, and against the mean when measured: both must say « avantage ».
    pub fn proven(&self, sell: bool) -> bool {
        let (s, m) = if sell { (&self.sell, &self.sell_vs_mean) } else { (&self.buy, &self.buy_vs_mean) };
        s.verdict == Some(Verdict::Edge) && m.as_ref().is_none_or(|m| m.verdict == Some(Verdict::Edge))
    }
}

fn span_of(rows: &[Row], preds: &[Option<Prediction>], keep: &dyn Fn(&Row) -> bool) -> (Option<i64>, Option<i64>) {
    let t: Vec<i64> = rows.iter().zip(preds).filter(|(r, p)| p.is_some() && keep(r)).map(|(r, _)| r.time).collect();
    (t.iter().min().copied(), t.iter().max().copied())
}

/// One configuration of one group and horizon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3Config {
    /// "absolute" / "peers" (v3's nested selections, the headline), "v2" (v2's selection), or a candidate's id.
    pub id: String,
    pub family: Family,
    pub candidate: Option<V3Candidate>,
    pub nested: bool,
    /// One of the 4 headline configurations (the only ones that may count in a decision).
    pub headline: bool,
    pub label: String,
    /// Blocks where it predicted, and (a candidate) where its family's selection chose it.
    pub trained_blocks: usize,
    pub chosen_blocks: usize,
    /// Basket, signals dated up to the pre-registration date (the result); extra universe (nested only); forward.
    pub main: ConfigStats,
    pub extra: Option<ConfigStats>,
    pub forward: ConfigStats,
}

/// Rounds kept by early stopping over the blocks (long trees).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Rounds {
    pub fits: usize,
    pub min: Option<usize>,
    pub median: Option<f64>,
    pub max: Option<usize>,
}

fn rounds_of(v: &[usize]) -> Rounds {
    Rounds {
        fits: v.len(),
        min: v.iter().min().copied(),
        median: median(&v.iter().map(|x| *x as f64).collect::<Vec<_>>()),
        max: v.iter().max().copied(),
    }
}

/// One retraining: its test period, v2's and v3's choices with the economic scores, the long trees' rounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3BlockOut {
    pub start: i64,
    pub end: Option<i64>,
    pub train_rows: usize,
    pub v2: Option<Candidate>,
    pub absolute: Option<V3Candidate>,
    pub peers: Option<V3Candidate>,
    pub absolute_scores: Vec<EconScore>,
    pub peers_scores: Vec<EconScore>,
    pub rounds_up: Option<usize>,
    pub rounds_down: Option<usize>,
    pub rounds_peers: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3Horizon {
    pub horizon: usize,
    pub blocks: usize,
    pub test_from: Option<i64>,
    pub test_to: Option<i64>,
    /// Headline first (absolute, peers), then v2's selection, then each candidate (for information).
    pub configs: Vec<V3Config>,
    pub selection: Vec<V3BlockOut>,
    pub rounds_up: Rounds,
    pub rounds_down: Rounds,
    pub rounds_peers: Rounds,
}

// ---------- Volatility-managed trend ----------

/// A daily return series: annualised return, volatility and Sharpe (no risk-free rate), max drawdown and total
/// return (%), mean exposure (0-1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SeriesStats {
    pub days: usize,
    pub annual_return: Option<f64>,
    pub annual_vol: Option<f64>,
    pub sharpe: Option<f64>,
    pub max_drawdown: Option<f64>,
    pub total_return: Option<f64>,
    pub mean_exposure: Option<f64>,
}

pub fn series_stats(r: &[f64], exposure: &[f64], periods: f64) -> SeriesStats {
    let n = r.len();
    if n < 2 {
        return SeriesStats { days: n, ..SeriesStats::default() };
    }
    let m = r.iter().sum::<f64>() / n as f64;
    let sd = (r.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt();
    let mut eq = 1.0f64;
    let mut peak = 1.0f64;
    let mut worst = 0.0f64;
    for x in r {
        eq *= 1.0 + x;
        peak = peak.max(eq);
        worst = worst.min(eq / peak - 1.0);
    }
    SeriesStats {
        days: n,
        annual_return: Some(round_to((eq.powf(periods / n as f64) - 1.0) * 100.0, 2)),
        annual_vol: Some(round_to(sd * periods.sqrt() * 100.0, 2)),
        sharpe: (sd > 0.0).then(|| round_to(m / sd * periods.sqrt(), 2)),
        max_drawdown: Some(round_to(worst * 100.0, 2)),
        total_return: Some(round_to((eq - 1.0) * 100.0, 2)),
        mean_exposure: mean(exposure).map(|x| round_to(x, 3)),
    }
}

/// Daily (day, hold return, managed return, exposure) of one asset from its candles, from `from` (ms) on.
pub fn vol_managed_days(raw: &[Candle], kind: Kind, from: i64) -> Vec<(i64, f64, f64, f64)> {
    let c = sanitize(raw);
    let half = round_trip_cost(kind) / 100.0 / 2.0;
    let mut out = Vec::new();
    let mut sigmas: Vec<f64> = Vec::new();
    let mut prev: Option<f64> = None;
    let mut csum = 0.0;
    let mut sums = Vec::with_capacity(c.len() + 1);
    sums.push(0.0);
    for x in &c {
        csum += x.close;
        sums.push(csum);
    }
    for i in 200..c.len() {
        let logs: Vec<f64> = (i - 19..=i).map(|j| (c[j].close / c[j - 1].close).ln()).collect();
        let m = logs.iter().sum::<f64>() / 20.0;
        let sigma = (logs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / 19.0).sqrt();
        let at = sigmas.partition_point(|s| *s < sigma);
        sigmas.insert(at, sigma);
        let star = {
            let n = sigmas.len();
            if n % 2 == 1 { sigmas[n / 2] } else { (sigmas[n / 2 - 1] + sigmas[n / 2]) / 2.0 }
        };
        let sma = (sums[i + 1] - sums[i + 1 - 200]) / 200.0;
        let raw_w = if c[i].close > sma && sigma > 0.0 {
            (star * star / (sigma * sigma)).min(1.0)
        } else if c[i].close > sma {
            1.0
        } else {
            0.0
        };
        let w = (raw_w * 4.0).floor() / 4.0;
        let before = prev.unwrap_or(w);
        prev = Some(w);
        if c[i].time < from || i + 2 >= c.len() || c[i + 1].open <= 0.0 {
            continue;
        }
        let r = c[i + 2].open / c[i + 1].open - 1.0;
        out.push((day_of(c[i].time), r, w * r - (w - before).abs() * half, w));
    }
    out
}

/// The volatility-managed trend of a group's basket assets vs holding them, over the test days before the forward
/// test (`main`) and after (`forward`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VolManaged {
    pub assets: usize,
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// Periods per year used to annualise (252 stocks, 365 cryptos).
    pub periods: f64,
    /// Equal-weight portfolio of the basket assets (daily mean of the available ones).
    pub hold: SeriesStats,
    pub managed: SeriesStats,
    /// Per asset: medians of the Sharpe and max drawdown, assets with a better Sharpe / a shallower drawdown.
    pub median_sharpe_hold: Option<f64>,
    pub median_sharpe_managed: Option<f64>,
    pub median_drawdown_hold: Option<f64>,
    pub median_drawdown_managed: Option<f64>,
    pub better_sharpe: usize,
    pub shallower_drawdown: usize,
    pub forward_hold: SeriesStats,
    pub forward_managed: SeriesStats,
    pub text: String,
}

pub fn vol_managed(histories: &[&History], periods: f64, from: i64) -> VolManaged {
    let mut per: Vec<(SeriesStats, SeriesStats)> = Vec::new();
    let mut port: BTreeMap<i64, (f64, f64, f64, usize)> = BTreeMap::new();
    for h in histories {
        let d = vol_managed_days(&h.candles, h.asset.kind, from);
        let main: Vec<&(i64, f64, f64, f64)> = d.iter().filter(|x| x.0 * DAY_MS < FORWARD_FROM).collect();
        if main.len() < 2 {
            continue;
        }
        let hold: Vec<f64> = main.iter().map(|x| x.1).collect();
        let man: Vec<f64> = main.iter().map(|x| x.2).collect();
        let w: Vec<f64> = main.iter().map(|x| x.3).collect();
        per.push((series_stats(&hold, &vec![1.0; hold.len()], periods), series_stats(&man, &w, periods)));
        for x in &d {
            let e = port.entry(x.0).or_default();
            e.0 += x.1;
            e.1 += x.2;
            e.2 += x.3;
            e.3 += 1;
        }
    }
    let split = |fwd: bool| {
        let days: Vec<(f64, f64, f64)> = port
            .iter()
            .filter(|(d, _)| (**d * DAY_MS >= FORWARD_FROM) == fwd)
            .map(|(_, e)| (e.0 / e.3 as f64, e.1 / e.3 as f64, e.2 / e.3 as f64))
            .collect();
        let h: Vec<f64> = days.iter().map(|x| x.0).collect();
        let m: Vec<f64> = days.iter().map(|x| x.1).collect();
        let w: Vec<f64> = days.iter().map(|x| x.2).collect();
        (series_stats(&h, &vec![1.0; h.len()], periods), series_stats(&m, &w, periods))
    };
    let (hold, managed) = split(false);
    let (forward_hold, forward_managed) = split(true);
    let med = |f: fn(&(SeriesStats, SeriesStats)) -> Option<f64>| r2(median(&per.iter().filter_map(f).collect::<Vec<_>>()));
    let main_days: Vec<i64> = port.keys().filter(|d| **d * DAY_MS < FORWARD_FROM).copied().collect();
    let mut v = VolManaged {
        assets: per.len(),
        from: main_days.first().map(|d| d * DAY_MS),
        to: main_days.last().map(|d| d * DAY_MS),
        periods,
        hold,
        managed,
        median_sharpe_hold: med(|x| x.0.sharpe),
        median_sharpe_managed: med(|x| x.1.sharpe),
        median_drawdown_hold: med(|x| x.0.max_drawdown),
        median_drawdown_managed: med(|x| x.1.max_drawdown),
        better_sharpe: per.iter().filter(|x| x.1.sharpe.unwrap_or(f64::MIN) > x.0.sharpe.unwrap_or(f64::MIN)).count(),
        shallower_drawdown: per.iter().filter(|x| x.1.max_drawdown.unwrap_or(f64::MIN) > x.0.max_drawdown.unwrap_or(f64::MIN)).count(),
        forward_hold,
        forward_managed,
        text: String::new(),
    };
    v.text = vol_text(&v);
    v
}

fn num(x: Option<f64>, d: usize) -> String {
    x.map(|v| format!("{}{}", if v < 0.0 { "−" } else { "" }, fr(v.abs(), 0, d))).unwrap_or_else(|| "—".into())
}

fn vol_text(v: &VolManaged) -> String {
    if v.assets == 0 {
        return "Pas assez de jours de test.".into();
    }
    format!(
        "Portefeuille à parts égales : ratio de Sharpe {} contre {} en gardant, pire baisse {} contre {}, rendement annuel {} contre {} (exposition moyenne {}). Actif par actif : Sharpe meilleur pour {} sur {}, pire baisse moins profonde pour {}.",
        num(v.managed.sharpe, 2),
        num(v.hold.sharpe, 2),
        signed(v.managed.max_drawdown, 1),
        signed(v.hold.max_drawdown, 1),
        signed(v.managed.annual_return, 1),
        signed(v.hold.annual_return, 1),
        v.managed.mean_exposure.map(|x| format!("{} %", fr(x * 100.0, 0, 0))).unwrap_or_else(|| "—".into()),
        v.better_sharpe,
        v.assets,
        v.shallower_drawdown
    )
}

// ---------- Report ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3Group {
    pub id: BotGroup,
    pub label: String,
    pub market: String,
    pub universe: bot::Universe,
    /// First day with at least `MIN_XS` assets (peers ranking possible).
    pub peers_from: Option<i64>,
    pub horizons: Vec<V3Horizon>,
    pub vol_managed: Option<VolManaged>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CandidateInfo {
    pub id: V3Candidate,
    pub family: Family,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3Parameters {
    pub horizons: Vec<usize>,
    pub stock_from: String,
    pub max_tree_rows: usize,
    pub long_max_rounds: usize,
    pub long_depth: usize,
    pub long_shrinkage: f64,
    pub long_min_leaf: usize,
    pub patience: usize,
    pub min_peers: usize,
    pub min_val_signals: usize,
    pub min_signals: usize,
    pub nudge: f64,
}

/// Computation facts filled by `run` (rows) and the route (time, memory: see `BotReport::timing`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Compute {
    /// Largest group × horizon, in feature rows, and the blocks trained in all.
    pub max_rows: usize,
    pub blocks: usize,
    pub row_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3Report {
    pub version: u32,
    pub prereg_date: String,
    /// First day of the forward test (ms, UTC midnight).
    pub forward_from: i64,
    /// Changes made after the pre-registration (listed, never hidden).
    pub after_prereg: Vec<String>,
    pub k: KCount,
    pub alpha: f64,
    /// |t| needed for an edge after K tests.
    pub t_required: f64,
    pub headline: String,
    pub forward_headline: String,
    pub groups: Vec<V3Group>,
    pub candidates: Vec<CandidateInfo>,
    pub changes: Vec<String>,
    pub method: Vec<String>,
    pub limits: Vec<String>,
    pub parameters: V3Parameters,
    pub compute: Compute,
}

impl V3Report {
    pub fn config(&self, g: BotGroup, h: usize, id: &str) -> Option<&V3Config> {
        self.groups.iter().find(|x| x.id == g)?.horizons.iter().find(|x| x.horizon == h)?.configs.iter().find(|c| c.id == id)
    }
}

/// Today's actions of a basket asset under the 4 headline configurations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3AssetNow {
    pub time: Option<i64>,
    pub absolute20: Option<BotAction>,
    pub absolute60: Option<BotAction>,
    pub peers20: Option<BotAction>,
    pub peers60: Option<BotAction>,
}

// ---------- Live models ----------

/// The models chosen « now » for one group and horizon, and the peers' reference of the last day.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct V3LiveH {
    pub horizon: usize,
    pub absolute: Option<(V3Candidate, Pair)>,
    pub peers: Option<(V3Candidate, PeerModel)>,
    /// Raw features of the universe assets on the group's last day, by symbol.
    pub reference: Vec<(String, Features)>,
    /// Score of the ⌊n/5⌋-th best and worst of the universe that day (ACHETER at or above, VENDRE at or below).
    pub cut_top: Option<f64>,
    pub cut_bottom: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct V3LiveGroup {
    pub group: BotGroup,
    pub horizons: Vec<V3LiveH>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct V3Live {
    pub groups: Vec<V3LiveGroup>,
}

/// Peers features of `x` against the day's reference (its own entry replaced when `own` is in it).
pub fn peers_features(x: &Features, reference: &[(String, Features)], own: &str) -> Features {
    let mut out = *x;
    for &j in &XS_RANKED {
        let others: Vec<f32> = reference.iter().filter(|(s, _)| !s.eq_ignore_ascii_case(own)).map(|(_, f)| f[j]).collect();
        out[j] = pct_rank_among(x[j], &others);
    }
    out
}

/// A peers action from a score and the day's cut-offs.
pub fn peers_action(score: f64, top: Option<f64>, bottom: Option<f64>) -> Option<BotAction> {
    let (t, b) = (top?, bottom?);
    Some(if score >= t {
        BotAction::Buy
    } else if score <= b {
        BotAction::Sell
    } else {
        BotAction::Wait
    })
}

impl V3LiveH {
    pub fn absolute_action(&self, x: &Features, trend: i8) -> Option<BotAction> {
        self.absolute.as_ref().map(|(_, p)| p.predict(x, trend).action)
    }

    pub fn peers_action(&self, x: &Features, symbol: &str) -> Option<BotAction> {
        let (_, m) = self.peers.as_ref()?;
        if self.reference.len() + 1 < MIN_XS {
            return None;
        }
        peers_action(m.score(&peers_features(x, &self.reference, symbol)), self.cut_top, self.cut_bottom)
    }
}

// ---------- Decision view ----------

/// One headline configuration today for one asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3Signal {
    pub family: Family,
    pub horizon: usize,
    /// The candidate chosen « now » by the family's selection.
    pub candidate: Option<V3Candidate>,
    pub action: Option<BotAction>,
    /// Main verdicts at the corrected threshold, and the forward test's signal counts and excesses so far.
    pub buy_verdict: Option<Verdict>,
    pub sell_verdict: Option<Verdict>,
    pub forward_buy_signals: usize,
    pub forward_sell_signals: usize,
    /// Today's side is proven but the forward test (≥ 30 signals) says otherwise.
    pub contradicted: bool,
    pub counts: bool,
    /// Today's side is proven on the past but awaits 30 confirming forward signals: shown only.
    pub pending: bool,
    pub text: String,
}

/// v3 in a decision: the 4 headline configurations today; `counts` when one of them is on a proven side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct V3View {
    pub available: bool,
    pub signals: Vec<V3Signal>,
    pub counts: bool,
    /// Confidence change for a buy-side verdict (± 3 at most, 0 when counting sides disagree).
    pub nudge: f64,
    pub t_required: f64,
    pub note: String,
    /// The decision's lines when they count: pro (buy side), con (sell side, family A), con for a holder (B).
    pub pro: Option<String>,
    pub con: Option<String>,
    pub con_held: Option<String>,
}

impl V3View {
    /// The pro or con line of a decision, when it counts: (is_pro, text).
    pub fn line(&self, held: bool) -> Option<(bool, String)> {
        if let Some(p) = &self.pro {
            return Some((true, p.clone()));
        }
        if let Some(c) = &self.con {
            return Some((false, c.clone()));
        }
        if held { self.con_held.clone().map(|c| (false, c)) } else { None }
    }
}

/// Where today's side stands (`AFTER_PREREG`, 2): proven at the corrected threshold (family B: against the median
/// and the mean), then counted only once the forward test confirms it — ≥ `MIN_TRADES` forward signals and a mean
/// excess ≥ 0 against each reference; below 30 it is pending (shown only), with a negative excess contradicted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideState {
    pub counts: bool,
    pub pending: bool,
    pub contradicted: bool,
}

pub fn side_state(c: &V3Config, a: BotAction) -> SideState {
    let none = SideState { counts: false, pending: false, contradicted: false };
    let sell = match a {
        BotAction::Buy => false,
        BotAction::Sell => true,
        BotAction::Wait => return none,
    };
    if !c.main.proven(sell) {
        return none;
    }
    let (fwd, fwd_mean) = if sell { (&c.forward.sell, &c.forward.sell_vs_mean) } else { (&c.forward.buy, &c.forward.buy_vs_mean) };
    let refs: Vec<&SideStats> = std::iter::once(fwd).chain(fwd_mean.as_ref()).collect();
    if refs.iter().any(|s| s.signals < MIN_TRADES) {
        return SideState { pending: true, ..none };
    }
    let confirmed = refs.iter().all(|s| s.excess.is_some_and(|e| e >= 0.0));
    SideState { counts: confirmed, pending: false, contradicted: !confirmed }
}

/// « Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : … » for a proven side awaiting its
/// forward test.
pub fn pending_note(c: &V3Config, sell: bool, t_req: f64) -> String {
    let s = if sell { c.main.sell_vs_mean.as_ref().unwrap_or(&c.main.sell) } else { c.main.buy_vs_mean.as_ref().unwrap_or(&c.main.buy) };
    let f = if sell { &c.forward.sell } else { &c.forward.buy };
    format!(
        "Avantage mesuré sur le passé (t = {} contre {} exigé) mais fragile : il ne comptera qu'après {MIN_TRADES} signaux sur l'avenir qui le confirment ({} à ce jour).",
        s.t.map(|t| fr(t, 0, 2)).unwrap_or_else(|| "—".into()),
        fr(t_req, 0, 2),
        f.signals
    )
}

/// v3's view of an asset from its features at the last closed daily candle; None without a v3 report and models.
pub fn view(report: &BotReport, group: BotGroup, symbol: &str, x: &Features, trend: i8) -> Option<V3View> {
    let r = report.v3.as_ref()?;
    let Some(live) = report.v3_live.as_ref().and_then(|l| l.groups.iter().find(|g| g.group == group)) else {
        let why = if report.v3_live.is_none() { "rapport relu depuis un fichier" } else { "pas de modèle v3 pour ce groupe" };
        return Some(V3View {
            available: false,
            t_required: r.t_required,
            note: format!("Bot v3 indisponible ({why}) : il ne compte pas."),
            ..V3View::default()
        });
    };
    let mut signals = Vec::new();
    let mut pending: Option<String> = None;
    let (mut pro, mut con, mut con_held) = (None, None, None);
    let (mut buy, mut sell) = (false, false);
    for family in [Family::Absolute, Family::Peers] {
        for lh in &live.horizons {
            let id = if family == Family::Absolute { "absolute" } else { "peers" };
            let Some(cfg) = r.config(group, lh.horizon, id) else { continue };
            let (candidate, action) = match family {
                Family::Absolute => (lh.absolute.as_ref().map(|a| a.0), lh.absolute_action(x, trend)),
                _ => (lh.peers.as_ref().map(|a| a.0), lh.peers_action(x, symbol)),
            };
            let st = action.map_or(SideState { counts: false, pending: false, contradicted: false }, |a| side_state(cfg, a));
            let (counts, contradicted, fb, fs) = (st.counts, st.contradicted, cfg.forward.buy.signals, cfg.forward.sell.signals);
            if st.pending {
                pending.get_or_insert_with(|| pending_note(cfg, action == Some(BotAction::Sell), r.t_required));
            }
            let what = match (family, action) {
                (_, None) => "pas d'avis".to_string(),
                (Family::Peers, Some(BotAction::Buy)) => "parmi les 20 % les mieux classés de son groupe".into(),
                (Family::Peers, Some(BotAction::Sell)) => "parmi les 20 % les moins bien classés de son groupe".into(),
                (Family::Peers, Some(_)) => "au milieu du classement de son groupe".into(),
                (_, Some(a)) => a.label().into(),
            };
            let base = format!("{} à {} jours : {what}", family.label(), lh.horizon);
            let text = format!(
                "{base}{}",
                if counts {
                    " (avantage démontré au seuil corrigé et confirmé sur l'avenir, compte un peu)"
                } else if st.pending {
                    " (avantage mesuré sur le passé mais fragile : en attente de 30 signaux sur l'avenir, ne compte pas encore)"
                } else if contradicted {
                    " (avantage passé contredit par le test sur l'avenir : ne compte pas)"
                } else {
                    ""
                }
            );
            if counts {
                match (family, action) {
                    (_, Some(BotAction::Buy)) => {
                        buy = true;
                        pro.get_or_insert_with(|| {
                            format!(
                                "Bot v3 favorable ({base}), avantage hors échantillon au seuil corrigé (t ≥ {}), à confirmer",
                                fr(r.t_required, 0, 1)
                            )
                        });
                    }
                    (Family::Absolute, Some(BotAction::Sell)) => {
                        sell = true;
                        con.get_or_insert_with(|| {
                            format!(
                                "Bot v3 défavorable ({base}), avantage hors échantillon au seuil corrigé (t ≥ {}), à confirmer",
                                fr(r.t_required, 0, 1)
                            )
                        });
                    }
                    (Family::Peers, Some(BotAction::Sell)) => {
                        sell = true;
                        con_held.get_or_insert_with(|| {
                            format!("Bot v3 : {base} ; alléger au profit d'actifs mieux classés peut se discuter (avantage au seuil corrigé, t ≥ {}, à confirmer)", fr(r.t_required, 0, 1))
                        });
                    }
                    _ => {}
                }
            }
            signals.push(V3Signal {
                family,
                horizon: lh.horizon,
                candidate,
                action,
                buy_verdict: cfg.main.buy.verdict,
                sell_verdict: cfg.main.sell.verdict,
                forward_buy_signals: fb,
                forward_sell_signals: fs,
                contradicted,
                counts,
                pending: st.pending,
                text,
            });
        }
    }
    let nudge = match (buy, sell) {
        (true, false) => NUDGE,
        (false, true) => -NUDGE,
        _ => 0.0,
    };
    if buy && sell {
        pro = None;
        con = None;
        con_held = None;
    }
    let counts = (buy || sell) && !(buy && sell);
    let note = if counts {
        format!(
            "Avantage démontré au seuil corrigé (t ≥ {}) pour ce signal : compte un peu dans la décision (au plus {} points), jamais contre un veto.",
            fr(r.t_required, 0, 2),
            fr(NUDGE, 0, 0)
        )
    } else if buy && sell {
        "Signaux prouvés contradictoires aujourd'hui : le bot ne compte pas.".into()
    } else if let Some(p) = pending {
        p
    } else {
        format!(
            "Aucun avantage démontré au seuil corrigé (t ≥ {}, {} tests) pour les signaux d'aujourd'hui : le bot ne compte pas dans la décision.",
            fr(r.t_required, 0, 2),
            r.k.total
        )
    };
    Some(V3View { available: true, signals, counts, nudge, t_required: r.t_required, note, pro, con, con_held })
}

// ---------- The whole run ----------

fn trained_blocks(preds: &[Option<Prediction>]) -> usize {
    preds.iter().flatten().map(|p| p.block).collect::<std::collections::BTreeSet<_>>().len()
}

/// A configuration's main (basket, up to the pre-registration), extra (nested only) and forward figures.
fn config(
    base: V3Config,
    rows: &[Row],
    preds: &[Option<Prediction>],
    is_extra: &[bool],
    t_req: f64,
    eval: &dyn Fn(&[Option<Prediction>], &dyn Fn(&Row) -> bool) -> Samples,
    control: Option<&dyn Fn(&[Option<Prediction>], &dyn Fn(&Row) -> bool) -> Samples>,
    with_exit: bool,
) -> V3Config {
    let main = |r: &Row| !is_extra[r.asset] && r.time < FORWARD_FROM;
    let extra = |r: &Row| is_extra[r.asset] && r.time < FORWARD_FROM;
    let fwd = |r: &Row| !is_extra[r.asset] && r.time >= FORWARD_FROM;
    let with_extra = base.nested && is_extra.iter().any(|x| *x);
    let stats = |keep: &dyn Fn(&Row) -> bool| {
        let span = span_of(rows, preds, keep);
        let s = ConfigStats::of(&eval(preds, keep), t_req, span, with_exit);
        match control {
            Some(c) => s.with_mean(ConfigStats::of(&c(preds, keep), t_req, span, with_exit)),
            None => s,
        }
    };
    V3Config { trained_blocks: trained_blocks(preds), main: stats(&main), extra: with_extra.then(|| stats(&extra)), forward: stats(&fwd), ..base }
}

fn head(id: &str, family: Family, candidate: Option<V3Candidate>, nested: bool, label: String) -> V3Config {
    V3Config { id: id.into(), family, candidate, nested, headline: nested && family != Family::V2, label, ..V3Config::default() }
}

fn group_text(g: BotGroup, horizons: &[V3Horizon], t_req: f64) -> String {
    let mut parts = Vec::new();
    for h in horizons {
        for c in h.configs.iter().filter(|c| c.headline) {
            let (b, s) = (&c.main.buy, &c.main.sell);
            parts.push(format!(
                "{} à {} jours : achats {} ({}), ventes {} ({})",
                c.family.label().to_lowercase(),
                h.horizon,
                points(b.excess),
                t_text(b.t),
                points(s.excess),
                t_text(s.t)
            ));
        }
    }
    format!("{} — {} ; seuil corrigé t ≥ {}.", g.label(), parts.join(" ; "), fr(t_req, 0, 2))
}

fn is_edge(s: &SideStats) -> bool {
    s.verdict == Some(Verdict::Edge)
}

/// The verdict in plain French, from the numbers only (headline configurations).
pub fn headline(groups: &[V3Group], t_req: f64, k: usize) -> String {
    if groups.is_empty() {
        return "Aucun actif du panier n'a pu servir à l'entraînement : rien à conclure.".into();
    }
    let mut edges = Vec::new();
    let mut fragile = Vec::new();
    let mut median_only = Vec::new();
    let mut negatives = Vec::new();
    let mut raw = Vec::new();
    for g in groups {
        for h in &g.horizons {
            for c in h.configs.iter().filter(|c| c.headline) {
                let what = format!("{} ({}, {} jours)", c.family.label().to_lowercase(), g.id.short(), h.horizon);
                for (side, s, m) in [("achats", &c.main.buy, &c.main.buy_vs_mean), ("ventes", &c.main.sell, &c.main.sell_vs_mean)] {
                    // Family B is judged against the group's mean too (the control): its figures decide the rest.
                    let judged = m.as_ref().unwrap_or(s);
                    let f = &c.forward;
                    let (fs, fm) = if side == "achats" { (&f.buy, &f.buy_vs_mean) } else { (&f.sell, &f.sell_vs_mean) };
                    let refs: Vec<&SideStats> = std::iter::once(fs).chain(fm.as_ref()).collect();
                    let confirmed = refs.iter().all(|x| x.signals >= MIN_TRADES && x.excess.is_some_and(|e| e >= 0.0));
                    if is_edge(s) && is_edge(judged) && !confirmed {
                        fragile.push(format!(
                            "les {side} en {what} (t = {} par jour contre {} exigé, {} par actif ; {} signaux sur l'avenir)",
                            num(judged.t, 2),
                            fr(t_req, 0, 2),
                            t_text(judged.t_by_asset),
                            fs.signals
                        ));
                    } else if is_edge(s) && is_edge(judged) {
                        // The t by asset next to it: an edge carried by a few assets reads very differently.
                        edges.push(format!("les {side} en {what}, {} par jour ({} par actif)", t_text(judged.t), t_text(judged.t_by_asset)));
                    } else if is_edge(s) {
                        median_only.push(format!("{side} en {what} ({} face à la médiane, {} face à la moyenne)", t_text(s.t), t_text(judged.t)));
                    } else if judged.verdict == Some(Verdict::Negative) {
                        negatives.push(format!("{side} en {what} pires que la référence ({})", t_text(judged.t)));
                    } else if judged.raw_verdict == Some(Verdict::Edge) {
                        raw.push(format!("{side} en {what}, {}", t_text(judged.t)));
                    }
                }
            }
        }
    }
    let mut out = if edges.is_empty() && !fragile.is_empty() {
        format!(
            "Bot v3 : avantage mesuré sur le passé au seuil corrigé pour {}, mais fragile : il ne comptera dans les décisions qu'après {MIN_TRADES} signaux sur l'avenir qui le confirment ; d'ici là, le bot ne pèse pas dans les décisions.",
            fragile.join(" ; ")
        )
    } else if edges.is_empty() {
        format!(
            "Bot v3 : aucun avantage démontré au seuil corrigé (t ≥ {}, {k} tests comptés), ni en hausse/baisse ni en classement entre pairs, à 20 comme à 60 jours : le bot ne pèse pas dans les décisions.",
            fr(t_req, 0, 2)
        )
    } else {
        format!(
            "Bot v3 : avantage au seuil corrigé (t ≥ {}) confirmé par le test sur l'avenir pour {} : seul ce côté compte, un peu, dans les décisions.",
            fr(t_req, 0, 2),
            edges.join(" ; ")
        )
    };
    if !edges.is_empty() && !fragile.is_empty() {
        out.push_str(&format!(" En attente de confirmation sur l'avenir (ne compte pas encore) : {}.", fragile.join(" ; ")));
    }
    if !median_only.is_empty() {
        out.push_str(&format!(
            " Face à la médiane du groupe (critère pré-enregistré), le seuil était passé pour {}, mais pas face à sa moyenne (contrôle ajouté après le premier calcul) : l'écart vient probablement de l'asymétrie des rendements (la moyenne dépasse la médiane), pas d'un vrai avantage ; il ne compte pas.",
            median_only.join(" ; ")
        ));
    }
    if !raw.is_empty() {
        out.push_str(&format!(" Au seuil simple t ≥ 2 seulement (insuffisant après tant d'essais) : {}.", raw.join(" ; ")));
    }
    if !negatives.is_empty() {
        out.push_str(&format!(" Attention : {}.", negatives.join(" ; ")));
    }
    out
}

/// The forward test so far, in plain French.
pub fn forward_headline(groups: &[V3Group]) -> String {
    let mut n = 0usize;
    let mut parts = Vec::new();
    for g in groups {
        for h in &g.horizons {
            for c in h.configs.iter().filter(|c| c.headline) {
                let s = c.forward.buy.signals + c.forward.sell.signals;
                n += s;
                if s > 0 {
                    parts.push(format!(
                        "{} {} {} j : {} achat{} ({}), {} vente{} ({})",
                        c.family.label().to_lowercase(),
                        g.id.short(),
                        h.horizon,
                        c.forward.buy.signals,
                        if c.forward.buy.signals > 1 { "s" } else { "" },
                        points(c.forward.buy.excess),
                        c.forward.sell.signals,
                        if c.forward.sell.signals > 1 { "s" } else { "" },
                        points(c.forward.sell.excess)
                    ));
                }
            }
        }
    }
    if n == 0 {
        format!(
            "Aucun signal daté après le {} dont l'issue est connue : le test sur l'avenir commence (il faut 20 à 60 jours de bourse pour qu'un signal soit jugé).",
            frdate(PREREG_DATE)
        )
    } else {
        format!("Depuis le {} : {}.", frdate(PREREG_DATE), parts.join(" ; "))
    }
}

fn frdate(iso: &str) -> String {
    iso.split('-').rev().collect::<Vec<_>>().join("/")
}

pub fn changes(k: &KCount, t_req: f64) -> Vec<String> {
    vec![
        "Plus d'historique : actions et ETF depuis 1990 (au lieu de 20 ans ; les mesures démarrent fin 1993, quand le S&P 500 via SPY a assez d'historique), cryptos depuis leur cotation.".into(),
        "Deux horizons jugés à part : 20 et 60 jours de bourse (le momentum se mesure plutôt sur 3 à 12 mois).".into(),
        "Nouvelle cible, le classement entre pairs : faire mieux que la médiane de son groupe (actions entre elles, cryptos entre elles) ; ACHETER les 20 % les mieux classés, VENDRE (alléger au profit de mieux classés) les 20 % les moins bien classés.".into(),
        format!(
            "Arbres entraînés plus longtemps et mieux : jusqu'à {} arbres de profondeur {}, pas de {}, nombre d'arbres choisi par arrêt anticipé sur une fenêtre interne, jamais sur le test.",
            bot_trees::LONG.trees,
            bot_trees::LONG.depth,
            fr(bot_trees::LONG.shrinkage, 0, 2)
        ),
        "Choix du modèle sur un critère économique fixé d'avance (gain moyen des achats face à leur référence sur la validation interne, compté par jour), et non plus sur la log-loss, qui favorisait la règle de tendance.".into(),
        format!(
            "Seuil corrigé des tests multiples : {} configurations × côtés essayées depuis la v1 ({} en v1, {} en v2, {} en v3), donc t ≥ {} exigé au lieu de 2.",
            k.total,
            k.v1,
            k.v2,
            k.v3,
            fr(t_req, 0, 2)
        ),
        format!("Protocole pré-enregistré le {} ; tout signal daté après forme un test sur l'avenir, présenté à part.", frdate(PREREG_DATE)),
        "Référence : la sélection v2 (log-loss) refaite sur les nouvelles données, et deux règles publiées sans apprentissage (momentum 12-1 entre pairs, tendance avec exposition réduite quand la volatilité monte).".into(),
        "Résultat de la v2 (29/09/2026) : aucun avantage hors échantillon (actions : achats t = −2,4, ventes t = −4,5 par jour ; cryptos non concluant) ; garder a fait mieux partout.".into(),
    ]
}

pub fn method(t_req: f64, k: usize) -> Vec<String> {
    vec![
        format!(
            "Test principal : le panier fixe de {} actifs, signaux datés jusqu'au {} inclus ; {} autres actifs servent à l'entraînement et sont jugés à part.",
            BASKET.len(),
            frdate(PREREG_DATE),
            bot::EXTRA.len()
        ),
        "Hausse ou baisse de l'actif (comme la v2) : 5 candidats — règle de tendance, logistique v1, logistique 24 mesures, arbres v2, arbres longs ; un achat est comparé à une entrée au hasard sur le même actif et la même période.".into(),
        format!(
            "Classement entre pairs : chaque jour, les mesures propres à l'actif sont classées entre les actifs du groupe (au moins {MIN_XS}) ; 3 candidats — momentum 12-1, logistique, arbres longs ; un achat est comparé à la médiane du groupe le même jour, une vente à ce qu'aurait rapporté l'actif médian (aller-retour payé)."
        ),
        format!(
            "À chaque réentraînement (tous les {RETRAIN_EVERY} jours), chaque famille choisit son candidat sur la dernière année de l'entraînement, après un écart égal à l'horizon ; au moins {MIN_VAL_SIGNALS} achats requis, sinon la règle ; le choix est réentraîné sur toute la fenêtre."
        ),
        format!(
            "Arbres longs : arrêt anticipé ({} arbres sans progrès) sur l'année précédant cette validation, puis réentraînement avec ce nombre d'arbres ; au plus {} lignes d'entraînement (une sur k, régulièrement).",
            bot_trees::PATIENCE,
            MAX_TREE_ROWS
        ),
        format!(
            "Verdict : au moins {MIN_TRADES} signaux et t par jour ≥ {} (seuil corrigé pour {k} tests, Bonferroni bilatéral à 5 %) ; le verdict au seuil simple t ≥ 2 est montré à côté, pour comparaison.",
            fr(t_req, 0, 2)
        ),
        "Tendance à volatilité gérée : exposition réduite quand la volatilité 20 jours dépasse sa médiane passée (au carré, plafonnée à 100 %, par quarts), seulement au-dessus de la moyenne 200 jours ; comparée à la détention par son ratio de Sharpe et sa pire baisse.".into(),
        format!(
            "Test sur l'avenir : les signaux datés après le {} sont jugés à part, par le même calcul figé ; c'est le seul test vraiment neuf.",
            frdate(PREREG_DATE)
        ),
        format!(
            "Dans une décision : seuls les 4 résultats principaux (2 familles × 2 horizons) peuvent compter, du côté prouvé au seuil corrigé et non contredit par le test sur l'avenir (au moins {MIN_TRADES} signaux) ; au plus {} points de confiance, jamais contre un veto.",
            fr(NUDGE, 0, 0)
        ),
    ]
}

pub fn limits() -> Vec<String> {
    vec![
        "Biais du survivant : 125 grands actifs d'aujourd'hui ; ceux qui ont disparu ou chuté depuis 1990 manquent, ce qui embellit le passé (et gonfle les actions « gagnantes » du classement).".into(),
        "Déjà regardé : les résultats v1 et v2 étaient connus quand ce protocole a été écrit ; le seuil corrigé et le test sur l'avenir sont là pour ça, sans l'effacer tout à fait.".into(),
        "Le seuil corrigé (Bonferroni) est prudent : il suppose les tests indépendants alors qu'ils se recoupent ; un vrai petit avantage peut passer inaperçu.".into(),
        "Actifs corrélés : même compté par jour, le t reste optimiste quand les jours voisins se ressemblent ; à 60 jours les signaux sont peu nombreux et se chevauchent entre actifs.".into(),
        "Classement entre pairs : un avantage relatif n'est pas un gain absolu ; en marché baissier, les mieux classés peuvent aussi baisser.".into(),
        "Cryptos : peu d'actifs avant 2018 ; le classement entre pairs n'existe qu'à partir de 5 actifs cotés le même jour.".into(),
        "Cours ajustés des divisions, pas des dividendes ; coûts supposés (frais, glissement, écart moyen de la classe), sans impôt.".into(),
        "Pour un actif hors de l'univers, les modèles de son groupe s'appliquent sans avoir été testés sur lui ; son classement est calculé contre les actifs de l'univers du même jour.".into(),
    ]
}

/// Gives the allocator's free memory back to the system (glibc keeps what the threads freed): the peak stays that of
/// one step, not of their sum.
pub fn release_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        // SAFETY: glibc's malloc_trim only returns unused heap pages to the system.
        unsafe {
            malloc_trim(0);
        }
    }
}

/// Stage timings on stderr when `ALTIM_BOT_TRACE` is set (time and memory of each step).
struct Trace(Option<std::time::Instant>);

impl Trace {
    fn new() -> Trace {
        Trace(std::env::var_os("ALTIM_BOT_TRACE").map(|_| std::time::Instant::now()))
    }

    fn at(&self, what: &str) {
        release_memory();
        if let Some(t) = self.0 {
            let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
            let kb = |key: &str| status.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)).unwrap_or("?").to_string();
            eprintln!("[bot v3] {:>7.1} s  RSS {} kB (pic {} kB)  {what}", t.elapsed().as_secs_f64(), kb("VmRSS:"), kb("VmHWM:"));
        }
    }
}

/// Trains and tests v3 (and v2's selection, in the report's v2-shaped fields) on the fetched histories (any order).
/// `threads`: blocks computed at the same time (same result).
pub fn run(histories: Vec<History>, failures: Vec<Failure>, now: i64, source: &str, threads: usize) -> BotReport {
    let mut histories = histories;
    let (mut failures, mut extra_failures) = bot::prepare(&mut histories, failures);
    let k = k_count();
    let t_req = t_required(k.total);
    let mut groups: Vec<BotGroupStat> = Vec::new();
    let mut overall = Samples::default();
    let mut assets: Vec<BotAssetRow> = Vec::new();
    let mut v3_groups: Vec<V3Group> = Vec::new();
    let mut live = V3Live::default();
    let mut compute = Compute { row_bytes: std::mem::size_of::<Row>(), ..Compute::default() };
    let mut nows: HashMap<String, V3AssetNow> = HashMap::new();
    let trace = Trace::new();
    // v4 (selective bots): same rows, blocks and fitted models; its own corrected threshold and forward test.
    let k4 = bot_v4::k_count();
    let t_req4 = bot_v4::t_required(k4.total);
    let mut bots4: BTreeMap<(usize, usize, bot_v4::Side), bot_v4::SelectiveBot> = BTreeMap::new();
    let mut live4 = bot_v4::V4Live::default();
    let mut nows4: HashMap<String, bot_v4::V4AssetNow> = HashMap::new();
    for g in GROUPS {
        // The rows once (features at each close, labels at the first horizon), then everything the candles give;
        // the candles are dropped (memory) and the labels of the other horizon come from the prices kept.
        let Some(mut d) = group_rows(g, &histories, HORIZONS[0], Some((&mut failures, &mut extra_failures))) else { continue };
        compute.max_rows = compute.max_rows.max(d.rows.len());
        trace.at(&format!("{g:?} : {} lignes", d.rows.len()));
        let dates = timeline(&d.rows);
        let basket: Vec<&History> = d.members.iter().filter(|k| !d.is_extra[**k] && d.spans.contains_key(*k)).map(|k| &histories[*k]).collect();
        let vol = dates.get(first_test(HORIZONS[0])).map(|from| vol_managed(&basket, if g == BotGroup::Crypto { 365.0 } else { 252.0 }, *from));
        let prices: HashMap<usize, Prices> = d.spans.keys().map(|k| (*k, Prices::of(&histories[*k].candles))).collect();
        for k in &d.members {
            histories[*k].candles = Vec::new();
        }
        let kinds: Vec<Kind> = histories.iter().map(|x| x.asset.kind).collect();
        let symbol = |r: &Row| histories[r.asset].asset.symbol;
        let is_extra = d.is_extra.clone();
        let mut vg = V3Group { id: g, label: g.label().into(), vol_managed: vol, ..V3Group::default() };
        let mut lg = V3LiveGroup { group: g, horizons: Vec::new() };
        let mut lg4 = bot_v4::V4LiveGroup { group: g, horizons: Vec::new() };
        let gi = GROUPS.iter().position(|x| *x == g).unwrap_or(0);
        let main4 = |r: &Row| !is_extra[r.asset] && r.time < bot_v4::FORWARD_FROM;
        let extra4 = |r: &Row| is_extra[r.asset] && r.time < bot_v4::FORWARD_FROM;
        let fwd4 = |r: &Row| !is_extra[r.asset] && r.time >= bot_v4::FORWARD_FROM;
        let cost = |_: usize| round_trip_cost(if g == BotGroup::Crypto { Kind::Crypto } else { Kind::Stock });
        // Family A, horizon by horizon, on the raw features.
        let mut a_parts: Vec<(Vec<(bot::Block, AChoice)>, Vec<V3Config>)> = Vec::new();
        for (hi, &h) in HORIZONS.iter().enumerate() {
            if hi > 0 {
                relabel(&mut d.rows, &prices, &kinds, h);
            }
            let aw = a_walk(&d.rows, h, threads);
            compute.blocks += aw.blocks.len();
            trace.at(&format!("{g:?} {h} j : famille A (walk-forward)"));
            let la = a_fit(&d.rows, &dates, dates.len(), h);
            if hi == 0 {
                let extra_failed = extra_failures.iter().filter(|f| BotGroup::of(f.kind) == g).count();
                let preds_of = |c: Option<Candidate>| match c {
                    None => aw.v2_nested(&d.rows, threads),
                    Some(c) => aw.slot(&d.rows, threads, c.index()),
                };
                let (group, a, samples) = bot::assemble(&d, &histories, &aw.blocks, &preds_of, &la.v2, extra_failed, t_req, FORWARD_FROM);
                overall.merge(&samples);
                assets.extend(a);
                vg.market = group.market.clone();
                vg.universe = group.universe.clone();
                groups.push(group);
            }
            let eval_a = |p: &[Option<Prediction>], keep: &dyn Fn(&Row) -> bool| evaluate_h(&d.rows, p, keep, cost, h).0;
            let mut configs = vec![
                config(
                    head("absolute", Family::Absolute, None, true, format!("{} (choix v3)", Family::Absolute.label())),
                    &d.rows,
                    &aw.nested(&d.rows, threads),
                    &is_extra,
                    t_req,
                    &eval_a,
                    None,
                    true,
                ),
                config(
                    head("v2", Family::V2, None, true, "Sélection v2 (log-loss), refaite sur les nouvelles données".into()),
                    &d.rows,
                    &aw.v2_nested(&d.rows, threads),
                    &is_extra,
                    t_req,
                    &eval_a,
                    None,
                    true,
                ),
            ];
            let (mut p_logit, mut p_long) = (Vec::new(), Vec::new());
            for c in A_CANDS {
                let preds = aw.slot(&d.rows, threads, c.a_slot().unwrap());
                let mut x =
                    config(head(c.id(), Family::Absolute, Some(c), false, c.label().into()), &d.rows, &preds, &is_extra, t_req, &eval_a, None, true);
                x.chosen_blocks = aw.choices.iter().filter(|b| b.chosen == Some(c)).count();
                configs.push(x);
                match c {
                    V3Candidate::Logit => p_logit = preds,
                    V3Candidate::TreesLong => p_long = preds,
                    _ => {}
                }
            }
            // v4, family A: the rise and fall bots, from the logistic and long trees' predictions.
            let gates: Vec<[bot_v4::Gate; 2]> = aw.choices.iter().map(|c| c.v4).collect();
            for side in [bot_v4::Side::Rise, bot_v4::Side::Fall] {
                let ev = |keep: &dyn Fn(&Row) -> bool| {
                    bot_v4::PrecisionStats::of(&bot_v4::evaluate_a(&d.rows, [&p_logit, &p_long], &gates, side, keep), t_req4)
                };
                let b = bot_v4::SelectiveBot::new(g, h, side, &gates).with_stats(ev(&main4), ev(&extra4), ev(&fwd4));
                bots4.insert((gi, h, side), b);
            }
            drop((p_logit, p_long));
            let a4 = match (&la.pairs[V3Candidate::Logit.a_slot().unwrap()], &la.pairs[V3Candidate::TreesLong.a_slot().unwrap()]) {
                (Some(p0), Some(p1)) => Some([p0.clone(), p1.clone()]),
                _ => None,
            };
            let lh4 = bot_v4::V4LiveH { horizon: h, a: a4, a_gates: la.v4, ..bot_v4::V4LiveH::default() };
            // Today: family A's models and actions.
            let absolute = la.chosen.and_then(|c| Some((c, la.pairs[c.a_slot()?].clone()?)));
            for (k, (a0, a1)) in &d.spans {
                if d.is_extra[*k] || a1 <= a0 {
                    continue;
                }
                let r = &d.rows[*a1 - 1];
                let n = nows.entry(histories[*k].asset.symbol.to_string()).or_default();
                n.time = Some(r.time);
                let act = absolute.as_ref().map(|(_, p)| p.predict(&r.x, r.trend).action);
                if h == 20 {
                    n.absolute20 = act
                } else {
                    n.absolute60 = act
                }
                let n4 = nows4.entry(histories[*k].asset.symbol.to_string()).or_default();
                n4.time = Some(r.time);
                for side in [bot_v4::Side::Rise, bot_v4::Side::Fall] {
                    if lh4.avis(side, &r.x, r.trend, None).is_some_and(|a| a.0) {
                        n4.bots.push(bot_v4::bot_id(g, h, side));
                        if let Some(b) = bots4.get_mut(&(gi, h, side)) {
                            b.today.push(histories[*k].asset.symbol.to_string());
                        }
                    }
                }
            }
            for side in [bot_v4::Side::Rise, bot_v4::Side::Fall] {
                if let Some(b) = bots4.get_mut(&(gi, h, side)) {
                    b.today_level = lh4.a_gates[side.slot()].level.map(|q| round_to(q * 100.0, 1));
                    b.today_time = d.rows.iter().filter(|r| !is_extra[r.asset]).map(|r| r.time).max();
                }
            }
            lg4.horizons.push(lh4);
            lg.horizons.push(V3LiveH { horizon: h, absolute, ..V3LiveH::default() });
            a_parts.push((aw.blocks.iter().cloned().zip(aw.choices.iter().cloned()).collect(), configs));
            trace.at(&format!("{g:?} {h} j : famille A (évaluations)"));
        }
        // The peers' reference (raw features of the last day), then the ranks among peers (the same for both
        // horizons: features do not depend on the horizon).
        let last_day = d.rows.iter().map(|r| day_of(r.time)).max();
        let last_rows: Vec<usize> = (0..d.rows.len()).filter(|i| Some(day_of(d.rows[*i].time)) == last_day).collect();
        let reference: Vec<(String, Features)> = last_rows.iter().map(|&i| (symbol(&d.rows[i]).to_string(), d.rows[i].x)).collect();
        let ranked = rank_peers(&mut d.rows);
        vg.peers_from = d.rows.iter().zip(&ranked).filter(|(_, x)| **x).map(|(r, _)| r.time).min();
        // Family B, from the last horizon's labels back to the first.
        let mut horizons: Vec<Option<V3Horizon>> = vec![None; HORIZONS.len()];
        for hi in (0..HORIZONS.len()).rev() {
            let h = HORIZONS[hi];
            if hi != HORIZONS.len() - 1 {
                relabel(&mut d.rows, &prices, &kinds, h);
            }
            let xs: Vec<XsRow> = peer_labels(&d.rows).into_iter().zip(&ranked).map(|(label, r)| XsRow { ranked: *r, label }).collect();
            let bw = b_walk(&d.rows, &xs, h, threads);
            compute.blocks += bw.starts.len();
            trace.at(&format!("{g:?} {h} j : famille B (walk-forward)"));
            let eval_b = |p: &[Option<Prediction>], keep: &dyn Fn(&Row) -> bool| evaluate_peers(&d.rows, &xs, p, keep, false);
            let eval_m = |p: &[Option<Prediction>], keep: &dyn Fn(&Row) -> bool| evaluate_peers(&d.rows, &xs, p, keep, true);
            let (a_blocks, mut configs) = std::mem::take(&mut a_parts[hi]);
            configs.insert(
                1,
                config(
                    head("peers", Family::Peers, None, true, format!("{} (choix v3)", Family::Peers.label())),
                    &d.rows,
                    &bw.nested(&d.rows, &xs, threads),
                    &is_extra,
                    t_req,
                    &eval_b,
                    Some(&eval_m),
                    false,
                ),
            );
            let (mut p_xlogit, mut p_xtrees) = (Vec::new(), Vec::new());
            for c in B_CANDS {
                let preds = bw.slot(&d.rows, &xs, threads, c.b_slot().unwrap());
                let mut x = config(
                    head(c.id(), Family::Peers, Some(c), false, c.label().into()),
                    &d.rows,
                    &preds,
                    &is_extra,
                    t_req,
                    &eval_b,
                    Some(&eval_m),
                    false,
                );
                x.chosen_blocks = bw.choices.iter().filter(|b| b.chosen == Some(c)).count();
                configs.push(x);
                match c {
                    V3Candidate::XsLogit => p_xlogit = preds,
                    V3Candidate::XsTrees => p_xtrees = preds,
                    _ => {}
                }
            }
            // v4, family B: the top and bottom bots.
            let gates: Vec<[bot_v4::Gate; 2]> = bw.choices.iter().map(|c| c.v4).collect();
            for side in [bot_v4::Side::Top, bot_v4::Side::Bottom] {
                let ev = |keep: &dyn Fn(&Row) -> bool| {
                    bot_v4::PrecisionStats::of(&bot_v4::evaluate_b(&d.rows, &xs, [&p_xlogit, &p_xtrees], &gates, side, keep), t_req4)
                };
                let b = bot_v4::SelectiveBot::new(g, h, side, &gates).with_stats(ev(&main4), ev(&extra4), ev(&fwd4));
                bots4.insert((gi, h, side), b);
            }
            drop((p_xlogit, p_xtrees));
            // Today: family B's model, the day's cut-offs and the basket's actions.
            let lb = b_fit(&d.rows, &xs, &dates, dates.len(), h);
            // v4 today: the peers bots on the basket's last day.
            if let Some(lh4) = lg4.horizons.iter_mut().find(|x| x.horizon == h) {
                lh4.b_gates = lb.v4;
                lh4.b = match (&lb.models[1], &lb.models[2]) {
                    (Some(m0), Some(m1)) => Some([m0.clone(), m1.clone()]),
                    _ => None,
                };
                for side in [bot_v4::Side::Top, bot_v4::Side::Bottom] {
                    let speaking: Vec<usize> = last_rows
                        .iter()
                        .copied()
                        .filter(|i| xs[*i].ranked && !d.is_extra[d.rows[*i].asset])
                        .filter(|i| lh4.avis(side, &d.rows[*i].x, d.rows[*i].trend, Some(&d.rows[*i].x)).is_some_and(|a| a.0))
                        .collect();
                    if let Some(b) = bots4.get_mut(&(gi, h, side)) {
                        b.today_level = lb.v4[side.slot()].level.map(|q| round_to(q * 100.0, 1));
                        b.today_time = last_day.map(|d| d * DAY_MS);
                        b.today = speaking.iter().map(|i| symbol(&d.rows[*i]).to_string()).collect();
                    }
                    for i in speaking {
                        let n4 = nows4.entry(symbol(&d.rows[i]).to_string()).or_default();
                        n4.bots.push(bot_v4::bot_id(g, h, side));
                    }
                }
            }
            let peers = lb.chosen.and_then(|c| Some((c, lb.models[c.b_slot()?].clone()?)));
            let (mut cut_top, mut cut_bottom) = (None, None);
            if let Some((_, m)) = &peers {
                let day: Vec<usize> = last_rows.iter().copied().filter(|i| xs[*i].ranked).collect();
                let actions = rank_actions(&d.rows, &day, m);
                let mut scores: Vec<f64> = day.iter().map(|i| m.score(&d.rows[*i].x)).collect();
                scores.sort_by(|a, b| b.total_cmp(a));
                let n = scores.len();
                if n >= MIN_XS {
                    (cut_top, cut_bottom) = (Some(scores[n / 5 - 1]), Some(scores[n - n / 5]));
                }
                for i in day {
                    if d.is_extra[d.rows[i].asset] {
                        continue;
                    }
                    let n = nows.entry(symbol(&d.rows[i]).to_string()).or_default();
                    if h == 20 { n.peers20 = actions.get(&i).copied() } else { n.peers60 = actions.get(&i).copied() }
                }
            }
            let lh = &mut lg.horizons[hi];
            (lh.peers, lh.reference, lh.cut_top, lh.cut_bottom) = (peers, reference.clone(), cut_top, cut_bottom);
            let rounds = |f: &dyn Fn(&AChoice) -> Option<usize>| rounds_of(&a_blocks.iter().filter_map(|(_, c)| f(c)).collect::<Vec<_>>());
            let selection: Vec<V3BlockOut> = a_blocks
                .iter()
                .zip(&bw.choices)
                .map(|((b, a), p)| V3BlockOut {
                    start: b.start,
                    end: (b.end != i64::MAX).then_some(b.end),
                    train_rows: b.train_rows,
                    v2: b.chosen,
                    absolute: a.chosen,
                    peers: p.chosen,
                    absolute_scores: a.scores.iter().map(|s| EconScore { score: s.score.map(|v| round_to(v, 4)), ..*s }).collect(),
                    peers_scores: p.scores.iter().map(|s| EconScore { score: s.score.map(|v| round_to(v, 4)), ..*s }).collect(),
                    rounds_up: a.rounds.0,
                    rounds_down: a.rounds.1,
                    rounds_peers: p.rounds,
                })
                .collect();
            let tested: Vec<i64> =
                d.rows.iter().filter(|r| !d.is_extra[r.asset] && dates.get(first_test(h)).is_some_and(|f| r.time >= *f)).map(|r| r.time).collect();
            horizons[hi] = Some(V3Horizon {
                horizon: h,
                blocks: a_blocks.len(),
                test_from: tested.iter().min().copied(),
                test_to: tested.iter().max().copied(),
                configs,
                rounds_up: rounds(&|c| c.rounds.0),
                rounds_down: rounds(&|c| c.rounds.1),
                rounds_peers: rounds_of(&bw.choices.iter().filter_map(|c| c.rounds).collect::<Vec<_>>()),
                selection,
            });
            trace.at(&format!("{g:?} {h} j : famille B (évaluations)"));
        }
        vg.horizons = horizons.into_iter().flatten().collect();
        vg.text = group_text(g, &vg.horizons, t_req);
        v3_groups.push(vg);
        live.groups.push(lg);
        live4.groups.push(lg4);
    }
    for a in assets.iter_mut() {
        a.v3 = nows.remove(&a.symbol);
        a.v4 = nows4.remove(&a.symbol).map(|mut n| {
            n.bots.sort_by_key(|id| bots4.values().position(|b| b.id == *id));
            n
        });
    }
    let bots4: Vec<bot_v4::SelectiveBot> = bots4.into_values().collect();
    let v4 = bot_v4::V4Report {
        version: bot_v4::VERSION,
        prereg_date: bot_v4::PREREG_DATE.into(),
        forward_from: bot_v4::FORWARD_FROM,
        after_prereg: bot_v4::AFTER_PREREG.iter().map(|s| s.to_string()).collect(),
        k: k4,
        alpha: bot_v4::ALPHA,
        t_required: round_to(t_req4, 4),
        headline: bot_v4::headline(&bots4, t_req4, k4.total),
        forward_headline: bot_v4::forward_headline(&bots4),
        method: bot_v4::method(t_req4, &k4),
        limits: bot_v4::limits(),
        parameters: bot_v4::V4Parameters {
            levels: bot_v4::LEVELS.iter().map(|q| round_to(q * 100.0, 1)).collect(),
            min_val_signals: bot_v4::MIN_VAL_SIGNALS,
            wilson_z: bot_v4::WILSON_Z,
            min_signals: MIN_TRADES,
            nudge: bot_v4::NUDGE,
        },
        bots: bots4,
    };
    let mut report = bot::report_of(groups, overall, assets, failures, extra_failures, now, source);
    let v3 = V3Report {
        version: VERSION,
        prereg_date: PREREG_DATE.into(),
        forward_from: FORWARD_FROM,
        after_prereg: AFTER_PREREG.iter().map(|s| s.to_string()).collect(),
        k,
        alpha: ALPHA,
        t_required: round_to(t_req, 4),
        headline: headline(&v3_groups, t_req, k.total),
        forward_headline: forward_headline(&v3_groups),
        candidates: A_CANDS
            .iter()
            .chain(&B_CANDS)
            .map(|c| CandidateInfo { id: *c, family: c.family(), label: c.label().into(), description: c.description() })
            .collect(),
        changes: changes(&k, t_req),
        method: method(t_req, k.total),
        limits: limits(),
        parameters: V3Parameters {
            horizons: HORIZONS.to_vec(),
            stock_from: "1990-01-01".into(),
            max_tree_rows: MAX_TREE_ROWS,
            long_max_rounds: bot_trees::LONG.trees,
            long_depth: bot_trees::LONG.depth,
            long_shrinkage: bot_trees::LONG.shrinkage,
            long_min_leaf: bot_trees::LONG.min_leaf,
            patience: bot_trees::PATIENCE,
            min_peers: MIN_XS,
            min_val_signals: MIN_VAL_SIGNALS,
            min_signals: MIN_TRADES,
            nudge: NUDGE,
        },
        compute,
        groups: v3_groups,
    };
    report.headline = v3.headline.clone();
    report.version = VERSION;
    report.changes = v3.changes.clone();
    report.parameters.stock_years = ((now / 1000 - STOCK_FROM_S) as f64 / (365.25 * 86_400.0)).floor() as i64;
    report.v3 = Some(v3);
    report.v3_live = Some(Arc::new(live));
    report.v4 = Some(v4);
    report.v4_live = Some(Arc::new(live4));
    report
}

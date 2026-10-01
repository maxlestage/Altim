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

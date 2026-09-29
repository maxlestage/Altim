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
/// Changes made after the pre-registration (none so far).
pub const AFTER_PREREG: [&str; 0] = [];

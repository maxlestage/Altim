//! « Bot Altim » (`/api/bot`, `bot` of `/api/decision`): two L2-regularised logistic regressions trained on the
//! daily candles of the validation's fixed basket (`validation::BASKET`), one estimating the probability that the
//! next 20 trading days end above the round-trip cost ("hausse"), the other below minus that cost ("baisse"). The
//! bot says ACHETER when the first is high enough, VENDRE (leave or stay out: Altim advises spot holders, never a
//! short sale) when the second is, ATTENDRE otherwise. Pure: the route fetches the histories, this module computes.
//!
//! Honesty guarantees:
//! - features at the close of day t read the candles up to t only; the trade is taken at the next open (t + 1) and
//!   closed at the open of t + 1 + H, like the validation's backtest (costs of the class included);
//! - walk-forward: models retrained every 126 dates on an expanding window, standardised with the training rows'
//!   statistics only, and a purge: a training row's label must end at least H dates before the test block starts
//!   (the 20-day labels of consecutive days overlap);
//! - thresholds fixed from the training window only (its base rate + 5 points), never tuned on the test periods;
//! - out-of-sample evaluation with non-overlapping signals per asset, compared with a day picked at random in the
//!   same asset and test block; the verdict reuses the validation's rule (≥ 30 signals, t ≥ 2);
//! - deterministic (Newton's method from zero, no randomness).
//!
//! One model pair per group: stocks and ETFs (22 assets, ≈ 5 years each), cryptos (Bitcoin, Ethereum and the ten
//! altcoins pooled: ≈ 2.7 years each, too short to split by class).
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::backtest::{FEE_RATE, Regime, regime_at, regime_label};
use super::metrics::{SLIPPAGE, SPREAD_CRYPTO, SPREAD_STOCK, with_costs};
use super::signal::{atr, ema, ema_opt, rsi, sanitize};
use super::validation::{AssetClass, BASKET, BASKET_FIXED_ON, BasketAsset, Failure, MIN_TRADES, Pooled, T_EDGE, Verdict, median, verdict};
use crate::js::fr;
use crate::types::{Candle, DAY_MS, Kind};

/// Forward horizon of the labels (trading days / daily candles).
pub const HORIZON: usize = 20;
/// Models retrained every this many dates of the group's timeline.
pub const RETRAIN_EVERY: usize = 126;
/// Dates between the end of the last training label and the start of the test block.
pub const PURGE: usize = HORIZON;
/// Dates of training before the first test block.
pub const MIN_TRAIN: usize = 504;
/// Candles before the first feature row (200-day average and high, and the regime's 220 candles).
pub const WARMUP: usize = 220;
/// Weight of the L2 penalty on the standardised coefficients (mean log-loss + L2 / 2 × Σ w²).
pub const L2: f64 = 0.05;
pub const NEWTON_STEPS: usize = 30;
/// A signal needs its probability at least this far above the training window's base rate.
pub const THRESHOLD_MARGIN: f64 = 0.05;
/// A block is only tested with at least this many training rows (and both outcomes present).
pub const MIN_TRAIN_ROWS: usize = 250;
/// A decision's confidence moves by at most this much, and only on a side with an out-of-sample edge.
pub const NUDGE: f64 = 3.0;
pub const LINK: &str = "/app/bot";
pub const NOT_COMPUTED: &str = "Bot pas encore entraîné : ouvrez l'écran « Bot Altim » pour lancer l'entraînement (quelques minutes).";
pub const NO_EDGE: &str = "Le bot n'a pas démontré d'avantage hors échantillon : il ne compte pas dans la décision.";

pub const N_FEATURES: usize = 16;

/// A feature: id (JSON), French label, what it measures.
pub struct FeatureDef {
    pub id: &'static str,
    pub label: &'static str,
    pub help: &'static str,
}

const fn f(id: &'static str, label: &'static str, help: &'static str) -> FeatureDef {
    FeatureDef { id, label, help }
}

pub const FEATURES: [FeatureDef; N_FEATURES] = [
    f("ret5", "Variation sur 5 jours", "Variation du cours de clôture sur 5 bougies, en %."),
    f("ret20", "Variation sur 20 jours", "Variation du cours de clôture sur 20 bougies, en %."),
    f("ret60", "Variation sur 60 jours", "Variation du cours de clôture sur 60 bougies, en %."),
    f("rsi14", "RSI 14", "Indice de force relative sur 14 jours (0 à 100)."),
    f("ema50", "Écart à la moyenne 50 jours", "Écart de la clôture à sa moyenne mobile exponentielle 50 jours, en %."),
    f("ema200", "Écart à la moyenne 200 jours", "Écart de la clôture à sa moyenne mobile exponentielle 200 jours, en %."),
    f("macd", "MACD (histogramme)", "Histogramme du MACD 12/26/9 rapporté au cours, en %."),
    f("atr14", "Amplitude moyenne (ATR 14)", "ATR 14 jours rapporté au cours, en %."),
    f("vol20", "Volatilité 20 jours", "Écart type des variations journalières sur 20 jours, en %."),
    f("volume", "Volume inhabituel", "Volume du jour en écarts types par rapport aux 20 jours précédents (0 sans volume)."),
    f("drawdown", "Recul depuis le plus haut 200 jours", "Écart de la clôture au plus haut des 200 derniers jours, en % (≤ 0)."),
    f("range20", "Position dans la fourchette 20 jours", "0 au plus bas des 20 derniers jours, 1 au plus haut."),
    f("bull", "Régime haussier", "1 en marché haussier (règle de la validation), sinon 0."),
    f("bear", "Régime baissier", "1 en marché baissier, sinon 0."),
    f("range", "Régime sans tendance", "1 en marché sans tendance, sinon 0."),
    f("crisis", "Régime de crise", "1 à plus de 30 % sous le plus haut d'un an, sinon 0."),
];

pub type Features = [f64; N_FEATURES];

fn round_to(x: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    (x * p).round() / p
}

// ---------- Groups ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BotGroup {
    #[default]
    Stock,
    Crypto,
}

pub const GROUPS: [BotGroup; 2] = [BotGroup::Stock, BotGroup::Crypto];

impl BotGroup {
    pub fn of(kind: Kind) -> BotGroup {
        match kind {
            Kind::Stock => BotGroup::Stock,
            Kind::Crypto => BotGroup::Crypto,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            BotGroup::Stock => "stock",
            BotGroup::Crypto => "crypto",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BotGroup::Stock => "Actions et ETF américains",
            BotGroup::Crypto => "Cryptos (bitcoin, ether, altcoins)",
        }
    }

    fn short(self) -> &'static str {
        match self {
            BotGroup::Stock => "actions",
            BotGroup::Crypto => "cryptos",
        }
    }
}

// ---------- Features (no look-ahead) ----------

/// Indicator series of one history, computed once; `at(i)` reads index ≤ i only (every series is recursive forward).
pub struct Indicators<'a> {
    c: &'a [Candle],
    ema50: Vec<Option<f64>>,
    ema200: Vec<Option<f64>>,
    rsi: Vec<Option<f64>>,
    hist: Vec<Option<f64>>,
    atr: Vec<Option<f64>>,
}

impl<'a> Indicators<'a> {
    /// `c` must be sanitized (sorted, valid).
    pub fn new(c: &'a [Candle]) -> Self {
        let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
        let (e12, e26) = (ema(&closes, 12), ema(&closes, 26));
        let macd: Vec<Option<f64>> = e12.iter().zip(&e26).map(|(a, b)| Some((*a)? - (*b)?)).collect();
        let signal = ema_opt(&macd, 9);
        let hist = macd.iter().zip(&signal).map(|(m, s)| Some((*m)? - (*s)?)).collect();
        Indicators { c, ema50: ema(&closes, 50), ema200: ema(&closes, 200), rsi: rsi(&closes, 14), hist, atr: atr(c, 14) }
    }

    /// Features at the close of candle i; None during the warm-up or with a missing value.
    pub fn at(&self, i: usize) -> Option<Features> {
        let c = self.c;
        if i < WARMUP || i >= c.len() {
            return None;
        }
        let close = c[i].close;
        let ret = |k: usize| (close / c[i - k].close - 1.0) * 100.0;
        let logs: Vec<f64> = (i - 19..=i).map(|j| (c[j].close / c[j - 1].close).ln()).collect();
        let m = logs.iter().sum::<f64>() / logs.len() as f64;
        let vol20 = (logs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (logs.len() - 1) as f64).sqrt() * 100.0;
        let prev: Vec<f64> = c[i - 20..i].iter().map(|x| x.volume).collect();
        let vm = prev.iter().sum::<f64>() / 20.0;
        let vsd = (prev.iter().map(|x| (x - vm).powi(2)).sum::<f64>() / 19.0).sqrt();
        let volume = if vm > 0.0 && vsd > 0.0 { ((c[i].volume - vm) / vsd).clamp(-5.0, 5.0) } else { 0.0 };
        let high200 = c[i - 199..=i].iter().fold(0.0f64, |a, x| a.max(x.high));
        let hi20 = c[i - 19..=i].iter().fold(f64::MIN, |a, x| a.max(x.high));
        let lo20 = c[i - 19..=i].iter().fold(f64::MAX, |a, x| a.min(x.low));
        let regime = regime_at(c, i);
        let one = |r: Regime| if regime == r { 1.0 } else { 0.0 };
        let x = [
            ret(5),
            ret(20),
            ret(60),
            self.rsi[i]?,
            (close / self.ema50[i]? - 1.0) * 100.0,
            (close / self.ema200[i]? - 1.0) * 100.0,
            self.hist[i]? / close * 100.0,
            self.atr[i]? / close * 100.0,
            vol20,
            volume,
            if high200 > 0.0 { (close / high200 - 1.0) * 100.0 } else { 0.0 },
            if hi20 > lo20 { (close - lo20) / (hi20 - lo20) } else { 0.5 },
            one(Regime::Bull),
            one(Regime::Bear),
            one(Regime::Range),
            one(Regime::Crisis),
        ];
        x.iter().all(|v| v.is_finite()).then_some(x)
    }
}

// ---------- Labels ----------

/// Cost paid on each side besides the fees: slippage + half the class's default spread (as the validation).
pub fn side_cost(kind: Kind) -> f64 {
    let spread = if kind == Kind::Crypto { SPREAD_CRYPTO } else { SPREAD_STOCK };
    SLIPPAGE + spread / 100.0 / 2.0
}

/// Return after the fees on both orders and the side costs, % (`gross` in %).
pub fn net_return(gross: f64, kind: Kind) -> f64 {
    with_costs(((1.0 + gross / 100.0) * (1.0 - FEE_RATE).powi(2) - 1.0) * 100.0, side_cost(kind))
}

/// Round-trip cost in % of the amount (≈ 0.3 %).
pub fn round_trip_cost(kind: Kind) -> f64 {
    -net_return(0.0, kind)
}

/// What followed the close of a day: bought at the next open, sold at the open H candles later.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Forward {
    /// Time of the exit candle (ms): the label is only known then.
    pub exit_time: i64,
    pub entry: f64,
    pub exit: f64,
    /// Return of the price, % (what holding made), and after the round-trip costs.
    pub gross: f64,
    pub net: f64,
    /// Worst low during the holding versus the entry, % (≤ 0).
    pub drawdown: f64,
    /// Net return > 0 ("hausse"), gross return < − the round-trip cost ("baisse").
    pub up: bool,
    pub down: bool,
}

pub fn forward(c: &[Candle], i: usize, kind: Kind) -> Option<Forward> {
    let (e, x) = (i + 1, i + 1 + HORIZON);
    if x >= c.len() || c[e].open <= 0.0 {
        return None;
    }
    let gross = (c[x].open / c[e].open - 1.0) * 100.0;
    let low = c[e..x].iter().fold(c[e].open, |a, k| a.min(k.low));
    let net = net_return(gross, kind);
    Some(Forward {
        exit_time: c[x].time,
        entry: c[e].open,
        exit: c[x].open,
        gross,
        net,
        drawdown: (low / c[e].open - 1.0) * 100.0,
        up: net > 0.0,
        down: gross < -round_trip_cost(kind),
    })
}

/// One day of one asset: its features at the close, and what followed when already known.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub asset: usize,
    pub time: i64,
    pub x: Features,
    pub fwd: Option<Forward>,
}

/// Every feature row of one history (`raw` in any order; sanitized here).
pub fn asset_rows(asset: usize, raw: &[Candle], kind: Kind) -> Vec<Row> {
    let c = sanitize(raw);
    let ind = Indicators::new(&c);
    (WARMUP..c.len()).filter_map(|i| ind.at(i).map(|x| Row { asset, time: c[i].time, x, fwd: forward(&c, i, kind) })).collect()
}

// ---------- Logistic regression ----------

/// Standardisation (training statistics) and coefficients of one logistic regression.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub mean: Features,
    pub sd: Features,
    pub coef: Features,
    pub intercept: f64,
    /// Share of positive labels in the training rows (0-1).
    pub base_rate: f64,
    pub rows: usize,
}

fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 { 1.0 / (1.0 + (-z).exp()) } else { z.exp() / (1.0 + z.exp()) }
}

/// Solves `a x = b` (Gaussian elimination, partial pivoting); None when singular.
#[allow(clippy::needless_range_loop)] // matrix indices read closer to the maths
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for k in 0..n {
        let p = (k..n).max_by(|i, j| a[*i][k].abs().total_cmp(&a[*j][k].abs()))?;
        if a[p][k].abs() < 1e-14 {
            return None;
        }
        a.swap(k, p);
        b.swap(k, p);
        for i in k + 1..n {
            let m = a[i][k] / a[k][k];
            if m != 0.0 {
                for j in k..n {
                    a[i][j] -= m * a[k][j];
                }
                b[i] -= m * b[k];
            }
        }
    }
    let mut x = vec![0.0; n];
    for k in (0..n).rev() {
        let s: f64 = (k + 1..n).map(|j| a[k][j] * x[j]).sum();
        x[k] = (b[k] - s) / a[k][k];
    }
    Some(x)
}

impl Model {
    pub fn z(&self, x: &Features) -> Features {
        std::array::from_fn(|j| (x[j] - self.mean[j]) / self.sd[j])
    }

    /// Probability (0-1) of the positive label.
    pub fn prob(&self, x: &Features) -> f64 {
        let z = self.z(x);
        sigmoid(self.intercept + (0..N_FEATURES).map(|j| self.coef[j] * z[j]).sum::<f64>())
    }

    /// Fits on `xs` / `ys` (same length, at least one row): standardised with these rows' mean and standard deviation
    /// (1 when constant), then Newton's method from zero on the mean log-loss + L2 / 2 × Σ coef² (intercept free).
    #[allow(clippy::needless_range_loop)]
    pub fn fit(xs: &[&Features], ys: &[bool]) -> Model {
        let n = xs.len().max(1) as f64;
        let mean: Features = std::array::from_fn(|j| xs.iter().map(|x| x[j]).sum::<f64>() / n);
        let sd: Features = std::array::from_fn(|j| {
            let v = (xs.iter().map(|x| (x[j] - mean[j]).powi(2)).sum::<f64>() / n).sqrt();
            if v > 1e-12 { v } else { 1.0 }
        });
        let base = ys.iter().filter(|y| **y).count() as f64 / n;
        let mut model = Model { mean, sd, coef: [0.0; N_FEATURES], intercept: 0.0, base_rate: base, rows: xs.len() };
        let zs: Vec<Features> = xs.iter().map(|x| model.z(x)).collect();
        let d = N_FEATURES + 1;
        // w[0..N] coefficients, w[N] intercept (starts at the base rate's log-odds).
        let mut w = vec![0.0; d];
        w[N_FEATURES] = (base.clamp(1e-6, 1.0 - 1e-6) / (1.0 - base.clamp(1e-6, 1.0 - 1e-6))).ln();
        for _ in 0..NEWTON_STEPS {
            let mut g = vec![0.0; d];
            let mut h = vec![vec![0.0; d]; d];
            for (z, y) in zs.iter().zip(ys) {
                let lin = w[N_FEATURES] + (0..N_FEATURES).map(|j| w[j] * z[j]).sum::<f64>();
                let p = sigmoid(lin);
                let r = p - if *y { 1.0 } else { 0.0 };
                let s = p * (1.0 - p);
                let v = |j: usize| if j == N_FEATURES { 1.0 } else { z[j] };
                for a in 0..d {
                    let va = v(a);
                    g[a] += r * va;
                    for b in a..d {
                        h[a][b] += s * va * v(b);
                    }
                }
            }
            for a in 0..d {
                g[a] /= n;
                for b in a..d {
                    h[a][b] /= n;
                    h[b][a] = h[a][b];
                }
                if a < N_FEATURES {
                    g[a] += L2 * w[a];
                    h[a][a] += L2;
                } else {
                    h[a][a] += 1e-9;
                }
            }
            let Some(step) = solve(h, g) else { break };
            let mut biggest = 0.0f64;
            for (wa, sa) in w.iter_mut().zip(&step) {
                *wa -= sa;
                biggest = biggest.max(sa.abs());
            }
            if biggest < 1e-10 {
                break;
            }
        }
        model.coef = std::array::from_fn(|j| w[j]);
        model.intercept = w[N_FEATURES];
        model
    }
}

// ---------- Actions ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BotAction {
    Buy,
    #[default]
    Wait,
    Sell,
}

impl BotAction {
    pub fn label(self) -> &'static str {
        match self {
            BotAction::Buy => "ACHETER",
            BotAction::Wait => "ATTENDRE",
            BotAction::Sell => "VENDRE",
        }
    }
}

/// ACHETER when the rise probability reaches its threshold and the fall one does not, VENDRE the other way round,
/// ATTENDRE otherwise (neither, or both).
pub fn action(up: f64, down: f64, threshold_up: f64, threshold_down: f64) -> BotAction {
    match (up >= threshold_up, down >= threshold_down) {
        (true, false) => BotAction::Buy,
        (false, true) => BotAction::Sell,
        _ => BotAction::Wait,
    }
}

/// The two models trained on the same rows, and their thresholds (training base rate + margin).
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub up: Model,
    pub down: Model,
}

impl Pair {
    pub fn fit(rows: &[&Row]) -> Option<Pair> {
        let labelled: Vec<(&Features, Forward)> = rows.iter().filter_map(|r| r.fwd.map(|f| (&r.x, f))).collect();
        if labelled.len() < MIN_TRAIN_ROWS {
            return None;
        }
        let xs: Vec<&Features> = labelled.iter().map(|(x, _)| *x).collect();
        let up: Vec<bool> = labelled.iter().map(|(_, f)| f.up).collect();
        let down: Vec<bool> = labelled.iter().map(|(_, f)| f.down).collect();
        let both = |v: &[bool]| v.iter().any(|y| *y) && v.iter().any(|y| !*y);
        (both(&up) && both(&down)).then(|| Pair { up: Model::fit(&xs, &up), down: Model::fit(&xs, &down) })
    }

    pub fn thresholds(&self) -> (f64, f64) {
        (self.up.base_rate + THRESHOLD_MARGIN, self.down.base_rate + THRESHOLD_MARGIN)
    }

    pub fn predict(&self, x: &Features) -> Prediction {
        let (tu, td) = self.thresholds();
        let (up, down) = (self.up.prob(x), self.down.prob(x));
        Prediction {
            up,
            down,
            threshold_up: tu,
            threshold_down: td,
            base_up: self.up.base_rate,
            base_down: self.down.base_rate,
            action: action(up, down, tu, td),
            block: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prediction {
    pub up: f64,
    pub down: f64,
    pub threshold_up: f64,
    pub threshold_down: f64,
    pub base_up: f64,
    pub base_down: f64,
    pub action: BotAction,
    /// Walk-forward block that produced it.
    pub block: usize,
}

// ---------- Walk-forward ----------

/// One retraining: test dates [start, end), trained on the rows whose label ended before `cutoff`.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub start: i64,
    /// Exclusive; `i64::MAX` for the last block.
    pub end: i64,
    pub cutoff: i64,
    pub train_rows: usize,
    pub trained: bool,
}

/// Out-of-sample predictions of a group's rows (same order; None outside the test blocks or in a block that could
/// not be trained). Timeline = the distinct dates of the rows. The first test date leaves `MIN_TRAIN` dates of
/// training plus the label horizon and the purge; each block is trained on the rows whose label ended before the
/// date `PURGE` dates ahead of its start.
pub fn walk_forward(rows: &[Row]) -> (Vec<Block>, Vec<Option<Prediction>>) {
    let mut dates: Vec<i64> = rows.iter().map(|r| r.time).collect();
    dates.sort_unstable();
    dates.dedup();
    let mut preds = vec![None; rows.len()];
    let mut blocks = Vec::new();
    let first = MIN_TRAIN + HORIZON + PURGE;
    let mut s = first;
    while s < dates.len() {
        let (start, cutoff) = (dates[s], dates[s - PURGE]);
        let end = dates.get(s + RETRAIN_EVERY).copied().unwrap_or(i64::MAX);
        let train: Vec<&Row> = rows.iter().filter(|r| r.fwd.is_some_and(|f| f.exit_time < cutoff)).collect();
        let pair = Pair::fit(&train);
        let k = blocks.len();
        if let Some(pair) = &pair {
            for (r, p) in rows.iter().zip(preds.iter_mut()) {
                if r.time >= start && r.time < end {
                    *p = Some(Prediction { block: k, ..pair.predict(&r.x) });
                }
            }
        }
        blocks.push(Block { start, end, cutoff, train_rows: train.len(), trained: pair.is_some() });
        s += RETRAIN_EVERY;
    }
    (blocks, preds)
}

// ---------- Evaluation ----------

/// Raw out-of-sample samples of a group (merged for the overall figures).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Samples {
    pub test_rows: usize,
    /// Net return of every labelled test day (random entry), and whether it was positive.
    pub all_net: Vec<f64>,
    pub all_gross: Vec<f64>,
    pub all_drawdown: Vec<f64>,
    /// Non-overlapping ACHETER signals: net return, and net return − the mean of the same asset and block.
    pub buy_net: Vec<f64>,
    pub buy_base: Vec<f64>,
    pub buy_excess: Vec<f64>,
    /// Non-overlapping VENDRE signals: what holding made next (gross, %), how much lower than a random day of the
    /// same asset and block (baseline − gross, > 0 = a fall avoided), the drawdown that followed and its baseline.
    pub sell_gross: Vec<f64>,
    pub sell_base: Vec<f64>,
    pub sell_avoided: Vec<f64>,
    pub sell_drawdown: Vec<f64>,
    pub sell_base_drawdown: Vec<f64>,
    /// Whether each VENDRE signal was followed by a fall beyond the round-trip cost.
    pub sell_fell: Vec<bool>,
    /// Net return of the ATTENDRE days (every one, overlapping).
    pub wait_net: Vec<f64>,
    /// (predicted, realised) of every labelled test day, both models, with the training base rates.
    pub up_pairs: Vec<(f64, bool, f64)>,
    pub down_pairs: Vec<(f64, bool, f64)>,
    /// Per asset: compounded ACHETER signals and buy-and-hold over its labelled test days (%), when it has some.
    pub per_asset: Vec<(f64, f64)>,
}

impl Samples {
    pub fn merge(&mut self, o: &Samples) {
        self.test_rows += o.test_rows;
        for (a, b) in [
            (&mut self.all_net, &o.all_net),
            (&mut self.all_gross, &o.all_gross),
            (&mut self.all_drawdown, &o.all_drawdown),
            (&mut self.buy_net, &o.buy_net),
            (&mut self.buy_base, &o.buy_base),
            (&mut self.buy_excess, &o.buy_excess),
            (&mut self.sell_gross, &o.sell_gross),
            (&mut self.sell_base, &o.sell_base),
            (&mut self.sell_avoided, &o.sell_avoided),
            (&mut self.sell_drawdown, &o.sell_drawdown),
            (&mut self.sell_base_drawdown, &o.sell_base_drawdown),
            (&mut self.wait_net, &o.wait_net),
        ] {
            a.extend_from_slice(b);
        }
        self.sell_fell.extend_from_slice(&o.sell_fell);
        self.up_pairs.extend_from_slice(&o.up_pairs);
        self.down_pairs.extend_from_slice(&o.down_pairs);
        self.per_asset.extend_from_slice(&o.per_asset);
    }
}

/// Out-of-sample figures of one asset (inside `evaluate`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssetEval {
    pub test_rows: usize,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub buys: Vec<f64>,
    pub buy_excess: Vec<f64>,
    pub sells: usize,
    pub sell_avoided: Vec<f64>,
    pub wait_days: usize,
    pub labelled: usize,
    pub bot_return: Option<f64>,
    pub hold_return: Option<f64>,
}

fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

/// Mean ÷ standard error (None under 2 values or without variation).
pub fn t_stat(v: &[f64]) -> Option<f64> {
    let n = v.len();
    let m = mean(v)?;
    let sd = if n > 1 { (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt() } else { 0.0 };
    (n > 1 && sd > 1e-12).then(|| m / (sd / (n as f64).sqrt()))
}

/// Walks the test rows of each asset in time order. `rows` sorted by (asset, time), `preds` from `walk_forward`.
/// A signal can only follow the previous one of the same side once its holding period is over (non-overlapping);
/// the baseline of a signal is the mean of the labelled test days of the same asset and block.
pub fn evaluate(rows: &[Row], preds: &[Option<Prediction>]) -> (Samples, HashMap<usize, AssetEval>) {
    let mut base: HashMap<(usize, usize), (f64, f64, f64, usize)> = HashMap::new();
    for (r, p) in rows.iter().zip(preds) {
        if let (Some(p), Some(f)) = (p, r.fwd) {
            let e = base.entry((r.asset, p.block)).or_default();
            e.0 += f.net;
            e.1 += f.gross;
            e.2 += f.drawdown;
            e.3 += 1;
        }
    }
    let mut s = Samples::default();
    let mut per: HashMap<usize, AssetEval> = HashMap::new();
    let mut next_buy: HashMap<usize, i64> = HashMap::new();
    let mut next_sell: HashMap<usize, i64> = HashMap::new();
    let mut span: HashMap<usize, (f64, f64)> = HashMap::new();
    for (r, p) in rows.iter().zip(preds) {
        let Some(p) = p else { continue };
        let a = per.entry(r.asset).or_default();
        a.test_rows += 1;
        s.test_rows += 1;
        a.from = Some(a.from.map_or(r.time, |t| t.min(r.time)));
        a.to = Some(a.to.map_or(r.time, |t| t.max(r.time)));
        let Some(f) = r.fwd else { continue };
        a.labelled += 1;
        let sp = span.entry(r.asset).or_insert((f.entry, f.exit));
        sp.1 = f.exit;
        let (bn, bg, bd, bc) = base[&(r.asset, p.block)];
        let (bn, bg, bd) = (bn / bc as f64, bg / bc as f64, bd / bc as f64);
        s.all_net.push(f.net);
        s.all_gross.push(f.gross);
        s.all_drawdown.push(f.drawdown);
        s.up_pairs.push((p.up, f.up, p.base_up));
        s.down_pairs.push((p.down, f.down, p.base_down));
        match p.action {
            BotAction::Buy if r.time >= *next_buy.get(&r.asset).unwrap_or(&i64::MIN) => {
                next_buy.insert(r.asset, f.exit_time);
                s.buy_net.push(f.net);
                s.buy_base.push(bn);
                s.buy_excess.push(f.net - bn);
                a.buys.push(f.net);
                a.buy_excess.push(f.net - bn);
            }
            BotAction::Sell if r.time >= *next_sell.get(&r.asset).unwrap_or(&i64::MIN) => {
                next_sell.insert(r.asset, f.exit_time);
                s.sell_gross.push(f.gross);
                s.sell_base.push(bg);
                s.sell_avoided.push(bg - f.gross);
                s.sell_drawdown.push(f.drawdown);
                s.sell_base_drawdown.push(bd);
                s.sell_fell.push(f.down);
                a.sells += 1;
                a.sell_avoided.push(bg - f.gross);
            }
            BotAction::Wait => {
                s.wait_net.push(f.net);
                a.wait_days += 1;
            }
            _ => {}
        }
    }
    let mut keys: Vec<usize> = per.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let a = per.get_mut(&k).unwrap();
        if let Some((entry, exit)) = span.get(&k) {
            a.hold_return = Some((exit / entry - 1.0) * 100.0);
            a.bot_return = Some((a.buys.iter().map(|x| 1.0 + x / 100.0).product::<f64>() - 1.0) * 100.0);
            s.per_asset.push((a.bot_return.unwrap(), a.hold_return.unwrap()));
        }
    }
    (s, per)
}

// ---------- Report ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    /// Probability range [from, to) in %.
    pub from: f64,
    pub to: f64,
    pub label: String,
    pub rows: usize,
    /// Mean predicted probability and realised share of the label, %.
    pub predicted: Option<f64>,
    pub realised: Option<f64>,
}

const EDGES: [f64; 7] = [0.0, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0];

pub fn calibration(pairs: &[(f64, bool, f64)]) -> Vec<Bucket> {
    EDGES
        .windows(2)
        .enumerate()
        .map(|(k, w)| {
            let last = k == EDGES.len() - 2;
            let inside: Vec<&(f64, bool, f64)> = pairs.iter().filter(|(p, _, _)| *p >= w[0] && (*p < w[1] || last)).collect();
            let n = inside.len();
            let label = match k {
                0 => format!("moins de {} %", fr(w[1] * 100.0, 0, 0)),
                _ if last => format!("{} % et plus", fr(w[0] * 100.0, 0, 0)),
                _ => format!("{} à {} %", fr(w[0] * 100.0, 0, 0), fr(w[1] * 100.0, 0, 0)),
            };
            Bucket {
                from: w[0] * 100.0,
                to: w[1] * 100.0,
                label,
                rows: n,
                predicted: (n > 0).then(|| round_to(inside.iter().map(|x| x.0).sum::<f64>() / n as f64 * 100.0, 1)),
                realised: (n > 0).then(|| round_to(inside.iter().filter(|x| x.1).count() as f64 / n as f64 * 100.0, 1)),
            }
        })
        .collect()
}

/// 1 − Brier(model) ÷ Brier(training base rate), %: > 0 when the probabilities beat always saying the base rate.
pub fn brier_skill(pairs: &[(f64, bool, f64)]) -> Option<f64> {
    let y = |b: bool| if b { 1.0 } else { 0.0 };
    let model: f64 = pairs.iter().map(|(p, o, _)| (p - y(*o)).powi(2)).sum();
    let reference: f64 = pairs.iter().map(|(_, o, b)| (b - y(*o)).powi(2)).sum();
    (!pairs.is_empty() && reference > 0.0).then(|| round_to((1.0 - model / reference) * 100.0, 1))
}

fn verdict_of(values: &[f64]) -> (Verdict, Option<f64>) {
    let t = t_stat(values).map(|t| round_to(t, 2));
    (verdict(&Pooled { trades: values.len(), t_stat: t, expectancy: mean(values), ..Pooled::default() }), t)
}

pub fn buy_verdict_label(v: Verdict) -> &'static str {
    match v {
        Verdict::Insufficient => "Trop peu d'achats pour conclure",
        Verdict::Edge => "Mieux qu'une entrée au hasard (t ≥ 2), à confirmer",
        Verdict::Negative => "Moins bien qu'une entrée au hasard (t ≤ −2)",
        Verdict::Unproven => "Avantage non démontré",
    }
}

pub fn sell_verdict_label(v: Verdict) -> &'static str {
    match v {
        Verdict::Insufficient => "Trop peu de ventes pour conclure",
        Verdict::Edge => "Baisse évitée par rapport à un jour au hasard (t ≥ 2), à confirmer",
        Verdict::Negative => "Ventes à contretemps : la hausse a suivi (t ≤ −2)",
        Verdict::Unproven => "Avantage non démontré",
    }
}

fn r2(x: Option<f64>) -> Option<f64> {
    x.map(|v| round_to(v, 2))
}

fn share(k: usize, n: usize) -> Option<f64> {
    (n > 0).then(|| round_to(k as f64 / n as f64 * 100.0, 1))
}

/// ACHETER signals out of sample. Returns in %, "excess" = the signal's net return − a random day's of the same
/// asset and block (points of %).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BuyStats {
    pub signals: usize,
    pub mean_net: Option<f64>,
    /// Mean of the random-day baselines matched to the signals (same asset and block): excess = mean − baseline.
    pub baseline_net: Option<f64>,
    /// Mean net return of every labelled test day (all assets and blocks together).
    pub all_days_net: Option<f64>,
    pub excess: Option<f64>,
    /// t of the excess (the verdict's), and of the raw net return.
    pub t_stat: Option<f64>,
    pub raw_t_stat: Option<f64>,
    pub hit_rate: Option<f64>,
    pub baseline_hit_rate: Option<f64>,
    /// Per asset: ACHETER signals compounded vs buy-and-hold over the same test days; medians and count beating it.
    pub median_bot_return: Option<f64>,
    pub median_hold_return: Option<f64>,
    pub beat_hold: usize,
    pub assets: usize,
    pub verdict: Option<Verdict>,
    pub verdict_label: String,
}

/// VENDRE signals out of sample: what holding made over the next 20 days after them and after a random day, how
/// much was avoided (baseline − after the signal, points of %), and the worst fall that followed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SellStats {
    pub signals: usize,
    pub mean_after: Option<f64>,
    /// Matched random-day baselines (same asset and block): avoided = baseline − mean after.
    pub baseline_after: Option<f64>,
    pub all_days_after: Option<f64>,
    pub avoided: Option<f64>,
    pub t_stat: Option<f64>,
    /// % of the signals followed by a fall beyond the round-trip cost, and the same over every test day.
    pub fall_rate: Option<f64>,
    pub baseline_fall_rate: Option<f64>,
    /// Mean worst drop from the entry during the 20 days (≤ 0), after a signal and after a matched random day.
    pub mean_drawdown: Option<f64>,
    pub baseline_drawdown: Option<f64>,
    pub verdict: Option<Verdict>,
    pub verdict_label: String,
}

/// ATTENDRE days: share of the labelled test days, and their mean net return vs every day's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WaitStats {
    pub days: usize,
    pub share: Option<f64>,
    pub mean_net: Option<f64>,
    pub baseline_net: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BotStats {
    /// Test days (with a prediction) and those whose 20-day outcome is known.
    pub test_rows: usize,
    pub labelled: usize,
    pub buy: BuyStats,
    pub sell: SellStats,
    pub wait: WaitStats,
    /// Brier skill of each model vs its training base rate (%), and its calibration by probability bucket.
    pub brier_skill_up: Option<f64>,
    pub brier_skill_down: Option<f64>,
    pub calibration_up: Vec<Bucket>,
    pub calibration_down: Vec<Bucket>,
}

pub fn stats(s: &Samples) -> BotStats {
    let n = s.all_net.len();
    let (bv, bt) = verdict_of(&s.buy_excess);
    let (sv, st) = verdict_of(&s.sell_avoided);
    let beat = s.per_asset.iter().filter(|(b, h)| b > h).count();
    let falls = s.down_pairs.iter().filter(|x| x.1).count();
    BotStats {
        test_rows: s.test_rows,
        labelled: n,
        buy: BuyStats {
            signals: s.buy_net.len(),
            mean_net: r2(mean(&s.buy_net)),
            baseline_net: r2(mean(&s.buy_base)),
            all_days_net: r2(mean(&s.all_net)),
            excess: r2(mean(&s.buy_excess)),
            t_stat: bt,
            raw_t_stat: r2(t_stat(&s.buy_net)),
            hit_rate: share(s.buy_net.iter().filter(|x| **x > 0.0).count(), s.buy_net.len()),
            baseline_hit_rate: share(s.all_net.iter().filter(|x| **x > 0.0).count(), n),
            median_bot_return: r2(median(&s.per_asset.iter().map(|x| x.0).collect::<Vec<_>>())),
            median_hold_return: r2(median(&s.per_asset.iter().map(|x| x.1).collect::<Vec<_>>())),
            beat_hold: beat,
            assets: s.per_asset.len(),
            verdict: Some(bv),
            verdict_label: buy_verdict_label(bv).into(),
        },
        sell: SellStats {
            signals: s.sell_gross.len(),
            mean_after: r2(mean(&s.sell_gross)),
            baseline_after: r2(mean(&s.sell_base)),
            all_days_after: r2(mean(&s.all_gross)),
            avoided: r2(mean(&s.sell_avoided)),
            t_stat: st,
            fall_rate: share(s.sell_fell.iter().filter(|x| **x).count(), s.sell_fell.len()),
            baseline_fall_rate: share(falls, n),
            mean_drawdown: r2(mean(&s.sell_drawdown)),
            baseline_drawdown: r2(mean(&s.sell_base_drawdown)),
            verdict: Some(sv),
            verdict_label: sell_verdict_label(sv).into(),
        },
        wait: WaitStats {
            days: s.wait_net.len(),
            share: share(s.wait_net.len(), n),
            mean_net: r2(mean(&s.wait_net)),
            baseline_net: r2(mean(&s.all_net)),
        },
        brier_skill_up: brier_skill(&s.up_pairs),
        brier_skill_down: brier_skill(&s.down_pairs),
        calibration_up: calibration(&s.up_pairs),
        calibration_down: calibration(&s.down_pairs),
    }
}

/// One coefficient of a published model: applied to (value − mean) ÷ sd.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Weight {
    pub id: String,
    pub coef: f64,
    pub mean: f64,
    pub sd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelOut {
    /// Training share of the label and the signal threshold (base rate + 5 points), %.
    pub base_rate: f64,
    pub threshold: f64,
    pub intercept: f64,
    pub weights: Vec<Weight>,
}

impl ModelOut {
    fn of(m: &Model) -> ModelOut {
        ModelOut {
            base_rate: m.base_rate * 100.0,
            threshold: (m.base_rate + THRESHOLD_MARGIN) * 100.0,
            intercept: m.intercept,
            weights: FEATURES.iter().enumerate().map(|(j, d)| Weight { id: d.id.into(), coef: m.coef[j], mean: m.mean[j], sd: m.sd[j] }).collect(),
        }
    }

    /// Back to a model (None when the weights do not match the features).
    pub fn model(&self) -> Option<Model> {
        if self.weights.len() != N_FEATURES || self.weights.iter().zip(&FEATURES).any(|(w, d)| w.id != d.id) {
            return None;
        }
        Some(Model {
            mean: std::array::from_fn(|j| self.weights[j].mean),
            sd: std::array::from_fn(|j| self.weights[j].sd),
            coef: std::array::from_fn(|j| self.weights[j].coef),
            intercept: self.intercept,
            base_rate: self.base_rate / 100.0,
            rows: 0,
        })
    }
}

/// The models used for today's views: trained on every row of the group whose label is known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveModel {
    pub trained_rows: usize,
    pub trained_from: Option<i64>,
    /// Last day whose 20-day outcome was known.
    pub trained_to: Option<i64>,
    pub up: ModelOut,
    pub down: ModelOut,
}

impl LiveModel {
    pub fn pair(&self) -> Option<Pair> {
        Some(Pair { up: self.up.model()?, down: self.down.model()? })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotGroupStat {
    pub id: BotGroup,
    pub label: String,
    pub assets: usize,
    /// First and last test day, walk-forward blocks tested / planned.
    pub test_from: Option<i64>,
    pub test_to: Option<i64>,
    pub blocks: usize,
    pub trained_blocks: usize,
    #[serde(flatten)]
    pub stats: BotStats,
    /// None when the group has too few rows to train.
    pub model: Option<LiveModel>,
    pub text: String,
}

/// Today's view of one asset: action and probabilities (%), None without enough history or model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NowView {
    pub time: Option<i64>,
    pub action: Option<BotAction>,
    pub up: Option<f64>,
    pub down: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotAssetRow {
    pub symbol: String,
    pub name: String,
    pub kind: Kind,
    pub class: AssetClass,
    pub group: BotGroup,
    pub test_rows: usize,
    pub test_from: Option<i64>,
    pub test_to: Option<i64>,
    pub buys: usize,
    /// Mean net return of its ACHETER signals and their mean excess over a random day (%, points).
    pub buy_mean: Option<f64>,
    pub buy_excess: Option<f64>,
    pub sells: usize,
    pub sell_avoided: Option<f64>,
    /// Share of its labelled test days on ATTENDRE (%).
    pub wait_share: Option<f64>,
    /// ACHETER signals compounded, and buy-and-hold over the same test days (%).
    pub bot_return: Option<f64>,
    pub hold_return: Option<f64>,
    pub now: NowView,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureInfo {
    pub id: String,
    pub label: String,
    pub help: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotParameters {
    pub horizon_days: usize,
    pub retrain_every: usize,
    pub purge_days: usize,
    pub min_train_days: usize,
    pub warmup: usize,
    pub l2: f64,
    /// Points of % above the training base rate.
    pub threshold_margin: f64,
    /// Round-trip cost (fees, slippage, spread) by group, %.
    pub cost_stock_pct: f64,
    pub cost_crypto_pct: f64,
    pub min_signals: usize,
    pub t_edge: f64,
    pub nudge: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotReport {
    pub as_of: i64,
    pub basket_fixed_on: String,
    /// Plain-French verdict.
    pub headline: String,
    pub overall: BotStats,
    /// Stocks then cryptos (those with at least one asset).
    pub groups: Vec<BotGroupStat>,
    /// Basket order, never ranked by performance.
    pub assets: Vec<BotAssetRow>,
    pub failures: Vec<Failure>,
    pub features: Vec<FeatureInfo>,
    pub parameters: BotParameters,
    /// How it was trained and tested (French sentences), and its limits.
    pub method: Vec<String>,
    pub limits: Vec<String>,
    pub source: String,
}

impl BotReport {
    pub fn group(&self, g: BotGroup) -> Option<&BotGroupStat> {
        self.groups.iter().find(|x| x.id == g)
    }
}

/// One history of the basket, fetched: its candles and source.
pub struct History<'a> {
    pub asset: &'a BasketAsset,
    pub candles: Vec<Candle>,
    pub source: String,
}

fn signed(x: Option<f64>, d: usize) -> String {
    match x {
        Some(v) => format!(
            "{}{} %",
            if v < 0.0 {
                "−"
            } else if v > 0.0 {
                "+"
            } else {
                ""
            },
            fr(v.abs(), 0, d)
        ),
        None => "—".into(),
    }
}

fn points(x: Option<f64>) -> String {
    match x {
        Some(v) => format!(
            "{}{} point{}",
            if v < 0.0 {
                "−"
            } else if v > 0.0 {
                "+"
            } else {
                ""
            },
            fr(v.abs(), 0, 2),
            if v.abs() >= 2.0 { "s" } else { "" }
        ),
        None => "—".into(),
    }
}

/// "cours ensuite inférieur de 1,2 point à un jour au hasard" (a fall avoided) or "supérieur" (the rise missed).
fn sell_phrase(avoided: Option<f64>) -> String {
    match avoided {
        Some(v) => format!(
            "cours ensuite {} de {} à un jour au hasard",
            if v >= 0.0 { "inférieur" } else { "supérieur" },
            points(Some(v.abs())).trim_start_matches('+')
        ),
        None => "aucune vente".into(),
    }
}

fn t_text(t: Option<f64>) -> String {
    t.map(|t| format!("t = {}{}", if t < 0.0 { "−" } else { "" }, fr(t.abs(), 0, 1))).unwrap_or_else(|| "t non calculable".into())
}

fn group_text(label: &str, s: &BotStats) -> String {
    let b = &s.buy;
    let v = &s.sell;
    format!(
        "{label} : {} achat{} suivi{} de {} en moyenne contre {} pour une entrée au hasard sur le même actif et la même période ({}, {}) ; {} vente{} suivie{} de {} contre {} ({}, {}) ; ATTENDRE {} des jours.",
        b.signals,
        if b.signals > 1 { "s" } else { "" },
        if b.signals > 1 { "s" } else { "" },
        signed(b.mean_net, 2),
        signed(b.baseline_net, 2),
        points(b.excess),
        t_text(b.t_stat),
        v.signals,
        if v.signals > 1 { "s" } else { "" },
        if v.signals > 1 { "s" } else { "" },
        signed(v.mean_after, 2),
        signed(v.baseline_after, 2),
        sell_phrase(v.avoided),
        t_text(v.t_stat),
        s.wait.share.map(|x| format!("{} %", fr(x, 0, 0))).unwrap_or_else(|| "—".into()),
    )
}

/// The verdict in plain French, from the numbers only.
pub fn headline(groups: &[BotGroupStat]) -> String {
    if groups.is_empty() {
        return "Aucun actif du panier n'a pu servir à l'entraînement : rien à conclure.".into();
    }
    let mut edges: Vec<String> = Vec::new();
    let mut negatives: Vec<String> = Vec::new();
    for g in groups {
        let (b, v) = (&g.stats.buy, &g.stats.sell);
        if b.verdict == Some(Verdict::Edge) {
            edges.push(format!("les achats sur les {} ({})", g.id.short(), t_text(b.t_stat)));
        }
        if v.verdict == Some(Verdict::Edge) {
            edges.push(format!("les ventes sur les {} ({})", g.id.short(), t_text(v.t_stat)));
        }
        if b.verdict == Some(Verdict::Negative) {
            negatives.push(format!("ses achats sur les {} ont fait moins bien qu'une entrée au hasard ({})", g.id.short(), t_text(b.t_stat)));
        }
        if v.verdict == Some(Verdict::Negative) {
            negatives.push(format!("ses ventes sur les {} ont été suivies de hausses ({})", g.id.short(), t_text(v.t_stat)));
        }
    }
    let scope = groups.iter().map(|g| format!("les {}", g.id.short())).collect::<Vec<_>>().join(" ni sur ");
    let mut out = if edges.is_empty() {
        format!(
            "Hors échantillon, le bot n'a pas fait mieux qu'une entrée ou une sortie au hasard de façon démontrée, {}sur {scope} : il ne compte pas dans les décisions.",
            if groups.len() > 1 { "ni " } else { "" }
        )
    } else {
        format!("Hors échantillon, avantage observé pour {}, à confirmer : seul ce côté compte, un peu, dans les décisions.", edges.join(" et "))
    };
    if !negatives.is_empty() {
        out.push_str(&format!(" Attention : {}.", negatives.join(" ; ")));
    }
    out
}

/// Today's view from a live model pair at the last row of a history.
fn now_of(pair: Option<&Pair>, rows: &[Row]) -> NowView {
    match (pair, rows.last()) {
        (Some(p), Some(r)) => {
            let pr = p.predict(&r.x);
            NowView { time: Some(r.time), action: Some(pr.action), up: Some(round_to(pr.up * 100.0, 1)), down: Some(round_to(pr.down * 100.0, 1)) }
        }
        _ => NowView::default(),
    }
}

/// Trains and tests the bot on the fetched histories (any order), plus the failures of the fetch.
pub fn run(histories: Vec<History>, mut failures: Vec<Failure>, now: i64, source: &str) -> BotReport {
    let index = |s: &str| BASKET.iter().position(|b| b.symbol == s).unwrap_or(usize::MAX);
    let mut histories = histories;
    histories.sort_by_key(|h| index(h.asset.symbol));
    failures.sort_by_key(|f| index(&f.symbol));
    let mut groups = Vec::new();
    let mut overall = Samples::default();
    let mut assets: Vec<BotAssetRow> = Vec::new();
    for g in GROUPS {
        let members: Vec<(usize, &History)> = histories.iter().enumerate().filter(|(_, h)| BotGroup::of(h.asset.kind) == g).collect();
        let mut rows: Vec<Row> = Vec::new();
        let mut per_asset_rows: HashMap<usize, Vec<Row>> = HashMap::new();
        for (k, h) in &members {
            let r = asset_rows(*k, &h.candles, h.asset.kind);
            if r.is_empty() {
                failures.push(Failure {
                    symbol: h.asset.symbol.into(),
                    name: h.asset.name.into(),
                    kind: h.asset.kind,
                    class: h.asset.class,
                    error: format!("historique journalier trop court ({} bougies, {} nécessaires)", h.candles.len(), WARMUP + 1),
                });
                continue;
            }
            rows.extend(r.iter().cloned());
            per_asset_rows.insert(*k, r);
        }
        if per_asset_rows.is_empty() {
            continue;
        }
        let (blocks, preds) = walk_forward(&rows);
        let (samples, per) = evaluate(&rows, &preds);
        let live_train: Vec<&Row> = rows.iter().filter(|r| r.fwd.is_some()).collect();
        let pair = Pair::fit(&live_train);
        let model = pair.as_ref().map(|p| LiveModel {
            trained_rows: live_train.len(),
            trained_from: live_train.iter().map(|r| r.time).min(),
            trained_to: live_train.iter().map(|r| r.time).max(),
            up: ModelOut::of(&p.up),
            down: ModelOut::of(&p.down),
        });
        for (k, h) in &members {
            let Some(r) = per_asset_rows.get(k) else { continue };
            let e = per.get(k).cloned().unwrap_or_default();
            assets.push(BotAssetRow {
                symbol: h.asset.symbol.into(),
                name: h.asset.name.into(),
                kind: h.asset.kind,
                class: h.asset.class,
                group: g,
                test_rows: e.test_rows,
                test_from: e.from,
                test_to: e.to,
                buys: e.buys.len(),
                buy_mean: r2(mean(&e.buys)),
                buy_excess: r2(mean(&e.buy_excess)),
                sells: e.sells,
                sell_avoided: r2(mean(&e.sell_avoided)),
                wait_share: share(e.wait_days, e.labelled),
                bot_return: r2(e.bot_return),
                hold_return: r2(e.hold_return),
                now: now_of(pair.as_ref(), r),
                source: h.source.clone(),
            });
        }
        let st = stats(&samples);
        overall.merge(&samples);
        let test_times: Vec<i64> = rows.iter().zip(&preds).filter(|(_, p)| p.is_some()).map(|(r, _)| r.time).collect();
        groups.push(BotGroupStat {
            id: g,
            label: g.label().into(),
            assets: per_asset_rows.len(),
            test_from: test_times.iter().min().copied(),
            test_to: test_times.iter().max().copied(),
            blocks: blocks.len(),
            trained_blocks: blocks.iter().filter(|b| b.trained).count(),
            text: group_text(g.label(), &st),
            stats: st,
            model,
        });
    }
    failures.sort_by_key(|f| index(&f.symbol));
    let headline = headline(&groups);
    let cost = |k: Kind| round_to(round_trip_cost(k), 3);
    let method = vec![
        format!(
            "Même panier fixe que la validation ({} actifs fixés le 29 septembre 2026, jamais choisis sur leurs performances), mêmes bougies journalières.",
            BASKET.len()
        ),
        format!(
            "À chaque clôture, {} mesures lues sur les seules bougies connues ce jour-là (variations 5/20/60 jours, RSI, écarts aux moyennes 50 et 200 jours, MACD, ATR, volatilité, volume, recul depuis le plus haut, position dans la fourchette, régime de marché).",
            N_FEATURES
        ),
        format!(
            "Deux régressions logistiques (pénalité L2 {}) estiment la probabilité d'une hausse (gain net des coûts d'un aller-retour, {} % actions, {} % cryptos) et d'une baisse (plus que ces coûts) sur les {HORIZON} bougies suivantes, achat à l'ouverture du lendemain.",
            fr(L2, 0, 2),
            fr(cost(Kind::Stock), 0, 2),
            fr(cost(Kind::Crypto), 0, 2)
        ),
        format!(
            "ACHETER si la probabilité de hausse dépasse de {} points sa fréquence dans les données d'entraînement ; VENDRE (sortir ou rester dehors, jamais de vente à découvert) si c'est celle de baisse ; ATTENDRE sinon. Seuils fixés sur l'entraînement seul.",
            fr(THRESHOLD_MARGIN * 100.0, 0, 0)
        ),
        format!(
            "Entraînement « walk-forward » : au moins {MIN_TRAIN} jours d'apprentissage, nouvel entraînement tous les {RETRAIN_EVERY} jours sur tout le passé, et un écart de {PURGE} jours entre la fin des résultats connus à l'entraînement et le début du test (les résultats à {HORIZON} jours se chevauchent)."
        ),
        "Les mesures sont standardisées avec les moyennes et écarts types de l'entraînement seulement ; calcul déterministe, sans hasard.".into(),
        format!(
            "Test hors échantillon : un signal n'est compté qu'une fois sa période de {HORIZON} jours finie (pas de chevauchement), et comparé à un jour pris au hasard sur le même actif et la même période. Avantage dit « observé » seulement avec au moins {MIN_TRADES} signaux et t ≥ {}.",
            fr(T_EDGE, 0, 0)
        ),
        "Un modèle pour les actions et ETF, un pour les cryptos (bitcoin, ether et altcoins ensemble : leur historique est trop court pour les séparer).".into(),
        format!(
            "Dans une décision, le bot ne compte que du côté (achat ou vente) où il a montré un avantage hors échantillon : une ligne pour ou contre et au plus {} points de confiance ; jamais contre un veto, jamais seul.",
            fr(NUDGE, 0, 0)
        ),
    ];
    let limits = vec![
        "Historiques courts : environ 5 ans pour les actions, 2,7 ans pour les cryptos ; la période de test couvre peu de cycles de marché (surtout pour les cryptos).".into(),
        "Biais du survivant : les actifs du panier sont de grands actifs d'aujourd'hui.".into(),
        "Actifs corrélés (les altcoins suivent le bitcoin, les actions le S&P 500) : les signaux d'un même jour ne sont pas indépendants, le t surestime la solidité d'un résultat.".into(),
        "Le panier a déjà servi à la validation du signal : ce test est hors échantillon pour le bot (entraîné sur le passé seul), mais le choix de ses mesures n'est pas vérifiable hors de ce panier.".into(),
        "Coûts supposés (frais, glissement, écart moyen de la classe), sans dividendes ni impôt.".into(),
        "Pour un actif hors du panier, le modèle de son groupe s'applique sans avoir été testé sur lui.".into(),
        "Le passé ne garantit pas l'avenir : un avantage mesuré ici peut disparaître.".into(),
    ];
    BotReport {
        as_of: now,
        basket_fixed_on: BASKET_FIXED_ON.into(),
        headline,
        overall: stats(&overall),
        groups,
        assets,
        failures,
        features: FEATURES.iter().map(|d| FeatureInfo { id: d.id.into(), label: d.label.into(), help: d.help.into() }).collect(),
        parameters: BotParameters {
            horizon_days: HORIZON,
            retrain_every: RETRAIN_EVERY,
            purge_days: PURGE,
            min_train_days: MIN_TRAIN,
            warmup: WARMUP,
            l2: L2,
            threshold_margin: THRESHOLD_MARGIN * 100.0,
            cost_stock_pct: cost(Kind::Stock),
            cost_crypto_pct: cost(Kind::Crypto),
            min_signals: MIN_TRADES,
            t_edge: T_EDGE,
            nudge: NUDGE,
        },
        method,
        limits,
        source: source.into(),
    }
}

// ---------- One asset, now (decision) ----------

/// A feature's weight in today's probability: coefficient × standardised value (log-odds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contribution {
    pub id: String,
    pub label: String,
    /// Raw value (unit of the feature, see `features` of /api/bot) and its French rendering.
    pub value: f64,
    pub value_text: String,
    pub weight: f64,
    /// "up": pushes the probability up, "down": down.
    pub effect: String,
    pub text: String,
}

/// « Bot Altim » in a decision (`bot` of `/api/decision`), from the cached report only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BotView {
    /// False when no report is cached yet or the group has no model.
    pub available: bool,
    pub group: BotGroup,
    pub group_label: String,
    /// Whether the asset is in the basket the bot was trained and tested on.
    pub in_basket: bool,
    /// "buy" | "wait" | "sell"; None without enough history.
    pub action: Option<BotAction>,
    pub action_label: Option<String>,
    /// Probabilities of a rise and of a fall over 20 days, thresholds and training base rates, %.
    pub up: Option<f64>,
    pub down: Option<f64>,
    pub threshold_up: Option<f64>,
    pub threshold_down: Option<f64>,
    pub base_up: Option<f64>,
    pub base_down: Option<f64>,
    /// Out-of-sample verdicts of the group's two sides.
    pub buy_verdict: Option<Verdict>,
    pub sell_verdict: Option<Verdict>,
    /// The side of today's action has an out-of-sample edge: it counts (a little) in the decision.
    pub counts: bool,
    /// Closed daily candle the view is computed at (ms).
    pub time: Option<i64>,
    /// Top 3 contributions to the probability of the action's model (rise for ACHETER / ATTENDRE, fall for VENDRE).
    pub contributions: Vec<Contribution>,
    pub text: String,
    /// Whether and how it counts.
    pub note: String,
    /// The report's time (ms).
    pub as_of: Option<i64>,
    pub link: String,
}

impl Default for BotView {
    fn default() -> Self {
        BotView {
            available: false,
            group: BotGroup::Stock,
            group_label: BotGroup::Stock.label().into(),
            in_basket: false,
            action: None,
            action_label: None,
            up: None,
            down: None,
            threshold_up: None,
            threshold_down: None,
            base_up: None,
            base_down: None,
            buy_verdict: None,
            sell_verdict: None,
            counts: false,
            time: None,
            contributions: vec![],
            text: NOT_COMPUTED.into(),
            note: NO_EDGE.into(),
            as_of: None,
            link: LINK.into(),
        }
    }
}

fn pct0(x: f64) -> String {
    format!("{} %", fr(x, 0, 0))
}

fn value_text(id: &str, v: f64) -> String {
    match id {
        "rsi14" => fr(v, 0, 0),
        "range20" => format!("{} %", fr(v * 100.0, 0, 0)),
        "volume" => format!("{}{} écart type", if v < 0.0 { "−" } else { "" }, fr(v.abs(), 0, 1)),
        "bull" | "bear" | "range" | "crisis" => if v > 0.5 { "oui" } else { "non" }.into(),
        _ => signed(Some(v), 1),
    }
}

/// Top contributions of `m` at `x` (regime dummies only when on), strongest first.
pub fn contributions(m: &Model, x: &Features, rising: bool) -> Vec<Contribution> {
    let z = m.z(x);
    let mut all: Vec<Contribution> = (0..N_FEATURES)
        .filter(|j| *j < 12 || x[*j] > 0.5)
        .map(|j| {
            let w = m.coef[j] * z[j];
            let d = &FEATURES[j];
            let vt = value_text(d.id, x[j]);
            // Rising model: + pushes towards ACHETER; falling model: + pushes towards VENDRE.
            let towards = match (rising, w >= 0.0) {
                (true, true) => "pousse vers la hausse",
                (true, false) => "pèse contre la hausse",
                (false, true) => "pousse vers la baisse",
                (false, false) => "pèse contre la baisse",
            };
            let label = if j >= 12 {
                format!("Régime : {}", regime_label([Regime::Bull, Regime::Bear, Regime::Range, Regime::Crisis][j - 12]).to_lowercase())
            } else {
                d.label.into()
            };
            Contribution {
                id: d.id.into(),
                text: if j >= 12 { format!("{label} ({towards})") } else { format!("{label} : {vt} ({towards})") },
                label,
                value: round_to(x[j], 4),
                value_text: vt,
                weight: round_to(w, 3),
                effect: if w >= 0.0 { "up" } else { "down" }.into(),
            }
        })
        .filter(|c| c.weight.abs() >= 0.01)
        .collect();
    all.sort_by(|a, b| b.weight.abs().total_cmp(&a.weight.abs()));
    all.truncate(3);
    all
}

/// The bot's view of one asset now: the group's live models at the last closed daily candle of `daily`.
pub fn bot_view(report: Option<&BotReport>, symbol: &str, kind: Kind, daily: &[Candle], now: i64) -> BotView {
    let group = BotGroup::of(kind);
    let base = BotView {
        group,
        group_label: group.label().into(),
        in_basket: BASKET.iter().any(|b| b.kind == kind && b.symbol.eq_ignore_ascii_case(symbol)),
        ..BotView::default()
    };
    let Some(report) = report else { return base };
    let g = report.group(group);
    let Some((g, pair)) = g.and_then(|g| Some((g, g.model.as_ref()?.pair()?))) else {
        return BotView {
            text: format!("Bot du {} : pas de modèle pour les {} (trop peu de données).", crate::js::iso_date(report.as_of), group.short()),
            as_of: Some(report.as_of),
            ..base
        };
    };
    let (bv, sv) = (g.stats.buy.verdict, g.stats.sell.verdict);
    let common = BotView { available: true, buy_verdict: bv, sell_verdict: sv, as_of: Some(report.as_of), ..base };
    let closed: Vec<Candle> = sanitize(daily).into_iter().filter(|c| c.time + DAY_MS <= now).collect();
    let ind = Indicators::new(&closed);
    let Some(x) = closed.len().checked_sub(1).and_then(|i| ind.at(i)) else {
        return BotView {
            text: format!("Historique journalier trop court ({} bougies, {} nécessaires) : pas d'avis du bot.", closed.len(), WARMUP + 1),
            ..common
        };
    };
    let p = pair.predict(&x);
    let counts = match p.action {
        BotAction::Buy => bv == Some(Verdict::Edge),
        BotAction::Sell => sv == Some(Verdict::Edge),
        BotAction::Wait => false,
    };
    let side_edge = match p.action {
        BotAction::Buy => bv == Some(Verdict::Edge),
        BotAction::Sell => sv == Some(Verdict::Edge),
        BotAction::Wait => bv == Some(Verdict::Edge) || sv == Some(Verdict::Edge),
    };
    let text = format!(
        "{} : probabilité de hausse à {HORIZON} jours {} (seuil {}), de baisse {} (seuil {}).",
        p.action.label(),
        pct0(p.up * 100.0),
        pct0(p.threshold_up * 100.0),
        pct0(p.down * 100.0),
        pct0(p.threshold_down * 100.0)
    );
    let note = if counts {
        let t = if p.action == BotAction::Buy { g.stats.buy.t_stat } else { g.stats.sell.t_stat };
        format!(
            "Avantage hors échantillon observé pour ce côté sur les {} ({}) : compte un peu dans la décision (au plus {} points de confiance), jamais contre un veto.",
            group.short(),
            t_text(t),
            fr(NUDGE, 0, 0)
        )
    } else if p.action == BotAction::Wait && side_edge {
        "ATTENDRE n'est pas un signal : le bot ne compte pas dans la décision.".into()
    } else {
        NO_EDGE.into()
    };
    let rising = p.action != BotAction::Sell;
    BotView {
        action: Some(p.action),
        action_label: Some(p.action.label().into()),
        up: Some(round_to(p.up * 100.0, 1)),
        down: Some(round_to(p.down * 100.0, 1)),
        threshold_up: Some(round_to(p.threshold_up * 100.0, 1)),
        threshold_down: Some(round_to(p.threshold_down * 100.0, 1)),
        base_up: Some(round_to(p.base_up * 100.0, 1)),
        base_down: Some(round_to(p.base_down * 100.0, 1)),
        counts,
        time: closed.last().map(|c| c.time),
        contributions: contributions(if rising { &pair.up } else { &pair.down }, &x, rising),
        text,
        note,
        ..common
    }
}

impl BotView {
    /// The pro or con line of a decision, when it counts: (is_pro, text). `held`: the user holds the asset.
    pub fn line(&self, held: bool) -> Option<(bool, String)> {
        if !self.counts {
            return None;
        }
        let (up, down) = (self.up.unwrap_or(0.0), self.down.unwrap_or(0.0));
        match self.action? {
            BotAction::Buy => Some((
                true,
                format!(
                    "Le bot appris est favorable (probabilité de hausse {}, seuil {}), avec un avantage hors échantillon à confirmer",
                    pct0(up),
                    pct0(self.threshold_up.unwrap_or(0.0))
                ),
            )),
            BotAction::Sell if held => Some((
                false,
                format!(
                    "Le bot appris conseille de sortir (probabilité de baisse {}, seuil {}), avec un avantage hors échantillon à confirmer",
                    pct0(down),
                    pct0(self.threshold_down.unwrap_or(0.0))
                ),
            )),
            BotAction::Sell => Some((
                false,
                format!(
                    "Le bot appris est défavorable (probabilité de baisse {}, seuil {}), avec un avantage hors échantillon à confirmer",
                    pct0(down),
                    pct0(self.threshold_down.unwrap_or(0.0))
                ),
            )),
            BotAction::Wait => None,
        }
    }

    /// Confidence change for a buy-side verdict (Buy / BuyZone): ± `NUDGE` when it counts, else 0.
    pub fn nudge(&self) -> f64 {
        match (self.counts, self.action) {
            (true, Some(BotAction::Buy)) => NUDGE,
            (true, Some(BotAction::Sell)) => -NUDGE,
            _ => 0.0,
        }
    }
}

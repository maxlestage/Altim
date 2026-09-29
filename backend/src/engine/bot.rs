//! « Bot Altim » v2 (`/api/bot`, `bot` of `/api/decision`): for each group (stocks and ETFs, cryptos), models trained
//! on long daily histories estimate the probability that the next 20 trading days end above the round-trip cost
//! ("hausse") and below minus that cost ("baisse"). The bot says ACHETER when the first is high enough, VENDRE (leave
//! or stay out: Altim advises spot holders, never a short sale) when the second is, ATTENDRE otherwise. Pure: the
//! route fetches the histories, this module computes.
//!
//! Pre-registered on 2026-09-29, before any v2 result was seen (nothing below was tuned on a test period):
//! - data: up to 20 years of daily stock candles, cryptos since their listing (`crate::bot_history`);
//! - universe: the validation's 34-asset basket (`validation::BASKET`, the headline test set) plus `EXTRA`, a fixed
//!   list of 72 large US stocks and ETFs and 19 cryptos used for training too and reported apart. Today's large
//!   assets only: survivorship bias, said in the limits;
//! - features: v1's 16 plus 8 (market trend, volatility and 60-day change of the S&P 500 via SPY or of bitcoin,
//!   relative strength over 20/60/120 days, 12-1 month momentum, distance to the 52-week high), point-in-time;
//! - candidates, a fixed list: (a) `V1` v1's logistic regression on the 16 features, (b) `Logit` the same on 24,
//!   (c) `Trees` gradient-boosted trees on 24 (`bot_trees`: 100 trees, depth 3, shrinkage 0.1, 32 quantile bins,
//!   leaves of ≥ 200 rows), (d) `Trend` a rule from the literature, not fitted: ACHETER when the close is above its
//!   200-day simple average and the 12-1 month momentum is positive, VENDRE when both are negative (its
//!   probabilities, for the log-loss and the calibration only, are the training frequencies of each rule state);
//! - nested walk-forward: every 252 dates, each candidate is fitted on an inner training window and scored on the last
//!   252 dates of the training window (inner validation, after a purge); the one with the lowest mean log-loss of its
//!   two models (hausse, baisse) is chosen (ties: the earlier one in the list; none scored: `V1`), then refitted on the
//!   whole training window to predict the next block. The headline result is this selection's; each candidate's own
//!   walk-forward result is shown for information only, never used to choose;
//! - training rows: one day in `TRAIN_STRIDE` of each asset (the 20-day labels of consecutive days overlap: little
//!   information lost, 5 times less computation); the tests use every day;
//! - verdict: ≥ 30 signals and a t ≥ 2 computed by date (the excess of the signals of one day averaged first: they
//!   are not independent), the same rule as the validation; also shown by asset;
//! - last 12 months of the test shown apart ("holdout"), read like the rest (no choice uses it).
//!
//! Honesty guarantees (as v1): features at the close of day t read the candles (and the market's) up to t only; the
//! trade is taken at the next open and closed at the open H candles later, costs of the class included; a purge of H
//! dates between the end of a training label and the start of what it is scored on; standardisation, bins and
//! thresholds from the training rows only; deterministic (no randomness; blocks computed in parallel give the same
//! result as one after another).
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::backtest::{FEE_RATE, Regime, regime_at, regime_label};
use super::bot_trees::{self, Forest};
use super::metrics::{SLIPPAGE, SPREAD_CRYPTO, SPREAD_STOCK, with_costs};
use super::signal::{atr, ema, ema_opt, rsi, sanitize};
use super::validation::{AssetClass, BASKET, BASKET_FIXED_ON, BasketAsset, Failure, MIN_TRADES, Pooled, T_EDGE, Verdict, median, verdict};
use crate::js::fr;
use crate::types::{Candle, DAY_MS, Kind};

/// Version of the report (v1 had none).
pub const VERSION: u32 = 2;
/// Forward horizon of the labels (trading days / daily candles).
pub const HORIZON: usize = 20;
/// Models retrained (and the candidate chosen again) every this many dates of the group's timeline.
pub const RETRAIN_EVERY: usize = 252;
/// Dates between the end of the last training label and the start of what it is scored on.
pub const PURGE: usize = HORIZON;
/// Dates of training before the first test block.
pub const MIN_TRAIN: usize = 504;
/// Dates of the inner validation (end of each training window) that choose the candidate.
pub const INNER_VAL: usize = 252;
/// One training row in this many days of each asset.
pub const TRAIN_STRIDE: usize = 5;
/// Candles before the first feature row (12-1 month momentum and 52-week high: 252; the regime's 220).
pub const WARMUP: usize = 260;
/// Candles of market history needed at a row (its 200-day average, 60-day change, 120-day relative strength).
pub const MARKET_WARMUP: usize = 200;
/// Weight of the L2 penalty on the standardised coefficients (mean log-loss + L2 / 2 × Σ w²).
pub const L2: f64 = 0.05;
pub const NEWTON_STEPS: usize = 30;
/// A signal needs its probability at least this far above the training window's base rate.
pub const THRESHOLD_MARGIN: f64 = 0.05;
/// A model is only fitted with at least this many training rows (and both outcomes present).
pub const MIN_TRAIN_ROWS: usize = 250;
/// The last test days shown apart.
pub const HOLDOUT_DAYS: i64 = 365;
/// A decision's confidence moves by at most this much, and only on a side with an out-of-sample edge.
pub const NUDGE: f64 = 3.0;
/// Blocks computed at the same time (same result as one at a time).
pub const MAX_THREADS: usize = 4;
pub const LINK: &str = "/app/bot";
pub const NOT_COMPUTED: &str = "Bot pas encore entraîné : ouvrez l'écran « Bot Altim » pour lancer l'entraînement (quelques minutes).";
pub const NO_EDGE: &str = "Le bot n'a pas démontré d'avantage hors échantillon : il ne compte pas dans la décision.";
/// Date the extra training universe was fixed (never changed after seeing a result).
pub const EXTRA_FIXED_ON: &str = "2026-09-29";

pub const N_FEATURES: usize = 24;
/// v1's features (the first 16).
pub const N_V1: usize = 16;

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
    f("mom12_1", "Momentum 12-1 mois", "Variation de la clôture entre il y a 252 et il y a 21 bougies (le dernier mois exclu), en %."),
    f("high52", "Écart au plus haut d'un an", "Écart de la clôture au plus haut des 252 dernières bougies, en % (≤ 0)."),
    f("rs20", "Force relative 20 jours", "Variation sur 20 jours de l'actif moins celle du marché (S&P 500 ou bitcoin), en points."),
    f("rs60", "Force relative 60 jours", "Variation sur 60 jours de l'actif moins celle du marché, en points."),
    f("rs120", "Force relative 120 jours", "Variation sur 120 jours de l'actif moins celle du marché, en points."),
    f("mkt_trend", "Tendance du marché", "Écart du marché (S&P 500 via SPY, ou bitcoin) à sa moyenne simple 200 jours, en %."),
    f("mkt_vol20", "Volatilité du marché", "Écart type des variations journalières du marché sur 20 jours, en %."),
    f("mkt_ret60", "Variation du marché sur 60 jours", "Variation du marché sur 60 bougies, en %."),
];

/// Feature values of a day (stored as f32: half the memory, far more precise than the data).
pub type Features = [f32; N_FEATURES];

pub const V1_COLS: [usize; N_V1] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
pub const ALL_COLS: [usize; N_FEATURES] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23];

fn round_to(x: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    (x * p).round() / p
}

// ---------- Groups and universe ----------

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

    /// The group's market: the S&P 500 (SPY) or bitcoin.
    pub fn market(self) -> &'static str {
        match self {
            BotGroup::Stock => "SPY",
            BotGroup::Crypto => "BTC",
        }
    }

    fn market_label(self) -> &'static str {
        match self {
            BotGroup::Stock => "S&P 500 (SPY)",
            BotGroup::Crypto => "bitcoin",
        }
    }
}

const fn xs(symbol: &'static str, name: &'static str, group: &'static str) -> BasketAsset {
    BasketAsset { symbol, name, kind: Kind::Stock, class: AssetClass::Stock, group }
}

const fn xc(symbol: &'static str, name: &'static str, group: &'static str) -> BasketAsset {
    BasketAsset { symbol, name, kind: Kind::Crypto, class: AssetClass::Altcoin, group }
}

/// Extra training universe, fixed on `EXTRA_FIXED_ON` (large and liquid today, across sectors; never chosen on a
/// result). Out of the headline test: their out-of-sample figures are shown apart.
pub static EXTRA: [BasketAsset; 91] = [
    xs("ORCL", "Oracle", "Technologie"),
    xs("CSCO", "Cisco", "Technologie"),
    xs("IBM", "IBM", "Technologie"),
    xs("ADBE", "Adobe", "Technologie"),
    xs("CRM", "Salesforce", "Technologie"),
    xs("TXN", "Texas Instruments", "Semi-conducteurs"),
    xs("QCOM", "Qualcomm", "Semi-conducteurs"),
    xs("AMD", "AMD", "Semi-conducteurs"),
    xs("AVGO", "Broadcom", "Semi-conducteurs"),
    xs("MU", "Micron", "Semi-conducteurs"),
    xs("NFLX", "Netflix", "Communication"),
    xs("DIS", "Disney", "Communication"),
    xs("CMCSA", "Comcast", "Communication"),
    xs("VZ", "Verizon", "Communication"),
    xs("T", "AT&T", "Communication"),
    xs("WMT", "Walmart", "Consommation de base"),
    xs("COST", "Costco", "Consommation de base"),
    xs("PEP", "PepsiCo", "Consommation de base"),
    xs("MO", "Altria", "Consommation de base"),
    xs("CL", "Colgate-Palmolive", "Consommation de base"),
    xs("MCD", "McDonald's", "Consommation cyclique"),
    xs("NKE", "Nike", "Consommation cyclique"),
    xs("SBUX", "Starbucks", "Consommation cyclique"),
    xs("LOW", "Lowe's", "Consommation cyclique"),
    xs("TGT", "Target", "Consommation cyclique"),
    xs("WFC", "Wells Fargo", "Finance"),
    xs("C", "Citigroup", "Finance"),
    xs("GS", "Goldman Sachs", "Finance"),
    xs("MS", "Morgan Stanley", "Finance"),
    xs("AXP", "American Express", "Finance"),
    xs("BLK", "BlackRock", "Finance"),
    xs("V", "Visa", "Finance"),
    xs("MA", "Mastercard", "Finance"),
    xs("PFE", "Pfizer", "Santé"),
    xs("MRK", "Merck", "Santé"),
    xs("ABT", "Abbott", "Santé"),
    xs("LLY", "Eli Lilly", "Santé"),
    xs("AMGN", "Amgen", "Santé"),
    xs("BMY", "Bristol-Myers Squibb", "Santé"),
    xs("MDT", "Medtronic", "Santé"),
    xs("TMO", "Thermo Fisher", "Santé"),
    xs("COP", "ConocoPhillips", "Énergie"),
    xs("SLB", "SLB", "Énergie"),
    xs("OXY", "Occidental", "Énergie"),
    xs("GE", "GE Aerospace", "Industrie"),
    xs("HON", "Honeywell", "Industrie"),
    xs("MMM", "3M", "Industrie"),
    xs("UPS", "UPS", "Industrie"),
    xs("DE", "Deere", "Industrie"),
    xs("LMT", "Lockheed Martin", "Industrie"),
    xs("RTX", "RTX", "Industrie"),
    xs("UNP", "Union Pacific", "Industrie"),
    xs("SO", "Southern Company", "Services publics"),
    xs("D", "Dominion Energy", "Services publics"),
    xs("AEP", "American Electric Power", "Services publics"),
    xs("LIN", "Linde", "Matériaux"),
    xs("SHW", "Sherwin-Williams", "Matériaux"),
    xs("AMT", "American Tower", "Immobilier"),
    xs("SPG", "Simon Property", "Immobilier"),
    xs("XLK", "ETF technologie (XLK)", "ETF sectoriel"),
    xs("XLF", "ETF finance (XLF)", "ETF sectoriel"),
    xs("XLE", "ETF énergie (XLE)", "ETF sectoriel"),
    xs("XLV", "ETF santé (XLV)", "ETF sectoriel"),
    xs("XLY", "ETF consommation cyclique (XLY)", "ETF sectoriel"),
    xs("XLP", "ETF consommation de base (XLP)", "ETF sectoriel"),
    xs("XLI", "ETF industrie (XLI)", "ETF sectoriel"),
    xs("XLU", "ETF services publics (XLU)", "ETF sectoriel"),
    xs("XLB", "ETF matériaux (XLB)", "ETF sectoriel"),
    xs("IWM", "iShares Russell 2000", "ETF indiciel"),
    xs("DIA", "SPDR Dow Jones", "ETF indiciel"),
    xs("EFA", "iShares MSCI EAFE", "ETF indiciel"),
    xs("EEM", "iShares MSCI Emerging Markets", "ETF indiciel"),
    xc("XLM", "Stellar", "Paiement"),
    xc("BCH", "Bitcoin Cash", "Paiement"),
    xc("ETC", "Ethereum Classic", "Plateforme"),
    xc("XMR", "Monero", "Confidentialité"),
    xc("ATOM", "Cosmos", "Plateforme"),
    xc("ALGO", "Algorand", "Plateforme"),
    xc("FIL", "Filecoin", "Stockage"),
    xc("AAVE", "Aave", "Finance décentralisée"),
    xc("EOS", "EOS", "Plateforme"),
    xc("XTZ", "Tezos", "Plateforme"),
    xc("HBAR", "Hedera", "Plateforme"),
    xc("SHIB", "Shiba Inu", "Mème"),
    xc("NEAR", "NEAR Protocol", "Plateforme"),
    xc("ZEC", "Zcash", "Confidentialité"),
    xc("DASH", "Dash", "Paiement"),
    xc("NEO", "Neo", "Plateforme"),
    xc("MKR", "Maker", "Finance décentralisée"),
    xc("SAND", "The Sandbox", "Jeu"),
    xc("MANA", "Decentraland", "Jeu"),
];

// ---------- Features (no look-ahead) ----------

fn day_of(t: i64) -> i64 {
    t.div_euclid(DAY_MS)
}

/// Indicator series of one history and of its market, computed once; `at(i)` reads index ≤ i only (every series is
/// recursive forward or a trailing window, the market candle is the last one of the same day or before).
pub struct Indicators<'a> {
    c: &'a [Candle],
    m: &'a [Candle],
    ema50: Vec<Option<f64>>,
    ema200: Vec<Option<f64>>,
    rsi: Vec<Option<f64>>,
    hist: Vec<Option<f64>>,
    atr: Vec<Option<f64>>,
    /// Prefix sums of the closes (simple averages), the asset's and the market's.
    csum: Vec<f64>,
    msum: Vec<f64>,
    /// Index of the market candle of the same day or the last one before, per asset candle.
    mi: Vec<Option<usize>>,
}

fn prefix(c: &[Candle]) -> Vec<f64> {
    let mut out = Vec::with_capacity(c.len() + 1);
    out.push(0.0);
    let mut s = 0.0;
    for x in c {
        s += x.close;
        out.push(s);
    }
    out
}

fn vol20(c: &[Candle], i: usize) -> f64 {
    let logs: Vec<f64> = (i - 19..=i).map(|j| (c[j].close / c[j - 1].close).ln()).collect();
    let m = logs.iter().sum::<f64>() / logs.len() as f64;
    (logs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (logs.len() - 1) as f64).sqrt() * 100.0
}

impl<'a> Indicators<'a> {
    /// `c` and `market` must be sanitized (sorted, valid); the market may be the asset itself (SPY, BTC).
    pub fn new(c: &'a [Candle], market: &'a [Candle]) -> Self {
        let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
        let (e12, e26) = (ema(&closes, 12), ema(&closes, 26));
        let macd: Vec<Option<f64>> = e12.iter().zip(&e26).map(|(a, b)| Some((*a)? - (*b)?)).collect();
        let signal = ema_opt(&macd, 9);
        let hist = macd.iter().zip(&signal).map(|(m, s)| Some((*m)? - (*s)?)).collect();
        let mut mi = Vec::with_capacity(c.len());
        let mut k = 0usize;
        for x in c {
            let d = day_of(x.time);
            while k < market.len() && day_of(market[k].time) <= d {
                k += 1;
            }
            mi.push(k.checked_sub(1));
        }
        Indicators {
            c,
            m: market,
            ema50: ema(&closes, 50),
            ema200: ema(&closes, 200),
            rsi: rsi(&closes, 14),
            hist,
            atr: atr(c, 14),
            csum: prefix(c),
            msum: prefix(market),
            mi,
        }
    }

    fn sma200(&self, i: usize) -> f64 {
        (self.csum[i + 1] - self.csum[i + 1 - 200]) / 200.0
    }

    fn mom12_1(&self, i: usize) -> f64 {
        (self.c[i - 21].close / self.c[i - 252].close - 1.0) * 100.0
    }

    /// The trend rule's state at the close of candle i: 1 (close above its 200-day simple average and positive 12-1
    /// month momentum), −1 (both negative), 0 otherwise or during the warm-up.
    pub fn trend(&self, i: usize) -> i8 {
        if i < WARMUP || i >= self.c.len() {
            return 0;
        }
        let (above, mom) = (self.c[i].close - self.sma200(i), self.mom12_1(i));
        match (above > 0.0, mom > 0.0, above < 0.0, mom < 0.0) {
            (true, true, _, _) => 1,
            (_, _, true, true) => -1,
            _ => 0,
        }
    }

    /// Features at the close of candle i; None during the warm-up (the asset's or its market's) or with a missing
    /// value.
    pub fn at(&self, i: usize) -> Option<Features> {
        let c = self.c;
        if i < WARMUP || i >= c.len() {
            return None;
        }
        let m = self.mi[i].filter(|k| *k >= MARKET_WARMUP)?;
        let mc = self.m;
        let close = c[i].close;
        let ret = |k: usize| (close / c[i - k].close - 1.0) * 100.0;
        let mret = |k: usize| if m >= k { Some((mc[m].close / mc[m - k].close - 1.0) * 100.0) } else { None };
        let prev: Vec<f64> = c[i - 20..i].iter().map(|x| x.volume).collect();
        let vm = prev.iter().sum::<f64>() / 20.0;
        let vsd = (prev.iter().map(|x| (x - vm).powi(2)).sum::<f64>() / 19.0).sqrt();
        let volume = if vm > 0.0 && vsd > 0.0 { ((c[i].volume - vm) / vsd).clamp(-5.0, 5.0) } else { 0.0 };
        let high200 = c[i - 199..=i].iter().fold(0.0f64, |a, x| a.max(x.high));
        let high252 = c[i - 251..=i].iter().fold(0.0f64, |a, x| a.max(x.high));
        let hi20 = c[i - 19..=i].iter().fold(f64::MIN, |a, x| a.max(x.high));
        let lo20 = c[i - 19..=i].iter().fold(f64::MAX, |a, x| a.min(x.low));
        let regime = regime_at(c, i);
        let one = |r: Regime| if regime == r { 1.0 } else { 0.0 };
        let msma = (self.msum[m + 1] - self.msum[m + 1 - 200]) / 200.0;
        let x: [f64; N_FEATURES] = [
            ret(5),
            ret(20),
            ret(60),
            self.rsi[i]?,
            (close / self.ema50[i]? - 1.0) * 100.0,
            (close / self.ema200[i]? - 1.0) * 100.0,
            self.hist[i]? / close * 100.0,
            self.atr[i]? / close * 100.0,
            vol20(c, i),
            volume,
            if high200 > 0.0 { (close / high200 - 1.0) * 100.0 } else { 0.0 },
            if hi20 > lo20 { (close - lo20) / (hi20 - lo20) } else { 0.5 },
            one(Regime::Bull),
            one(Regime::Bear),
            one(Regime::Range),
            one(Regime::Crisis),
            self.mom12_1(i),
            if high252 > 0.0 { (close / high252 - 1.0) * 100.0 } else { 0.0 },
            ret(20) - mret(20)?,
            ret(60) - mret(60)?,
            ret(120) - mret(120)?,
            (mc[m].close / msma - 1.0) * 100.0,
            vol20(mc, m),
            mret(60)?,
        ];
        x.iter().all(|v| v.is_finite()).then(|| std::array::from_fn(|j| x[j] as f32))
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
    /// Trend rule's state (1, 0, −1).
    pub trend: i8,
    /// One of the training rows (one day in `TRAIN_STRIDE` of the asset).
    pub train: bool,
    pub fwd: Option<Forward>,
    /// Holding from the next open to the one after, % (the sell side's day-by-day comparison with holding).
    pub day: Option<f32>,
}

/// Every feature row of one history (`raw` and `market` in any order; sanitized here). `market` empty: the asset is
/// its own market.
pub fn asset_rows(asset: usize, raw: &[Candle], market: &[Candle], kind: Kind) -> Vec<Row> {
    let c = sanitize(raw);
    let m = if market.is_empty() { c.clone() } else { sanitize(market) };
    let ind = Indicators::new(&c, &m);
    let mut out = Vec::new();
    for i in WARMUP..c.len() {
        let Some(x) = ind.at(i) else { continue };
        let day = (i + 2 < c.len() && c[i + 1].open > 0.0).then(|| ((c[i + 2].open / c[i + 1].open - 1.0) * 100.0) as f32);
        out.push(Row {
            asset,
            time: c[i].time,
            x,
            trend: ind.trend(i),
            train: (i - WARMUP).is_multiple_of(TRAIN_STRIDE),
            fwd: forward(&c, i, kind),
            day,
        });
    }
    out
}

// ---------- Logistic regression ----------

/// Standardisation (training statistics) and coefficients of one logistic regression on the feature columns `cols`.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub cols: Vec<usize>,
    pub mean: Vec<f64>,
    pub sd: Vec<f64>,
    pub coef: Vec<f64>,
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
    pub fn z(&self, x: &[f32]) -> Vec<f64> {
        self.cols.iter().enumerate().map(|(k, &j)| (x[j] as f64 - self.mean[k]) / self.sd[k]).collect()
    }

    /// Probability (0-1) of the positive label.
    pub fn prob(&self, x: &[f32]) -> f64 {
        let z = self.z(x);
        sigmoid(self.intercept + self.coef.iter().zip(&z).map(|(c, v)| c * v).sum::<f64>())
    }

    /// Fits on `xs` / `ys` (same length, at least one row) with the columns `cols`: standardised with these rows'
    /// mean and standard deviation (1 when constant), then Newton's method from the base rate on the mean log-loss
    /// + L2 / 2 × Σ coef² (intercept free).
    #[allow(clippy::needless_range_loop)]
    pub fn fit(xs: &[&[f32]], ys: &[bool], cols: &[usize]) -> Model {
        let n = xs.len().max(1) as f64;
        let p = cols.len();
        let mean: Vec<f64> = cols.iter().map(|&j| xs.iter().map(|x| x[j] as f64).sum::<f64>() / n).collect();
        let sd: Vec<f64> = cols
            .iter()
            .enumerate()
            .map(|(k, &j)| {
                let v = (xs.iter().map(|x| (x[j] as f64 - mean[k]).powi(2)).sum::<f64>() / n).sqrt();
                if v > 1e-12 { v } else { 1.0 }
            })
            .collect();
        let base = ys.iter().filter(|y| **y).count() as f64 / n;
        let mut model = Model { cols: cols.to_vec(), mean, sd, coef: vec![0.0; p], intercept: 0.0, base_rate: base, rows: xs.len() };
        let d = p + 1;
        // w[0..p] coefficients, w[p] intercept (starts at the base rate's log-odds).
        let mut w = vec![0.0; d];
        let b = base.clamp(1e-6, 1.0 - 1e-6);
        w[p] = (b / (1.0 - b)).ln();
        let mut v = vec![0.0; d];
        for _ in 0..NEWTON_STEPS {
            let mut g = vec![0.0; d];
            let mut h = vec![vec![0.0; d]; d];
            for (x, y) in xs.iter().zip(ys) {
                // Standardised on the fly (not stored: memory).
                for (k, &j) in model.cols.iter().enumerate() {
                    v[k] = (x[j] as f64 - model.mean[k]) / model.sd[k];
                }
                v[p] = 1.0;
                let lin: f64 = w.iter().zip(&v).map(|(a, b)| a * b).sum();
                let pr = sigmoid(lin);
                let r = pr - if *y { 1.0 } else { 0.0 };
                let s = pr * (1.0 - pr);
                for a in 0..d {
                    let va = v[a];
                    g[a] += r * va;
                    let sa = s * va;
                    let row = &mut h[a];
                    for b in a..d {
                        row[b] += sa * v[b];
                    }
                }
            }
            for a in 0..d {
                g[a] /= n;
                for b in a..d {
                    h[a][b] /= n;
                    h[b][a] = h[a][b];
                }
                if a < p {
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
            if biggest < 1e-8 {
                break;
            }
        }
        model.coef = w[..p].to_vec();
        model.intercept = w[p];
        model
    }
}

// ---------- Candidates ----------

/// The fixed list of candidate models (see the module doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Candidate {
    #[default]
    V1,
    Logit,
    Trees,
    Trend,
}

pub const CANDIDATES: [Candidate; 4] = [Candidate::V1, Candidate::Logit, Candidate::Trees, Candidate::Trend];

impl Candidate {
    pub fn index(self) -> usize {
        CANDIDATES.iter().position(|c| *c == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            Candidate::V1 => "Régression logistique v1",
            Candidate::Logit => "Régression logistique 24 mesures",
            Candidate::Trees => "Arbres boostés",
            Candidate::Trend => "Règle de tendance",
        }
    }

    pub fn description(self) -> String {
        match self {
            Candidate::V1 => format!("Le modèle de la v1 : régression logistique (pénalité L2 {}) sur les {N_V1} mesures d'origine.", fr(L2, 0, 2)),
            Candidate::Logit => format!("Même régression sur les {N_FEATURES} mesures (contexte de marché, force relative, momentum ajoutés)."),
            Candidate::Trees => format!(
                "{} petits arbres de décision (profondeur {} au plus, pas de {}, au moins {} jours par feuille) sur les {N_FEATURES} mesures : capte des effets non linéaires.",
                bot_trees::TREES,
                bot_trees::DEPTH,
                fr(bot_trees::SHRINKAGE, 0, 2),
                bot_trees::MIN_LEAF
            ),
            Candidate::Trend => "Règle publiée, sans apprentissage : ACHETER au-dessus de la moyenne 200 jours avec un momentum 12-1 mois positif, VENDRE sous la moyenne avec un momentum négatif.".into(),
        }
    }
}

/// The trend rule's probabilities: the training frequency of the label in each rule state (−1, 0, 1), smoothed
/// by one success and one failure (never 0 or 1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TrendSide {
    pub probs: [f64; 3],
    pub base_rate: f64,
    pub rows: usize,
}

impl TrendSide {
    pub fn fit(trends: &[i8], ys: &[bool]) -> TrendSide {
        let mut k = [0usize; 3];
        let mut n = [0usize; 3];
        for (t, y) in trends.iter().zip(ys) {
            let s = (*t + 1) as usize;
            n[s] += 1;
            k[s] += *y as usize;
        }
        TrendSide {
            probs: std::array::from_fn(|s| (k[s] as f64 + 1.0) / (n[s] as f64 + 2.0)),
            base_rate: ys.iter().filter(|y| **y).count() as f64 / ys.len().max(1) as f64,
            rows: ys.len(),
        }
    }
}

/// One side's fitted model (hausse or baisse).
#[derive(Debug, Clone, PartialEq)]
pub enum SideModel {
    Logit(Model),
    Trees(Forest),
    Trend(TrendSide),
}

impl SideModel {
    pub fn prob(&self, x: &[f32], trend: i8) -> f64 {
        match self {
            SideModel::Logit(m) => m.prob(x),
            SideModel::Trees(f) => f.prob(x),
            SideModel::Trend(t) => t.probs[(trend.clamp(-1, 1) + 1) as usize],
        }
    }

    pub fn base_rate(&self) -> f64 {
        match self {
            SideModel::Logit(m) => m.base_rate,
            SideModel::Trees(f) => f.base_rate,
            SideModel::Trend(t) => t.base_rate,
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

/// The trend rule's action from its state.
pub fn trend_action(trend: i8) -> BotAction {
    match trend {
        1 => BotAction::Buy,
        -1 => BotAction::Sell,
        _ => BotAction::Wait,
    }
}

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

/// A candidate's two models trained on the same rows; thresholds = training base rate + margin.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub candidate: Candidate,
    pub up: SideModel,
    pub down: SideModel,
}

/// Out-of-sample prediction of one row (compact: millions of them are kept).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prediction {
    pub up: f32,
    pub down: f32,
    pub base_up: f32,
    pub base_down: f32,
    pub action: BotAction,
    /// Walk-forward block that produced it.
    pub block: u32,
}

impl Prediction {
    pub fn threshold_up(&self) -> f64 {
        self.base_up as f64 + THRESHOLD_MARGIN
    }
    pub fn threshold_down(&self) -> f64 {
        self.base_down as f64 + THRESHOLD_MARGIN
    }
}

impl Pair {
    /// Fits a candidate on labelled rows; None under `MIN_TRAIN_ROWS` or with a label that never (or always) happens.
    pub fn fit(candidate: Candidate, rows: &[&Row]) -> Option<Pair> {
        let labelled: Vec<(&Row, Forward)> = rows.iter().filter_map(|r| r.fwd.map(|f| (*r, f))).collect();
        if labelled.len() < MIN_TRAIN_ROWS {
            return None;
        }
        let up: Vec<bool> = labelled.iter().map(|(_, f)| f.up).collect();
        let down: Vec<bool> = labelled.iter().map(|(_, f)| f.down).collect();
        let both = |v: &[bool]| v.iter().any(|y| *y) && v.iter().any(|y| !*y);
        if !(both(&up) && both(&down)) {
            return None;
        }
        let xs: Vec<&[f32]> = labelled.iter().map(|(r, _)| &r.x[..]).collect();
        let side = |ys: &[bool]| match candidate {
            Candidate::V1 => SideModel::Logit(Model::fit(&xs, ys, &V1_COLS)),
            Candidate::Logit => SideModel::Logit(Model::fit(&xs, ys, &ALL_COLS)),
            Candidate::Trees => SideModel::Trees(Forest::fit(&xs, ys, &ALL_COLS)),
            Candidate::Trend => SideModel::Trend(TrendSide::fit(&labelled.iter().map(|(r, _)| r.trend).collect::<Vec<_>>(), ys)),
        };
        Some(Pair { candidate, up: side(&up), down: side(&down) })
    }

    pub fn thresholds(&self) -> (f64, f64) {
        (self.up.base_rate() + THRESHOLD_MARGIN, self.down.base_rate() + THRESHOLD_MARGIN)
    }

    pub fn predict(&self, x: &[f32], trend: i8) -> Prediction {
        let (tu, td) = self.thresholds();
        let (up, down) = (self.up.prob(x, trend), self.down.prob(x, trend));
        Prediction {
            up: up as f32,
            down: down as f32,
            base_up: self.up.base_rate() as f32,
            base_down: self.down.base_rate() as f32,
            action: if self.candidate == Candidate::Trend { trend_action(trend) } else { action(up, down, tu, td) },
            block: 0,
        }
    }

    /// Mean log-loss of the two models on labelled rows (probabilities clipped to [1e-6, 1 − 1e-6]).
    pub fn log_loss(&self, rows: &[&Row]) -> Option<f64> {
        let mut s = 0.0;
        let mut n = 0usize;
        let ll = |p: f64, y: bool| {
            let p = p.clamp(1e-6, 1.0 - 1e-6);
            -(if y { p.ln() } else { (1.0 - p).ln() })
        };
        for r in rows {
            let Some(f) = r.fwd else { continue };
            s += ll(self.up.prob(&r.x, r.trend), f.up) + ll(self.down.prob(&r.x, r.trend), f.down);
            n += 1;
        }
        (n > 0).then(|| s / (2 * n) as f64)
    }
}

// ---------- Nested walk-forward ----------

/// Inner-validation log-loss of one candidate at one retraining (None: could not be fitted or scored).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScore {
    pub id: Candidate,
    pub log_loss: Option<f64>,
}

/// The choice at one retraining: every candidate fitted on the whole training window, the inner scores and the
/// chosen one (None when no candidate could be fitted).
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub pairs: Vec<Option<Pair>>,
    pub scores: Vec<CandidateScore>,
    pub chosen: Option<Candidate>,
    pub train_rows: usize,
}

/// Chooses and fits at the date index `ci` of the timeline `dates`: training = the training rows whose label ended
/// before `dates[ci]` (every labelled one when `ci` is past the end); inner validation = the labelled rows of the
/// `INNER_VAL` dates that end the training window, inner training = those whose label ended `PURGE` dates before
/// the inner validation starts.
pub fn select(rows: &[Row], dates: &[i64], ci: usize) -> Selection {
    let cutoff = dates.get(ci).copied().unwrap_or(i64::MAX);
    let known = |r: &&Row, t: i64| r.fwd.is_some_and(|f| f.exit_time < t);
    let train: Vec<&Row> = rows.iter().filter(|r| r.train && known(r, cutoff)).collect();
    let pairs: Vec<Option<Pair>> = CANDIDATES.iter().map(|c| Pair::fit(*c, &train)).collect();
    let v = ci.min(dates.len()).checked_sub(HORIZON + INNER_VAL);
    let scores: Vec<CandidateScore> = match v.filter(|v| *v >= PURGE) {
        Some(v) => {
            let (val_start, inner_cutoff) = (dates[v], dates[v - PURGE]);
            let val: Vec<&Row> = rows.iter().filter(|r| r.time >= val_start && known(r, cutoff)).collect();
            let inner: Vec<&Row> = rows.iter().filter(|r| r.train && known(r, inner_cutoff)).collect();
            CANDIDATES.iter().map(|c| CandidateScore { id: *c, log_loss: Pair::fit(*c, &inner).and_then(|p| p.log_loss(&val)) }).collect()
        }
        None => CANDIDATES.iter().map(|c| CandidateScore { id: *c, log_loss: None }).collect(),
    };
    let mut best: Option<(Candidate, f64)> = None;
    for s in &scores {
        if let Some(l) = s.log_loss.filter(|_| pairs[s.id.index()].is_some())
            && best.is_none_or(|(_, b)| l < b)
        {
            best = Some((s.id, l));
        }
    }
    let chosen = best.map(|b| b.0).or_else(|| CANDIDATES.iter().copied().find(|c| pairs[c.index()].is_some()));
    Selection { pairs, scores, chosen, train_rows: train.len() }
}

/// One retraining: test dates [start, end), trained on the rows whose label ended before `cutoff`.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub start: i64,
    /// Exclusive; `i64::MAX` for the last block.
    pub end: i64,
    pub cutoff: i64,
    pub train_rows: usize,
    pub trained: bool,
    pub chosen: Option<Candidate>,
    pub scores: Vec<CandidateScore>,
}

/// Every candidate's out-of-sample predictions (`preds[candidate index][row]`) and the blocks with their choice.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WalkForward {
    pub blocks: Vec<Block>,
    pub preds: Vec<Vec<Option<Prediction>>>,
}

impl WalkForward {
    /// The nested result: in each block, the chosen candidate's predictions.
    pub fn nested(&self) -> Vec<Option<Prediction>> {
        let n = self.preds.first().map_or(0, |p| p.len());
        (0..n)
            .map(|i| {
                let p = self.preds.iter().find_map(|v| v[i])?;
                let chosen = self.blocks[p.block as usize].chosen?;
                self.preds[chosen.index()][i]
            })
            .collect()
    }
}

/// The distinct dates of the rows, sorted.
pub fn timeline(rows: &[Row]) -> Vec<i64> {
    let mut dates: Vec<i64> = rows.iter().map(|r| r.time).collect();
    dates.sort_unstable();
    dates.dedup();
    dates
}

/// Nested walk-forward over a group's rows. Timeline = the distinct dates of the rows. The first test date leaves
/// `MIN_TRAIN` dates of training plus the label horizon and the purge; each block is chosen and trained with
/// `select` at the date `PURGE` dates before its start. Blocks run on up to `threads` threads (same result).
pub fn walk_forward(rows: &[Row], threads: usize) -> WalkForward {
    let dates = timeline(rows);
    let first = MIN_TRAIN + HORIZON + PURGE;
    let starts: Vec<usize> = (first..dates.len()).step_by(RETRAIN_EVERY).collect();
    let run_block = |k: usize| {
        let s = starts[k];
        let sel = select(rows, &dates, s - PURGE);
        let (start, end) = (dates[s], dates.get(s + RETRAIN_EVERY).copied().unwrap_or(i64::MAX));
        let mut preds: Vec<Vec<(usize, Prediction)>> = vec![Vec::new(); CANDIDATES.len()];
        for (i, r) in rows.iter().enumerate() {
            if r.time >= start && r.time < end {
                for (c, p) in sel.pairs.iter().enumerate() {
                    if let Some(p) = p {
                        preds[c].push((i, Prediction { block: k as u32, ..p.predict(&r.x, r.trend) }));
                    }
                }
            }
        }
        let block = Block {
            start,
            end,
            cutoff: dates[s - PURGE],
            train_rows: sel.train_rows,
            trained: sel.chosen.is_some(),
            chosen: sel.chosen,
            scores: sel.scores,
        };
        (block, preds)
    };
    let threads = threads.clamp(1, MAX_THREADS);
    let mut results: Vec<Option<(Block, Vec<Vec<(usize, Prediction)>>)>> = vec![None; starts.len()];
    if threads == 1 || starts.len() < 2 {
        for (k, slot) in results.iter_mut().enumerate() {
            *slot = Some(run_block(k));
        }
    } else {
        let next = std::sync::atomic::AtomicUsize::new(0);
        let done = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|sc| {
            for _ in 0..threads {
                sc.spawn(|| {
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if k >= starts.len() {
                            break;
                        }
                        let r = run_block(k);
                        done.lock().unwrap().push((k, r));
                    }
                });
            }
        });
        for (k, r) in done.into_inner().unwrap() {
            results[k] = Some(r);
        }
    }
    let mut wf = WalkForward { blocks: Vec::new(), preds: vec![vec![None; rows.len()]; CANDIDATES.len()] };
    for (block, preds) in results.into_iter().flatten() {
        for (c, list) in preds.into_iter().enumerate() {
            for (i, p) in list {
                wf.preds[c][i] = Some(p);
            }
        }
        wf.blocks.push(block);
    }
    wf
}

// ---------- Evaluation ----------

/// Raw out-of-sample samples of a set of rows (merged for the overall figures).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Samples {
    pub test_rows: usize,
    /// Net return of every labelled test day (random entry), and whether it was positive.
    pub all_net: Vec<f64>,
    pub all_gross: Vec<f64>,
    pub all_drawdown: Vec<f64>,
    /// Non-overlapping ACHETER signals: net return, and net return − the mean of the same asset and block; the day
    /// (ms) and asset of each (clustered t).
    pub buy_net: Vec<f64>,
    pub buy_base: Vec<f64>,
    pub buy_excess: Vec<f64>,
    pub buy_time: Vec<i64>,
    pub buy_asset: Vec<usize>,
    /// Non-overlapping VENDRE signals: what holding made next (gross, %), how much lower than a random day of the
    /// same asset and block (baseline − gross, > 0 = a fall avoided), the drawdown that followed and its baseline.
    pub sell_gross: Vec<f64>,
    pub sell_base: Vec<f64>,
    pub sell_avoided: Vec<f64>,
    pub sell_drawdown: Vec<f64>,
    pub sell_base_drawdown: Vec<f64>,
    pub sell_time: Vec<i64>,
    pub sell_asset: Vec<usize>,
    /// Whether each VENDRE signal was followed by a fall beyond the round-trip cost.
    pub sell_fell: Vec<bool>,
    /// Net return of the ATTENDRE days (every one, overlapping).
    pub wait_net: Vec<f64>,
    /// (predicted, realised) of every labelled test day, both models, with the training base rates.
    pub up_pairs: Vec<(f64, bool, f64)>,
    pub down_pairs: Vec<(f64, bool, f64)>,
    /// Per asset: compounded ACHETER signals and buy-and-hold over its labelled test days (%), when it has some.
    pub per_asset: Vec<(f64, f64)>,
    /// Per asset: holding vs holding but out for 20 days after each VENDRE signal (day by day from the next open).
    pub exits: Vec<Exit>,
}

/// Holding an asset over its test days, and the same while leaving at each VENDRE signal (sold at the next open,
/// bought back 20 days later, round-trip cost paid).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Exit {
    pub days: usize,
    pub out_days: usize,
    /// Max drawdown (%, ≤ 0) and return (%) of each.
    pub hold_max_drawdown: f64,
    pub bot_max_drawdown: f64,
    pub hold_return: f64,
    pub bot_return: f64,
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
        // Assets of another set: offset so that they never collide in the clusters by asset.
        let off = self.buy_asset.iter().chain(&self.sell_asset).max().map_or(0, |m| m + 1);
        self.buy_asset.extend(o.buy_asset.iter().map(|a| a + off));
        self.sell_asset.extend(o.sell_asset.iter().map(|a| a + off));
        self.buy_time.extend_from_slice(&o.buy_time);
        self.sell_time.extend_from_slice(&o.sell_time);
        self.sell_fell.extend_from_slice(&o.sell_fell);
        self.up_pairs.extend_from_slice(&o.up_pairs);
        self.down_pairs.extend_from_slice(&o.down_pairs);
        self.per_asset.extend_from_slice(&o.per_asset);
        self.exits.extend_from_slice(&o.exits);
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
    pub exit: Option<Exit>,
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

/// Means of `values` grouped by `keys` (first-seen order of the sorted keys).
pub fn cluster_means<K: Ord + Copy>(values: &[f64], keys: &[K]) -> Vec<f64> {
    let mut m: std::collections::BTreeMap<K, (f64, usize)> = std::collections::BTreeMap::new();
    for (v, k) in values.iter().zip(keys) {
        let e = m.entry(*k).or_default();
        e.0 += v;
        e.1 += 1;
    }
    m.values().map(|(s, n)| s / *n as f64).collect()
}

/// Max drawdown (%, ≤ 0) of a sequence of equity values starting at 1.
fn max_drawdown(equity: &[f64]) -> f64 {
    let mut peak = 1.0f64;
    let mut worst = 0.0f64;
    for e in equity {
        peak = peak.max(*e);
        worst = worst.min((e / peak - 1.0) * 100.0);
    }
    worst
}

/// Walks the test rows of each asset in time order (`rows` sorted by (asset, time), `preds` one per row), only the
/// rows `keep` accepts. A signal can only follow the previous one of the same side once its holding period is over
/// (non-overlapping); the baseline of a signal is the mean of the kept labelled test days of the same asset and block.
pub fn evaluate(
    rows: &[Row],
    preds: &[Option<Prediction>],
    keep: impl Fn(&Row) -> bool,
    cost: impl Fn(usize) -> f64,
) -> (Samples, HashMap<usize, AssetEval>) {
    let mut base: HashMap<(usize, u32), (f64, f64, f64, usize)> = HashMap::new();
    for (r, p) in rows.iter().zip(preds) {
        if let (Some(p), Some(f), true) = (p, r.fwd, keep(r)) {
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
    // Per asset: (hold equity, bot equity, days out left, hold curve, bot curve, days, out days).
    let mut eq: HashMap<usize, (f64, f64, usize, Vec<f64>, Vec<f64>, usize, usize)> = HashMap::new();
    for (r, p) in rows.iter().zip(preds) {
        let Some(p) = p else { continue };
        if !keep(r) {
            continue;
        }
        let a = per.entry(r.asset).or_default();
        a.test_rows += 1;
        s.test_rows += 1;
        a.from = Some(a.from.map_or(r.time, |t| t.min(r.time)));
        a.to = Some(a.to.map_or(r.time, |t| t.max(r.time)));
        let e = eq.entry(r.asset).or_insert((1.0, 1.0, 0, Vec::new(), Vec::new(), 0, 0));
        let mut sold = false;
        if let Some(f) = r.fwd {
            a.labelled += 1;
            let sp = span.entry(r.asset).or_insert((f.entry, f.exit));
            sp.1 = f.exit;
            let (bn, bg, bd, bc) = base[&(r.asset, p.block)];
            let (bn, bg, bd) = (bn / bc as f64, bg / bc as f64, bd / bc as f64);
            s.all_net.push(f.net);
            s.all_gross.push(f.gross);
            s.all_drawdown.push(f.drawdown);
            s.up_pairs.push((p.up as f64, f.up, p.base_up as f64));
            s.down_pairs.push((p.down as f64, f.down, p.base_down as f64));
            match p.action {
                BotAction::Buy if r.time >= *next_buy.get(&r.asset).unwrap_or(&i64::MIN) => {
                    next_buy.insert(r.asset, f.exit_time);
                    s.buy_net.push(f.net);
                    s.buy_base.push(bn);
                    s.buy_excess.push(f.net - bn);
                    s.buy_time.push(r.time);
                    s.buy_asset.push(r.asset);
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
                    s.sell_time.push(r.time);
                    s.sell_asset.push(r.asset);
                    a.sells += 1;
                    a.sell_avoided.push(bg - f.gross);
                    sold = true;
                }
                BotAction::Wait => {
                    s.wait_net.push(f.net);
                    a.wait_days += 1;
                }
                _ => {}
            }
        }
        // Day by day from the next open: out for the next HORIZON days after a VENDRE signal (cost paid once).
        if let Some(d) = r.day {
            let d = d as f64 / 100.0;
            if sold && e.2 == 0 {
                e.2 = HORIZON;
                e.1 *= 1.0 - cost(r.asset) / 100.0;
            }
            e.0 *= 1.0 + d;
            if e.2 > 0 {
                e.2 -= 1;
                e.6 += 1;
            } else {
                e.1 *= 1.0 + d;
            }
            e.5 += 1;
            let (h, b) = (e.0, e.1);
            e.3.push(h);
            e.4.push(b);
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
        if let Some(e) = eq.get(&k).filter(|e| e.5 > 0) {
            let x = Exit {
                days: e.5,
                out_days: e.6,
                hold_max_drawdown: max_drawdown(&e.3),
                bot_max_drawdown: max_drawdown(&e.4),
                hold_return: (e.0 - 1.0) * 100.0,
                bot_return: (e.1 - 1.0) * 100.0,
            };
            a.exit = Some(x);
            s.exits.push(x);
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

/// t of a side's excesses, three ways: per signal (as v1), by date (the signals of one day averaged first: the
/// verdict's) and by asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Clustered {
    pub by_date: Option<f64>,
    pub dates: usize,
    pub by_asset: Option<f64>,
    pub assets: usize,
    pub per_signal: Option<f64>,
}

pub fn clustered(values: &[f64], times: &[i64], assets: &[usize]) -> Clustered {
    let d = cluster_means(values, &times.iter().map(|t| day_of(*t)).collect::<Vec<_>>());
    let a = cluster_means(values, assets);
    Clustered { by_date: r2(t_stat(&d)), dates: d.len(), by_asset: r2(t_stat(&a)), assets: a.len(), per_signal: r2(t_stat(values)) }
}

/// Verdict from the excesses with the t by date.
fn verdict_of(values: &[f64], c: &Clustered) -> Verdict {
    verdict(&Pooled { trades: values.len(), t_stat: c.by_date, expectancy: mean(values), ..Pooled::default() })
}

pub fn buy_verdict_label(v: Verdict) -> &'static str {
    match v {
        Verdict::Insufficient => "Trop peu d'achats pour conclure",
        Verdict::Edge => "Mieux qu'une entrée au hasard (t ≥ 2 par jour), à confirmer",
        Verdict::Negative => "Moins bien qu'une entrée au hasard (t ≤ −2 par jour)",
        Verdict::Unproven => "Avantage non démontré",
    }
}

pub fn sell_verdict_label(v: Verdict) -> &'static str {
    match v {
        Verdict::Insufficient => "Trop peu de ventes pour conclure",
        Verdict::Edge => "Baisse évitée par rapport à un jour au hasard (t ≥ 2 par jour), à confirmer",
        Verdict::Negative => "Ventes à contretemps : la hausse a suivi (t ≤ −2 par jour)",
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
    /// t of the excess, the verdict's: by date since v2 (`clustered.bySignal` is v1's); and of the raw net return.
    pub t_stat: Option<f64>,
    pub raw_t_stat: Option<f64>,
    pub clustered: Clustered,
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

/// Holding vs leaving for 20 days at each VENDRE signal, per asset over its test days (medians).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExitStats {
    pub assets: usize,
    /// Share of the test days spent out of the market, %.
    pub out_share: Option<f64>,
    /// Median max drawdowns (%, ≤ 0) and their median difference per asset (bot − hold, points: > 0 = shallower).
    pub median_hold_max_drawdown: Option<f64>,
    pub median_bot_max_drawdown: Option<f64>,
    pub median_drawdown_avoided: Option<f64>,
    pub median_hold_return: Option<f64>,
    pub median_bot_return: Option<f64>,
    /// Assets whose return with the exits beat holding.
    pub beat_hold: usize,
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
    /// t by date (the verdict's, see `BuyStats`).
    pub t_stat: Option<f64>,
    pub clustered: Clustered,
    /// % of the signals followed by a fall beyond the round-trip cost, and the same over every test day.
    pub fall_rate: Option<f64>,
    pub baseline_fall_rate: Option<f64>,
    /// Mean worst drop from the entry during the 20 days (≤ 0), after a signal and after a matched random day.
    pub mean_drawdown: Option<f64>,
    pub baseline_drawdown: Option<f64>,
    pub exit: ExitStats,
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

fn exit_stats(e: &[Exit]) -> ExitStats {
    let col = |f: fn(&Exit) -> f64| r2(median(&e.iter().map(f).collect::<Vec<_>>()));
    ExitStats {
        assets: e.len(),
        out_share: share(e.iter().map(|x| x.out_days).sum(), e.iter().map(|x| x.days).sum()),
        median_hold_max_drawdown: col(|x| x.hold_max_drawdown),
        median_bot_max_drawdown: col(|x| x.bot_max_drawdown),
        median_drawdown_avoided: col(|x| x.bot_max_drawdown - x.hold_max_drawdown),
        median_hold_return: col(|x| x.hold_return),
        median_bot_return: col(|x| x.bot_return),
        beat_hold: e.iter().filter(|x| x.bot_return > x.hold_return).count(),
    }
}

pub fn stats(s: &Samples) -> BotStats {
    let n = s.all_net.len();
    let bc = clustered(&s.buy_excess, &s.buy_time, &s.buy_asset);
    let sc = clustered(&s.sell_avoided, &s.sell_time, &s.sell_asset);
    let (bv, sv) = (verdict_of(&s.buy_excess, &bc), verdict_of(&s.sell_avoided, &sc));
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
            t_stat: bc.by_date,
            raw_t_stat: r2(t_stat(&s.buy_net)),
            clustered: bc,
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
            t_stat: sc.by_date,
            clustered: sc,
            fall_rate: share(s.sell_fell.iter().filter(|x| **x).count(), s.sell_fell.len()),
            baseline_fall_rate: share(falls, n),
            mean_drawdown: r2(mean(&s.sell_drawdown)),
            baseline_drawdown: r2(mean(&s.sell_base_drawdown)),
            exit: exit_stats(&s.exits),
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

/// One coefficient of a published logistic model: applied to (value − mean) ÷ sd.
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
    pub fn of(m: &Model) -> ModelOut {
        ModelOut {
            base_rate: m.base_rate * 100.0,
            threshold: (m.base_rate + THRESHOLD_MARGIN) * 100.0,
            intercept: m.intercept,
            weights: m
                .cols
                .iter()
                .enumerate()
                .map(|(k, &j)| Weight { id: FEATURES[j].id.into(), coef: m.coef[k], mean: m.mean[k], sd: m.sd[k] })
                .collect(),
        }
    }

    /// Back to a model (None with an unknown feature id).
    pub fn model(&self) -> Option<Model> {
        let cols: Vec<usize> = self.weights.iter().map(|w| FEATURES.iter().position(|d| d.id == w.id)).collect::<Option<_>>()?;
        if cols.is_empty() {
            return None;
        }
        Some(Model {
            cols,
            mean: self.weights.iter().map(|w| w.mean).collect(),
            sd: self.weights.iter().map(|w| w.sd).collect(),
            coef: self.weights.iter().map(|w| w.coef).collect(),
            intercept: self.intercept,
            base_rate: self.base_rate / 100.0,
            rows: 0,
        })
    }
}

/// A live side model in JSON: `logit` (weights), `trees` (compact nodes) or `trend` (a probability per rule state).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveSide {
    /// Training share of the label and the signal threshold, %.
    pub base_rate: f64,
    pub threshold: f64,
    pub logit: Option<ModelOut>,
    pub trees: Option<Forest>,
    pub trend: Option<TrendSide>,
}

impl LiveSide {
    pub fn of(m: &SideModel) -> LiveSide {
        let b = m.base_rate();
        let mut out = LiveSide { base_rate: b * 100.0, threshold: (b + THRESHOLD_MARGIN) * 100.0, ..LiveSide::default() };
        match m {
            SideModel::Logit(x) => out.logit = Some(ModelOut::of(x)),
            SideModel::Trees(x) => out.trees = Some(x.clone()),
            SideModel::Trend(x) => out.trend = Some(*x),
        }
        out
    }

    pub fn model(&self) -> Option<SideModel> {
        if let Some(l) = &self.logit {
            return l.model().map(SideModel::Logit);
        }
        if let Some(t) = &self.trees {
            return (t.cols.iter().all(|c| *c < N_FEATURES)
                && t.trees.iter().all(|tr| !tr.is_empty() && tr.iter().all(|n| (n.feature as usize) < N_FEATURES)))
            .then(|| SideModel::Trees(t.clone()));
        }
        self.trend.map(SideModel::Trend)
    }
}

/// The models used for today's views: the candidate chosen the same way as in the walk-forward (inner validation
/// = the last year of known outcomes), refitted on every training row of the group whose label is known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveModel {
    pub trained_rows: usize,
    pub trained_from: Option<i64>,
    /// Last day whose 20-day outcome was known.
    pub trained_to: Option<i64>,
    /// v1 shape, filled when the chosen candidate is a logistic regression (else empty weights).
    pub up: ModelOut,
    pub down: ModelOut,
    /// Since v2: the chosen candidate, its inner-validation scores and both side models whatever their kind.
    pub candidate: Candidate,
    pub scores: Vec<CandidateScore>,
    pub up_model: LiveSide,
    pub down_model: LiveSide,
}

impl LiveModel {
    pub fn pair(&self) -> Option<Pair> {
        match (self.up_model.model(), self.down_model.model()) {
            (Some(up), Some(down)) => Some(Pair { candidate: self.candidate, up, down }),
            // A v1 report: its logistic weights.
            _ => Some(Pair { candidate: Candidate::V1, up: SideModel::Logit(self.up.model()?), down: SideModel::Logit(self.down.model()?) }),
        }
    }
}

/// One candidate's own walk-forward result, for information only (never used to choose).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateStat {
    pub id: Candidate,
    pub label: String,
    pub description: String,
    /// Retrainings where it was fitted, and where the nested selection chose it.
    pub trained_blocks: usize,
    pub chosen_blocks: usize,
    #[serde(flatten)]
    pub stats: BotStats,
}

/// One retraining of the nested walk-forward: its test period, training rows, inner scores and choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockOut {
    pub start: i64,
    /// Exclusive; None for the last block (until today).
    pub end: Option<i64>,
    pub train_rows: usize,
    pub chosen: Option<Candidate>,
    pub scores: Vec<CandidateScore>,
}

/// The data behind a group: assets and years of daily history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Universe {
    /// Assets used: of the basket (headline test) and extra (training, reported apart).
    pub basket: usize,
    pub extra: usize,
    /// Extra assets that could not be used.
    pub extra_failed: usize,
    /// Feature rows (days × assets) and first candle of the group.
    pub rows: usize,
    pub data_from: Option<i64>,
    /// Median and longest history per asset, years.
    pub median_years: Option<f64>,
    pub max_years: Option<f64>,
}

/// The last `HOLDOUT_DAYS` of the test, apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Holdout {
    pub from: Option<i64>,
    pub to: Option<i64>,
    #[serde(flatten)]
    pub stats: BotStats,
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
    /// The nested selection's result on the basket (the headline).
    #[serde(flatten)]
    pub stats: BotStats,
    /// None when the group has too few rows to train.
    pub model: Option<LiveModel>,
    pub text: String,
    /// Since v2 (additive).
    #[serde(default)]
    pub universe: Universe,
    #[serde(default)]
    pub data_years: Option<f64>,
    #[serde(default)]
    pub selection: Vec<BlockOut>,
    #[serde(default)]
    pub candidates: Vec<CandidateStat>,
    #[serde(default)]
    pub holdout: Option<Holdout>,
    /// The nested result on the extra training assets (out of sample too, not the headline).
    #[serde(default)]
    pub extra: Option<BotStats>,
    #[serde(default)]
    pub market: String,
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
    /// Since v2: years of daily history used and its first candle; share of test days out after VENDRE and the max
    /// drawdowns holding / with those exits (%).
    #[serde(default)]
    pub years: Option<f64>,
    #[serde(default)]
    pub data_from: Option<i64>,
    #[serde(default)]
    pub out_share: Option<f64>,
    #[serde(default)]
    pub hold_max_drawdown: Option<f64>,
    #[serde(default)]
    pub bot_max_drawdown: Option<f64>,
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
    /// Since v2.
    #[serde(default)]
    pub inner_validation_days: usize,
    #[serde(default)]
    pub train_stride: usize,
    #[serde(default)]
    pub holdout_days: i64,
    #[serde(default)]
    pub stock_years: i64,
    #[serde(default)]
    pub trees: usize,
    #[serde(default)]
    pub tree_depth: usize,
    #[serde(default)]
    pub shrinkage: f64,
    #[serde(default)]
    pub min_leaf: usize,
    /// How the candidate is chosen at each retraining (French).
    #[serde(default)]
    pub selection: String,
}

/// Time spent (filled by the route; ms).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Timing {
    pub fetch_ms: i64,
    pub compute_ms: i64,
    pub threads: usize,
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
    /// Basket assets not used.
    pub failures: Vec<Failure>,
    pub features: Vec<FeatureInfo>,
    pub parameters: BotParameters,
    /// How it was trained and tested (French sentences), and its limits.
    pub method: Vec<String>,
    pub limits: Vec<String>,
    pub source: String,
    /// Since v2 (additive): 2; what changed since v1; the extra training assets not used; when the extra universe
    /// was fixed; the time spent.
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub changes: Vec<String>,
    #[serde(default)]
    pub extra_failures: Vec<Failure>,
    #[serde(default)]
    pub extra_fixed_on: String,
    #[serde(default)]
    pub timing: Option<Timing>,
}

impl BotReport {
    pub fn group(&self, g: BotGroup) -> Option<&BotGroupStat> {
        self.groups.iter().find(|x| x.id == g)
    }
}

/// One fetched history: its candles and source; `extra` for the extra training universe.
pub struct History<'a> {
    pub asset: &'a BasketAsset,
    pub candles: Vec<Candle>,
    pub source: String,
    pub extra: bool,
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
        "{label} : {} achat{} suivi{} de {} en moyenne contre {} pour une entrée au hasard sur le même actif et la même période ({}, {} par jour) ; {} vente{} suivie{} de {} contre {} ({}, {} par jour) ; ATTENDRE {} des jours.",
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

/// Today's view from a live pair at the last row of a history.
fn now_of(pair: Option<&Pair>, rows: &[Row]) -> NowView {
    match (pair, rows.last()) {
        (Some(p), Some(r)) => {
            let pr = p.predict(&r.x, r.trend);
            NowView {
                time: Some(r.time),
                action: Some(pr.action),
                up: Some(round_to(pr.up as f64 * 100.0, 1)),
                down: Some(round_to(pr.down as f64 * 100.0, 1)),
            }
        }
        _ => NowView::default(),
    }
}

fn failure(a: &BasketAsset, error: String) -> Failure {
    Failure { symbol: a.symbol.into(), name: a.name.into(), kind: a.kind, class: a.class, error }
}

fn years_of(c: &[Candle]) -> Option<f64> {
    Some(round_to((c.last()?.time - c.first()?.time) as f64 / (365.25 * DAY_MS as f64), 1))
}

pub fn changes() -> Vec<String> {
    vec![
        format!(
            "Plus de données : jusqu'à {} ans d'historique journalier pour les actions et depuis leur cotation pour les cryptos (≈ 5 et 2,7 ans en v1).",
            crate::bot_history::STOCK_YEARS
        ),
        format!(
            "Plus d'actifs à l'entraînement : {} actions et ETF et {} cryptos de plus, fixés d'avance ; le test principal reste le panier des {} actifs.",
            EXTRA.iter().filter(|a| a.kind == Kind::Stock).count(),
            EXTRA.iter().filter(|a| a.kind == Kind::Crypto).count(),
            BASKET.len()
        ),
        format!("{} mesures au lieu de {N_V1} : contexte de marché, force relative, momentum 12-1 mois, écart au plus haut d'un an.", N_FEATURES),
        "4 modèles candidats fixés d'avance (dont celui de la v1 et une règle de tendance sans apprentissage), choisis à chaque réentraînement sur une validation interne, jamais sur le test.".into(),
        "Jugement plus strict : t calculé par jour (les signaux d'un même jour comptent ensemble), baisse maximale évitée par les ventes, dernière année présentée à part.".into(),
        "Résultat de la v1 (29/09/2026) : aucun avantage démontré (achats sur les actions −0,15 point, t = −0,1 ; sur les cryptos +0,52 point, t = 0,5 ; ventes sans effet ; probabilités moins précises que la fréquence de base).".into(),
    ]
}

/// Trains and tests the bot on the fetched histories (any order), plus the failures of the fetch (basket and extra
/// assets). `threads`: blocks computed at the same time (same result).
pub fn run(histories: Vec<History>, failures: Vec<Failure>, now: i64, source: &str, threads: usize) -> BotReport {
    let index = |s: &str, k: Kind| {
        BASKET
            .iter()
            .position(|b| b.symbol == s && b.kind == k)
            .or_else(|| EXTRA.iter().position(|b| b.symbol == s && b.kind == k).map(|i| BASKET.len() + i))
            .unwrap_or(usize::MAX)
    };
    let in_basket = |s: &str, k: Kind| BASKET.iter().any(|b| b.symbol == s && b.kind == k);
    let mut histories = histories;
    histories.sort_by_key(|h| index(h.asset.symbol, h.asset.kind));
    let (mut failures, mut extra_failures): (Vec<Failure>, Vec<Failure>) = failures.into_iter().partition(|f| in_basket(&f.symbol, f.kind));
    let mut groups = Vec::new();
    let mut overall = Samples::default();
    let mut assets: Vec<BotAssetRow> = Vec::new();
    for g in GROUPS {
        let members: Vec<(usize, &History)> = histories.iter().enumerate().filter(|(_, h)| BotGroup::of(h.asset.kind) == g).collect();
        let market: Vec<Candle> =
            members.iter().find(|(_, h)| h.asset.symbol == g.market() && !h.extra).map(|(_, h)| h.candles.clone()).unwrap_or_default();
        let mut rows: Vec<Row> = Vec::new();
        let mut per_asset_rows: HashMap<usize, (usize, usize)> = HashMap::new();
        let mut years: Vec<f64> = Vec::new();
        let mut data_from: Option<i64> = None;
        let (mut basket_n, mut extra_n) = (0, 0);
        for (k, h) in &members {
            let r = asset_rows(*k, &h.candles, &market, h.asset.kind);
            if r.is_empty() {
                let f = failure(
                    h.asset,
                    format!(
                        "historique journalier trop court ({} bougies, {} nécessaires, et {} du marché)",
                        h.candles.len(),
                        WARMUP + 1,
                        MARKET_WARMUP + 1
                    ),
                );
                if h.extra {
                    extra_failures.push(f)
                } else {
                    failures.push(f)
                }
                continue;
            }
            if h.extra {
                extra_n += 1
            } else {
                basket_n += 1
            }
            years.extend(years_of(&sanitize(&h.candles)));
            data_from = h.candles.iter().map(|c| c.time).min().into_iter().chain(data_from).min();
            per_asset_rows.insert(*k, (rows.len(), rows.len() + r.len()));
            rows.extend(r);
        }
        if basket_n == 0 {
            continue;
        }
        let is_extra: Vec<bool> = members.iter().fold(vec![false; histories.len()], |mut v, (k, h)| {
            v[*k] = h.extra;
            v
        });
        let cost = |_: usize| round_trip_cost(if g == BotGroup::Crypto { Kind::Crypto } else { Kind::Stock });
        let wf = walk_forward(&rows, threads);
        let nested = wf.nested();
        let basket = |r: &Row| !is_extra[r.asset];
        let (samples, per) = evaluate(&rows, &nested, basket, cost);
        let (extra_samples, _) = evaluate(&rows, &nested, |r: &Row| is_extra[r.asset], cost);
        let last = rows.iter().zip(&nested).filter(|(r, p)| p.is_some() && basket(r)).map(|(r, _)| r.time).max();
        let holdout = last.map(|t| {
            let from = t - HOLDOUT_DAYS * DAY_MS;
            let (s, _) = evaluate(&rows, &nested, |r: &Row| basket(r) && r.time > from, cost);
            Holdout { from: Some(from + DAY_MS), to: Some(t), stats: stats(&s) }
        });
        let candidates: Vec<CandidateStat> = CANDIDATES
            .iter()
            .map(|c| {
                let (s, _) = evaluate(&rows, &wf.preds[c.index()], basket, cost);
                CandidateStat {
                    id: *c,
                    label: c.label().into(),
                    description: c.description(),
                    trained_blocks: wf.preds[c.index()].iter().flatten().map(|p| p.block).collect::<std::collections::BTreeSet<_>>().len(),
                    chosen_blocks: wf.blocks.iter().filter(|b| b.chosen == Some(*c)).count(),
                    stats: stats(&s),
                }
            })
            .collect();
        // Live: chosen and fitted on every known outcome.
        let dates = timeline(&rows);
        let live = select(&rows, &dates, dates.len());
        let live_pair = live.chosen.and_then(|c| live.pairs[c.index()].clone());
        let live_train: Vec<&Row> = rows.iter().filter(|r| r.train && r.fwd.is_some()).collect();
        let model = live_pair.as_ref().map(|p| {
            let logit = |m: &SideModel| if let SideModel::Logit(x) = m { ModelOut::of(x) } else { ModelOut::default() };
            LiveModel {
                trained_rows: live_train.len(),
                trained_from: live_train.iter().map(|r| r.time).min(),
                trained_to: live_train.iter().map(|r| r.time).max(),
                up: logit(&p.up),
                down: logit(&p.down),
                candidate: p.candidate,
                scores: live.scores.clone(),
                up_model: LiveSide::of(&p.up),
                down_model: LiveSide::of(&p.down),
            }
        });
        for (k, h) in &members {
            if h.extra {
                continue;
            }
            let Some((a0, a1)) = per_asset_rows.get(k) else { continue };
            let e = per.get(k).cloned().unwrap_or_default();
            let clean = sanitize(&h.candles);
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
                now: now_of(live_pair.as_ref(), &rows[*a0..*a1]),
                source: h.source.clone(),
                years: years_of(&clean),
                data_from: clean.first().map(|c| c.time),
                out_share: e.exit.and_then(|x| share(x.out_days, x.days)),
                hold_max_drawdown: e.exit.map(|x| round_to(x.hold_max_drawdown, 2)),
                bot_max_drawdown: e.exit.map(|x| round_to(x.bot_max_drawdown, 2)),
            });
        }
        let st = stats(&samples);
        overall.merge(&samples);
        let test_times: Vec<i64> = rows.iter().zip(&nested).filter(|(r, p)| p.is_some() && basket(r)).map(|(r, _)| r.time).collect();
        let extra_failed = extra_failures.iter().filter(|f| BotGroup::of(f.kind) == g).count();
        groups.push(BotGroupStat {
            id: g,
            label: g.label().into(),
            assets: basket_n,
            test_from: test_times.iter().min().copied(),
            test_to: test_times.iter().max().copied(),
            blocks: wf.blocks.len(),
            trained_blocks: wf.blocks.iter().filter(|b| b.trained).count(),
            text: group_text(g.label(), &st),
            stats: st,
            model,
            universe: Universe {
                basket: basket_n,
                extra: extra_n,
                extra_failed,
                rows: rows.len(),
                data_from,
                median_years: r2(median(&years)).map(|y| round_to(y, 1)),
                max_years: years.iter().copied().reduce(f64::max).map(|y| round_to(y, 1)),
            },
            data_years: r2(median(&years)).map(|y| round_to(y, 1)),
            selection: wf
                .blocks
                .iter()
                .map(|b| BlockOut {
                    start: b.start,
                    end: (b.end != i64::MAX).then_some(b.end),
                    train_rows: b.train_rows,
                    chosen: b.chosen,
                    scores: b.scores.iter().map(|s| CandidateScore { id: s.id, log_loss: s.log_loss.map(|l| round_to(l, 5)) }).collect(),
                })
                .collect(),
            candidates,
            holdout,
            extra: (extra_n > 0).then(|| stats(&extra_samples)),
            market: if market.is_empty() { "l'actif lui-même (marché indisponible)".into() } else { g.market_label().into() },
        });
    }
    let index_f = |f: &Failure| index(&f.symbol, f.kind);
    failures.sort_by_key(index_f);
    extra_failures.sort_by_key(index_f);
    let headline = headline(&groups);
    let cost = |k: Kind| round_to(round_trip_cost(k), 3);
    let selection_rule = format!(
        "À chaque réentraînement, chaque candidat est entraîné sans la dernière année de la fenêtre d'entraînement puis noté sur cette année (log-loss moyenne des deux modèles, après un écart de {PURGE} jours) ; le meilleur est réentraîné sur toute la fenêtre."
    );
    let method = vec![
        format!(
            "Test principal sur le panier fixe de la validation ({} actifs fixés le 29 septembre 2026, jamais choisis sur leurs performances). Entraînement aussi sur {} autres actifs liquides fixés le même jour, jugés à part.",
            BASKET.len(),
            EXTRA.len()
        ),
        format!(
            "À chaque clôture, {N_FEATURES} mesures lues sur les seules bougies connues ce jour-là : variations, RSI, moyennes, MACD, ATR, volatilité, volume, reculs, régime, momentum 12-1 mois, force relative et contexte du marché (S&P 500 ou bitcoin)."
        ),
        format!(
            "Deux modèles estiment la probabilité d'une hausse (gain net des coûts d'un aller-retour, {} % actions, {} % cryptos) et d'une baisse (plus que ces coûts) sur les {HORIZON} bougies suivantes, achat à l'ouverture du lendemain.",
            fr(cost(Kind::Stock), 0, 2),
            fr(cost(Kind::Crypto), 0, 2)
        ),
        "4 candidats fixés d'avance : la régression logistique de la v1 (16 mesures), la même sur 24 mesures, des arbres de décision boostés, et une règle de tendance publiée (moyenne 200 jours + momentum 12-1 mois) sans apprentissage.".into(),
        selection_rule.clone(),
        format!(
            "ACHETER si la probabilité de hausse dépasse de {} points sa fréquence à l'entraînement ; VENDRE (sortir ou rester dehors, jamais de vente à découvert) si c'est celle de baisse ; ATTENDRE sinon (la règle de tendance donne directement son action).",
            fr(THRESHOLD_MARGIN * 100.0, 0, 0)
        ),
        format!(
            "Walk-forward : au moins {MIN_TRAIN} jours d'apprentissage, nouvel entraînement tous les {RETRAIN_EVERY} jours sur tout le passé, écart de {PURGE} jours entre la fin des résultats connus et le début du test ; un jour sur {TRAIN_STRIDE} de chaque actif sert à l'entraînement (les résultats à {HORIZON} jours se chevauchent), tous les jours au test."
        ),
        "Mesures standardisées, découpages des arbres et seuils calculés sur l'entraînement seul ; calcul déterministe, sans hasard.".into(),
        format!(
            "Un signal n'est compté qu'une fois sa période de {HORIZON} jours finie et comparé à un jour pris au hasard sur le même actif et la même période. Avantage dit « observé » seulement avec au moins {MIN_TRADES} signaux et t ≥ {} calculé par jour (les signaux d'un même jour comptent pour un).",
            fr(T_EDGE, 0, 0)
        ),
        format!(
            "Chaque candidat est aussi montré seul, à titre d'information : il ne sert pas à choisir. La dernière année ({HOLDOUT_DAYS} jours) est présentée à part."
        ),
        format!(
            "Dans une décision, le bot ne compte que du côté (achat ou vente) où il a montré un avantage hors échantillon : une ligne pour ou contre et au plus {} points de confiance ; jamais contre un veto, jamais seul.",
            fr(NUDGE, 0, 0)
        ),
    ];
    let limits = vec![
        "Biais du survivant : les actifs (panier et univers élargi) sont de grands actifs d'aujourd'hui ; ceux qui ont disparu ou chuté entre-temps manquent, ce qui embellit le passé.".into(),
        "Actifs corrélés (les altcoins suivent le bitcoin, les actions le S&P 500) : même compté par jour, le t reste optimiste si les jours voisins se ressemblent.".into(),
        "Changements de régime : 20 ans couvrent plusieurs cycles pour les actions, beaucoup moins pour les cryptos (bitcoin seul avant 2017) ; un avantage passé peut disparaître.".into(),
        "Le panier a déjà servi à la validation du signal et à la v1 du bot : la liste des candidats a été fixée avant de voir les résultats v2, mais pas en ignorant ceux de la v1.".into(),
        "Le critère de choix (log-loss) favorise les probabilités prudentes : la règle de tendance, dont les probabilités ne sont que des fréquences par état, peut être retenue alors que ses actions ne découlent pas de ces probabilités. Critère fixé avant les résultats, pas changé après.".into(),
        "Cours ajustés des divisions, pas des dividendes ; coûts supposés (frais, glissement, écart moyen de la classe), sans impôt.".into(),
        "Pour un actif hors du panier, le modèle de son groupe s'applique sans avoir été testé sur lui.".into(),
        "Sources : Yahoo Finance (actions, cryptos) et Bitstamp (cryptos) ; en cas d'échec, l'historique habituel d'Altim (≈ 3 à 5 ans) est utilisé pour les actifs du panier.".into(),
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
            inner_validation_days: INNER_VAL,
            train_stride: TRAIN_STRIDE,
            holdout_days: HOLDOUT_DAYS,
            stock_years: crate::bot_history::STOCK_YEARS,
            trees: bot_trees::TREES,
            tree_depth: bot_trees::DEPTH,
            shrinkage: bot_trees::SHRINKAGE,
            min_leaf: bot_trees::MIN_LEAF,
            selection: selection_rule,
        },
        method,
        limits,
        source: source.into(),
        version: VERSION,
        changes: changes(),
        extra_failures,
        extra_fixed_on: EXTRA_FIXED_ON.into(),
        timing: None,
    }
}

// ---------- One asset, now (decision) ----------

/// A feature's weight in today's probability (log-odds; for the trend rule, ±1 for each of its two conditions).
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
    /// Whether the asset is in the basket the bot was tested on.
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
    /// Since v2: the candidate model behind the view.
    pub model: Option<Candidate>,
    pub model_label: Option<String>,
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
            model: None,
            model_label: None,
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
        "rs20" | "rs60" | "rs120" => points(Some(v)),
        _ => signed(Some(v), 1),
    }
}

fn is_dummy(j: usize) -> bool {
    (12..16).contains(&j)
}

fn contribution(j: usize, x: &Features, w: f64, rising: bool) -> Contribution {
    let d = &FEATURES[j];
    let v = x[j] as f64;
    let vt = value_text(d.id, v);
    // Rising model: + pushes towards ACHETER; falling model: + pushes towards VENDRE.
    let towards = match (rising, w >= 0.0) {
        (true, true) => "pousse vers la hausse",
        (true, false) => "pèse contre la hausse",
        (false, true) => "pousse vers la baisse",
        (false, false) => "pèse contre la baisse",
    };
    let label = if is_dummy(j) {
        format!("Régime : {}", regime_label([Regime::Bull, Regime::Bear, Regime::Range, Regime::Crisis][j - 12]).to_lowercase())
    } else {
        d.label.into()
    };
    Contribution {
        id: d.id.into(),
        text: if is_dummy(j) { format!("{label} ({towards})") } else { format!("{label} : {vt} ({towards})") },
        label,
        value: round_to(v, 4),
        value_text: vt,
        weight: round_to(w, 3),
        effect: if w >= 0.0 { "up" } else { "down" }.into(),
    }
}

/// Top contributions of a side model at `x` (regime dummies only when on), strongest first: coefficient ×
/// standardised value (logistic), change of the node values along each tree's path (trees), the rule's two
/// conditions (trend: ±1 each, "ema200" standing for the 200-day average).
pub fn contributions(m: &SideModel, x: &Features, rising: bool) -> Vec<Contribution> {
    let weights: Vec<(usize, f64)> = match m {
        SideModel::Logit(l) => {
            let z = l.z(x);
            l.cols.iter().enumerate().map(|(k, &j)| (j, l.coef[k] * z[k])).collect()
        }
        SideModel::Trees(f) => f.contributions(x, N_FEATURES).into_iter().enumerate().collect(),
        SideModel::Trend(_) => {
            let s = |v: f32| if v > 0.0 { 1.0 } else { -1.0 };
            let sign = if rising { 1.0 } else { -1.0 };
            vec![(5, sign * s(x[5])), (16, sign * s(x[16]))]
        }
    };
    let mut all: Vec<Contribution> = weights
        .into_iter()
        .filter(|(j, _)| !is_dummy(*j) || x[*j] > 0.5)
        .map(|(j, w)| contribution(j, x, w, rising))
        .filter(|c| c.weight.abs() >= 0.01)
        .collect();
    all.sort_by(|a, b| b.weight.abs().total_cmp(&a.weight.abs()));
    all.truncate(3);
    all
}

/// The bot's view of one asset now: the group's live models at the last closed daily candle of `daily`, with the
/// group's market (`market`: SPY or BTC daily candles; the asset itself when it is that market).
pub fn bot_view(report: Option<&BotReport>, symbol: &str, kind: Kind, daily: &[Candle], market: &[Candle], now: i64) -> BotView {
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
    let common = BotView {
        available: true,
        buy_verdict: bv,
        sell_verdict: sv,
        as_of: Some(report.as_of),
        model: Some(pair.candidate),
        model_label: Some(pair.candidate.label().into()),
        ..base
    };
    let closed: Vec<Candle> = sanitize(daily).into_iter().filter(|c| c.time + DAY_MS <= now).collect();
    let own = symbol.eq_ignore_ascii_case(group.market());
    let m: Vec<Candle> = if own { closed.clone() } else { sanitize(market).into_iter().filter(|c| c.time + DAY_MS <= now).collect() };
    let ind = Indicators::new(&closed, &m);
    let last = closed.len().checked_sub(1);
    let Some((i, x)) = last.and_then(|i| Some((i, ind.at(i)?))) else {
        let text = if closed.len() <= WARMUP {
            format!("Historique journalier trop court ({} bougies, {} nécessaires) : pas d'avis du bot.", closed.len(), WARMUP + 1)
        } else {
            format!("Historique du marché ({}) trop court ou indisponible : pas d'avis du bot.", group.market_label())
        };
        return BotView { text, ..common };
    };
    let trend = ind.trend(i);
    let p = pair.predict(&x, trend);
    let (up, down) = (p.up as f64, p.down as f64);
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
        "{} : probabilité de hausse à {HORIZON} jours {} (seuil {}), de baisse {} (seuil {}){}.",
        p.action.label(),
        pct0(up * 100.0),
        pct0(p.threshold_up() * 100.0),
        pct0(down * 100.0),
        pct0(p.threshold_down() * 100.0),
        if pair.candidate == Candidate::Trend { " ; action donnée par la règle de tendance" } else { "" }
    );
    let note = if counts {
        let t = if p.action == BotAction::Buy { g.stats.buy.t_stat } else { g.stats.sell.t_stat };
        format!(
            "Avantage hors échantillon observé pour ce côté sur les {} ({} par jour) : compte un peu dans la décision (au plus {} points de confiance), jamais contre un veto.",
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
        up: Some(round_to(up * 100.0, 1)),
        down: Some(round_to(down * 100.0, 1)),
        threshold_up: Some(round_to(p.threshold_up() * 100.0, 1)),
        threshold_down: Some(round_to(p.threshold_down() * 100.0, 1)),
        base_up: Some(round_to(p.base_up as f64 * 100.0, 1)),
        base_down: Some(round_to(p.base_down as f64 * 100.0, 1)),
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

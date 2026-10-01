//! Cross-asset validation of the signal (`/api/validation`): the backtest of the decision's track record
//! (`metrics::track_run`: same signal, same parameters, same fees, slippage and default spread) run on a basket of
//! assets fixed in advance, then pooled by asset class, by market regime and overall. Pure: the route fetches the
//! daily histories, this module only computes.
//!
//! Anti-snooping: the basket is a constant chosen on size and diversity (never on past performance) and dated; the
//! parameters are the live ones (no optimisation); the regime of each day or trade is read on the closed candle
//! before it (no look-ahead); an edge is only called "observed" with enough trades and a t statistic ≥ 2.
use serde::{Deserialize, Serialize};

use super::backtest::{
    CRISIS_DRAWDOWN, FEE_RATE, LOOKBACK, REGIME_SLOPE, REGIME_SMA, REWARD_RISK, Regime, WARMUP, regime_at, regime_label, trade_regimes, trade_stats,
};
use super::metrics::{SLIPPAGE, SPREAD_CRYPTO, SPREAD_STOCK, track_run};
use super::signal::sanitize;
use crate::js::fr;
use crate::types::{Candle, Kind};

/// Date the basket below was fixed (before any validation run); changing it must change this date.
pub const BASKET_FIXED_ON: &str = "2026-09-29";
/// French flat tax (PFU) assumed on a net positive result, %: an illustration, not the user's situation.
pub const TAX_RATE: f64 = 30.0;
/// Below this many trades, a group (class, regime, overall) is "échantillon trop faible".
pub const MIN_TRADES: usize = 30;
/// An edge is "observed" only when the mean net trade return is at least this many standard errors above 0.
pub const T_EDGE: f64 = 2.0;
/// An asset counts in a regime's comparison with buy-and-hold only with at least this many days in it.
pub const MIN_REGIME_DAYS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssetClass {
    Stock,
    Btc,
    Eth,
    Altcoin,
}

pub const CLASSES: [AssetClass; 4] = [AssetClass::Stock, AssetClass::Btc, AssetClass::Eth, AssetClass::Altcoin];

impl AssetClass {
    /// Same as its JSON value.
    pub fn id(self) -> &'static str {
        match self {
            AssetClass::Stock => "stock",
            AssetClass::Btc => "btc",
            AssetClass::Eth => "eth",
            AssetClass::Altcoin => "altcoin",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AssetClass::Stock => "Actions et ETF américains",
            AssetClass::Btc => "Bitcoin",
            AssetClass::Eth => "Ethereum",
            AssetClass::Altcoin => "Altcoins",
        }
    }
}

pub struct BasketAsset {
    pub symbol: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub class: AssetClass,
    /// Sector (stocks) or category (cryptos), to show the diversity of the basket.
    pub group: &'static str,
}

const fn stock(symbol: &'static str, name: &'static str, group: &'static str) -> BasketAsset {
    BasketAsset { symbol, name, kind: Kind::Stock, class: AssetClass::Stock, group }
}

const fn alt(symbol: &'static str, name: &'static str, group: &'static str) -> BasketAsset {
    BasketAsset { symbol, name, kind: Kind::Crypto, class: AssetClass::Altcoin, group }
}

/// The basket, fixed on 2026-09-29 (`BASKET_FIXED_ON`) before any run: 20 large US stocks, two per sector over ten
/// sectors, by market capitalisation and liquidity; the two main index ETFs; Bitcoin; Ethereum; ten altcoins among
/// the largest of the app's crypto list that have traded for more than three years. Never chosen or changed on past
/// performance (survivorship bias remains: they are today's large assets, stated in the limits).
pub const BASKET: [BasketAsset; 34] = [
    stock("AAPL", "Apple", "Technologie"),
    stock("MSFT", "Microsoft", "Technologie"),
    stock("GOOGL", "Alphabet", "Communication"),
    stock("META", "Meta Platforms", "Communication"),
    stock("AMZN", "Amazon", "Consommation cyclique"),
    stock("HD", "Home Depot", "Consommation cyclique"),
    stock("JPM", "JPMorgan Chase", "Finance"),
    stock("BAC", "Bank of America", "Finance"),
    stock("JNJ", "Johnson & Johnson", "Santé"),
    stock("UNH", "UnitedHealth", "Santé"),
    stock("XOM", "Exxon Mobil", "Énergie"),
    stock("CVX", "Chevron", "Énergie"),
    stock("PG", "Procter & Gamble", "Consommation de base"),
    stock("KO", "Coca-Cola", "Consommation de base"),
    stock("CAT", "Caterpillar", "Industrie"),
    stock("BA", "Boeing", "Industrie"),
    stock("NEE", "NextEra Energy", "Services publics"),
    stock("DUK", "Duke Energy", "Services publics"),
    stock("NVDA", "NVIDIA", "Semi-conducteurs"),
    stock("INTC", "Intel", "Semi-conducteurs"),
    stock("SPY", "SPDR S&P 500 ETF", "ETF indiciel"),
    stock("QQQ", "Invesco QQQ (Nasdaq 100)", "ETF indiciel"),
    BasketAsset { symbol: "BTC", name: "Bitcoin", kind: Kind::Crypto, class: AssetClass::Btc, group: "Bitcoin" },
    BasketAsset { symbol: "ETH", name: "Ethereum", kind: Kind::Crypto, class: AssetClass::Eth, group: "Ethereum" },
    alt("SOL", "Solana", "Plateforme"),
    alt("BNB", "BNB", "Plateforme d'échange"),
    alt("XRP", "XRP", "Paiement"),
    alt("ADA", "Cardano", "Plateforme"),
    alt("DOGE", "Dogecoin", "Mème"),
    alt("AVAX", "Avalanche", "Plateforme"),
    alt("DOT", "Polkadot", "Plateforme"),
    alt("LINK", "Chainlink", "Oracle"),
    alt("LTC", "Litecoin", "Paiement"),
    alt("TRX", "TRON", "Plateforme"),
];

fn round_to(x: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    (x * p).round() / p
}

/// Result (%) after the flat tax, paid on a net positive result only.
pub fn after_tax(result_pct: f64) -> f64 {
    if result_pct > 0.0 { result_pct * (1.0 - TAX_RATE / 100.0) } else { result_pct }
}

/// One closed trade of the backtest: return net of every cost (%), initial risk to the stop (% of the entry) and the
/// market regime on its signal candle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeOutcome {
    pub net_return: f64,
    pub risk: Option<f64>,
    pub regime: Regime,
}

/// Days of one market regime on one asset (regime read on the day before), and what the signal (costs included) and
/// buy-and-hold made over those days, compounded (%).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegimeDays {
    pub regime: Regime,
    pub days: usize,
    pub signal: f64,
    pub hold: f64,
}

/// One asset of the basket, tested. Percentages in %, times in ms.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssetResult {
    pub symbol: String,
    pub name: String,
    pub kind: Option<Kind>,
    pub class: Option<AssetClass>,
    pub group: String,
    /// First and last tested daily candle.
    pub from: i64,
    pub to: i64,
    /// Daily candles tested (after the 60-candle warm-up).
    pub bars: usize,
    pub trades: usize,
    pub win_rate: Option<f64>,
    pub profit_factor: Option<f64>,
    /// Mean net return per trade (%), costs included.
    pub expectancy: Option<f64>,
    pub avg_r: Option<f64>,
    /// Worst fall of the signal's equity (%, ≤ 0), and of buy-and-hold over the same days.
    pub max_drawdown: f64,
    pub hold_max_drawdown: f64,
    pub sharpe: Option<f64>,
    pub sortino: Option<f64>,
    pub total_return: f64,
    pub buy_and_hold: f64,
    /// Share of the tested days with a position (%).
    pub exposure: f64,
    pub beat_hold: bool,
    /// Results after the flat tax assumption (on a net positive result only).
    pub after_tax: f64,
    pub hold_after_tax: f64,
    /// Fewer than 30 trades: "échantillon trop faible".
    pub low_sample: bool,
    pub regime_days: Vec<RegimeDays>,
    /// Source of the daily candles.
    pub source: String,
    #[serde(skip)]
    pub outcomes: Vec<TradeOutcome>,
}

/// An asset of the basket that could not be tested, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub symbol: String,
    pub name: String,
    pub kind: Kind,
    pub class: AssetClass,
    pub error: String,
}

const REGIMES: [Regime; 5] = [Regime::Bull, Regime::Bear, Regime::Range, Regime::Crisis, Regime::Unknown];

/// The asset's backtest, exactly the decision's track record with the default spread of its class, plus the trades'
/// regimes and the regime-by-regime comparison with buy-and-hold. None when the history is too short to test.
pub fn run_asset(a: &BasketAsset, candles_daily: &[Candle], source: &str) -> Option<AssetResult> {
    let (track, run) = track_run(candles_daily, a.kind, None)?;
    let c = sanitize(candles_daily);
    let regimes = trade_regimes(candles_daily, &run.result.trades);
    let outcomes: Vec<TradeOutcome> = run
        .net_returns
        .iter()
        .enumerate()
        .map(|(i, x)| TradeOutcome { net_return: *x, risk: run.risks.get(i).copied().filter(|r| r.is_finite() && *r > 0.0), regime: regimes[i] })
        .collect();
    // Curve point k is the candle WARMUP + k (one point per tested candle). The move of candle i (close i−1 → close
    // i, and the signal's equity over the same candle) is filed under the regime known at the close of i − 1.
    let mut acc = [(0usize, 1.0f64, 1.0f64); 5];
    for k in 1..run.curve.len() {
        let i = WARMUP + k;
        if i >= c.len() || run.curve[k - 1] <= 0.0 || c[i - 1].close <= 0.0 {
            continue;
        }
        let slot = REGIMES.iter().position(|r| *r == regime_at(&c, i - 1)).unwrap_or(4);
        acc[slot].0 += 1;
        acc[slot].1 *= run.curve[k] / run.curve[k - 1];
        acc[slot].2 *= c[i].close / c[i - 1].close;
    }
    let regime_days = REGIMES
        .iter()
        .zip(acc)
        .filter(|(_, (d, _, _))| *d > 0)
        .map(|(r, (days, s, h))| RegimeDays { regime: *r, days, signal: round_to((s - 1.0) * 100.0, 2), hold: round_to((h - 1.0) * 100.0, 2) })
        .collect();
    // Buy-and-hold drawdown over the tested days, from the same starting open as its return.
    let mut peak = c.get(WARMUP).map_or(0.0, |x| x.open);
    let mut hold_dd = 0.0f64;
    for x in c.iter().skip(WARMUP) {
        peak = peak.max(x.close);
        if peak > 0.0 {
            hold_dd = hold_dd.max((peak - x.close) / peak);
        }
    }
    let n = track.trades;
    Some(AssetResult {
        symbol: a.symbol.into(),
        name: a.name.into(),
        kind: Some(a.kind),
        class: Some(a.class),
        group: a.group.into(),
        from: run.result.equity[0].time,
        to: run.result.equity[run.result.equity.len() - 1].time,
        bars: track.details.tested_bars,
        trades: n,
        win_rate: (n > 0).then_some(track.win_rate),
        profit_factor: track.profit_factor,
        expectancy: track.details.expectancy,
        avg_r: track.details.avg_r,
        max_drawdown: track.max_drawdown,
        hold_max_drawdown: round_to(-hold_dd * 100.0, 2),
        sharpe: track.sharpe,
        sortino: track.sortino,
        total_return: track.total_return,
        buy_and_hold: track.buy_and_hold,
        exposure: round_to(run.result.exposure_percent, 1),
        beat_hold: track.total_return > track.buy_and_hold,
        after_tax: round_to(after_tax(track.total_return), 2),
        hold_after_tax: round_to(after_tax(track.buy_and_hold), 2),
        low_sample: n < MIN_TRADES,
        regime_days,
        source: source.into(),
        outcomes,
    })
}

// ---------- Aggregation ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Fewer than 30 trades.
    Insufficient,
    /// Mean net trade return > 0 with t ≥ 2.
    Edge,
    /// Mean net trade return < 0 with t ≤ −2.
    Negative,
    /// Anything in between.
    Unproven,
}

pub fn verdict_label(v: Verdict) -> &'static str {
    match v {
        Verdict::Insufficient => "Échantillon trop faible",
        Verdict::Edge => "Gain moyen positif (t ≥ 2), à confirmer",
        Verdict::Negative => "Perte moyenne (t ≤ −2)",
        Verdict::Unproven => "Avantage non démontré",
    }
}

/// Trade statistics over every trade of a group, pooled (each trade weighs the same).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pooled {
    pub trades: usize,
    /// % of trades with a positive net return.
    pub win_rate: Option<f64>,
    /// Sum of the gains ÷ sum of the losses (None without a losing trade).
    pub profit_factor: Option<f64>,
    /// Mean net return per trade (%) = average win × win rate − |average loss| × loss rate.
    pub expectancy: Option<f64>,
    /// Mean of (net return ÷ initial risk to the stop).
    pub avg_r: Option<f64>,
    /// Mean ÷ standard error of the net trade returns (None with fewer than 2 trades or no variation).
    pub t_stat: Option<f64>,
}

pub fn pooled<'a>(outcomes: impl IntoIterator<Item = &'a TradeOutcome>) -> Pooled {
    let (mut returns, mut risks) = (Vec::new(), Vec::new());
    for o in outcomes {
        returns.push(o.net_return);
        risks.push(o.risk.unwrap_or(f64::NAN));
    }
    let n = returns.len();
    if n == 0 {
        return Pooled::default();
    }
    let stats = trade_stats(&returns, &risks);
    let gains: f64 = returns.iter().filter(|x| **x > 0.0).sum();
    let losses: f64 = -returns.iter().filter(|x| **x <= 0.0).sum::<f64>();
    let mean = returns.iter().sum::<f64>() / n as f64;
    let sd = if n > 1 { (returns.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt() } else { 0.0 };
    Pooled {
        trades: n,
        win_rate: Some(round_to(returns.iter().filter(|x| **x > 0.0).count() as f64 / n as f64 * 100.0, 1)),
        profit_factor: (losses > 0.0).then(|| round_to(gains / losses, 2)),
        expectancy: stats.expectancy.map(|x| round_to(x, 2)),
        avg_r: stats.avg_r.map(|x| round_to(x, 2)),
        t_stat: (n > 1 && sd > 1e-12).then(|| round_to(mean / (sd / (n as f64).sqrt()), 2)),
    }
}

pub fn verdict(p: &Pooled) -> Verdict {
    match (p.trades, p.t_stat, p.expectancy) {
        (n, _, _) if n < MIN_TRADES => Verdict::Insufficient,
        (_, Some(t), Some(e)) if t >= T_EDGE && e > 0.0 => Verdict::Edge,
        (_, Some(t), _) if t <= -T_EDGE => Verdict::Negative,
        _ => Verdict::Unproven,
    }
}

/// Median (mean of the two middle values for an even count); None when empty.
pub fn median(values: &[f64]) -> Option<f64> {
    let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let m = v.len() / 2;
    Some(if v.len().is_multiple_of(2) { (v[m - 1] + v[m]) / 2.0 } else { v[m] })
}

/// Pooled trades of one regime, and the regime-days comparison with buy-and-hold across the group's assets.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegimeGroup {
    pub regime: Regime,
    pub label: String,
    #[serde(flatten)]
    pub pooled: Pooled,
    /// Asset-days spent in this regime (sum over the assets).
    pub days: usize,
    /// Assets with at least 20 days in it, and among them those where the signal made more than buy-and-hold over
    /// these days.
    pub assets: usize,
    pub beat_hold: usize,
    pub beat_share: Option<f64>,
    /// Medians over those assets of what the signal and buy-and-hold made during these days (compounded, %).
    pub median_signal: Option<f64>,
    pub median_hold: Option<f64>,
    pub low_sample: bool,
    pub verdict: Verdict,
    pub verdict_label: String,
}

/// `RegimeGroup` without its flattened `pooled`, read from the same JSON object (see `deserialize_flattened!`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegimeGroupFields {
    regime: Regime,
    label: String,
    days: usize,
    assets: usize,
    beat_hold: usize,
    beat_share: Option<f64>,
    median_signal: Option<f64>,
    median_hold: Option<f64>,
    low_sample: bool,
    verdict: Verdict,
    verdict_label: String,
}

crate::web::json::deserialize_flattened!(
    RegimeGroup,
    pooled,
    RegimeGroupFields { regime, label, days, assets, beat_hold, beat_share, median_signal, median_hold, low_sample, verdict, verdict_label }
);

/// One asset named with a value (the worst of a group).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Named {
    pub symbol: String,
    pub value: f64,
}

/// A group of assets (one class, or all): pooled trade statistics, dispersion of the per-asset results, comparison
/// with buy-and-hold and the same by market regime. Percentages in %.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupStat {
    /// "all" or the asset class.
    pub id: String,
    pub label: String,
    /// Assets tested in the group.
    pub assets: usize,
    /// Median tested span of its assets (years).
    pub years: Option<f64>,
    #[serde(flatten)]
    pub pooled: Pooled,
    /// Per-asset total return of the signal (costs included): median, worst.
    pub median_return: Option<f64>,
    pub worst_return: Option<Named>,
    pub median_hold: Option<f64>,
    /// Per-asset maximum drawdown (≤ 0): median, worst; buy-and-hold median over the same days.
    pub median_drawdown: Option<f64>,
    pub worst_drawdown: Option<Named>,
    pub median_hold_drawdown: Option<f64>,
    /// Per-asset Sharpe and Sortino (annualised, daily returns), medians.
    pub median_sharpe: Option<f64>,
    pub median_sortino: Option<f64>,
    pub median_exposure: Option<f64>,
    /// Assets where the signal made more than buy-and-hold over the whole period, and their share (%).
    pub beat_hold: usize,
    pub beat_share: Option<f64>,
    /// Medians after the flat tax assumption.
    pub median_after_tax: Option<f64>,
    pub median_hold_after_tax: Option<f64>,
    pub low_sample: bool,
    pub verdict: Verdict,
    pub verdict_label: String,
    pub regimes: Vec<RegimeGroup>,
}

/// `GroupStat` without its flattened `pooled`, read from the same JSON object (see `deserialize_flattened!`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupStatFields {
    id: String,
    label: String,
    assets: usize,
    years: Option<f64>,
    median_return: Option<f64>,
    worst_return: Option<Named>,
    median_hold: Option<f64>,
    median_drawdown: Option<f64>,
    worst_drawdown: Option<Named>,
    median_hold_drawdown: Option<f64>,
    median_sharpe: Option<f64>,
    median_sortino: Option<f64>,
    median_exposure: Option<f64>,
    beat_hold: usize,
    beat_share: Option<f64>,
    median_after_tax: Option<f64>,
    median_hold_after_tax: Option<f64>,
    low_sample: bool,
    verdict: Verdict,
    verdict_label: String,
    regimes: Vec<RegimeGroup>,
}

crate::web::json::deserialize_flattened!(
    GroupStat,
    pooled,
    GroupStatFields {
        id,
        label,
        assets,
        years,
        median_return,
        worst_return,
        median_hold,
        median_drawdown,
        worst_drawdown,
        median_hold_drawdown,
        median_sharpe,
        median_sortino,
        median_exposure,
        beat_hold,
        beat_share,
        median_after_tax,
        median_hold_after_tax,
        low_sample,
        verdict,
        verdict_label,
        regimes
    }
);

fn span_years(a: &AssetResult) -> f64 {
    (a.to - a.from) as f64 / (365.25 * 86_400_000.0)
}

fn share(k: usize, n: usize) -> Option<f64> {
    (n > 0).then(|| round_to(k as f64 / n as f64 * 100.0, 1))
}

fn med(assets: &[&AssetResult], f: impl Fn(&AssetResult) -> Option<f64>) -> Option<f64> {
    median(&assets.iter().filter_map(|a| f(a)).collect::<Vec<_>>()).map(|x| round_to(x, 2))
}

fn worst(assets: &[&AssetResult], f: impl Fn(&AssetResult) -> f64) -> Option<Named> {
    assets.iter().min_by(|a, b| f(a).total_cmp(&f(b))).map(|a| Named { symbol: a.symbol.clone(), value: f(a) })
}

fn regime_group(regime: Regime, assets: &[&AssetResult]) -> RegimeGroup {
    let p = pooled(assets.iter().flat_map(|a| a.outcomes.iter().filter(|o| o.regime == regime)));
    let days: Vec<RegimeDays> = assets.iter().filter_map(|a| a.regime_days.iter().find(|d| d.regime == regime).copied()).collect();
    let counted: Vec<&RegimeDays> = days.iter().filter(|d| d.days >= MIN_REGIME_DAYS).collect();
    let beat = counted.iter().filter(|d| d.signal > d.hold).count();
    let v = verdict(&p);
    RegimeGroup {
        regime,
        label: regime_label(regime).into(),
        pooled: p,
        days: days.iter().map(|d| d.days).sum(),
        assets: counted.len(),
        beat_hold: beat,
        beat_share: share(beat, counted.len()),
        median_signal: median(&counted.iter().map(|d| d.signal).collect::<Vec<_>>()).map(|x| round_to(x, 2)),
        median_hold: median(&counted.iter().map(|d| d.hold).collect::<Vec<_>>()).map(|x| round_to(x, 2)),
        low_sample: p.trades < MIN_TRADES,
        verdict: v,
        verdict_label: verdict_label(v).into(),
    }
}

pub fn group(id: &str, label: &str, assets: &[&AssetResult]) -> GroupStat {
    let p = pooled(assets.iter().flat_map(|a| a.outcomes.iter()));
    let beat = assets.iter().filter(|a| a.beat_hold).count();
    let v = verdict(&p);
    // Bull, bear, range and crisis always; unknown (history too short to classify) only when it has trades or days.
    let regimes =
        REGIMES.iter().map(|r| regime_group(*r, assets)).filter(|g| g.regime != Regime::Unknown || g.pooled.trades > 0 || g.days > 0).collect();
    GroupStat {
        id: id.into(),
        label: label.into(),
        assets: assets.len(),
        years: median(&assets.iter().map(|a| span_years(a)).collect::<Vec<_>>()).map(|y| round_to(y, 1)),
        pooled: p,
        median_return: med(assets, |a| Some(a.total_return)),
        worst_return: worst(assets, |a| a.total_return),
        median_hold: med(assets, |a| Some(a.buy_and_hold)),
        median_drawdown: med(assets, |a| Some(a.max_drawdown)),
        worst_drawdown: worst(assets, |a| a.max_drawdown),
        median_hold_drawdown: med(assets, |a| Some(a.hold_max_drawdown)),
        median_sharpe: med(assets, |a| a.sharpe),
        median_sortino: med(assets, |a| a.sortino),
        median_exposure: med(assets, |a| Some(a.exposure)),
        beat_hold: beat,
        beat_share: share(beat, assets.len()),
        median_after_tax: med(assets, |a| Some(a.after_tax)),
        median_hold_after_tax: med(assets, |a| Some(a.hold_after_tax)),
        low_sample: p.trades < MIN_TRADES,
        verdict: v,
        verdict_label: verdict_label(v).into(),
        regimes,
    }
}

/// The basket as documented in the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasketEntry {
    pub symbol: String,
    pub name: String,
    pub kind: Kind,
    pub class: AssetClass,
    pub group: String,
}

/// Fixed parameters of the test (those of the live signal and of the decision's track record).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameters {
    /// Per order, %.
    pub fees_pct: f64,
    pub slippage_pct: f64,
    /// Full bid/ask spread assumed (half paid per order), %.
    pub spread_stock_pct: f64,
    pub spread_crypto_pct: f64,
    /// Candles the signal reads, warm-up before the first test, target in multiples of the risk.
    pub lookback: usize,
    pub warmup: usize,
    pub reward_risk: f64,
    pub tax_rate_pct: f64,
    pub min_trades: usize,
    pub t_edge: f64,
    pub regime_rule: String,
}

/// Whether the tested period was unseen when the parameters were chosen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutOfSample {
    /// "verified" | "notVerifiable".
    pub status: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub as_of: i64,
    pub basket_fixed_on: String,
    /// Plain-French verdict.
    pub headline: String,
    /// Earliest and latest tested daily candle over the assets; median, shortest and longest tested span (years).
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub years: Option<f64>,
    pub min_years: Option<f64>,
    pub max_years: Option<f64>,
    pub overall: GroupStat,
    /// Stocks, Bitcoin, Ethereum, altcoins (in that order, those with at least one tested asset).
    pub classes: Vec<GroupStat>,
    /// In the basket's order (by class), never ranked by performance.
    pub assets: Vec<AssetResult>,
    pub failures: Vec<Failure>,
    pub basket: Vec<BasketEntry>,
    pub parameters: Parameters,
    /// How the test protects itself against snooping and look-ahead (French sentences).
    pub protections: Vec<String>,
    pub out_of_sample: OutOfSample,
    /// Biases and limits (French sentences).
    pub limits: Vec<String>,
    pub source: String,
}

const MONTHS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

fn month_year(ms: i64) -> String {
    use chrono::Datelike;
    match chrono::DateTime::from_timestamp_millis(ms) {
        Some(d) => format!("{} {}", MONTHS[d.month0() as usize], d.year()),
        None => "?".into(),
    }
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n > 1 { "s" } else { "" })
}

fn t_text(p: &Pooled) -> String {
    p.t_stat.map(|t| format!(" (t = {}{})", if t < 0.0 { "−" } else { "" }, fr(t.abs(), 0, 1))).unwrap_or_default()
}

/// "Sur 34 actifs et 2,6 à 4,8 ans d'historique, …": the verdict in plain French, from the numbers only. `span`:
/// shortest and longest tested history (years).
pub fn headline(overall: &GroupStat, span: Option<(f64, f64)>) -> String {
    if overall.assets == 0 {
        return "Aucun actif du panier n'a pu être testé : rien à conclure.".into();
    }
    let span = match span {
        Some((a, b)) if (b - a).abs() >= 0.2 => format!(" et de {} à {} ans d'historique selon l'actif", fr(a, 0, 1), fr(b, 0, 1)),
        Some((_, b)) => format!(" et environ {} ans d'historique", fr(b, 0, 1)),
        None => String::new(),
    };
    let mut parts = vec![format!(
        "Sur {}{span} (frais, glissement et écart compris), le signal a battu la simple détention sur {}",
        plural(overall.assets, "actif"),
        plural(overall.beat_hold, "actif")
    )];
    parts.push(match overall.verdict {
        Verdict::Insufficient => format!("trop peu de trades ({}) pour conclure", overall.pooled.trades),
        Verdict::Edge => format!("gain moyen par trade positif{}, à confirmer", t_text(&overall.pooled)),
        Verdict::Negative => format!("perte moyenne par trade{}", t_text(&overall.pooled)),
        Verdict::Unproven => format!("avantage par trade non démontré{}", t_text(&overall.pooled)),
    });
    if let Some(bear) = overall.regimes.iter().find(|g| g.regime == Regime::Bear) {
        parts.push(match bear.verdict {
            Verdict::Insufficient => format!("trop peu de trades en marché baissier ({}) pour conclure", bear.pooled.trades),
            Verdict::Edge => "avantage observé en marché baissier, à confirmer".into(),
            Verdict::Negative => "perte moyenne par trade en marché baissier".into(),
            Verdict::Unproven => "avantage non démontré en marché baissier".into(),
        });
    }
    format!("{}.", parts.join(" ; "))
}

fn pct3(x: f64) -> String {
    format!("{} %", fr(x, 0, 3))
}

/// Pools the per-asset results (any order: they are put back in the basket's order) into the report.
pub fn aggregate(mut assets: Vec<AssetResult>, failures: Vec<Failure>, now: i64, source: &str) -> ValidationReport {
    let index = |s: &str| BASKET.iter().position(|b| b.symbol == s).unwrap_or(usize::MAX);
    assets.sort_by_key(|a| index(&a.symbol));
    let mut failures = failures;
    failures.sort_by_key(|f| index(&f.symbol));
    let all: Vec<&AssetResult> = assets.iter().collect();
    let overall = group("all", "Tous les actifs", &all);
    let classes: Vec<GroupStat> = CLASSES
        .iter()
        .filter_map(|c| {
            let members: Vec<&AssetResult> = assets.iter().filter(|a| a.class == Some(*c)).collect();
            (!members.is_empty()).then(|| group(c.id(), c.label(), &members))
        })
        .collect();
    let from = assets.iter().map(|a| a.from).min();
    let to = assets.iter().map(|a| a.to).max();
    let spans: Vec<f64> = assets.iter().map(span_years).collect();
    let years = median(&spans).map(|y| round_to(y, 1));
    let min_years = spans.iter().copied().reduce(f64::min).map(|y| round_to(y, 1));
    let max_years = spans.iter().copied().reduce(f64::max).map(|y| round_to(y, 1));
    let headline = headline(&overall, min_years.zip(max_years));
    let trades = overall.pooled.trades;
    let protections = vec![
        format!(
            "Panier fixé le 29 septembre 2026, avant tout calcul : {} choisis sur leur taille, leur liquidité et la diversité des secteurs, jamais sur leurs performances passées.",
            plural(BASKET.len(), "actif")
        ),
        "Paramètres fixes, ceux du signal en direct et de l'historique de la carte Décision (moyennes 20/50/200, RSI 14, MACD 12/26/9, ADX 14, stop à 2 ATR, objectif 2 R) : aucune optimisation, ni par actif ni sur ce panier.".into(),
        "Signal calculé sur la bougie journalière clôturée, exécuté à l'ouverture suivante ; régime de marché lu la veille, avec les seules données connues ce jour-là.".into(),
        format!(
            "Coûts payés à chaque ordre : frais {}, glissement {}, demi-écart achat/vente {} (actions) ou {} (cryptos), sans mesure du carnet d'ordres.",
            pct3(FEE_RATE * 100.0),
            pct3(SLIPPAGE * 100.0),
            pct3(SPREAD_STOCK / 2.0),
            pct3(SPREAD_CRYPTO / 2.0)
        ),
        format!(
            "Échantillon : {} au total ; tout groupe sous {MIN_TRADES} trades est marqué « échantillon trop faible ».",
            plural(trades, "trade")
        ),
        format!(
            "Un gain par trade n'est dit « positif » que s'il dépasse {} fois son erreur type (t ≥ {}) avec au moins {MIN_TRADES} trades ; la comparaison avec la simple détention est faite à part, actif par actif.",
            fr(T_EDGE, 0, 0),
            fr(T_EDGE, 0, 0)
        ),
    ];
    let out_of_sample = OutOfSample {
        status: "notVerifiable".into(),
        note: "Non vérifiable. Les réglages du signal sont des valeurs classiques, inchangées dans l'historique du dépôt, qui ne contient aucun code d'optimisation. Mais cet historique ne commence qu'en septembre 2026 et la période testée précède ou recouvre la conception du signal : on ne peut pas prouver qu'elle n'a pas influencé ces choix. Ce test n'est donc pas « hors échantillon » au sens strict.".into(),
    };
    let since = from.map(month_year).unwrap_or_else(|| "?".into());
    let limits = vec![
        "Biais du survivant : le panier réunit de grands actifs d'aujourd'hui ; ceux qui ont disparu ou chuté du classement ne sont pas testés, ce qui flatte surtout la détention.".into(),
        format!(
            "Période limitée à l'historique journalier disponible (depuis {since} au plus tôt, moins pour les cryptos) : peu de cycles de marché, et peu de phases baissières pour certains actifs."
        ),
        format!(
            "Régime inconnu pendant les {} premières bougies de chaque historique (il faut une moyenne sur 200 jours et sa pente sur 20 jours).",
            REGIME_SMA + REGIME_SLOPE
        ),
        "Actifs corrélés (les altcoins suivent le bitcoin, les actions le S&P 500) : les trades ne sont pas indépendants, le t surestime la solidité d'un résultat commun.".into(),
        "Sharpe, Sortino et drawdown des groupes : médianes des actifs pris un par un, pas ceux d'un portefeuille.".into(),
        "Détention sans frais ni dividendes, signal sans dividendes : l'écart avec la détention d'actions à dividende est un peu flatté pour le signal.".into(),
        "Écart achat/vente supposé, non mesuré : il est souvent plus large sur les altcoins moins échangés.".into(),
        format!(
            "Fiscalité : hypothèse forfaitaire (PFU de {} %) appliquée au seul résultat net positif de chaque actif, à la fin ; en réalité l'impôt est annuel, les moins-values se compensent et les cryptos ont leur propre régime. Illustration seulement.",
            fr(TAX_RATE, 0, 0)
        ),
        "Le passé ne garantit pas l'avenir : même un avantage observé ici peut disparaître.".into(),
    ];
    ValidationReport {
        as_of: now,
        basket_fixed_on: BASKET_FIXED_ON.into(),
        headline,
        from,
        to,
        years,
        min_years,
        max_years,
        overall,
        classes,
        assets,
        failures,
        basket: BASKET
            .iter()
            .map(|b| BasketEntry { symbol: b.symbol.into(), name: b.name.into(), kind: b.kind, class: b.class, group: b.group.into() })
            .collect(),
        parameters: Parameters {
            fees_pct: round_to(FEE_RATE * 100.0, 4),
            slippage_pct: round_to(SLIPPAGE * 100.0, 4),
            spread_stock_pct: SPREAD_STOCK,
            spread_crypto_pct: SPREAD_CRYPTO,
            lookback: LOOKBACK,
            warmup: WARMUP,
            reward_risk: REWARD_RISK,
            tax_rate_pct: TAX_RATE,
            min_trades: MIN_TRADES,
            t_edge: T_EDGE,
            regime_rule: format!(
                "Crise : plus de {} % sous le plus haut d'un an ; haussier : clôture au-dessus d'une moyenne {REGIME_SMA} jours qui monte (sur {REGIME_SLOPE} jours) ; baissier : sous une moyenne qui baisse ; sinon sans tendance.",
                fr(CRISIS_DRAWDOWN * 100.0, 0, 0)
            ),
        },
        protections,
        out_of_sample,
        limits,
        source: source.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basket_is_fixed_and_unique() {
        let mut s: Vec<&str> = BASKET.iter().map(|b| b.symbol).collect();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), BASKET.len());
        assert_eq!(BASKET.iter().filter(|b| b.class == AssetClass::Stock).count(), 22);
        assert_eq!(BASKET.iter().filter(|b| b.class == AssetClass::Altcoin).count(), 10);
    }

    #[test]
    fn median_and_tax() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(after_tax(10.0), 7.0);
        assert_eq!(after_tax(-10.0), -10.0);
    }
}

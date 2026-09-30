//! Risk of the portfolio the user holds, beyond `holdings` (`web/src/engine/portfolio-risk.ts`): market stress
//! scenarios through each line's beta, checks against the user's risk settings (weight per line, crypto share,
//! correlated clusters, daily loss), the positions that became dangerous (stop broken or close, loss beyond the risk
//! accepted per idea) and « Et si… ? » (a shock on one market factor). Pure, deterministic functions.
use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::holdings::{LineAnalysis, PortfolioAnalysis, aligned_returns, correlation, market_key, positive};
use crate::engine::signal::{Candle, atr};
use crate::js::{fr, iso_date, to_fixed};
use crate::types::Kind;
use crate::web::danger::{Danger, DangerCode, DangerReason};
use crate::web::store::RiskSettings;

// ---------- Beta ----------

/// Window of the beta (same as the correlations of `analyze_portfolio`) and minimum of shared daily returns.
pub const BETA_DAYS: usize = 90;
pub const MIN_BETA_DAYS: usize = 30;
/// "Et si… ?": one year of shared sessions, steadier than 90 days for a shock on one market.
pub const WHATIF_BETA_DAYS: usize = 250;
/// Below this |correlation| the beta explains little of the line's moves: said next to the line.
pub const WEAK_CORRELATION: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Benchmark {
    pub symbol: &'static str,
    pub kind: Kind,
    pub label: &'static str,
}

/// Benchmark of each asset class: Bitcoin for cryptos, the S&P 500 (via the SPY ETF) for stocks.
pub fn benchmark(kind: Kind) -> Benchmark {
    match kind {
        Kind::Crypto => Benchmark { symbol: "BTC", kind: Kind::Crypto, label: "Bitcoin" },
        Kind::Stock => Benchmark { symbol: "SPY", kind: Kind::Stock, label: "S&P 500" },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BetaEstimate {
    pub beta: f64,
    /// Shared daily returns used; 0 with the fallback.
    pub days: usize,
    /// false: history too short or missing, beta 1 assumed.
    pub estimated: bool,
    /// The asset is the benchmark itself (beta 1 by definition).
    #[serde(default)]
    pub reference: bool,
}

const FALLBACK: BetaEstimate = BetaEstimate { beta: 1.0, days: 0, estimated: false, reference: false };

/// Beta of an asset to its benchmark = cov(asset, benchmark) ÷ var(benchmark) on the aligned daily returns.
pub fn estimate_beta(asset: &[Candle], benchmark: &[Candle], days: usize) -> BetaEstimate {
    if asset.len() < 2 || benchmark.len() < 2 {
        return FALLBACK;
    }
    let r = aligned_returns(&[asset, benchmark], days);
    let (a, b) = (&r[0], &r[1]);
    let n = a.len().min(b.len());
    if n < MIN_BETA_DAYS {
        return FALLBACK;
    }
    let (x, y) = (&a[a.len() - n..], &b[b.len() - n..]);
    let mx = x.iter().sum::<f64>() / n as f64;
    let my = y.iter().sum::<f64>() / n as f64;
    let (mut cov, mut vary) = (0.0, 0.0);
    for i in 0..n {
        cov += (x[i] - mx) * (y[i] - my);
        vary += (y[i] - my).powi(2);
    }
    if vary > 0.0 { BetaEstimate { beta: cov / vary, days: n, estimated: true, reference: false } } else { FALLBACK }
}

// ---------- Stress scenarios ----------

/// Shock of each benchmark, % (negative = fall).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StressScenario {
    pub key: &'static str,
    pub label: &'static str,
    pub crypto: f64,
    pub stock: f64,
}

pub const STRESS_SCENARIOS: [StressScenario; 6] = [
    StressScenario { key: "all5", label: "Marchés −5 %", crypto: -5.0, stock: -5.0 },
    StressScenario { key: "all10", label: "Marchés −10 %", crypto: -10.0, stock: -10.0 },
    StressScenario { key: "all20", label: "Marchés −20 %", crypto: -20.0, stock: -20.0 },
    StressScenario { key: "all30", label: "Marchés −30 %", crypto: -30.0, stock: -30.0 },
    StressScenario { key: "crypto20", label: "Crypto −20 %, actions stables", crypto: -20.0, stock: 0.0 },
    StressScenario { key: "stock10crypto30", label: "Actions −10 %, crypto −30 %", crypto: -30.0, stock: -10.0 },
];

#[derive(Debug, Clone, PartialEq)]
pub struct StressWorst {
    pub symbol: String,
    pub name: String,
    pub loss: f64,
    pub move_percent: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StressResult {
    pub scenario: StressScenario,
    /// Amount lost (USD, positive = loss; negative when the scenario would gain).
    pub loss: f64,
    /// Of the whole portfolio (cash included).
    pub loss_percent: f64,
    /// Of the invested part only (without cash): the difference is what the cash cushions.
    pub invested_loss_percent: f64,
    pub worst: Option<StressWorst>,
}

/// Each line moves by beta × the shock of its benchmark (never below −100 %); cash does not move. Lines of the same
/// asset share its beta (key `kind:symbol`, beta 1 when missing).
pub fn stress_test(a: &PortfolioAnalysis, betas: &HashMap<String, BetaEstimate>, scenarios: &[StressScenario]) -> Vec<StressResult> {
    scenarios
        .iter()
        .map(|scenario| {
            let mut by_asset: IndexMap<String, StressWorst> = IndexMap::new();
            for l in &a.lines {
                let k = market_key(l.kind, &l.symbol);
                let beta = betas.get(&k).map(|b| b.beta).unwrap_or(1.0);
                let mv = (beta * if l.kind == Kind::Crypto { scenario.crypto } else { scenario.stock }).max(-100.0);
                let loss = -l.value * mv / 100.0;
                let prev = by_asset.get(&k).map(|p| p.loss).unwrap_or(0.0);
                by_asset.insert(k, StressWorst { symbol: l.symbol.clone(), name: l.name.clone(), loss: prev + loss, move_percent: mv });
            }
            let loss: f64 = by_asset.values().map(|x| x.loss).sum();
            let worst =
                by_asset.values().fold(None::<&StressWorst>, |w, x| if x.loss > 0.0 && w.is_none_or(|w| x.loss > w.loss) { Some(x) } else { w });
            StressResult {
                scenario: *scenario,
                loss,
                loss_percent: if a.total > 0.0 { loss / a.total * 100.0 } else { 0.0 },
                invested_loss_percent: if a.market_value > 0.0 { loss / a.market_value * 100.0 } else { 0.0 },
                worst: worst.cloned(),
            }
        })
        .collect()
}

// ---------- Correlated clusters ----------

pub const CLUSTER_CORRELATION: f64 = 0.7;

#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    pub symbols: Vec<String>,
    pub average_correlation: f64,
    pub weight: f64,
    pub days: usize,
}

/// One asset of `correlated_clusters`: weight in % of the portfolio, daily candles.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterInput<'a> {
    pub symbol: String,
    pub weight: f64,
    pub daily: &'a [Candle],
}

/// Groups of assets linked by a correlation above 0.7 (connected by pairs), kept when their average pairwise
/// correlation stays above 0.7.
pub fn correlated_clusters(series: &[ClusterInput], days: usize) -> Vec<Cluster> {
    let usable: Vec<&ClusterInput> = series.iter().filter(|s| s.daily.len() >= 20).collect();
    if usable.len() < 2 {
        return Vec::new();
    }
    let returns = aligned_returns(&usable.iter().map(|s| s.daily).collect::<Vec<_>>(), days);
    let n = usable.len();
    let mut corr: Vec<Vec<Option<f64>>> = vec![vec![None; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let c = correlation(&returns[i], &returns[j]);
            corr[i][j] = c;
            corr[j][i] = c;
        }
    }
    let mut seen: HashSet<usize> = HashSet::new();
    let mut clusters = Vec::new();
    for s in 0..n {
        if seen.contains(&s) {
            continue;
        }
        let mut group = vec![s];
        seen.insert(s);
        let mut k = 0;
        while k < group.len() {
            for (j, c) in corr[group[k]].iter().enumerate() {
                if !seen.contains(&j) && c.unwrap_or(-1.0) > CLUSTER_CORRELATION {
                    seen.insert(j);
                    group.push(j);
                }
            }
            k += 1;
        }
        if group.len() < 2 {
            continue;
        }
        let mut pairs = Vec::new();
        for x in 0..group.len() {
            for y in x + 1..group.len() {
                if let Some(c) = corr[group[x]][group[y]] {
                    pairs.push(c);
                }
            }
        }
        let average_correlation = pairs.iter().sum::<f64>() / pairs.len() as f64;
        if average_correlation <= CLUSTER_CORRELATION {
            continue;
        }
        clusters.push(Cluster {
            symbols: group.iter().map(|g| usable[*g].symbol.clone()).collect(),
            average_correlation,
            weight: group.iter().map(|g| usable[*g].weight).sum(),
            days: returns.iter().map(Vec::len).min().unwrap_or(0),
        });
    }
    clusters.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
    clusters
}

// ---------- Today's change ----------

/// Previous close: the close of the last daily candle before today's (UTC calendar day; for stocks, the last
/// session). None without candles.
pub fn previous_close(daily: &[Candle], now: i64) -> Option<f64> {
    let mut sorted = daily.to_vec();
    sorted.sort_by_key(|c| c.time);
    let last = sorted.last()?;
    if iso_date(last.time) == iso_date(now) {
        return (sorted.len() > 1).then(|| sorted[sorted.len() - 2].close);
    }
    Some(last.close)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DailyChange {
    /// USD, negative = loss today.
    pub change: f64,
    /// Of the portfolio value at the previous close (cash included).
    pub percent: f64,
    /// Lines with a live price and a previous close / all lines.
    pub covered: usize,
    pub lines: usize,
}

/// Portfolio change since the previous close, from the current prices of the analysis.
pub fn daily_change(a: &PortfolioAnalysis, daily: &HashMap<String, Vec<Candle>>, now: i64) -> Option<DailyChange> {
    let (mut change, mut covered) = (0.0, 0);
    for l in &a.lines {
        let prev = daily.get(&market_key(l.kind, &l.symbol)).and_then(|d| previous_close(d, now));
        let (Some(price), Some(prev)) = (l.price, prev) else { continue };
        if prev <= 0.0 {
            continue;
        }
        change += l.quantity * (price - prev);
        covered += 1;
    }
    if covered == 0 {
        return None;
    }
    let before = a.total - change;
    Some(DailyChange { change, percent: if before > 0.0 { change / before * 100.0 } else { 0.0 }, covered, lines: a.lines.len() })
}

// ---------- Limits of the user's settings ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitLevel {
    Danger,
    Warning,
    Ok,
    Na,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LimitCheck {
    pub code: &'static str,
    pub level: LimitLevel,
    pub label: String,
    pub detail: String,
}

pub const DAILY_LOSS_REACHED: &str = "Limite de perte du jour atteinte : n'ouvrez plus de position aujourd'hui.";

/// `toLocaleString("fr-FR", { maximumFractionDigits: d })`.
fn frd(v: f64, d: usize) -> String {
    fr(v, 0, d)
}

/// `money(Math.abs(v), 0, 0, " ")`.
fn usd0(v: f64) -> String {
    crate::fx::money_with(v.abs(), |x| fr(x, 0, 0))
}

/// Loss (USD) if the stop of the line is hit: the user's stop when set, else the protective stop of the analysis.
fn loss_at_stop(l: &LineAnalysis, user_stop: Option<f64>) -> Option<f64> {
    let stop = user_stop.filter(|s| *s > 0.0).or(l.stop);
    let (price, stop) = (l.price?, stop?);
    Some((l.quantity * (price - stop)).max(0.0))
}

/// Checks of the portfolio against the settings: loss at the stop per asset vs risk per idea, weight per asset vs
/// maximum, crypto share vs cap, correlated clusters above twice the maximum weight, today's loss vs the daily limit.
/// `stops`: the user's stops by line id.
pub fn check_limits(
    a: &PortfolioAnalysis,
    s: &RiskSettings,
    clusters: &[Cluster],
    daily: Option<&DailyChange>,
    stops: &HashMap<String, f64>,
) -> Vec<LimitCheck> {
    let mut out = Vec::new();
    if a.lines.is_empty() || a.total <= 0.0 {
        return out;
    }
    struct Asset {
        symbol: String,
        weight: f64,
        loss: Option<f64>,
    }
    let mut assets: IndexMap<String, Asset> = IndexMap::new();
    for l in &a.lines {
        let k = market_key(l.kind, &l.symbol);
        let loss = loss_at_stop(l, stops.get(&l.id).copied());
        let p = assets.get(&k);
        let prev_loss = p.and_then(|p| p.loss);
        let weight = p.map(|p| p.weight).unwrap_or(0.0) + l.weight;
        let loss = if prev_loss.is_none() && loss.is_none() { None } else { Some(prev_loss.unwrap_or(0.0) + loss.unwrap_or(0.0)) };
        assets.insert(k, Asset { symbol: l.symbol.clone(), weight, loss });
    }

    let budget = a.total * s.risk_per_trade_percent / 100.0;
    let label = format!("Risque par ligne (max {} %)", frd(s.risk_per_trade_percent, 2));
    let risky: Vec<&Asset> = assets.values().filter(|x| x.loss.is_some_and(|l| l > budget * 1.0001)).collect();
    out.push(if risky.is_empty() {
        LimitCheck {
            code: "risk_per_trade",
            level: LimitLevel::Ok,
            label,
            detail: format!("Aucune ligne ne perdrait plus de {} à son stop.", usd0(budget)),
        }
    } else {
        let list: Vec<String> = risky
            .iter()
            .map(|x| {
                let loss = x.loss.unwrap_or(0.0);
                format!("{} coûterait {} ({} %)", x.symbol, usd0(loss), frd(loss / a.total * 100.0, 1))
            })
            .collect();
        LimitCheck {
            code: "risk_per_trade",
            level: LimitLevel::Warning,
            label,
            detail: format!(
                "Si le stop était touché, {} : plus que votre risque accepté par idée ({}). Réduisez la ligne ou rapprochez le stop.",
                list.join(", "),
                usd0(budget)
            ),
        }
    });

    let label = format!("Poids max d'une ligne ({} %)", frd(s.max_position_percent, 0));
    let heavy: Vec<&Asset> = assets.values().filter(|x| x.weight > s.max_position_percent).collect();
    out.push(if !heavy.is_empty() && assets.len() > 1 {
        let list: Vec<String> = heavy.iter().map(|x| format!("{} {} %", x.symbol, frd(x.weight, 1))).collect();
        LimitCheck {
            code: "max_weight",
            level: LimitLevel::Danger,
            label,
            detail: format!("{} : au-dessus de votre maximum. Surexposition à un seul actif.", list.join(", ")),
        }
    } else {
        LimitCheck { code: "max_weight", level: LimitLevel::Ok, label, detail: "Aucune ligne au-dessus de votre maximum.".into() }
    });

    let crypto = a.allocation.crypto;
    let label = format!("Part crypto (max {} %)", frd(s.max_crypto_percent, 0));
    out.push(if crypto > s.max_crypto_percent {
        LimitCheck {
            code: "crypto_cap",
            level: LimitLevel::Warning,
            label,
            detail: format!(
                "{} % du patrimoine en crypto, au-dessus de votre plafond : un repli des cryptos toucherait tout le portefeuille.",
                frd(crypto, 1)
            ),
        }
    } else {
        LimitCheck { code: "crypto_cap", level: LimitLevel::Ok, label, detail: format!("{} % du patrimoine en crypto.", frd(crypto, 1)) }
    });

    let cluster_max = (s.max_position_percent * 2.0).min(100.0);
    let label = format!("Actifs corrélés (groupe max {} %)", frd(cluster_max, 0));
    let over: Vec<&Cluster> = clusters.iter().filter(|c| c.weight > cluster_max).collect();
    out.push(if !over.is_empty() {
        let list: Vec<String> = over
            .iter()
            .map(|c| {
                format!(
                    "{} : {} % du patrimoine, corrélation moyenne {} sur {} jours",
                    c.symbols.join(" + "),
                    frd(c.weight, 1),
                    to_fixed(c.average_correlation, 2),
                    c.days
                )
            })
            .collect();
        LimitCheck {
            code: "cluster",
            level: LimitLevel::Warning,
            label,
            detail: format!("{}. Ils se comportent comme une seule grosse ligne.", list.join(" ; ")),
        }
    } else {
        let detail = if clusters.is_empty() {
            "Aucun groupe d'actifs corrélés à plus de 0,7.".to_string()
        } else {
            let list: Vec<String> = clusters.iter().map(|c| format!("{} {} %", c.symbols.join(" + "), frd(c.weight, 1))).collect();
            format!("Groupe(s) corrélé(s) sous le seuil : {}.", list.join(" ; "))
        };
        LimitCheck { code: "cluster", level: LimitLevel::Ok, label, detail }
    });

    let label = format!("Perte du jour (max {} %)", frd(s.daily_loss_limit_percent, 2));
    match daily {
        None => out.push(LimitCheck {
            code: "daily_loss",
            level: LimitLevel::Na,
            label,
            detail: "Clôture de la veille indisponible : variation du jour inconnue.".into(),
        }),
        Some(d) => {
            let partial = if d.covered < d.lines { format!(" ({} ligne(s) sur {} mesurées)", d.covered, d.lines) } else { String::new() };
            let text = format!(
                "{}{} ({}{} %) depuis la clôture de la veille{partial}.",
                if d.change >= 0.0 { "+" } else { "−" },
                usd0(d.change),
                if d.percent >= 0.0 { "+" } else { "−" },
                frd(d.percent.abs(), 2)
            );
            out.push(if d.percent <= -s.daily_loss_limit_percent {
                LimitCheck { code: "daily_loss", level: LimitLevel::Danger, label, detail: format!("{DAILY_LOSS_REACHED} {text}") }
            } else {
                LimitCheck { code: "daily_loss", level: LimitLevel::Ok, label, detail: text }
            });
        }
    }
    out
}

// ---------- Positions that became dangerous ----------

/// `moneyFmt(v, (v) => fr(v, 4), " ")`.
fn price4(v: f64) -> String {
    crate::fx::money_with(v, |x| fr(x, 0, 4))
}

/// A line is dangerous when its price broke the user's stop, is within one daily ATR (14) of it, or when its
/// unrealised loss exceeds the risk accepted per idea (% of the portfolio). `stops`: the user's stops by line id.
pub fn dangerous_positions(
    a: &PortfolioAnalysis,
    s: &RiskSettings,
    daily: &HashMap<String, Vec<Candle>>,
    stops: &HashMap<String, f64>,
) -> Vec<Danger> {
    let mut out = Vec::new();
    let budget = a.total * s.risk_per_trade_percent / 100.0;
    for l in &a.lines {
        let mut reasons = Vec::new();
        let stop = stops.get(&l.id).copied();
        let candles = daily.get(&market_key(l.kind, &l.symbol)).map(Vec::as_slice).unwrap_or(&[]);
        let range = if candles.is_empty() { None } else { atr(candles, 14)[candles.len() - 1] };
        if let (Some(price), Some(stop)) = (l.price, stop.filter(|s| *s > 0.0)) {
            if price <= stop {
                reasons.push(DangerReason {
                    code: DangerCode::StopBroken,
                    text: format!("Stop cassé : cours {} sous votre stop {}.", price4(price), price4(stop)),
                });
            } else if let Some(range) = range.filter(|r| *r != 0.0 && price - stop <= *r) {
                reasons.push(DangerReason {
                    code: DangerCode::NearStop,
                    text: format!("À moins d'une volatilité journalière (ATR {}) de votre stop {}.", price4(range), price4(stop)),
                });
            }
        }
        let loss = l.invested - l.value;
        if l.price.is_some() && loss > budget && budget > 0.0 {
            reasons.push(DangerReason {
                code: DangerCode::LossOverRisk,
                text: format!(
                    "Perte latente de {} ({} % du patrimoine), au-delà de votre risque accepté par idée ({} %).",
                    usd0(loss),
                    frd(loss / a.total * 100.0, 1),
                    frd(s.risk_per_trade_percent, 2)
                ),
            });
        }
        if !reasons.is_empty() {
            out.push(Danger { id: l.id.clone(), symbol: l.symbol.clone(), kind: l.kind, name: l.name.clone(), reasons });
        }
    }
    out
}

// ---------- "Et si… ?": a shock on one market factor ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactorKey {
    Qqq,
    Spy,
    Btc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Factor {
    pub symbol: &'static str,
    pub kind: Kind,
    pub label: &'static str,
    pub name: &'static str,
}

/// Factors of the simulator, in their order on screen.
pub const FACTOR_KEYS: [FactorKey; 3] = [FactorKey::Qqq, FactorKey::Spy, FactorKey::Btc];

impl FactorKey {
    /// The Nasdaq-100 (via the QQQ ETF), the S&P 500 (via SPY) and Bitcoin.
    pub fn factor(self) -> Factor {
        match self {
            FactorKey::Qqq => Factor { symbol: "QQQ", kind: Kind::Stock, label: "Nasdaq-100 (QQQ)", name: "le Nasdaq-100" },
            FactorKey::Spy => Factor { symbol: "SPY", kind: Kind::Stock, label: "S&P 500 (SPY)", name: "le S&P 500" },
            FactorKey::Btc => Factor { symbol: "BTC", kind: Kind::Crypto, label: "Bitcoin", name: "le Bitcoin" },
        }
    }
}

pub const FACTOR_SHOCKS: [f64; 5] = [-5.0, -10.0, -20.0, -30.0, -50.0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FactorBeta {
    pub beta: f64,
    pub days: usize,
    pub estimated: bool,
    /// Correlation of the daily returns with the factor (None: too few days).
    pub correlation: Option<f64>,
}

/// Beta of an asset to a factor on the days where BOTH have a daily close (no forward fill: a crypto's weekends do
/// not become days where a stock "did not move"), returns between consecutive shared days, last `days` of them.
/// Not estimated (estimated: false, beta 1 only as a placeholder) under MIN_BETA_DAYS shared returns.
pub fn factor_beta(asset: &[Candle], factor: &[Candle], days: usize) -> FactorBeta {
    let none = |n: usize| FactorBeta { beta: 1.0, days: n, estimated: false, correlation: None };
    let mut fa: HashMap<String, f64> = HashMap::new();
    for c in factor.iter().filter(|c| c.close > 0.0) {
        fa.insert(iso_date(c.time), c.close);
    }
    let mut by_day: IndexMap<String, f64> = IndexMap::new();
    for c in asset.iter().filter(|c| c.close > 0.0) {
        by_day.insert(iso_date(c.time), c.close);
    }
    let mut shared: Vec<(String, f64)> = by_day.into_iter().filter(|(d, _)| fa.contains_key(d)).collect();
    shared.sort_by(|a, b| a.0.cmp(&b.0));
    let shared = &shared[shared.len().saturating_sub(days + 1)..];
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for i in 1..shared.len() {
        x.push(shared[i].1 / shared[i - 1].1 - 1.0);
        y.push(fa[&shared[i].0] / fa[&shared[i - 1].0] - 1.0);
    }
    let n = x.len();
    if n < MIN_BETA_DAYS {
        return none(n);
    }
    let mx = x.iter().sum::<f64>() / n as f64;
    let my = y.iter().sum::<f64>() / n as f64;
    let (mut cov, mut vx, mut vy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        cov += (x[i] - mx) * (y[i] - my);
        vx += (x[i] - mx).powi(2);
        vy += (y[i] - my).powi(2);
    }
    if !positive(vy) {
        return none(n);
    }
    FactorBeta { beta: cov / vy, days: n, estimated: true, correlation: (vx > 0.0).then(|| cov / (vx * vy).sqrt()) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhatIfLine {
    pub key: String,
    pub symbol: String,
    pub name: String,
    pub kind: Kind,
    /// Value of the line in the simulated amount (USD).
    pub value: f64,
    /// % of the simulated amount.
    pub weight: f64,
    pub beta: Option<f64>,
    pub days: usize,
    pub correlation: Option<f64>,
    /// The line is the factor itself (beta 1 by definition).
    pub reference: bool,
    /// Move of the line (%), None when its beta could not be measured ("non couvert").
    pub move_percent: Option<f64>,
    /// USD, positive = loss; None when not covered.
    pub loss: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhatIfResult {
    pub factor: FactorKey,
    pub shock: f64,
    /// Simulated amount: the portfolio's value, or the amount entered spread on the current weights.
    pub base: f64,
    pub scaled: bool,
    pub cash: f64,
    pub lines: Vec<WhatIfLine>,
    /// Total loss of the covered lines (USD, positive = loss) and its share of `base`.
    pub loss: f64,
    pub loss_percent: f64,
    /// Value of the lines whose beta could not be measured (left out of the total, never guessed).
    pub uncovered_value: f64,
    pub uncovered: Vec<String>,
    pub worst: Option<WhatIfLine>,
    /// Shortest and longest window of the betas used (days).
    pub min_days: Option<usize>,
    pub max_days: Option<usize>,
}

/// Each line moves by its beta to the factor × the shock (never below −100 %); cash does not move. Lines of the same
/// asset are merged. With `amount` (> 0), the lines are rescaled to that amount on the current weights (cash included).
pub fn what_if(a: &PortfolioAnalysis, factor: FactorKey, shock: f64, betas: &HashMap<String, FactorBeta>, amount: Option<f64>) -> WhatIfResult {
    let f = factor.factor();
    let scaled = amount.is_some_and(|x| x.is_finite() && x > 0.0) && a.total > 0.0;
    let k = if scaled { amount.unwrap_or(0.0) / a.total } else { 1.0 };
    let base = if scaled { amount.unwrap_or(0.0) } else { a.total };
    struct Merged {
        symbol: String,
        name: String,
        kind: Kind,
        value: f64,
    }
    let mut merged: IndexMap<String, Merged> = IndexMap::new();
    for l in &a.lines {
        let key = market_key(l.kind, &l.symbol);
        let prev = merged.get(&key).map(|m| m.value).unwrap_or(0.0);
        merged.insert(key, Merged { symbol: l.symbol.clone(), name: l.name.clone(), kind: l.kind, value: prev + l.value * k });
    }
    let mut lines: Vec<WhatIfLine> = merged
        .into_iter()
        .map(|(key, m)| {
            let reference = m.symbol == f.symbol && m.kind == f.kind;
            let b = betas.get(&key);
            let beta = if reference { Some(1.0) } else { b.filter(|b| b.estimated).map(|b| b.beta) };
            let move_percent = beta.map(|beta| (beta * shock).max(-100.0));
            WhatIfLine {
                weight: if base > 0.0 { m.value / base * 100.0 } else { 0.0 },
                beta,
                days: if reference { 0 } else { b.map(|b| b.days).unwrap_or(0) },
                correlation: if reference { Some(1.0) } else { b.and_then(|b| b.correlation) },
                reference,
                move_percent,
                loss: move_percent.map(|mv| -m.value * mv / 100.0),
                key,
                symbol: m.symbol,
                name: m.name,
                kind: m.kind,
                value: m.value,
            }
        })
        .collect();
    // (y.loss ?? −∞) − (x.loss ?? −∞) || y.value − x.value
    lines.sort_by(|x, y| {
        let d = y.loss.unwrap_or(f64::NEG_INFINITY) - x.loss.unwrap_or(f64::NEG_INFINITY);
        let d = if d.is_nan() || d == 0.0 { y.value - x.value } else { d };
        d.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
    });
    let covered: Vec<&WhatIfLine> = lines.iter().filter(|l| l.loss.is_some()).collect();
    let loss: f64 = covered.iter().map(|l| l.loss.unwrap_or(0.0)).sum();
    let worst = covered.iter().fold(None::<&WhatIfLine>, |w, l| {
        let ll = l.loss.unwrap_or(0.0);
        if ll > 0.0 && w.is_none_or(|w| ll > w.loss.unwrap_or(0.0)) { Some(l) } else { w }
    });
    let measured: Vec<usize> = covered.iter().filter(|l| !l.reference).map(|l| l.days).collect();
    WhatIfResult {
        factor,
        shock,
        base,
        scaled,
        cash: a.cash * k,
        loss,
        loss_percent: if base > 0.0 { loss / base * 100.0 } else { 0.0 },
        uncovered_value: lines.iter().filter(|l| l.loss.is_none()).map(|l| l.value).sum(),
        uncovered: lines.iter().filter(|l| l.loss.is_none()).map(|l| l.symbol.clone()).collect(),
        worst: worst.cloned(),
        min_days: measured.iter().min().copied(),
        max_days: measured.iter().max().copied(),
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::super::holdings::{Holding, LineSignal, MarketInput, analyze_portfolio};
    use super::*;
    use crate::engine::reliability::ReliabilityLevel;
    use crate::engine::signal::Action;
    use crate::web::store::DEFAULT_RISK;

    const DAY: i64 = 86_400_000;

    fn close(a: f64, b: f64, digits: i32) {
        assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≠ {b}");
    }

    /// Deterministic pseudo-random daily returns (± `amp` / 2).
    fn returns(n: usize, seed: f64, amp: f64) -> Vec<f64> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x = (x * 16807.0) % 2_147_483_647.0;
                (x / 2_147_483_647.0 - 0.5) * amp
            })
            .collect()
    }

    fn path(r: &[f64], start: f64) -> Vec<f64> {
        let mut out = vec![start];
        for x in r {
            let last = out[out.len() - 1];
            out.push(last * (1.0 + x));
        }
        out
    }

    fn h(id: &str, symbol: &str, kind: Kind, quantity: f64, average_price: f64, stop: Option<f64>) -> Holding {
        Holding { id: id.into(), symbol: symbol.into(), kind, name: symbol.into(), quantity, average_price, stop }
    }

    fn m(price: f64, daily: &[Candle]) -> MarketInput {
        let hold = Some(LineSignal { action: Action::Hold, score: 0.0 });
        MarketInput { price, daily: daily.to_vec(), day_signal: hold, short_signal: hold, reliability: Some(ReliabilityLevel::High) }
    }

    mod risk {
        use super::*;

        /// `Date.UTC(2026, 0, 1)`.
        const T0: i64 = 1_767_225_600_000;

        /// Daily candles from closes (high/low ± 1 %).
        fn series(closes: &[f64], start: i64) -> Vec<Candle> {
            closes
                .iter()
                .enumerate()
                .map(|(i, c)| Candle { time: start + i as i64 * DAY, open: *c, high: c * 1.01, low: c * 0.99, close: *c, volume: 1.0 })
                .collect()
        }

        fn ret(n: usize, seed: f64) -> Vec<f64> {
            returns(n, seed, 0.06)
        }

        #[test]
        fn beta_of_twice_the_benchmark() {
            let bench = ret(120, 7.0);
            let twice: Vec<f64> = bench.iter().map(|x| 2.0 * x).collect();
            let b = estimate_beta(&series(&path(&twice, 100.0), T0), &series(&path(&bench, 100.0), T0), BETA_DAYS);
            assert!(b.estimated);
            assert_eq!(b.days, 90);
            close(b.beta, 2.0, 6);
        }

        #[test]
        fn short_history_falls_back_to_one() {
            let bench = ret(120, 7.0);
            // A recent listing: 20 days shared with the benchmark.
            let b = estimate_beta(&series(&path(&bench[..20], 100.0), T0 + 100 * DAY), &series(&path(&bench, 100.0), T0), BETA_DAYS);
            assert_eq!(b, BetaEstimate { beta: 1.0, days: 0, estimated: false, reference: false });
        }

        fn stress_portfolio() -> PortfolioAnalysis {
            let holdings = [h("1", "ETH", Kind::Crypto, 1.0, 1000.0, None), h("2", "AAPL", Kind::Stock, 10.0, 100.0, None)];
            analyze_portfolio(&holdings, 1000.0, &HashMap::from([("crypto:ETH".into(), m(1000.0, &[])), ("stock:AAPL".into(), m(100.0, &[]))]))
        }

        fn betas(beta: f64) -> HashMap<String, BetaEstimate> {
            HashMap::from([("crypto:ETH".into(), BetaEstimate { beta, days: 90, estimated: true, reference: false })])
        }

        #[test]
        fn stress_beta_times_shock() {
            let a = stress_portfolio();
            let r = stress_test(&a, &betas(1.5), &STRESS_SCENARIOS);
            let all10 = r.iter().find(|x| x.scenario.key == "all10").unwrap();
            // ETH 1000 × 1.5 × 10 % + AAPL 1000 × 1 × 10 % (no beta: 1)
            close(all10.loss, 250.0, 9);
            close(all10.loss_percent, 250.0 / 3000.0 * 100.0, 9);
            close(all10.invested_loss_percent, 250.0 / 2000.0 * 100.0, 9);
            assert_eq!(all10.worst.as_ref().unwrap().symbol, "ETH");
            // Mixed scenarios: crypto only, then stocks −10 % with crypto −30 %.
            close(r.iter().find(|x| x.scenario.key == "crypto20").unwrap().loss, 300.0, 9);
            close(r.iter().find(|x| x.scenario.key == "stock10crypto30").unwrap().loss, 450.0 + 100.0, 9);
            assert_eq!(r.iter().map(|x| x.scenario.key).collect::<Vec<_>>(), STRESS_SCENARIOS.iter().map(|s| s.key).collect::<Vec<_>>());
            // A line never loses more than its value.
            let big = stress_test(&a, &betas(5.0), &STRESS_SCENARIOS);
            close(big.iter().find(|x| x.scenario.key == "all30").unwrap().loss, 1000.0 + 300.0, 9);
        }

        fn limit_series() -> (Vec<Candle>, Vec<Candle>, Vec<Candle>) {
            let bench = ret(120, 11.0);
            let eth = series(&path(&bench, 100.0), T0);
            let sol_r: Vec<f64> = bench.iter().enumerate().map(|(i, x)| x * 1.2 + if i % 2 == 1 { 0.001 } else { -0.001 }).collect();
            let sol = series(&path(&sol_r, 100.0), T0);
            let aapl = series(&path(&ret(120, 99.0), 100.0), T0);
            (eth, sol, aapl)
        }

        #[test]
        fn correlated_cluster() {
            let (eth, sol, aapl) = limit_series();
            let c = correlated_clusters(
                &[
                    ClusterInput { symbol: "ETH".into(), weight: 30.0, daily: &eth },
                    ClusterInput { symbol: "SOL".into(), weight: 25.0, daily: &sol },
                    ClusterInput { symbol: "AAPL".into(), weight: 20.0, daily: &aapl },
                ],
                BETA_DAYS,
            );
            assert_eq!(c.len(), 1);
            let mut s = c[0].symbols.clone();
            s.sort();
            assert_eq!(s, vec!["ETH", "SOL"]);
            assert_eq!(c[0].weight, 55.0);
            assert!(c[0].average_correlation > 0.7);
        }

        #[test]
        fn limits_against_the_settings() {
            let (eth, sol, aapl) = limit_series();
            let now = T0 + 120 * DAY + 3_600_000;
            let last = |c: &[Candle]| c[c.len() - 1].close;
            let prev = |c: &[Candle]| c[c.len() - 2].close;
            let holdings = [
                h("1", "ETH", Kind::Crypto, 1.0, last(&eth), None),
                h("2", "SOL", Kind::Crypto, 1.0, last(&sol), None),
                h("3", "AAPL", Kind::Stock, 1.0, last(&aapl), None),
            ];
            // Prices 10 % under the previous close (the last candle is today's): a big daily loss.
            let a = analyze_portfolio(
                &holdings,
                0.0,
                &HashMap::from([
                    ("crypto:ETH".into(), m(prev(&eth) * 0.9, &eth)),
                    ("crypto:SOL".into(), m(prev(&sol) * 0.9, &sol)),
                    ("stock:AAPL".into(), m(prev(&aapl) * 0.9, &aapl)),
                ]),
            );
            let daily: HashMap<String, Vec<Candle>> =
                HashMap::from([("crypto:ETH".into(), eth.clone()), ("crypto:SOL".into(), sol.clone()), ("stock:AAPL".into(), aapl.clone())]);
            let d = daily_change(&a, &daily, now).unwrap();
            close(d.percent, -10.0, 6);
            assert_eq!(d.covered, 3);
            let inputs: Vec<ClusterInput> = a
                .lines
                .iter()
                .map(|l| ClusterInput { symbol: l.symbol.clone(), weight: l.weight, daily: &daily[&market_key(l.kind, &l.symbol)] })
                .collect();
            let clusters = correlated_clusters(&inputs, BETA_DAYS);
            let s = RiskSettings { max_position_percent: 20.0, max_crypto_percent: 30.0, ..DEFAULT_RISK };
            let checks = check_limits(&a, &s, &clusters, Some(&d), &HashMap::new());
            let by = |code: &str| checks.iter().find(|c| c.code == code).unwrap().clone();
            assert_eq!(by("max_weight").level, LimitLevel::Danger);
            assert_eq!(by("crypto_cap").level, LimitLevel::Warning);
            assert_eq!(by("daily_loss").level, LimitLevel::Danger);
            assert!(by("daily_loss").detail.contains(DAILY_LOSS_REACHED));
            if a.allocation.crypto > 40.0 && clusters.iter().any(|c| c.weight > 40.0) {
                assert_eq!(by("cluster").level, LimitLevel::Warning);
            }
            let calm_s = RiskSettings {
                max_position_percent: 100.0,
                max_crypto_percent: 100.0,
                daily_loss_limit_percent: 50.0,
                risk_per_trade_percent: 100.0,
                ..DEFAULT_RISK
            };
            let calm = check_limits(&a, &calm_s, &[], Some(&d), &HashMap::new());
            assert!(calm.iter().all(|c| c.level == LimitLevel::Ok));
            let na = check_limits(&a, &DEFAULT_RISK, &[], None, &HashMap::new());
            assert_eq!(na.iter().find(|c| c.code == "daily_loss").unwrap().level, LimitLevel::Na);
        }

        #[test]
        fn previous_close_of_yesterday_or_last() {
            let c = series(&[10.0, 11.0, 12.0], T0);
            assert_eq!(previous_close(&c, T0 + 2 * DAY + 5_000), Some(11.0));
            assert_eq!(previous_close(&c, T0 + 3 * DAY + 5_000), Some(12.0));
            assert_eq!(previous_close(&[], T0), None);
        }

        #[test]
        fn dangerous_positions_found() {
            let closes: Vec<f64> = (0..30).map(|i| 100.0 + (i % 2) as f64).collect();
            let c = series(&closes, T0);
            let holdings = [
                h("a", "BTC", Kind::Crypto, 1.0, 100.0, Some(101.5)),
                h("b", "ETH", Kind::Crypto, 1.0, 100.0, Some(99.5)),
                h("c", "AAPL", Kind::Stock, 1.0, 200.0, None),
                h("d", "MSFT", Kind::Stock, 1.0, 50.0, Some(10.0)),
            ];
            let market: HashMap<String, MarketInput> =
                ["crypto:BTC", "crypto:ETH", "stock:AAPL", "stock:MSFT"].iter().map(|k| (k.to_string(), m(101.0, &c))).collect();
            let a = analyze_portfolio(&holdings, 10_000.0, &market);
            let stops: HashMap<String, f64> = holdings.iter().filter_map(|h| h.stop.map(|s| (h.id.clone(), s))).collect();
            let daily: HashMap<String, Vec<Candle>> = market.iter().map(|(k, v)| (k.clone(), v.daily.clone())).collect();
            let d = dangerous_positions(&a, &DEFAULT_RISK, &daily, &stops);
            let by = |s: &str| d.iter().find(|x| x.symbol == s).map(|x| x.reasons.iter().map(|r| r.code).collect::<Vec<_>>());
            assert_eq!(by("BTC"), Some(vec![DangerCode::StopBroken]));
            assert_eq!(by("ETH"), Some(vec![DangerCode::NearStop]));
            // AAPL: −99 $ latent on a 10 404 $ portfolio (1 % = 104 $): not yet; with 0.5 % it is.
            assert_eq!(by("AAPL"), None);
            assert_eq!(by("MSFT"), None);
            let strict = dangerous_positions(&a, &RiskSettings { risk_per_trade_percent: 0.5, ..DEFAULT_RISK }, &HashMap::new(), &HashMap::new());
            assert_eq!(strict.iter().map(|x| x.symbol.as_str()).collect::<Vec<_>>(), vec!["AAPL"]);
            assert_eq!(d[0].reasons[0].text, "Stop cassé : cours 101 $ sous votre stop 101,5 $.");
            assert_eq!(strict[0].reasons[0].text, "Perte latente de 99 $ (1 % du patrimoine), au-delà de votre risque accepté par idée (0,5 %).");
        }
    }

    mod whatif {
        use super::*;

        /// `Date.UTC(2026, 0, 5)`, a Monday.
        const T0: i64 = 1_767_571_200_000;

        fn series(closes: &[f64], times: &[i64]) -> Vec<Candle> {
            closes.iter().zip(times).map(|(c, t)| Candle { time: *t, open: *c, high: *c, low: *c, close: *c, volume: 1.0 }).collect()
        }
        fn ret(n: usize, seed: f64) -> Vec<f64> {
            returns(n, seed, 0.04)
        }
        fn every_day(n: usize) -> Vec<i64> {
            (0..n as i64).map(|i| T0 + i * DAY).collect()
        }
        /// Weekday of day i after T0 (a Monday): Saturday and Sunday are 5 and 6.
        fn weekdays(n: usize) -> Vec<i64> {
            every_day((n as f64 * 1.5).ceil() as usize).into_iter().filter(|t| (t - T0) / DAY % 7 < 5).take(n).collect()
        }

        #[test]
        fn factor_beta_on_shared_days_only() {
            let days = weekdays(121);
            let q = ret(120, 11.0);
            let factor = series(&path(&q, 100.0), &days);
            // The crypto moves 1.5 × QQQ on weekdays and has extra weekend candles with big moves.
            let crypto_times = every_day((121.0_f64 * 1.5).ceil() as usize);
            let q15: Vec<f64> = q.iter().map(|x| 1.5 * x).collect();
            let p15 = path(&q15, 100.0);
            let by_day: HashMap<i64, f64> = days.iter().enumerate().map(|(i, t)| (*t, p15[i])).collect();
            let mut last = 100.0;
            let closes: Vec<f64> = crypto_times
                .iter()
                .map(|t| match by_day.get(t) {
                    Some(v) => {
                        last = *v;
                        last
                    }
                    None => last * 1.03,
                })
                .collect();
            let crypto = series(&closes, &crypto_times);
            let b = factor_beta(&crypto, &factor, BETA_DAYS);
            assert!(b.estimated);
            assert_eq!(b.days, 90);
            close(b.beta, 1.5, 6);
            close(b.correlation.unwrap(), 1.0, 6);
        }

        #[test]
        fn under_thirty_days_not_estimated() {
            let t = every_day(20);
            let b = factor_beta(&series(&path(&ret(19, 3.0), 100.0), &t), &series(&path(&ret(19, 5.0), 100.0), &t), BETA_DAYS);
            assert!(!b.estimated);
            assert_eq!(b.days, 19);
            assert_eq!(MIN_BETA_DAYS, 30);
        }

        fn portfolio() -> PortfolioAnalysis {
            let hh = |id: &str, symbol: &str, kind: Kind, q: f64| h(id, symbol, kind, q, 100.0, None);
            let mm =
                |price: f64| MarketInput { price, daily: vec![], day_signal: None, short_signal: None, reliability: Some(ReliabilityLevel::High) };
            analyze_portfolio(
                &[
                    hh("1", "NVDA", Kind::Stock, 30.0),
                    hh("2", "NVDA", Kind::Stock, 10.0),
                    hh("3", "QQQ", Kind::Stock, 20.0),
                    hh("4", "DOGE", Kind::Crypto, 20.0),
                ],
                2_000.0,
                &HashMap::from([("stock:NVDA".into(), mm(100.0)), ("stock:QQQ".into(), mm(100.0)), ("crypto:DOGE".into(), mm(100.0))]),
            )
        }

        fn fb(beta: f64, days: usize, estimated: bool, correlation: Option<f64>) -> FactorBeta {
            FactorBeta { beta, days, estimated, correlation }
        }

        #[test]
        fn each_line_moves_by_its_beta() {
            let a = portfolio();
            let betas = HashMap::from([("stock:NVDA".into(), fb(2.0, 90, true, Some(0.8))), ("crypto:DOGE".into(), fb(1.0, 12, false, None))]);
            let r = what_if(&a, FactorKey::Qqq, -10.0, &betas, None);
            assert_eq!(r.base, 10_000.0);
            let line = |s: &str| r.lines.iter().find(|l| l.symbol == s).unwrap();
            let nvda = line("NVDA");
            assert_eq!((nvda.value, nvda.move_percent, nvda.loss, nvda.beta), (4_000.0, Some(-20.0), Some(800.0), Some(2.0)));
            assert!(line("QQQ").reference);
            assert_eq!(line("QQQ").loss, Some(200.0));
            assert_eq!((line("DOGE").loss, line("DOGE").move_percent), (None, None));
            assert_eq!(r.loss, 1_000.0);
            assert_eq!(r.loss_percent, 10.0);
            assert_eq!(r.uncovered, vec!["DOGE"]);
            assert_eq!(r.uncovered_value, 2_000.0);
            assert_eq!(r.worst.as_ref().unwrap().symbol, "NVDA");
            assert_eq!((r.min_days, r.max_days), (Some(90), Some(90)));
        }

        #[test]
        fn amount_spread_on_the_weights() {
            let a = portfolio();
            let r = what_if(&a, FactorKey::Qqq, -50.0, &HashMap::from([("stock:NVDA".into(), fb(3.0, 60, true, Some(0.9)))]), Some(1_000.0));
            assert!(r.scaled);
            assert_eq!(r.cash, 200.0);
            let nvda = r.lines.iter().find(|l| l.symbol == "NVDA").unwrap();
            assert_eq!((nvda.value, nvda.move_percent, nvda.loss), (400.0, Some(-100.0), Some(400.0)));
            assert_eq!(r.loss, 500.0);
            assert_eq!(r.loss_percent, 50.0);
        }

        #[test]
        fn negative_beta_gains() {
            let a = portfolio();
            let betas = HashMap::from([("stock:NVDA".into(), fb(-0.5, 90, true, Some(-0.3))), ("stock:QQQ".into(), fb(1.2, 90, true, Some(0.95)))]);
            let r = what_if(&a, FactorKey::Spy, -10.0, &betas, None);
            assert_eq!(r.lines.iter().find(|l| l.symbol == "NVDA").unwrap().loss, Some(-200.0));
            assert_eq!(r.worst.as_ref().unwrap().symbol, "QQQ");
        }
    }
}

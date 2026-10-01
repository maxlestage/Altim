//! Analysis of the portfolio the user actually holds (`web/src/engine/holdings.ts`): value, gains, weights, a
//! recommendation per line, risk figures (volatility, 1-day VaR, correlation) and plain-language insights.
//! Pure, deterministic functions (same inputs, same results). Amounts in dollars; texts through `crate::fx`.
use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::engine::reliability::ReliabilityLevel;
use crate::engine::signal::{Action, Candle, atr, is_buy, is_sell};
use crate::js::{fr, iso_date, number_to_string, to_fixed};
use crate::types::Kind;
pub use crate::web::store::Holding;

/// Signal of one timeframe as the radar gives it (`{ action, score }`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineSignal {
    pub action: Action,
    pub score: f64,
}

/// Market data of one asset (`MarketInput`), keyed `kind:symbol`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarketInput {
    pub price: f64,
    /// Daily candles (volatility, correlation, protective stop).
    pub daily: Vec<Candle>,
    pub day_signal: Option<LineSignal>,
    pub short_signal: Option<LineSignal>,
    pub reliability: Option<ReliabilityLevel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Recommendation {
    Sell,
    Protect,
    Lighten,
    Strengthen,
    Hold,
    Unknown,
}

impl Recommendation {
    /// `RECOMMENDATION_LABEL`.
    pub fn label(self) -> &'static str {
        match self {
            Recommendation::Sell => "Vendre ou protéger",
            Recommendation::Protect => "Protéger (stop)",
            Recommendation::Lighten => "Alléger",
            Recommendation::Strengthen => "Renforcer possible",
            Recommendation::Hold => "Conserver",
            Recommendation::Unknown => "Données insuffisantes",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InsightLevel {
    Danger,
    Warning,
    Info,
    Good,
}

impl InsightLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            InsightLevel::Danger => "danger",
            InsightLevel::Warning => "warning",
            InsightLevel::Info => "info",
            InsightLevel::Good => "good",
        }
    }
    fn rank(self) -> usize {
        match self {
            InsightLevel::Danger => 0,
            InsightLevel::Warning => 1,
            InsightLevel::Good => 2,
            InsightLevel::Info => 3,
        }
    }
}

/// `{ level, code, values }`: the values of the text, numbers or strings (same JSON as the TypeScript).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Insight {
    pub level: InsightLevel,
    pub code: String,
    pub values: Map<String, Value>,
}

impl Insight {
    pub fn new(level: InsightLevel, code: &str, values: &[(&str, Value)]) -> Self {
        Insight { level, code: code.into(), values: values.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect() }
    }
    /// A number of the values (NaN when absent).
    pub fn num(&self, k: &str) -> f64 {
        self.values.get(k).and_then(Value::as_f64).unwrap_or(f64::NAN)
    }
    /// A value as `${v}` writes it in the TypeScript templates.
    pub fn text(&self, k: &str) -> String {
        match self.values.get(k) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => number_to_string(n.as_f64().unwrap_or(f64::NAN)),
            Some(v) => v.to_string(),
            None => "undefined".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineAnalysis {
    pub id: String,
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    pub quantity: f64,
    pub average_price: f64,
    pub price: Option<f64>,
    pub value: f64,
    pub invested: f64,
    pub pnl: f64,
    pub pnl_percent: f64,
    /// % of the total (cash included).
    pub weight: f64,
    /// Protective stop: price − 2 × daily ATR.
    pub stop: Option<f64>,
    pub loss_at_stop: Option<f64>,
    pub recommendation: Recommendation,
    /// Reason codes (`REASON_TEXT`).
    pub reasons: Vec<String>,
    /// Suggested sale amount for "lighten" (USD), otherwise 0.
    pub trim_value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Allocation {
    pub crypto: f64,
    pub stock: f64,
    pub cash: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioRisk {
    pub max_weight: f64,
    pub effective_assets: f64,
    pub volatility_annual: Option<f64>,
    /// Loss (USD) exceeded 1 day out of 20, positive.
    pub var95_day: Option<f64>,
    pub var95_day_percent: Option<f64>,
    pub average_correlation: Option<f64>,
    pub loss_at_stops: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioAnalysis {
    pub total: f64,
    pub cash: f64,
    pub invested: f64,
    pub market_value: f64,
    pub pnl: f64,
    pub pnl_percent: f64,
    pub allocation: Allocation,
    pub lines: Vec<LineAnalysis>,
    pub risk: PortfolioRisk,
    pub insights: Vec<Insight>,
}

pub const MAX_WEIGHT: f64 = 35.0;
pub const TARGET_WEIGHT: f64 = 30.0;

fn is_buy_opt(a: Option<Action>) -> bool {
    a.is_some_and(is_buy)
}
fn is_sell_opt(a: Option<Action>) -> bool {
    a.is_some_and(is_sell)
}

/// Daily closes indexed by UTC date (crypto and stocks aligned by calendar day); the last close of a day wins.
fn closes_by_day(c: &[Candle]) -> HashMap<String, f64> {
    let mut sorted = c.to_vec();
    sorted.sort_by_key(|x| x.time);
    let mut m = HashMap::new();
    for x in sorted {
        m.insert(iso_date(x.time), x.close);
    }
    m
}

/// Daily returns over the last `days` shared calendar days, carrying the price forward when a market is closed.
pub fn aligned_returns(series: &[&[Candle]], days: usize) -> Vec<Vec<f64>> {
    let maps: Vec<HashMap<String, f64>> = series.iter().map(|s| closes_by_day(s)).collect();
    let all: Vec<String> = maps.iter().flat_map(|m| m.keys().cloned()).collect::<BTreeSet<_>>().into_iter().collect();
    let first_keys: Vec<Option<&String>> = maps.iter().map(|m| m.keys().min()).collect();
    let Some(start) = all.iter().position(|d| first_keys.iter().all(|k| k.is_some_and(|k| k <= d))) else {
        return series.iter().map(|_| Vec::new()).collect();
    };
    let tail = &all[start..];
    let dates = &tail[tail.len().saturating_sub(days + 1)..];
    maps.iter()
        .map(|m| {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let mut last = f64::NAN;
            for k in keys {
                if *k <= dates[0] {
                    last = m[k];
                }
            }
            let closes: Vec<f64> = dates
                .iter()
                .map(|d| {
                    if let Some(v) = m.get(d) {
                        last = *v;
                    }
                    last
                })
                .collect();
            closes.windows(2).map(|w| if w[0] > 0.0 { w[1] / w[0] - 1.0 } else { 0.0 }).collect()
        })
        .collect()
}

/// `x > 0` (false for NaN): the TypeScript's `!(x > 0)` guards read `!positive(x)`.
pub fn positive(x: f64) -> bool {
    x > 0.0
}

pub(crate) fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

fn std(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return 0.0;
    }
    let m = mean(v);
    (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt()
}

/// Pearson correlation of the last common values (at least 10), None when undefined.
pub fn correlation(a: &[f64], b: &[f64]) -> Option<f64> {
    let n = a.len().min(b.len());
    if n < 10 {
        return None;
    }
    let x = &a[a.len() - n..];
    let y = &b[b.len() - n..];
    let (mx, my) = (mean(x), mean(y));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        sxy += (x[i] - mx) * (y[i] - my);
        sxx += (x[i] - mx).powi(2);
        syy += (y[i] - my).powi(2);
    }
    (sxx > 0.0 && syy > 0.0).then(|| sxy / (sxx * syy).sqrt())
}

/// Percentile with linear interpolation (`p` = 0.05 for the 5th).
pub fn percentile(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if s.is_empty() {
        return 0.0;
    }
    let idx = (s.len() - 1) as f64 * p;
    let (lo, hi) = (idx.floor() as usize, idx.ceil() as usize);
    s[lo] + (s[hi] - s[lo]) * (idx - lo as f64)
}

/// `kind:symbol`, the key of `market`.
pub fn market_key(kind: Kind, symbol: &str) -> String {
    crate::web::store::asset_key(symbol, kind)
}

pub fn analyze_portfolio(holdings: &[Holding], cash: f64, market: &HashMap<String, MarketInput>) -> PortfolioAnalysis {
    struct Base<'a> {
        h: &'a Holding,
        m: Option<&'a MarketInput>,
        price: Option<f64>,
        value: f64,
        invested: f64,
    }
    let base: Vec<Base> = holdings
        .iter()
        .map(|h| {
            let m = market.get(&market_key(h.kind, &h.symbol));
            let price = m.filter(|m| m.price > 0.0).map(|m| m.price);
            Base { h, m, price, value: h.quantity * price.unwrap_or(h.average_price), invested: h.quantity * h.average_price }
        })
        .collect();
    let market_value: f64 = base.iter().map(|b| b.value).sum();
    let safe_cash = cash.max(0.0);
    let total = market_value + safe_cash;
    let invested_total: f64 = base.iter().map(|b| b.invested).sum();
    let pnl = market_value - invested_total;
    let many = holdings.len() > 1;

    let lines: Vec<LineAnalysis> = base
        .iter()
        .map(|b| {
            let (h, m, price, value, invested) = (b.h, b.m, b.price, b.value, b.invested);
            let weight = if total > 0.0 { value / total * 100.0 } else { 0.0 };
            let pnl_l = value - invested;
            let pnl_percent = if invested > 0.0 { pnl_l / invested * 100.0 } else { 0.0 };
            let a = m.filter(|m| !m.daily.is_empty()).and_then(|m| atr(&m.daily, 14)[m.daily.len() - 1]);
            // `price && a`: both known and non-zero.
            let stop = match (price, a) {
                (Some(p), Some(a)) if p != 0.0 && a != 0.0 => Some((p - 2.0 * a).max(p * 0.01)),
                _ => None,
            };
            let loss_at_stop = match (stop, price) {
                (Some(s), Some(p)) if s != 0.0 => Some(h.quantity * (p - s)),
                _ => None,
            };
            let mut reasons: Vec<String> = Vec::new();
            let day = m.and_then(|m| m.day_signal).map(|s| s.action);
            let short = m.and_then(|m| m.short_signal).map(|s| s.action);
            let unreliable = m.is_none_or(|m| m.reliability == Some(ReliabilityLevel::Low) || (m.day_signal.is_none() && m.short_signal.is_none()));
            let rec = if price.is_none() || unreliable {
                reasons.push(if price.is_none() { "no_price" } else { "unreliable" }.into());
                Recommendation::Unknown
            } else if is_sell_opt(day) && is_sell_opt(short) {
                reasons.push("bearish_day_and_short".into());
                Recommendation::Sell
            } else if is_sell_opt(day) || short == Some(Action::StrongSell) {
                reasons.push(if is_sell_opt(day) { "bearish_day" } else { "strong_bearish_short" }.into());
                Recommendation::Protect
            } else if weight > MAX_WEIGHT && many {
                reasons.push("overweight".into());
                Recommendation::Lighten
            } else if pnl_percent > 50.0 && !is_buy_opt(day) {
                reasons.push("take_profit".into());
                Recommendation::Lighten
            } else if is_buy_opt(day) && is_buy_opt(short) && weight < 20.0 {
                reasons.push("bullish_day_and_short".into());
                Recommendation::Strengthen
            } else {
                reasons.push(if is_buy_opt(day) { "bullish_day" } else { "neutral" }.into());
                Recommendation::Hold
            };
            if rec != Recommendation::Lighten && weight > MAX_WEIGHT && many {
                reasons.push("overweight".into());
            }
            if pnl_percent < -20.0 {
                reasons.push("deep_loss".into());
            }
            let trim_value = if rec == Recommendation::Lighten {
                if reasons[0] == "overweight" { (value - TARGET_WEIGHT / 100.0 * total).max(0.0) } else { value * 0.25 }
            } else {
                0.0
            };
            LineAnalysis {
                id: h.id.clone(),
                symbol: h.symbol.clone(),
                kind: h.kind,
                name: h.name.clone(),
                quantity: h.quantity,
                average_price: h.average_price,
                price,
                value,
                invested,
                pnl: pnl_l,
                pnl_percent,
                weight,
                stop,
                loss_at_stop,
                recommendation: rec,
                reasons,
                trim_value,
            }
        })
        .collect();

    let crypto: f64 = lines.iter().filter(|l| l.kind == Kind::Crypto).map(|l| l.value).sum();
    let stock: f64 = lines.iter().filter(|l| l.kind == Kind::Stock).map(|l| l.value).sum();
    let pct = |v: f64| if total > 0.0 { v / total * 100.0 } else { 0.0 };
    let hhi: f64 = lines.iter().map(|l| (l.weight / 100.0).powi(2)).sum();
    let max_weight = lines.iter().fold(0.0_f64, |a, l| a.max(l.weight));

    // Volatility, VaR and correlation from aligned daily returns.
    let with_data: Vec<(&LineAnalysis, &[Candle])> =
        lines.iter().zip(&base).map(|(l, b)| (l, b.m.map(|m| m.daily.as_slice()).unwrap_or(&[]))).filter(|(_, d)| d.len() >= 20).collect();
    let (mut volatility_annual, mut var95_day, mut average_correlation) = (None, None, None);
    if !with_data.is_empty() && total > 0.0 {
        let returns = aligned_returns(&with_data.iter().map(|x| x.1).collect::<Vec<_>>(), 90);
        let n = returns.iter().map(Vec::len).min().unwrap_or(0);
        if n >= 20 {
            let port: Vec<f64> = (0..n)
                .map(|t| with_data.iter().enumerate().fold(0.0, |a, (i, x)| a + (x.0.value / total) * returns[i][returns[i].len() - n + t]))
                .collect();
            volatility_annual = Some(std(&port) * 365f64.sqrt() * 100.0);
            var95_day = Some((-percentile(&port, 0.05) * total).max(0.0));
            let mut pairs = Vec::new();
            for i in 0..returns.len() {
                for j in i + 1..returns.len() {
                    if let Some(c) = correlation(&returns[i], &returns[j]) {
                        pairs.push(c);
                    }
                }
            }
            average_correlation = (!pairs.is_empty()).then(|| mean(&pairs));
        }
    }
    let loss_at_stops: f64 = lines.iter().map(|l| l.loss_at_stop.unwrap_or(0.0)).sum();

    let mut insights: Vec<Insight> = Vec::new();
    let mut add = |level: InsightLevel, code: &str, values: &[(&str, Value)]| insights.push(Insight::new(level, code, values));
    let symbols = |ls: &[&LineAnalysis]| json!(ls.iter().map(|l| l.symbol.as_str()).collect::<Vec<_>>().join(", "));
    if lines.is_empty() {
        add(InsightLevel::Info, "empty", &[]);
    } else {
        let pnl_percent = if invested_total > 0.0 { pnl / invested_total * 100.0 } else { 0.0 };
        add(if pnl >= 0.0 { InsightLevel::Good } else { InsightLevel::Warning }, "pnl", &[("pnl", json!(pnl)), ("pnlPercent", json!(pnl_percent))]);
        let sells: Vec<&LineAnalysis> = lines.iter().filter(|l| matches!(l.recommendation, Recommendation::Sell | Recommendation::Protect)).collect();
        if !sells.is_empty() {
            add(InsightLevel::Danger, "act_bearish", &[("count", json!(sells.len())), ("symbols", symbols(&sells))]);
        }
        let top = lines.iter().fold(&lines[0], |a, l| if l.weight > a.weight { l } else { a });
        if many && top.weight > 40.0 {
            add(InsightLevel::Danger, "concentration", &[("symbol", json!(top.symbol)), ("weight", json!(top.weight))]);
        } else if many && top.weight > 25.0 {
            add(InsightLevel::Warning, "concentration", &[("symbol", json!(top.symbol)), ("weight", json!(top.weight))]);
        } else if lines.len() == 1 {
            add(InsightLevel::Warning, "single_asset", &[("symbol", json!(top.symbol))]);
        }
        if pct(crypto) > 60.0 {
            add(InsightLevel::Warning, "crypto_heavy", &[("weight", json!(pct(crypto)))]);
        }
        if many && hhi > 0.0 && 1.0 / hhi < 3.0 {
            add(InsightLevel::Warning, "low_diversification", &[("effective", json!(1.0 / hhi))]);
        }
        if let Some(c) = average_correlation.filter(|c| *c > 0.7 && many) {
            add(InsightLevel::Warning, "correlated", &[("correlation", json!(c))]);
        }
        if let Some(v) = var95_day {
            add(InsightLevel::Info, "var", &[("amount", json!(v)), ("percent", json!(if total > 0.0 { v / total * 100.0 } else { 0.0 }))]);
        }
        if loss_at_stops > 0.0 {
            let percent = if total > 0.0 { loss_at_stops / total * 100.0 } else { 0.0 };
            add(InsightLevel::Info, "stops", &[("amount", json!(loss_at_stops)), ("percent", json!(percent))]);
        }
        if pct(safe_cash) < 5.0 {
            add(InsightLevel::Info, "low_cash", &[("weight", json!(pct(safe_cash)))]);
        }
        let deep: Vec<&LineAnalysis> = lines.iter().filter(|l| l.pnl_percent < -20.0).collect();
        if !deep.is_empty() {
            add(InsightLevel::Warning, "deep_loss", &[("symbols", symbols(&deep))]);
        }
        let strong: Vec<&LineAnalysis> = lines.iter().filter(|l| l.recommendation == Recommendation::Strengthen).collect();
        if !strong.is_empty() {
            add(InsightLevel::Good, "opportunities", &[("symbols", symbols(&strong))]);
        }
        let unknown: Vec<&LineAnalysis> = lines.iter().filter(|l| l.recommendation == Recommendation::Unknown).collect();
        if !unknown.is_empty() {
            add(InsightLevel::Warning, "unknown", &[("symbols", symbols(&unknown))]);
        }
    }
    insights.sort_by_key(|i| i.level.rank());

    PortfolioAnalysis {
        total,
        cash: safe_cash,
        invested: invested_total,
        market_value,
        pnl,
        pnl_percent: if invested_total > 0.0 { pnl / invested_total * 100.0 } else { 0.0 },
        allocation: Allocation { crypto: pct(crypto), stock: pct(stock), cash: pct(safe_cash) },
        lines,
        risk: PortfolioRisk {
            max_weight,
            effective_assets: if hhi > 0.0 { 1.0 / hhi } else { 0.0 },
            volatility_annual,
            var95_day,
            var95_day_percent: var95_day.filter(|_| total > 0.0).map(|v| v / total * 100.0),
            average_correlation,
            loss_at_stops,
        },
        insights,
    }
}

// ---------- Plain-language texts (French) ----------

/// `money(v, 0, 0, " ")`.
fn usd(v: f64) -> String {
    crate::fx::money_with(v, |x| fr(x, 0, 0))
}

/// "12,3 %".
pub fn pc(v: f64) -> String {
    format!("{} %", fr(v, 0, 1))
}

/// `REASON_TEXT[code] ?? code`.
pub fn reason_text(code: &str) -> String {
    match code {
        "bearish_day_and_short" => "Signaux baissiers en journalier et en 4 h : la tendance s'est retournée.".into(),
        "bearish_day" => "Signal journalier baissier : placez ou remontez un stop pour limiter la baisse.".into(),
        "strong_bearish_short" => "Forte pression vendeuse à court terme (4 h) : protégez la position.".into(),
        "overweight" => format!("Cette ligne dépasse {} % de votre patrimoine : trop dépendant d'un seul actif.", number_to_string(MAX_WEIGHT)),
        "take_profit" => "Plus de 50 % de gain sans signal haussier : sécuriser une partie des gains.".into(),
        "bullish_day_and_short" => "Signaux haussiers en journalier et en 4 h, poids encore modéré.".into(),
        "bullish_day" => "Tendance journalière haussière : rien à faire.".into(),
        "neutral" => "Pas de signal fort : rien à faire pour l'instant.".into(),
        "unreliable" => "Sources de données absentes ou en désaccord : aucun conseil plutôt qu'un mauvais conseil.".into(),
        "no_price" => "Cours introuvable pour ce symbole.".into(),
        "deep_loss" => "Perte de plus de 20 % : évitez de moyenner à la baisse sans signal haussier.".into(),
        other => other.into(),
    }
}

fn sign(v: f64) -> &'static str {
    if v >= 0.0 { "+" } else { "−" }
}

pub fn insight_text(i: &Insight) -> String {
    match i.code.as_str() {
        "empty" => "Ajoutez vos actifs pour obtenir une analyse complète.".into(),
        "pnl" => {
            let (p, pp) = (i.num("pnl"), i.num("pnlPercent"));
            format!("Plus-value latente : {}{} ({}{}).", sign(p), usd(p.abs()), sign(pp), pc(pp.abs()))
        }
        "act_bearish" => format!("{} ligne(s) à surveiller de près (signaux baissiers) : {}.", i.text("count"), i.text("symbols")),
        "concentration" => {
            format!("{} pèse {} de votre patrimoine : une baisse de cet actif vous toucherait fortement.", i.text("symbol"), pc(i.num("weight")))
        }
        "single_asset" => format!("Tout est investi sur {} : aucune diversification.", i.text("symbol")),
        "crypto_heavy" => format!("{} en crypto : portefeuille très volatil.", pc(i.num("weight"))),
        "low_diversification" => format!("Diversification faible : l'équivalent de {} actif(s) de même poids.", to_fixed(i.num("effective"), 1)),
        "correlated" => format!(
            "Vos actifs évoluent ensemble (corrélation moyenne {}) : ils baisseront probablement en même temps.",
            to_fixed(i.num("correlation"), 2)
        ),
        "var" => format!("Lors d'une mauvaise journée (1 sur 20), vous pourriez perdre environ {} ({}).", usd(i.num("amount")), pc(i.num("percent"))),
        "stops" => format!("Si tous les stops conseillés étaient touchés : perte d'environ {} ({}).", usd(i.num("amount")), pc(i.num("percent"))),
        "low_cash" => format!("Peu de liquidités ({}) : aucune réserve pour saisir une opportunité.", pc(i.num("weight"))),
        "deep_loss" => format!("Lignes en perte de plus de 20 % : {}.", i.text("symbols")),
        "opportunities" => format!("Renforcement possible selon les signaux : {}.", i.text("symbols")),
        "unknown" => format!("Analyse impossible pour : {} (données insuffisantes).", i.text("symbols")),
        other => other.into(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;
    /// `Date.UTC(2025, 0, 1)`.
    const T0: i64 = 1_735_689_600_000;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../../backend/tests/samples/swift-fixture.json")).unwrap()
    }

    /// Fixture series re-timed as daily candles (crypto: every day; stocks: skip weekends).
    pub(crate) fn daily(fx: &Value, case: usize, stock: bool) -> Vec<Candle> {
        let rows = fx[case]["candles"].as_array().unwrap();
        let mut out = Vec::new();
        let mut d: i64 = 0;
        for r in rows {
            let v: Vec<f64> = r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
            if stock {
                // 1 Jan. 2025 is a Wednesday: day d is a weekend when (d + 3) % 7 is 0 (Sunday) or 6 (Saturday).
                while [0, 6].contains(&((d + 3) % 7)) {
                    d += 1;
                }
            }
            out.push(Candle {
                time: T0 + d * DAY + if stock { 13 * 3_600_000 + 1_800_000 } else { 0 },
                open: v[1],
                high: v[2],
                low: v[3],
                close: v[4],
                volume: v[5],
            });
            d += 1;
        }
        out
    }

    pub(crate) fn h(id: &str, symbol: &str, kind: Kind, quantity: f64, average_price: f64) -> Holding {
        Holding { id: id.into(), symbol: symbol.into(), kind, name: symbol.into(), quantity, average_price, stop: None }
    }

    fn sig(action: Action, score: f64) -> Option<LineSignal> {
        Some(LineSignal { action, score })
    }

    fn m(daily: Vec<Candle>, day: Option<LineSignal>, short: Option<LineSignal>, rel: ReliabilityLevel) -> MarketInput {
        // Current price = last daily close.
        MarketInput { price: daily[daily.len() - 1].close, daily, day_signal: day, short_signal: short, reliability: Some(rel) }
    }

    fn close(a: f64, b: f64, digits: i32) {
        assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≠ {b}");
    }

    #[test]
    fn base_statistics() {
        close(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.05), 1.2, 12);
        let a: Vec<f64> = (1..=10).map(f64::from).collect();
        let b: Vec<f64> = a.iter().map(|x| x * 2.0).collect();
        close(correlation(&a, &b).unwrap(), 1.0, 12);
        // Stocks closed at the weekend: price carried forward, zero return.
        let fx = fixture();
        let stock = daily(&fx, 3, true);
        let r = aligned_returns(&[&daily(&fx, 0, false), &stock[..180]], 90);
        assert_eq!(r[0].len(), r[1].len());
        assert!(r[1].iter().filter(|x| **x == 0.0).count() > 5);
    }

    fn diversified(fx: &Value) -> (Vec<Holding>, f64, HashMap<String, MarketInput>) {
        let holdings =
            vec![h("1", "BTC", Kind::Crypto, 0.05, 60.0), h("2", "ETH", Kind::Crypto, 2.0, 110.0), h("3", "AAPL", Kind::Stock, 10.0, 90.0)];
        let market = HashMap::from([
            ("crypto:BTC".into(), m(daily(fx, 0, false), sig(Action::Buy, 30.0), sig(Action::Buy, 28.0), ReliabilityLevel::High)),
            ("crypto:ETH".into(), m(daily(fx, 1, false), sig(Action::Sell, -30.0), sig(Action::Sell, -40.0), ReliabilityLevel::High)),
            ("stock:AAPL".into(), m(daily(fx, 3, true), sig(Action::Hold, 10.0), sig(Action::Hold, 5.0), ReliabilityLevel::Medium)),
        ]);
        (holdings, 500.0, market)
    }

    #[test]
    fn diversified_portfolio() {
        let fx = fixture();
        let (holdings, cash, market) = diversified(&fx);
        let a = analyze_portfolio(&holdings, cash, &market);
        let sum: f64 = a.lines.iter().map(|l| l.value).sum::<f64>() + a.cash;
        close(a.total, sum, 9);
        close(a.allocation.crypto + a.allocation.stock + a.allocation.cash, 100.0, 9);
        close(a.lines.iter().map(|l| l.weight).sum::<f64>() + a.allocation.cash, 100.0, 9);
        let eth = a.lines.iter().find(|l| l.symbol == "ETH").unwrap();
        assert_eq!(eth.recommendation, Recommendation::Sell);
        assert!(eth.stop.unwrap() < eth.price.unwrap());
        assert!(a.risk.volatility_annual.unwrap() > 0.0);
        assert!(a.risk.var95_day.unwrap() > 0.0);
        assert_eq!(a.insights[0].level, InsightLevel::Danger);
        for i in &a.insights {
            assert_ne!(insight_text(i), i.code);
        }
    }

    #[test]
    fn overweight_and_unreliable() {
        let fx = fixture();
        let holdings = vec![h("1", "SOL", Kind::Crypto, 100.0, 40.0), h("2", "NVDA", Kind::Stock, 1.0, 80.0)];
        let mut market = HashMap::from([
            ("crypto:SOL".to_string(), m(daily(&fx, 2, false), sig(Action::Hold, 5.0), sig(Action::StrongSell, -55.0), ReliabilityLevel::High)),
            ("stock:NVDA".to_string(), m(daily(&fx, 4, true), None, None, ReliabilityLevel::Low)),
        ]);
        let a = analyze_portfolio(&holdings, 0.0, &market);
        let sol = a.lines.iter().find(|l| l.symbol == "SOL").unwrap();
        // A strong 4 h sell has priority over the overweight.
        assert_eq!(sol.recommendation, Recommendation::Protect);
        assert!(sol.reasons.iter().any(|r| r == "overweight"));
        assert_eq!(a.lines.iter().find(|l| l.symbol == "NVDA").unwrap().recommendation, Recommendation::Unknown);
        market.get_mut("crypto:SOL").unwrap().short_signal = sig(Action::Hold, 0.0);
        let heavy = analyze_portfolio(&holdings, 0.0, &market);
        let sol2 = heavy.lines.iter().find(|l| l.symbol == "SOL").unwrap();
        assert_eq!(sol2.recommendation, Recommendation::Lighten);
        close((sol2.value - sol2.trim_value) / heavy.total, 0.3, 9);
    }

    #[test]
    fn empty_portfolio() {
        let a = analyze_portfolio(&[], 1000.0, &HashMap::new());
        assert_eq!(a.total, 1000.0);
        assert_eq!(a.insights.iter().map(|i| i.code.as_str()).collect::<Vec<_>>(), vec!["empty"]);
        assert_eq!(insight_text(&a.insights[0]), "Ajoutez vos actifs pour obtenir une analyse complète.");
    }

    #[test]
    fn texts() {
        let i = Insight::new(InsightLevel::Good, "pnl", &[("pnl", json!(-1234.4)), ("pnlPercent", json!(-12.34))]);
        assert_eq!(insight_text(&i), "Plus-value latente : −1\u{202f}234 $ (−12,3 %).");
        let i = Insight::new(InsightLevel::Danger, "act_bearish", &[("count", json!(2)), ("symbols", json!("ETH, SOL"))]);
        assert_eq!(insight_text(&i), "2 ligne(s) à surveiller de près (signaux baissiers) : ETH, SOL.");
        let i = Insight::new(InsightLevel::Warning, "low_diversification", &[("effective", json!(2.25))]);
        assert_eq!(insight_text(&i), "Diversification faible : l'équivalent de 2.3 actif(s) de même poids.");
        assert_eq!(reason_text("overweight"), "Cette ligne dépasse 35 % de votre patrimoine : trop dépendant d'un seul actif.");
        assert_eq!(reason_text("momentum"), "momentum");
    }
}

//! Decision tools that only compute (no advice added, `web/src/engine/tools.ts`): comparison of assets over the
//! same days, position size for a chosen risk, sale after fees and tax, projection, and rebalancing towards a target
//! allocation. Same computations on the iPhone and Android.
use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::holdings::positive;
use crate::engine::history::Close;
use crate::types::{DAY_MS, Kind};

fn day_of(t: i64) -> i64 {
    t.div_euclid(DAY_MS) * DAY_MS
}

// ---------- Comparison ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareStat {
    pub id: String,
    /// Change over the period, in %.
    pub change: f64,
    /// Annualised volatility of the daily returns, in % (365 days a year for crypto, 252 for stocks).
    pub volatility: f64,
    /// Deepest fall from a previous high, in % (≤ 0).
    pub max_drawdown: f64,
    /// Change since the first day, one point per calendar day of the common period.
    pub pct: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub stats: Vec<CompareStat>,
    /// Correlation of the daily returns, on the days both assets closed; None with fewer than 20 common days.
    pub correlation: Vec<Vec<Option<f64>>>,
    pub days: Vec<i64>,
    pub missing: Vec<String>,
}

/// Daily returns by day (a second close on the same day replaces the first, keeping its place).
fn returns(closes: &[Close]) -> IndexMap<i64, f64> {
    let mut out = IndexMap::new();
    for i in 1..closes.len() {
        out.insert(day_of(closes[i].0), closes[i].1 / closes[i - 1].1 - 1.0);
    }
    out
}

fn correlation(a: &IndexMap<i64, f64>, b: &IndexMap<i64, f64>) -> Option<f64> {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for (d, x) in a {
        if let Some(y) = b.get(d) {
            xs.push(*x);
            ys.push(*y);
        }
    }
    if xs.len() < 20 {
        return None;
    }
    let mx = xs.iter().sum::<f64>() / xs.len() as f64;
    let my = ys.iter().sum::<f64>() / ys.len() as f64;
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..xs.len() {
        sxy += (xs[i] - mx) * (ys[i] - my);
        sxx += (xs[i] - mx).powi(2);
        syy += (ys[i] - my).powi(2);
    }
    (sxx > 0.0 && syy > 0.0).then(|| sxy / (sxx * syy).sqrt())
}

/// `ids`: "crypto:BTC"…; `series`: daily closes by id. None when no asset has two closes or the period is too short.
pub fn compare_assets(series: &HashMap<String, Vec<Close>>, ids: &[String], days: i64, now: i64) -> Option<Comparison> {
    let mut clean: HashMap<&str, Vec<Close>> = HashMap::new();
    let mut missing = Vec::new();
    for id in ids {
        let mut c: Vec<Close> = series.get(id).map(|s| s.iter().filter(|(t, v)| *v > 0.0 && *t <= now).copied().collect()).unwrap_or_default();
        c.sort_by_key(|x| x.0);
        if c.len() < 2 {
            missing.push(id.clone());
        } else {
            clean.insert(id, c);
        }
    }
    let kept: Vec<&String> = ids.iter().filter(|id| clean.contains_key(id.as_str())).collect();
    if kept.is_empty() {
        return None;
    }
    let end = day_of(kept.iter().map(|id| clean[id.as_str()].last().map(|c| c.0).unwrap_or(0)).max().unwrap_or(0));
    // Common period: from the latest first day among the assets (a younger asset shortens it) to the last close.
    let start = kept.iter().map(|id| day_of(clean[id.as_str()][0].0)).fold(end - days * DAY_MS, i64::max);
    if end - start < DAY_MS {
        return None;
    }
    let grid: Vec<i64> = (0..).map(|i| start + i * DAY_MS).take_while(|d| *d <= end).collect();

    let in_period = |id: &str| -> Vec<Close> { clean[id].iter().filter(|(t, _)| day_of(*t) >= start && day_of(*t) <= end).copied().collect() };
    let stats: Vec<CompareStat> = kept
        .iter()
        .map(|id| {
            let c = in_period(id);
            let all = &clean[id.as_str()];
            let mut i = 0;
            let mut last = f64::NAN;
            let on_grid: Vec<f64> = grid
                .iter()
                .map(|d| {
                    while i < all.len() && day_of(all[i].0) <= *d {
                        last = all[i].1;
                        i += 1;
                    }
                    last
                })
                .collect();
            let base = on_grid[0];
            let mut peak = c.first().map(|x| x.1).unwrap_or(f64::NAN);
            let mut dd: f64 = 0.0;
            for (_, v) in &c {
                peak = peak.max(*v);
                dd = dd.min((v / peak - 1.0) * 100.0);
            }
            let r: Vec<f64> = returns(&c).into_values().collect();
            let mean = r.iter().sum::<f64>() / r.len().max(1) as f64;
            let sd = (r.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / r.len().saturating_sub(1).max(1) as f64).sqrt();
            let per_year: f64 = if id.starts_with("crypto:") { 365.0 } else { 252.0 };
            CompareStat {
                id: (*id).clone(),
                change: (on_grid[on_grid.len() - 1] / base - 1.0) * 100.0,
                volatility: sd * per_year.sqrt() * 100.0,
                max_drawdown: dd,
                pct: on_grid.iter().map(|v| (v / base - 1.0) * 100.0).collect(),
            }
        })
        .collect();
    let rets: Vec<IndexMap<i64, f64>> = kept.iter().map(|id| returns(&in_period(id))).collect();
    let corr = (0..kept.len()).map(|a| (0..kept.len()).map(|b| if a == b { Some(1.0) } else { correlation(&rets[a], &rets[b]) }).collect()).collect();
    Some(Comparison { stats, correlation: corr, days: grid, missing })
}

// ---------- Position size ----------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionInput {
    /// Money available for the idea (capital or budget), in $.
    pub capital: f64,
    /// Share of the capital accepted as a loss if the stop is hit, in % (1 % is a common rule).
    pub risk_pct: f64,
    pub entry: f64,
    pub stop: f64,
    pub target: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionSize {
    pub quantity: f64,
    pub amount: f64,
    /// Amount as a share of the capital, in %.
    pub capital_share: f64,
    /// Loss if the stop is hit, in $.
    pub risk: f64,
    /// Distance to the stop, in % of the entry.
    pub stop_distance: f64,
    pub reward: Option<f64>,
    /// Reward / risk ratio (R).
    pub ratio: Option<f64>,
    /// The size was cut to the capital (the stop is too close for the risk chosen).
    pub capped: bool,
}

/// Quantity such that hitting the stop costs `risk_pct` % of the capital, never more than the capital itself.
pub fn position_size(p: &PositionInput) -> Option<PositionSize> {
    if !positive(p.capital) || !positive(p.risk_pct) || !positive(p.entry) || !positive(p.stop) || p.stop >= p.entry {
        return None;
    }
    let per_unit = p.entry - p.stop;
    let mut quantity = p.capital * p.risk_pct / 100.0 / per_unit;
    let mut capped = false;
    if quantity * p.entry > p.capital {
        quantity = p.capital / p.entry;
        capped = true;
    }
    let risk = quantity * per_unit;
    let reward = p.target.filter(|t| *t != 0.0 && *t > p.entry).map(|t| quantity * (t - p.entry));
    Some(PositionSize {
        quantity,
        amount: quantity * p.entry,
        capital_share: quantity * p.entry / p.capital * 100.0,
        risk,
        stop_distance: per_unit / p.entry * 100.0,
        reward,
        ratio: reward.map(|r| r / risk),
        capped,
    })
}

// ---------- Sale after fees and tax ----------

#[derive(Debug, Clone, PartialEq)]
pub struct SaleLine {
    pub id: String,
    pub value: f64,
    /// Purchase cost (quantity × average price), None when unknown.
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaleResult {
    pub gross: f64,
    pub fees: f64,
    /// Gain after fees (negative: loss); None when a cost is unknown.
    pub gain: Option<f64>,
    pub tax: f64,
    pub net: f64,
}

/// What a sale leaves once the fees and the tax on the gain are paid (France: flat tax "PFU" of 30 % = 12,8 % income
/// tax + 17,2 % social contributions). A loss pays no tax. Line by line: for cryptos, France computes the gain on the
/// whole portfolio at each sale (art. 150 VH bis), so this is an approximation, said as such on screen.
pub fn sale_after_tax(line: &SaleLine, tax_pct: f64, fee_pct: f64) -> SaleResult {
    let fees = line.value.max(0.0) * fee_pct.max(0.0) / 100.0;
    let gain = line.cost.map(|c| line.value - fees - c);
    let tax = gain.filter(|g| *g > 0.0).map(|g| g * tax_pct.max(0.0) / 100.0).unwrap_or(0.0);
    SaleResult { gross: line.value, fees, gain, tax, net: line.value - fees - tax }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaleTotal {
    pub total: SaleResult,
    pub lines: Vec<(String, SaleResult)>,
    pub unknown_cost: usize,
}

/// All lines sold: totals; gains and losses of the same year offset each other before the tax (France).
pub fn sale_total(lines: &[SaleLine], tax_pct: f64, fee_pct: f64) -> SaleTotal {
    let each: Vec<(String, SaleResult)> = lines.iter().map(|l| (l.id.clone(), sale_after_tax(l, tax_pct, fee_pct))).collect();
    let known: Vec<f64> = each.iter().filter_map(|(_, l)| l.gain).collect();
    let gain = (!known.is_empty()).then(|| known.iter().sum::<f64>());
    let gross: f64 = each.iter().map(|(_, l)| l.gross).sum();
    let fees: f64 = each.iter().map(|(_, l)| l.fees).sum();
    let tax = gain.filter(|g| *g > 0.0).map(|g| g * tax_pct.max(0.0) / 100.0).unwrap_or(0.0);
    let unknown_cost = each.len() - known.len();
    SaleTotal { total: SaleResult { gross, fees, gain, tax, net: gross - fees - tax }, lines: each, unknown_cost }
}

// ---------- Projection ----------

/// Yearly returns shown side by side: hypotheses to compare, not forecasts.
pub const PROJECTION_RATES: [f64; 3] = [0.0, 4.0, 8.0];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProjectionPoint {
    pub year: f64,
    pub value: f64,
    pub paid: f64,
}

/// Value after `years` of a starting amount plus a contribution at the end of each month, compounded monthly at a
/// yearly rate. One point per year (year 0 = today).
pub fn projection(start: f64, monthly: f64, years: f64, rate_pct: f64) -> Vec<ProjectionPoint> {
    let r = (1.0 + rate_pct / 100.0).powf(1.0 / 12.0) - 1.0;
    let mut v = start.max(0.0);
    let mut paid = v;
    let mut out = vec![ProjectionPoint { year: 0.0, value: v, paid }];
    let months = crate::js::round(years) as i64 * 12;
    for m in 1..=months {
        v = v * (1.0 + r) + monthly.max(0.0);
        paid += monthly.max(0.0);
        if m % 12 == 0 {
            out.push(ProjectionPoint { year: (m / 12) as f64, value: v, paid });
        }
    }
    out
}

// ---------- Rebalancing ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetClass {
    Crypto,
    Stock,
    Cash,
}

pub const ASSET_CLASSES: [AssetClass; 3] = [AssetClass::Crypto, AssetClass::Stock, AssetClass::Cash];

impl AssetClass {
    pub fn key(self) -> &'static str {
        match self {
            AssetClass::Crypto => "crypto",
            AssetClass::Stock => "stock",
            AssetClass::Cash => "cash",
        }
    }
}

/// A value per class (crypto, stock, cash).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ByClass {
    pub crypto: f64,
    pub stock: f64,
    pub cash: f64,
}

impl ByClass {
    pub fn get(&self, k: AssetClass) -> f64 {
        match k {
            AssetClass::Crypto => self.crypto,
            AssetClass::Stock => self.stock,
            AssetClass::Cash => self.cash,
        }
    }
    fn get_mut(&mut self, k: AssetClass) -> &mut f64 {
        match k {
            AssetClass::Crypto => &mut self.crypto,
            AssetClass::Stock => &mut self.stock,
            AssetClass::Cash => &mut self.cash,
        }
    }
}

fn class_of(k: Kind) -> AssetClass {
    match k {
        Kind::Crypto => AssetClass::Crypto,
        Kind::Stock => AssetClass::Stock,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RebalanceLine {
    pub id: String,
    pub kind: Kind,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rebalance {
    pub total: f64,
    pub current: ByClass,
    /// Amount to buy (+) or sell (−) per class, in $.
    pub moves: ByClass,
    /// Split of each class's move over its lines, pro rata of their value: (line id, amount).
    pub lines: Vec<(String, f64)>,
}

/// Buys and sells that bring the portfolio to the target split (in %, summing to 100).
pub fn rebalance(lines: &[RebalanceLine], cash: f64, target: &ByClass) -> Option<Rebalance> {
    let sum = target.crypto + target.stock + target.cash;
    if (sum - 100.0).abs() > 0.01 || [target.crypto, target.stock, target.cash].iter().any(|v| *v < 0.0) {
        return None;
    }
    let mut by = ByClass { cash: cash.max(0.0), ..Default::default() };
    for l in lines.iter().filter(|l| l.value > 0.0) {
        *by.get_mut(class_of(l.kind)) += l.value;
    }
    let total = by.crypto + by.stock + by.cash;
    if !positive(total) {
        return None;
    }
    let (mut current, mut moves) = (ByClass::default(), ByClass::default());
    for k in ASSET_CLASSES {
        *current.get_mut(k) = by.get(k) / total * 100.0;
        *moves.get_mut(k) = total * target.get(k) / 100.0 - by.get(k);
    }
    let split = lines
        .iter()
        .filter(|l| l.value > 0.0 && by.get(class_of(l.kind)) > 0.0)
        .map(|l| (l.id.clone(), moves.get(class_of(l.kind)) * l.value / by.get(class_of(l.kind))))
        .collect();
    Some(Rebalance { total, current, moves, lines: split })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const DAY: i64 = DAY_MS;

    fn close(a: f64, b: f64, digits: i32) {
        assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≠ {b}");
    }

    /// `Date.parse("2026-09-01T00:00:00Z")`.
    const SEP1: i64 = 1_788_220_800_000;

    pub(crate) fn sample() -> (i64, HashMap<String, Vec<Close>>) {
        let v: Value = serde_json::from_str(include_str!("../../../../backend/tests/samples/history-sample.json")).unwrap();
        let series = v["series"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let id = format!("{}:{}", s["kind"].as_str().unwrap(), s["symbol"].as_str().unwrap());
                let closes = s["closes"].as_array().unwrap().iter().map(|c| (c[0].as_i64().unwrap(), c[1].as_f64().unwrap())).collect();
                (id, closes)
            })
            .collect();
        (v["asOf"].as_i64().unwrap(), series)
    }

    #[test]
    fn compare_on_real_closes() {
        let (as_of, real) = sample();
        let ids: Vec<String> = ["crypto:BTC", "crypto:ETH", "stock:AAPL"].iter().map(|s| s.to_string()).collect();
        let c = compare_assets(&real, &ids, 90, as_of).unwrap();
        assert_eq!(c.stats.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ids);
        for s in &c.stats {
            assert_eq!(s.pct.len(), c.days.len());
            assert_eq!(s.pct[0], 0.0);
            close(s.change, s.pct[s.pct.len() - 1], 9);
            assert!(s.max_drawdown <= 0.0);
            assert!(s.volatility > 5.0);
        }
        // Ether moves more than Bitcoin, which moves more than Apple; the two cryptos move together.
        let (btc, eth, aapl) = (&c.stats[0], &c.stats[1], &c.stats[2]);
        assert!(eth.volatility > btc.volatility);
        assert!(btc.volatility > aapl.volatility);
        assert!(c.correlation[0][1].unwrap() > 0.5);
        assert_eq!(c.correlation[1][0], c.correlation[0][1]);
        assert_eq!(c.correlation[2][2], Some(1.0));
    }

    #[test]
    fn compare_exact_and_younger_asset() {
        let a: Vec<Close> = [100.0, 110.0, 99.0, 120.0].iter().enumerate().map(|(i, v)| (SEP1 + i as i64 * DAY, *v)).collect();
        let b: Vec<Close> = [50.0, 55.0].iter().enumerate().map(|(i, v)| (SEP1 + 2 * DAY + i as i64 * DAY, *v)).collect();
        let series = HashMap::from([("crypto:A".to_string(), a), ("crypto:B".to_string(), b)]);
        let ids: Vec<String> = ["crypto:A", "crypto:B", "crypto:GONE"].iter().map(|s| s.to_string()).collect();
        let c = compare_assets(&series, &ids, 30, SEP1 + 4 * DAY).unwrap();
        assert_eq!(c.missing, vec!["crypto:GONE"]);
        // The common period starts on 3 Sept., the first day of B.
        assert_eq!(c.days[0], SEP1 + 2 * DAY);
        assert_eq!(c.stats[0].pct.iter().map(|v| crate::js::round(v * 100.0) / 100.0).collect::<Vec<_>>(), vec![0.0, 21.21]);
        assert_eq!(c.stats[0].max_drawdown, 0.0);
        close(c.stats[1].change, 10.0, 9);
        // Fewer than 20 common days.
        assert_eq!(c.correlation[0][1], None);
    }

    #[test]
    fn position_size_one_percent() {
        let p = position_size(&PositionInput { capital: 10_000.0, risk_pct: 1.0, entry: 100.0, stop: 95.0, target: Some(110.0) }).unwrap();
        close(p.quantity, 20.0, 9);
        close(p.amount, 2000.0, 9);
        close(p.capital_share, 20.0, 9);
        close(p.risk, 100.0, 9);
        close(p.stop_distance, 5.0, 9);
        close(p.reward.unwrap(), 200.0, 9);
        close(p.ratio.unwrap(), 2.0, 9);
        assert!(!p.capped);
        // Very close stop: the size is capped at the capital.
        let p = position_size(&PositionInput { capital: 1000.0, risk_pct: 2.0, entry: 100.0, stop: 99.5, target: None }).unwrap();
        assert!(p.capped);
        close(p.amount, 1000.0, 9);
        close(p.risk, 5.0, 9);
        assert_eq!(p.reward, None);
        // Impossible inputs.
        let pi = |capital: f64, stop: f64| PositionInput { capital, risk_pct: 1.0, entry: 100.0, stop, target: None };
        assert_eq!(position_size(&pi(1000.0, 100.0)), None);
        assert_eq!(position_size(&pi(1000.0, 120.0)), None);
        assert_eq!(position_size(&pi(0.0, 90.0)), None);
    }

    fn sl(id: &str, value: f64, cost: Option<f64>) -> SaleLine {
        SaleLine { id: id.into(), value, cost }
    }

    #[test]
    fn sale_fees_and_flat_tax() {
        // 10 000 $ sold, bought 6 000 $, fees 0,1 % = 10 $: gain 3 990 $, tax 30 % = 1 197 $.
        assert_eq!(
            sale_after_tax(&sl("a", 10_000.0, Some(6000.0)), 30.0, 0.1),
            SaleResult { gross: 10_000.0, fees: 10.0, gain: Some(3990.0), tax: 1197.0, net: 8793.0 }
        );
        // A loss pays no tax.
        let b = sale_after_tax(&sl("b", 5000.0, Some(7000.0)), 30.0, 0.1);
        assert_eq!((b.tax, b.net, b.gain), (0.0, 4995.0, Some(-2005.0)));
        let c = sale_after_tax(&sl("c", 1000.0, None), 30.0, 0.1);
        assert_eq!((c.gain, c.tax, c.net), (None, 0.0, 999.0));
        // Everything sold: the losses of the year offset the gains.
        let t = sale_total(&[sl("a", 10_000.0, Some(6000.0)), sl("b", 5000.0, Some(7000.0)), sl("c", 1000.0, None)], 30.0, 0.1);
        assert_eq!(t.total.gross, 16_000.0);
        close(t.total.fees, 16.0, 9);
        close(t.total.gain.unwrap(), 3990.0 - 2005.0, 9);
        close(t.total.tax, (3990.0 - 2005.0) * 0.3, 9);
        close(t.total.net, 16_000.0 - 16.0 - (3990.0 - 2005.0) * 0.3, 9);
        assert_eq!(t.unknown_cost, 1);
        // Net loss overall: no tax at all.
        assert_eq!(sale_total(&[sl("b", 5000.0, Some(7000.0))], 30.0, 0.1).total.tax, 0.0);
    }

    #[test]
    fn projections() {
        // Without return, what was paid in.
        let p = projection(1000.0, 100.0, 10.0, 0.0);
        assert_eq!(p.len(), 11);
        close(p[10].value, 1000.0 + 100.0 * 120.0, 9);
        close(p[10].paid, 13_000.0, 9);
        // Capital alone at 8 % a year: × 1,08 each year.
        let p = projection(10_000.0, 0.0, 5.0, 8.0);
        close(p[1].value, 10_800.0, 6);
        close(p[5].value, 10_000.0 * 1.08f64.powi(5), 6);
        // Monthly contributions: annuity formula.
        let r = 1.04f64.powf(1.0 / 12.0) - 1.0;
        let p = projection(0.0, 200.0, 20.0, 4.0);
        close(p[p.len() - 1].value, 200.0 * ((1.0 + r).powi(240) - 1.0) / r, 6);
    }

    #[test]
    fn rebalancing() {
        let rl = |id: &str, kind: Kind, value: f64| RebalanceLine { id: id.into(), kind, value };
        let r = rebalance(
            &[rl("crypto:BTC", Kind::Crypto, 6000.0), rl("crypto:ETH", Kind::Crypto, 2000.0), rl("stock:AAPL", Kind::Stock, 1500.0)],
            500.0,
            &ByClass { crypto: 50.0, stock: 40.0, cash: 10.0 },
        )
        .unwrap();
        assert_eq!(r.total, 10_000.0);
        close(r.current.crypto, 80.0, 9);
        assert_eq!(r.moves, ByClass { crypto: -3000.0, stock: 2500.0, cash: 500.0 });
        assert_eq!(r.lines, vec![("crypto:BTC".to_string(), -2250.0), ("crypto:ETH".to_string(), -750.0), ("stock:AAPL".to_string(), 2500.0)]);
        // Buying and selling balance out.
        close(r.moves.crypto + r.moves.stock + r.moves.cash, 0.0, 9);
        // A target that does not make 100 %.
        assert_eq!(rebalance(&[rl("crypto:BTC", Kind::Crypto, 1.0)], 0.0, &ByClass { crypto: 60.0, stock: 30.0, cash: 0.0 }), None);
    }
}

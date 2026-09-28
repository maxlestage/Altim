//! Company fundamentals for `/api/decision`: figures from the company's own filings (SEC EDGAR, official and free)
//! and the earnings calendar, surprises and consensus revisions published by Nasdaq (Zacks data, free).
//!
//! Every figure is read from a filing or computed from filed figures (trailing twelve months); what the filings do
//! not report stays `None`, never estimated. Funds and ETFs file no income statement: they get `None` everywhere,
//! with a `period` / `source` saying why.
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use chrono::NaiveDate;
use regex::Regex;
use serde_json::Value;

use crate::cache::cached;
use crate::engine::decision_types::{EarningsDate, EarningsSurprise, Revisions, StockFundamentals};
use crate::http::{Error, Result, get_json_with, get_text_with};
use crate::js::now_ms;

/// The SEC asks for a descriptive User-Agent naming the author. It answers HTTP 403 to any User-Agent containing
/// "github" (checked 28/09/2026), hence the repository written without its host. `ALTIM_SEC_USER_AGENT` replaces
/// it (e.g. to add the operator's contact address, which is never written in the code).
pub const SEC_UA: &str = "Altim/1.0 (open-source trading advice app; repository maxlestage/altim)";

static SEC_USER_AGENT: LazyLock<String> =
    LazyLock::new(|| std::env::var("ALTIM_SEC_USER_AGENT").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| SEC_UA.to_string()));
/// Nasdaq answers browsers only.
const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const HOUR: i64 = 3_600_000;

pub const SECTOR_NOTE: &str = "Comparaison au secteur non disponible : aucune source gratuite et vérifiable ne publie les moyennes sectorielles.";

// ---------- Small JSON helpers ----------

/// A number, or a numeric string ("$341.29", "1,234.5", "4.69", "-0.2%"); None otherwise.
pub fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64().filter(|x| x.is_finite()),
        Value::String(s) => {
            let t: String = s.trim().chars().filter(|c| !matches!(c, '$' | ',' | '%' | ' ')).collect();
            if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+' | b'e' | b'E')) {
                return None;
            }
            t.parse::<f64>().ok().filter(|x| x.is_finite())
        }
        _ => None,
    }
}

/// Rounded to `d` decimals (ratios and percentages shown to the user).
pub fn round_to(x: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    (x * p).round() / p
}

fn day(s: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    Some(d.signed_duration_since(NaiveDate::from_ymd_opt(1970, 1, 1)?).num_days())
}

fn date_of(days: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).unwrap() + chrono::Duration::days(days)
}

fn fr_date(days: i64) -> String {
    date_of(days).format("%d/%m/%Y").to_string()
}

// ---------- SEC EDGAR ----------

/// One reported value (dates in days since 1970-01-01).
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    /// None for a balance-sheet (instant) value.
    pub start: Option<i64>,
    pub end: i64,
    pub val: f64,
    pub form: String,
    pub filed: i64,
}

impl Fact {
    fn days(&self) -> Option<i64> {
        self.start.map(|s| self.end - s)
    }
}

/// The concepts read from a company's facts, already filtered (10-K / 10-Q only, one value per period: the
/// latest filed).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub name: String,
    pub concepts: HashMap<String, Vec<Fact>>,
}

const REVENUE: &[&str] =
    &["Revenues", "RevenueFromContractWithCustomerExcludingAssessedTax", "SalesRevenueNet", "RevenueFromContractWithCustomerIncludingAssessedTax"];
const NET_INCOME: &[&str] = &["NetIncomeLoss", "ProfitLoss"];
const EPS: &[&str] = &["EarningsPerShareDiluted", "EarningsPerShareBasicAndDiluted"];
const GROSS_PROFIT: &[&str] = &["GrossProfit"];
const OPERATING_INCOME: &[&str] = &["OperatingIncomeLoss"];
const OPERATING_CASH: &[&str] = &["NetCashProvidedByUsedInOperatingActivities"];
const CAPEX: &[&str] = &["PaymentsToAcquirePropertyPlantAndEquipment", "PaymentsToAcquireProductiveAssets"];
const DEPRECIATION: &[&str] = &["DepreciationDepletionAndAmortization", "DepreciationAndAmortization", "DepreciationAmortizationAndAccretionNet"];
const DIVIDENDS: &[&str] = &["CommonStockDividendsPerShareDeclared", "CommonStockDividendsPerShareCashPaid"];
const EQUITY: &[&str] = &["StockholdersEquity", "StockholdersEquityIncludingPortionAttributableToNoncontrollingInterest"];
const CASH: &[&str] = &["CashAndCashEquivalentsAtCarryingValue", "CashCashEquivalentsRestrictedCashAndRestrictedCashEquivalents"];
const DEBT_PARTS: &[&str] =
    &["LongTermDebt", "LongTermDebtNoncurrent", "LongTermDebtCurrent", "DebtCurrent", "CommercialPaper", "ShortTermBorrowings"];
const SHARES_GAAP: &str = "CommonStockSharesOutstanding";
const SHARES_DEI: &str = "EntityCommonStockSharesOutstanding";

fn all_concepts() -> impl Iterator<Item = &'static str> {
    [REVENUE, NET_INCOME, EPS, GROSS_PROFIT, OPERATING_INCOME, OPERATING_CASH, CAPEX, DEPRECIATION, DIVIDENDS, EQUITY, CASH, DEBT_PARTS]
        .into_iter()
        .flatten()
        .copied()
        .chain([SHARES_GAAP])
}

pub mod parse {
    use super::*;

    /// `company_tickers.json`: {"0": {"cik_str": 320193, "ticker": "AAPL", "title": "Apple Inc."}, …} → ticker → CIK.
    pub fn tickers(d: &Value) -> HashMap<String, u64> {
        let mut out = HashMap::new();
        if let Some(o) = d.as_object() {
            for e in o.values() {
                if let (Some(t), Some(c)) = (e.get("ticker").and_then(|t| t.as_str()), e.get("cik_str").and_then(|c| c.as_u64())) {
                    out.entry(t.to_uppercase()).or_insert(c);
                }
            }
        }
        out
    }

    /// `include/ticker.txt`: "aapl\t320193" per line.
    pub fn ticker_txt(text: &str) -> HashMap<String, u64> {
        let mut out = HashMap::new();
        for line in text.lines() {
            let mut it = line.split('\t');
            if let (Some(t), Some(c)) = (it.next(), it.next().and_then(|c| c.trim().parse::<u64>().ok())) {
                if !t.is_empty() {
                    out.entry(t.trim().to_uppercase()).or_insert(c);
                }
            }
        }
        out
    }

    /// EDGAR company search `efts.sec.gov/LATEST/search-index?keysTyped=SYM`: the entity listing exactly this
    /// ticker ("tickers": "GOOG, GOOGN, GOOGM, GOOGL").
    pub fn efts(d: &Value, symbol: &str) -> Option<u64> {
        d.get("hits")?.get("hits")?.as_array()?.iter().find_map(|h| {
            let tickers = h.get("_source")?.get("tickers")?.as_str()?;
            if tickers.split(',').any(|t| t.trim().eq_ignore_ascii_case(symbol)) { h.get("_id")?.as_str()?.parse().ok() } else { None }
        })
    }

    fn series(d: &Value, taxonomy: &str, concept: &str) -> Vec<Fact> {
        let Some(units) = d.get("facts").and_then(|f| f.get(taxonomy)).and_then(|t| t.get(concept)).and_then(|c| c.get("units")) else {
            return vec![];
        };
        let Some(arr) = ["USD", "USD/shares", "shares"].iter().find_map(|u| units.get(*u)).and_then(|a| a.as_array()) else {
            return vec![];
        };
        // One value per period: the one filed last (a later filing may restate it).
        let mut by_period: HashMap<(Option<i64>, i64), Fact> = HashMap::new();
        for f in arr {
            let form = f.get("form").and_then(|x| x.as_str()).unwrap_or("");
            if !(form.starts_with("10-K") || form.starts_with("10-Q")) {
                continue;
            }
            let (Some(end), Some(val), Some(filed)) =
                (f.get("end").and_then(|x| x.as_str()).and_then(day), num(f.get("val")), f.get("filed").and_then(|x| x.as_str()).and_then(day))
            else {
                continue;
            };
            let start = f.get("start").and_then(|x| x.as_str()).and_then(day);
            let fact = Fact { start, end, val, form: form.to_string(), filed };
            match by_period.get(&(start, end)) {
                Some(old) if old.filed > filed => {}
                _ => {
                    by_period.insert((start, end), fact);
                }
            }
        }
        let mut v: Vec<Fact> = by_period.into_values().collect();
        v.sort_by_key(|f| (f.end, f.start));
        v
    }

    /// `companyfacts/CIK##########.json` → the concepts Altim uses.
    pub fn company_facts(d: &Value) -> Facts {
        let mut concepts = HashMap::new();
        for c in all_concepts() {
            let s = series(d, "us-gaap", c);
            if !s.is_empty() {
                concepts.insert(c.to_string(), s);
            }
        }
        let dei = series(d, "dei", SHARES_DEI);
        if !dei.is_empty() {
            concepts.insert(SHARES_DEI.to_string(), dei);
        }
        Facts { name: d.get("entityName").and_then(|x| x.as_str()).unwrap_or("").to_string(), concepts }
    }

    static MDY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b").unwrap());
    static MON_DY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([A-Z][a-z]{2}) (\d{1,2}), (\d{4})\b").unwrap());

    fn month(m: &str) -> Option<u32> {
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().position(|x| *x == m).map(|i| i as u32 + 1)
    }

    fn utc_midnight(y: i32, m: u32, d: u32) -> Option<i64> {
        Some(NaiveDate::from_ymd_opt(y, m, d)?.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis())
    }

    /// Nasdaq `analyst/{SYM}/earnings-date`: the date in `reportText` ("… is estimated to report earnings on
    /// 10/29/2026. The upcoming earnings date is derived from an algorithm …"), else in `announcement`
    /// ("… : Oct 29, 2026"). Estimated when the text says so. Date = midnight UTC of that New York day.
    pub fn earnings_date(d: &Value) -> Option<EarningsDate> {
        let data = d.get("data")?;
        let text = data.get("reportText").and_then(|x| x.as_str()).unwrap_or("");
        let ann = data.get("announcement").and_then(|x| x.as_str()).unwrap_or("");
        let date = MDY
            .captures(text)
            .and_then(|c| utc_midnight(c[3].parse().ok()?, c[1].parse().ok()?, c[2].parse().ok()?))
            .or_else(|| MON_DY.captures(ann).and_then(|c| utc_midnight(c[3].parse().ok()?, month(&c[1])?, c[2].parse().ok()?)))?;
        let lower = text.to_lowercase();
        // Without the explanatory text, the date cannot be told confirmed: kept as estimated.
        let estimated = text.is_empty() || lower.contains("estimated") || lower.contains("algorithm");
        Some(EarningsDate { date, estimated })
    }

    /// "Jun 2026" → "T2 2026" (calendar quarter of the fiscal quarter's last month).
    pub fn quarter_label(s: &str) -> Option<String> {
        let mut it = s.split_whitespace();
        let m = month(it.next()?)?;
        let y: i32 = it.next()?.parse().ok()?;
        Some(format!("T{} {y}", (m - 1) / 3 + 1))
    }

    /// Nasdaq `company/{SYM}/earnings-surprise`: last quarters, most recent first.
    pub fn surprises(d: &Value) -> Vec<EarningsSurprise> {
        let rows = d.get("data").and_then(|x| x.get("earningsSurpriseTable")).and_then(|x| x.get("rows")).and_then(|x| x.as_array());
        rows.map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    let quarter = quarter_label(r.get("fiscalQtrEnd")?.as_str()?)?;
                    let eps = num(r.get("eps"))?;
                    let consensus = num(r.get("consensusForecast"))?;
                    let surprise_pct = num(r.get("percentageSurprise"))
                        .or_else(|| (consensus != 0.0).then(|| round_to((eps - consensus) / consensus.abs() * 100.0, 2)))?;
                    Some(EarningsSurprise { quarter, eps, consensus, surprise_pct })
                })
                .take(4)
                .collect()
        })
        .unwrap_or_default()
    }

    /// Nasdaq `analyst/{SYM}/estimate-momentum`: consensus EPS of the fiscal year, one month ago and now.
    pub fn revisions(d: &Value) -> Option<Revisions> {
        let c = d.get("data")?.get("changeInConsensus")?;
        let month_ago = num(c.get("monthData")?.get("yrMean"))?;
        let now = num(c.get("currentData")?.get("yrMean"))?;
        if month_ago == 0.0 {
            return None;
        }
        Some(Revisions { month_ago, now, change_pct: round_to((now - month_ago) / month_ago.abs() * 100.0, 2) })
    }
}

// ---------- Trailing twelve months ----------

const TOL: i64 = 3;

fn near(a: i64, b: i64, tol: i64) -> bool {
    (a - b).abs() <= tol
}

/// A quarter ending on `end` (±3 days): reported as such, or derived from two cumulative values with the same start
/// (Q4 = fiscal year − 9 months, Q3 = 9 months − 6 months…). Returns (start, value).
fn quarter(facts: &[Fact], end: i64) -> Option<(i64, f64)> {
    if let Some(f) = facts.iter().rev().find(|f| near(f.end, end, TOL) && f.days().is_some_and(|d| (80..=100).contains(&d))) {
        return Some((f.start?, f.val));
    }
    for a in facts.iter().filter(|f| near(f.end, end, TOL) && f.days().is_some_and(|d| d > 100 && d <= 380)) {
        let s = a.start?;
        if let Some(b) = facts.iter().find(|b| b.start.is_some_and(|bs| near(bs, s, TOL)) && (80..=100).contains(&(a.end - b.end))) {
            return Some((b.end + 1, a.val - b.val));
        }
    }
    None
}

/// Twelve months ending on `end`: the annual figure when there is one, else the sum of the last four quarters.
pub fn ttm(facts: &[Fact], end: i64) -> Option<f64> {
    if let Some(f) = facts.iter().rev().find(|f| near(f.end, end, TOL) && f.days().is_some_and(|d| (350..=380).contains(&d))) {
        return Some(f.val);
    }
    let mut sum = 0.0;
    let mut at = end;
    for _ in 0..4 {
        let (start, v) = quarter(facts, at)?;
        sum += v;
        at = start - 1;
    }
    // Four quarters must cover about a year.
    if !(350..=380).contains(&(end - at)) {
        return None;
    }
    Some(sum)
}

fn concept_ttm(f: &Facts, names: &[&str], end: i64) -> Option<f64> {
    names.iter().find_map(|n| f.concepts.get(*n).and_then(|s| ttm(s, end)))
}

/// Balance-sheet value at `end` (±`tol` days).
fn instant(f: &Facts, names: &[&str], end: i64, tol: i64) -> Option<f64> {
    names.iter().find_map(|n| {
        f.concepts
            .get(*n)
            .and_then(|s| s.iter().filter(|x| x.start.is_none() && near(x.end, end, tol)).min_by_key(|x| (x.end - end).abs()).map(|x| x.val))
    })
}

/// The end of a period about one year before `end`, as filed (a 52/53-week year moves it by a few days).
fn year_before(f: &Facts, names: &[&str], end: i64) -> Option<i64> {
    let target = end - 364;
    names.iter().filter_map(|n| f.concepts.get(*n)).flatten().map(|x| x.end).filter(|e| near(*e, target, 10)).min_by_key(|e| (e - target).abs())
}

/// Financial debt at `end` (leases excluded): long-term debt (current portion included) + short-term borrowings.
fn debt_at(f: &Facts, end: i64) -> Option<f64> {
    let get = |n: &str| instant(f, &[n], end, TOL);
    let long_total = get("LongTermDebt");
    let long_nc = get("LongTermDebtNoncurrent");
    let long_cur = get("LongTermDebtCurrent");
    let debt_cur = get("DebtCurrent");
    let short = [get("CommercialPaper"), get("ShortTermBorrowings")].into_iter().flatten().sum::<f64>();
    // DebtCurrent already holds the current portion of long-term debt and the short-term borrowings.
    if let Some(dc) = debt_cur {
        let nc = long_nc.or_else(|| Some(long_total? - long_cur?)).or(long_total)?;
        return Some(nc + dc);
    }
    let long = long_total.or_else(|| Some(long_nc? + long_cur.unwrap_or(0.0))).or(long_cur);
    match long {
        Some(l) => Some(l + short),
        None if short > 0.0 => Some(short),
        None => None,
    }
}

/// Share count series: the one (cover page or balance sheet) with the most recent date.
fn shares_series(f: &Facts) -> Option<&Vec<Fact>> {
    [SHARES_DEI, SHARES_GAAP].iter().filter_map(|n| f.concepts.get(*n)).filter(|s| !s.is_empty()).max_by_key(|s| s.last().map(|x| x.end).unwrap_or(0))
}

/// Latest share count and its change over about a year (%).
fn shares(f: &Facts) -> (Option<f64>, Option<f64>) {
    let Some(s) = shares_series(f) else { return (None, None) };
    let Some(last) = s.last().filter(|x| x.val > 0.0) else { return (None, None) };
    let target = last.end - 365;
    let before = s.iter().filter(|x| near(x.end, target, 60) && x.val > 0.0).min_by_key(|x| (x.end - target).abs());
    (Some(last.val), before.map(|b| round_to((last.val / b.val - 1.0) * 100.0, 2)))
}

fn growth(now: Option<f64>, before: Option<f64>) -> Option<f64> {
    let (n, b) = (now?, before?);
    // A change from a loss is not a growth rate.
    if b <= 0.0 {
        return None;
    }
    Some(round_to((n / b - 1.0) * 100.0, 2))
}

fn pct(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    let (a, b) = (a?, b?);
    if b <= 0.0 {
        return None;
    }
    let p = a / b * 100.0;
    (-1000.0..=100.0).contains(&p).then(|| round_to(p, 2))
}

/// Figures from the filings, before the market data (price) and Nasdaq.
#[derive(Debug, Clone, PartialEq)]
pub struct Filed {
    pub period: String,
    pub revenue: Option<f64>,
    pub revenue_growth: Option<f64>,
    pub net_income: Option<f64>,
    pub eps: Option<f64>,
    pub eps_growth: Option<f64>,
    pub gross_margin: Option<f64>,
    pub operating_margin: Option<f64>,
    pub net_margin: Option<f64>,
    pub free_cash_flow: Option<f64>,
    pub fcf_margin: Option<f64>,
    pub debt: Option<f64>,
    pub cash: Option<f64>,
    pub net_debt: Option<f64>,
    pub roe: Option<f64>,
    pub ebitda: Option<f64>,
    pub dividends_per_share: Option<f64>,
    pub shares: Option<f64>,
    pub share_change: Option<f64>,
    /// Date of the last period (days since 1970-01-01).
    pub end: i64,
}

/// Trailing-twelve-month figures from the facts; None when the company files no income statement.
pub fn filed(f: &Facts) -> Option<Filed> {
    // Last period: the most recent end of a reported net income (every operating company reports it).
    let (end, form) = NET_INCOME
        .iter()
        .filter_map(|n| f.concepts.get(*n))
        .flatten()
        .filter(|x| x.start.is_some())
        .max_by_key(|x| (x.end, x.filed))
        .map(|x| (x.end, x.form.clone()))?;
    let prev = year_before(f, &[REVENUE, NET_INCOME].concat(), end);
    let revenue = concept_ttm(f, REVENUE, end).filter(|v| *v > 0.0);
    let net_income = concept_ttm(f, NET_INCOME, end);
    let eps = concept_ttm(f, EPS, end).map(|v| round_to(v, 4));
    let gross = concept_ttm(f, GROSS_PROFIT, end);
    let operating = concept_ttm(f, OPERATING_INCOME, end);
    let ocf = concept_ttm(f, OPERATING_CASH, end);
    let capex = concept_ttm(f, CAPEX, end);
    let fcf = match (ocf, capex) {
        (Some(o), Some(c)) => Some(o - c),
        _ => None,
    };
    let depreciation = concept_ttm(f, DEPRECIATION, end);
    let ebitda = match (operating, depreciation) {
        (Some(o), Some(d)) => Some(o + d),
        _ => None,
    };
    let debt = debt_at(f, end);
    let cash = instant(f, CASH, end, TOL);
    let equity = instant(f, EQUITY, end, TOL);
    let equity_before = instant(f, EQUITY, end - 364, 10);
    let roe = match (net_income, equity, equity_before) {
        (Some(n), Some(e1), Some(e0)) if e1 + e0 > 0.0 => Some(round_to(n / ((e1 + e0) / 2.0) * 100.0, 2)),
        _ => None,
    };
    let (shares, share_change) = shares(f);
    let form_label = if form.starts_with("10-K") { "10-K" } else { "10-Q" };
    Some(Filed {
        period: format!("12 mois au {} ({form_label})", fr_date(end)),
        revenue,
        revenue_growth: prev.and_then(|p| growth(revenue, concept_ttm(f, REVENUE, p))),
        net_income,
        eps,
        eps_growth: prev.and_then(|p| growth(eps, concept_ttm(f, EPS, p))),
        gross_margin: pct(gross, revenue),
        operating_margin: pct(operating, revenue),
        net_margin: pct(net_income, revenue),
        free_cash_flow: fcf,
        fcf_margin: pct(fcf, revenue),
        debt,
        cash,
        net_debt: match (debt, cash) {
            (Some(d), Some(c)) => Some(d - c),
            _ => None,
        },
        roe,
        ebitda,
        dividends_per_share: concept_ttm(f, DIVIDENDS, end).filter(|v| *v >= 0.0),
        shares,
        share_change,
        end,
    })
}

/// Nasdaq data of a stock (each part None / empty when not published).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Street {
    pub next_earnings: Option<EarningsDate>,
    pub surprises: Vec<EarningsSurprise>,
    pub revisions: Option<Revisions>,
}

/// Everything together: valuation ratios need the price. `now` drops an earnings date already past.
pub fn assemble(filed: Option<&Filed>, street: Option<&Street>, price: Option<f64>, now: i64, why_no_filings: &str) -> StockFundamentals {
    let price = price.filter(|p| p.is_finite() && *p > 0.0);
    let f = filed;
    let market_cap = match (price, f.and_then(|x| x.shares)) {
        (Some(p), Some(s)) => Some(p * s),
        _ => None,
    };
    let eps = f.and_then(|x| x.eps);
    let per = match (price, eps) {
        (Some(p), Some(e)) if e > 0.0 => Some(round_to(p / e, 2)),
        _ => None,
    };
    let peg = match (per, f.and_then(|x| x.eps_growth)) {
        (Some(p), Some(g)) if g > 0.0 => Some(round_to(p / g, 2)),
        _ => None,
    };
    let ev_ebitda = match (market_cap, f.and_then(|x| x.debt), f.and_then(|x| x.cash), f.and_then(|x| x.ebitda)) {
        (Some(m), Some(d), Some(c), Some(e)) if e > 0.0 => Some(round_to((m + d - c) / e, 2)),
        _ => None,
    };
    let dividend_yield = match (price, f.and_then(|x| x.dividends_per_share)) {
        (Some(p), Some(d)) => Some(round_to(d / p * 100.0, 2)),
        _ => None,
    };
    let s = street.cloned().unwrap_or_default();
    let mut sources = Vec::new();
    if f.is_some() {
        sources.push("SEC EDGAR (10-K, 10-Q)");
    }
    if s.next_earnings.is_some() || !s.surprises.is_empty() || s.revisions.is_some() {
        sources.push("Nasdaq (données Zacks)");
    }
    StockFundamentals {
        period: f.map(|x| x.period.clone()).unwrap_or_else(|| why_no_filings.to_string()),
        revenue: f.and_then(|x| x.revenue),
        revenue_growth: f.and_then(|x| x.revenue_growth),
        net_income: f.and_then(|x| x.net_income),
        eps,
        eps_growth: f.and_then(|x| x.eps_growth),
        gross_margin: f.and_then(|x| x.gross_margin),
        operating_margin: f.and_then(|x| x.operating_margin),
        net_margin: f.and_then(|x| x.net_margin),
        free_cash_flow: f.and_then(|x| x.free_cash_flow),
        fcf_margin: f.and_then(|x| x.fcf_margin),
        debt: f.and_then(|x| x.debt),
        cash: f.and_then(|x| x.cash),
        net_debt: f.and_then(|x| x.net_debt),
        roe: f.and_then(|x| x.roe),
        per,
        peg,
        ev_ebitda,
        dividend_yield,
        share_change: f.and_then(|x| x.share_change),
        next_earnings: s.next_earnings.filter(|d| d.date >= now - 86_400_000),
        surprises: s.surprises,
        revisions: s.revisions,
        sector_note: SECTOR_NOTE.to_string(),
        source: if sources.is_empty() { "Aucune donnée publiée (SEC EDGAR, Nasdaq)".to_string() } else { sources.join(", ") },
    }
}

// ---------- Fetchers ----------

/// The SEC front end sometimes answers HTTP 403 to a request it accepts a second later: up to three tries, spaced
/// (far below its limit of 10 requests per second). A 404 is final.
async fn sec_json(url: &str) -> Result<Value> {
    let mut last = Error("SEC EDGAR".into());
    for attempt in 0..3u64 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(1_500 * attempt)).await;
        }
        match get_json_with(url, &[("User-Agent", SEC_USER_AGENT.as_str())], Duration::from_secs(20)).await {
            Ok(v) => return Ok(v),
            Err(e) if e.0 == "HTTP 404" => return Err(e),
            Err(e) => last = e,
        }
    }
    Err(last)
}

async fn nasdaq_json(url: &str) -> Result<Value> {
    get_json_with(url, &[("User-Agent", BROWSER_UA), ("Accept-Language", "en-US,en;q=0.9")], Duration::from_secs(10)).await
}

/// The SEC's list of tickers (www.sec.gov). Its front end refuses some HTTP clients (HTTP 403 to this server's
/// client while curl passes, checked 28/09/2026): a failure is kept for the day and EDGAR's own search is used.
async fn ciks() -> Arc<Option<HashMap<String, u64>>> {
    let r = cached("sec:tickers", 24 * HOUR, || async {
        let ua = [("User-Agent", SEC_USER_AGENT.as_str())];
        if let Ok(d) = get_json_with("https://www.sec.gov/files/company_tickers.json", &ua, Duration::from_secs(20)).await {
            let m = parse::tickers(&d);
            if !m.is_empty() {
                return Ok(Some(m));
            }
        }
        // Same list, plain text.
        if let Ok(t) = get_text_with("https://www.sec.gov/include/ticker.txt", &ua, Duration::from_secs(20)).await {
            let m = parse::ticker_txt(&t);
            if !m.is_empty() {
                return Ok(Some(m));
            }
        }
        Ok(None)
    })
    .await;
    r.unwrap_or_else(|_| Arc::new(None))
}

/// CIK of a ticker: the SEC list, else EDGAR's company search (efts.sec.gov). Ok(None): not a SEC filer.
async fn cik(symbol: &str) -> Result<Option<u64>> {
    let sym = sec_symbol(symbol);
    if let Some(m) = &*ciks().await {
        return Ok(m.get(&sym).copied());
    }
    let r = cached(&format!("sec:cik:{sym}"), 24 * HOUR, move || async move {
        let url = format!("https://efts.sec.gov/LATEST/search-index?keysTyped={}", crate::guard::encode_uri_component(&sym));
        Ok(parse::efts(&sec_json(&url).await?, &sym))
    })
    .await?;
    Ok(*r)
}

/// SEC symbols write class shares with a dash (BRK-B).
fn sec_symbol(symbol: &str) -> String {
    symbol.trim().to_uppercase().replace(['.', '/'], "-")
}

/// Ok(None): no filings under this symbol (fund, ETF, foreign issuer).
async fn company_facts(symbol: &str) -> Result<Option<Arc<Facts>>> {
    let Some(cik) = cik(symbol).await? else { return Ok(None) };
    let facts = cached(&format!("sec:facts:{cik}"), 12 * HOUR, move || async move {
        match sec_json(&format!("https://data.sec.gov/api/xbrl/companyfacts/CIK{cik:010}.json")).await {
            Ok(d) => Ok(Some(parse::company_facts(&d))),
            // A filer without XBRL financial statements (SPY's trust, for instance).
            Err(e) if e.0 == "HTTP 404" => Ok(None),
            Err(e) => Err(e),
        }
    })
    .await?;
    Ok((*facts).clone().map(Arc::new))
}

async fn street(symbol: &str) -> Result<Arc<Street>> {
    let sym = symbol.trim().to_uppercase().replace('-', ".");
    cached(&format!("nasdaq:street:{sym}"), 6 * HOUR, move || async move {
        let enc = crate::guard::encode_uri_component(&sym);
        let urls = [
            format!("https://api.nasdaq.com/api/analyst/{enc}/earnings-date"),
            format!("https://api.nasdaq.com/api/company/{enc}/earnings-surprise"),
            format!("https://api.nasdaq.com/api/analyst/{enc}/estimate-momentum"),
        ];
        let (d, s, r) = tokio::join!(nasdaq_json(&urls[0]), nasdaq_json(&urls[1]), nasdaq_json(&urls[2]));
        if let (Err(e), Err(_), Err(_)) = (&d, &s, &r) {
            return Err(e.clone());
        }
        Ok(Street {
            next_earnings: d.ok().as_ref().and_then(parse::earnings_date),
            surprises: s.ok().as_ref().map(parse::surprises).unwrap_or_default(),
            revisions: r.ok().as_ref().and_then(parse::revisions),
        })
    })
    .await
}

/// Fundamentals of a US-listed stock or fund. An error only when neither the SEC nor Nasdaq answered; a fund
/// without filings gets `None` figures and a `period` saying why.
pub async fn stock_fundamentals(symbol: &str, price: Option<f64>) -> Result<StockFundamentals> {
    let (facts, street) = tokio::join!(company_facts(symbol), street(symbol));
    if let (Err(e), Err(_)) = (&facts, &street) {
        return Err(Error(format!("SEC EDGAR et Nasdaq indisponibles : {e}")));
    }
    let why = match &facts {
        Err(e) => format!("Comptes non disponibles : SEC EDGAR n'a pas répondu ({e})"),
        Ok(None) => "Comptes non disponibles : aucun état financier (10-K, 10-Q) déposé à la SEC sous ce symbole (fonds, ETF ou émetteur étranger)"
            .to_string(),
        Ok(Some(_)) => "Comptes non disponibles : aucun compte de résultat dans les dépôts SEC (fonds ou ETF)".to_string(),
    };
    let filed = facts.ok().flatten().and_then(|f| filed(&f));
    let street = street.ok();
    Ok(assemble(filed.as_ref(), street.as_deref(), price, now_ms(), &why))
}

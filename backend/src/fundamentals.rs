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
use crate::engine::decision_types::{
    EarningsDate, EarningsSurprise, Peer, PeerComparison, RatioHistory, Revisions, Sector, StockFundamentals, ValuationHistory,
};
use crate::http::{Error, Result, get_json_with, get_text_with};
use crate::js::{fr, now_ms};
use crate::types::{Candle, Interval};

/// The SEC asks for a descriptive User-Agent naming the author. It answers HTTP 403 to any User-Agent containing
/// "github" (checked 28/09/2026), hence the repository written without its host. `ALTIM_SEC_USER_AGENT` replaces
/// it (e.g. to add the operator's contact address, which is never written in the code).
pub const SEC_UA: &str = "Altim/1.0 (open-source trading advice app; repository maxlestage/altim)";

static SEC_USER_AGENT: LazyLock<String> =
    LazyLock::new(|| std::env::var("ALTIM_SEC_USER_AGENT").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| SEC_UA.to_string()));
/// Nasdaq answers browsers only.
const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const HOUR: i64 = 3_600_000;

pub const SECTOR_NOTE: &str = "Comparaison sectorielle non disponible : pas assez de sociétés comparables dont les comptes ont pu être lus (classement Nasdaq, comptes SEC EDGAR).";
pub const GUIDANCE_NOTE: &str = "Prévisions de la direction non disponibles : aucune source gratuite ne les publie sous une forme structurée et vérifiable (elles figurent en texte libre dans les communiqués déposés à la SEC).";
/// US federal statutory corporate tax rate since 2018, used for the ROIC only when the effective rate cannot be
/// computed (said so to the user).
const STATUTORY_TAX: f64 = 0.21;

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
const TAX: &[&str] = &["IncomeTaxExpenseBenefit"];
const PRETAX: &[&str] = &[
    "IncomeLossFromContinuingOperationsBeforeIncomeTaxesExtraordinaryItemsNoncontrollingInterest",
    "IncomeLossFromContinuingOperationsBeforeIncomeTaxesMinorityInterestAndIncomeLossFromEquityMethodInvestments",
];
const SHARES_GAAP: &str = "CommonStockSharesOutstanding";
const SHARES_DEI: &str = "EntityCommonStockSharesOutstanding";

fn all_concepts() -> impl Iterator<Item = &'static str> {
    [REVENUE, NET_INCOME, EPS, GROSS_PROFIT, OPERATING_INCOME, OPERATING_CASH, CAPEX, DEPRECIATION, DIVIDENDS, EQUITY, CASH, DEBT_PARTS, TAX, PRETAX]
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

    /// SEC `submissions/CIK##########.json`: the SIC code and its description, grouped by SIC division.
    pub fn submissions(d: &Value) -> Option<Sector> {
        let sic = d.get("sic").and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty())?.to_string();
        let label = sic_division(&sic)?;
        Some(Sector {
            label: label.to_string(),
            sic_description: d.get("sicDescription").and_then(|x| x.as_str()).unwrap_or("").trim().to_string(),
            sic,
            source: "SEC EDGAR (code SIC déclaré)".to_string(),
        })
    }

    /// SEC submissions read for the sector exposure: the operating sector, or a fund (no SIC code and an entity
    /// type other than "operating": SPY's and QQQ's trusts, for instance).
    pub fn filer(d: &Value) -> SecFiler {
        let no_sic = d.get("sic").and_then(|x| x.as_str()).is_none_or(|s| s.trim().is_empty());
        let entity = d.get("entityType").and_then(|x| x.as_str()).unwrap_or("").trim().to_lowercase();
        SecFiler {
            sector: submissions(d),
            fund: no_sic && !entity.is_empty() && entity != "operating",
            name: d.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string(),
        }
    }

    /// Short French label of a SIC division (first two digits of the code, SEC / OSHA division table).
    pub fn sic_division(sic: &str) -> Option<&'static str> {
        let major: u32 = sic.get(..2)?.parse().ok()?;
        Some(match major {
            1..=9 => "Agriculture",
            10..=14 => "Mines, pétrole et gaz",
            15..=17 => "Construction",
            20..=39 => "Industrie",
            40..=49 => "Transports, télécoms et énergie",
            50..=51 => "Commerce de gros",
            52..=59 => "Commerce de détail",
            60..=67 => "Finance et immobilier",
            70..=89 => "Services",
            91..=97 => "Administration publique",
            99 => "Non classé",
            _ => return None,
        })
    }

    /// Nasdaq screener (`api.nasdaq.com/api/screener/stocks?tableonly=true&download=true`): every US listing with
    /// its activity (`industry`), last price and market cap. Rows without an activity are left out.
    pub fn screener(d: &Value) -> Vec<Listing> {
        let rows = d.get("data").and_then(|x| x.get("rows")).and_then(|x| x.as_array());
        rows.map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    let industry = r.get("industry")?.as_str()?.trim();
                    if industry.is_empty() {
                        return None;
                    }
                    let raw = r.get("name").and_then(|x| x.as_str()).unwrap_or("");
                    let lower = raw.to_lowercase();
                    Some(Listing {
                        symbol: r.get("symbol")?.as_str()?.trim().to_uppercase().replace(['.', '/'], "-"),
                        name: crate::universe::clean_stock_name(raw),
                        common: (lower.contains("common stock") || lower.contains("ordinary share"))
                            && !lower.contains("depositary")
                            && !lower.contains("preferred"),
                        industry: industry.to_string(),
                        sector: r.get("sector").and_then(|x| x.as_str()).unwrap_or("").trim().to_string(),
                        price: num(r.get("lastsale")).filter(|p| *p > 0.0),
                        market_cap: num(r.get("marketCap")).filter(|c| *c > 0.0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
    }
}

/// A line of the Nasdaq screener.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub symbol: String,
    pub name: String,
    pub industry: String,
    /// Nasdaq sector ("Technology"), empty when the screener gives none.
    pub sector: String,
    /// Common shares (the ones the filed EPS and share count are about): not preferred, depositary shares, units…
    pub common: bool,
    pub price: Option<f64>,
    pub market_cap: Option<f64>,
}

/// What the SEC submissions say about a filer.
#[derive(Debug, Clone, PartialEq)]
pub struct SecFiler {
    /// Operating sector (SIC division), None without a SIC code.
    pub sector: Option<Sector>,
    /// No SIC code and a non-operating entity: a fund or a trust (ETF).
    pub fund: bool,
    pub name: String,
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
    /// Stockholders' equity at `end`.
    pub equity: Option<f64>,
    pub operating_income: Option<f64>,
    /// Effective tax rate (fraction): income tax ÷ pre-tax income, kept within 0–0.5; None when pre-tax income is
    /// not positive or not filed.
    pub tax_rate: Option<f64>,
    /// Date of the last period and of its filing (days since 1970-01-01).
    pub end: i64,
    pub filed: i64,
}

/// Trailing-twelve-month figures from the facts; None when the company files no income statement.
pub fn filed(f: &Facts) -> Option<Filed> {
    // Last period: the most recent end of a reported net income (every operating company reports it).
    let (end, form, filed_on) = NET_INCOME
        .iter()
        .filter_map(|n| f.concepts.get(*n))
        .flatten()
        .filter(|x| x.start.is_some())
        .max_by_key(|x| (x.end, x.filed))
        .map(|x| (x.end, x.form.clone(), x.filed))?;
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
    let tax_rate = match (concept_ttm(f, TAX, end), concept_ttm(f, PRETAX, end)) {
        (Some(t), Some(p)) if p > 0.0 => Some((t / p).clamp(0.0, 0.5)),
        _ => None,
    };
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
        equity,
        operating_income: operating,
        tax_rate,
        end,
        filed: filed_on,
    })
}

/// Nasdaq data of a stock (each part None / empty when not published).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Street {
    pub next_earnings: Option<EarningsDate>,
    pub surprises: Vec<EarningsSurprise>,
    pub revisions: Option<Revisions>,
}

/// Price ratios of filed figures: (market cap, P/E, P/S, P/B). Market cap = price × latest share count; P/E =
/// price ÷ diluted EPS (12 months, positive only); P/S = market cap ÷ revenue (12 months); P/B = market cap ÷
/// stockholders' equity (positive only).
pub fn price_ratios(f: Option<&Filed>, price: Option<f64>) -> (Option<f64>, Option<f64>, Option<f64>, Option<f64>) {
    let price = price.filter(|p| p.is_finite() && *p > 0.0);
    let market_cap = match (price, f.and_then(|x| x.shares)) {
        (Some(p), Some(s)) => Some(p * s),
        _ => None,
    };
    let per = match (price, f.and_then(|x| x.eps)) {
        (Some(p), Some(e)) if e > 0.0 => Some(round_to(p / e, 2)),
        _ => None,
    };
    let ps = match (market_cap, f.and_then(|x| x.revenue)) {
        (Some(m), Some(r)) if r > 0.0 => Some(round_to(m / r, 2)),
        _ => None,
    };
    let pb = match (market_cap, f.and_then(|x| x.equity)) {
        (Some(m), Some(e)) if e > 0.0 => Some(round_to(m / e, 2)),
        _ => None,
    };
    (market_cap, per, ps, pb)
}

/// ROIC, %, and the tax rate used (%, statutory or not): NOPAT = operating income × (1 − tax rate), invested
/// capital = debt + equity − cash at the period end (positive only).
pub fn roic(f: &Filed) -> Option<(f64, f64, bool)> {
    let (op, debt, equity, cash) = (f.operating_income?, f.debt?, f.equity?, f.cash?);
    let invested = debt + equity - cash;
    if invested <= 0.0 {
        return None;
    }
    let (rate, statutory) = match f.tax_rate {
        Some(t) => (t, false),
        None => (STATUTORY_TAX, true),
    };
    Some((round_to(op * (1.0 - rate) / invested * 100.0, 2), round_to(rate * 100.0, 1), statutory))
}

fn ms_of(days: i64) -> i64 {
    days * 86_400_000
}

/// Everything together: valuation ratios need the price. `now` drops an earnings date already past.
pub fn assemble(filed: Option<&Filed>, street: Option<&Street>, price: Option<f64>, now: i64, why_no_filings: &str) -> StockFundamentals {
    let price = price.filter(|p| p.is_finite() && *p > 0.0);
    let f = filed;
    let (market_cap, per, ps, pb) = price_ratios(f, price);
    let eps = f.and_then(|x| x.eps);
    let roic = f.and_then(roic);
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
        ps,
        pb,
        roic: roic.map(|r| r.0),
        roic_tax_rate: roic.map(|r| r.1),
        roic_tax_statutory: roic.is_some_and(|r| r.2),
        period_end: f.map(|x| ms_of(x.end)),
        filed_at: f.map(|x| ms_of(x.filed)),
        sector: None,
        valuation_history: None,
        peers: None,
        valuation_verdict: None,
        guidance: GUIDANCE_NOTE.to_string(),
    }
}

// ---------- Valuation against its own history ----------

/// Filings are due 40 days after a quarter (large filers): a twelve-month figure is used from 45 days after its
/// period end, so that no day uses a figure not yet published.
const PUBLICATION_LAG: i64 = 45;
/// A figure older than this (days after its period end) is stale: no ratio that day.
const STALE_AFTER: i64 = 400;
/// History window: 5 years.
const HISTORY_DAYS: i64 = 5 * 365;
/// At least a year of daily values to compare with.
const MIN_HISTORY: usize = 250;

/// Twelve-month values of `names` at each period end filed: (end, value), by end.
fn ttm_points(f: &Facts, names: &[&str]) -> Vec<(i64, f64)> {
    let mut ends: Vec<i64> = names.iter().filter_map(|n| f.concepts.get(*n)).flatten().filter(|x| x.start.is_some()).map(|x| x.end).collect();
    ends.sort_unstable();
    ends.dedup_by(|a, b| near(*a, *b, TOL));
    ends.into_iter().filter_map(|end| concept_ttm(f, names, end).map(|v| (end, v))).collect()
}

/// The last point usable on day `t`: published (period end + 45 days) and not stale.
fn usable(points: &[(i64, f64)], t: i64) -> Option<(i64, f64)> {
    points.iter().rev().find(|(end, _)| end + PUBLICATION_LAG <= t && t - end <= STALE_AFTER).copied()
}

/// A share count on the same basis as today's (a stock split multiplies it): within ×0.67–×1.5 of the latest.
fn same_basis(shares: f64, latest: f64) -> bool {
    latest > 0.0 && (0.67..=1.5).contains(&(shares / latest))
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

fn stats(current: f64, values: &[(i64, f64)]) -> Option<RatioHistory> {
    if values.len() < MIN_HISTORY || !current.is_finite() || current <= 0.0 {
        return None;
    }
    let mut v: Vec<f64> = values.iter().map(|x| x.1).collect();
    let below = v.iter().filter(|x| **x <= current).count();
    let med = median(&mut v)?;
    Some(RatioHistory {
        current,
        median: round_to(med, 2),
        min: round_to(v[0], 2),
        max: round_to(v[v.len() - 1], 2),
        percentile: round_to(below as f64 / v.len() as f64 * 100.0, 0),
        days: v.len(),
        from: values[0].0,
        to: values[values.len() - 1].0,
    })
}

pub const HISTORY_METHOD: &str = "Chaque jour : cours de clôture ÷ BPA dilué des 12 derniers mois (PER), capitalisation ÷ chiffre d'affaires des 12 derniers mois (P/S), chiffres retenus 45 jours après la fin de leur trimestre ; jours antérieurs à un fractionnement d'actions pas encore retraité dans les comptes exclus";

/// P/E of each day of `window` (ms, day number, close): close ÷ diluted twelve-month EPS usable that day (published
/// 45 days after its period end), its basis checked with the implied share count (net income ÷ EPS); days with a
/// loss or a share basis different from today's left out.
fn per_series(f: &Facts, window: &[(i64, i64, f64)]) -> Vec<(i64, f64)> {
    let eps = ttm_points(f, EPS);
    let income = ttm_points(f, NET_INCOME);
    let implied = |end: i64, e: f64| income.iter().find(|(x, _)| near(*x, end, TOL)).map(|(_, n)| n / e).filter(|s| *s > 0.0);
    let Some(latest) = eps.iter().rev().filter(|(_, e)| *e > 0.0).find_map(|(end, e)| implied(*end, *e)) else { return vec![] };
    window
        .iter()
        .filter_map(|(ms, day, c)| {
            let (end, e) = usable(&eps, *day)?;
            (e > 0.0 && same_basis(implied(end, e)?, latest)).then(|| (*ms, c / e))
        })
        .collect()
}

/// Daily P/E over all of `closes` (ms, close), same rules as the valuation history (strategy comparator).
pub fn daily_per(f: &Facts, closes: &[(i64, f64)]) -> Vec<(i64, f64)> {
    let window: Vec<(i64, i64, f64)> = closes.iter().filter(|(_, c)| *c > 0.0).map(|(t, c)| (*t, t.div_euclid(86_400_000), *c)).collect();
    per_series(f, &window)
}

/// P/E and P/S of each day (close, twelve months filed) over up to 5 years, against today's. `closes`: (ms, close),
/// split-adjusted as the price sources publish them. A day whose share basis differs from today's (split not yet
/// restated in the filings) is left out rather than adjusted.
pub fn valuation_history(f: &Facts, closes: &[(i64, f64)], per: Option<f64>, ps: Option<f64>, price_source: &str) -> Option<ValuationHistory> {
    let last = closes.last()?.0;
    let window: Vec<(i64, i64, f64)> =
        closes.iter().filter(|(t, c)| *c > 0.0 && last - t <= ms_of(HISTORY_DAYS)).map(|(t, c)| (*t, t.div_euclid(86_400_000), *c)).collect();
    let per_days: Vec<(i64, f64)> = if per.is_some() { per_series(f, &window) } else { vec![] };
    // P/S: close × shares outstanding (last count filed) ÷ revenue.
    let revenue = ttm_points(f, REVENUE);
    let shares: Vec<(i64, f64)> = shares_series(f).map(|s| s.iter().filter(|x| x.val > 0.0).map(|x| (x.end, x.val)).collect()).unwrap_or_default();
    let ps_days: Vec<(i64, f64)> = match (ps, shares.last().map(|x| x.1)) {
        (Some(_), Some(latest)) => window
            .iter()
            .filter_map(|(ms, day, c)| {
                let (_, r) = usable(&revenue, *day).filter(|(_, r)| *r > 0.0)?;
                let (_, n) = shares.iter().rev().find(|(end, _)| end <= day && day - end <= STALE_AFTER)?;
                same_basis(*n, latest).then(|| (*ms, c * n / r))
            })
            .collect(),
        _ => vec![],
    };
    let per_h = per.and_then(|p| stats(p, &per_days));
    let ps_h = ps.and_then(|p| stats(p, &ps_days));
    if per_h.is_none() && ps_h.is_none() {
        return None;
    }
    Some(ValuationHistory {
        per: per_h,
        ps: ps_h,
        method: HISTORY_METHOD.into(),
        source: format!("SEC EDGAR (BPA, chiffre d'affaires, actions), cours de clôture {price_source}"),
    })
}

/// Valuation against its own history, then whether growth justifies it (PEG, EPS growth). None without history.
pub fn valuation_verdict(history: Option<&ValuationHistory>, peg: Option<f64>, eps_growth: Option<f64>) -> Option<String> {
    let h = history?;
    let (name, r) = match (&h.per, &h.ps) {
        (Some(r), _) => ("PER", r),
        (None, Some(r)) => ("P/S", r),
        _ => return None,
    };
    let years = ((r.to - r.from) as f64 / (365.25 * 86_400_000.0)).round().max(1.0);
    let level = if r.percentile >= 80.0 {
        "élevée"
    } else if r.percentile <= 20.0 {
        "basse"
    } else {
        "dans la moyenne"
    };
    let mut s = format!(
        "Valorisation {level} par rapport à sa propre histoire ({name} de {} ; plus haut que {} % des jours sur {} an{})",
        fr(r.current, 0, 1),
        fr(r.percentile, 0, 0),
        fr(years, 0, 0),
        if years > 1.0 { "s" } else { "" }
    );
    let growth = match (peg.filter(|p| *p > 0.0), eps_growth) {
        (_, Some(g)) if g <= 0.0 => Some(format!("bénéfice par action en recul ({} % sur un an)", fr(g, 0, 1))),
        (Some(p), _) if p < 1.0 => Some(format!("croissance qui la justifie (PEG {})", fr(p, 0, 2))),
        (Some(p), _) if p < 2.0 => Some(format!("croissance qui la justifie en partie (PEG {})", fr(p, 0, 2))),
        (Some(p), _) => Some(format!("croissance qui ne la justifie pas (PEG {})", fr(p, 0, 2))),
        _ => None,
    };
    if let Some(g) = growth {
        s.push_str(", ");
        s.push_str(&g);
    }
    Some(s)
}

// ---------- Peers ----------

/// Up to `n` listings of the same activity closest in market cap (logarithmic distance), the stock itself excluded;
/// with the activity's name.
pub fn pick_peers<'a>(rows: &'a [Listing], symbol: &str, n: usize) -> Option<(&'a str, Vec<&'a Listing>)> {
    let sym = sec_symbol(symbol);
    let me = rows.iter().find(|r| r.symbol == sym)?;
    let cap = me.market_cap?;
    let dist = |r: &Listing| r.market_cap.map_or(f64::MAX, |c| (c / cap).ln().abs());
    let mut same: Vec<&Listing> =
        rows.iter().filter(|r| r.industry == me.industry && r.common && r.symbol != sym && r.price.is_some() && r.market_cap.is_some()).collect();
    same.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
    same.truncate(n);
    Some((me.industry.as_str(), same))
}

/// A peer's figures: its filings, the screener's price.
pub fn peer(listing: &Listing, f: &Filed) -> Peer {
    let (_, per, ps, _) = price_ratios(Some(f), listing.price);
    Peer {
        symbol: listing.symbol.clone(),
        name: listing.name.clone(),
        per,
        ps,
        operating_margin: f.operating_margin,
        net_margin: f.net_margin,
        revenue_growth: f.revenue_growth,
        period_end: ms_of(f.end),
    }
}

/// Medians of the peers (each needs 3 values); None under 3 peers.
pub fn compare(group: &str, peers: Vec<Peer>, date: i64) -> Option<PeerComparison> {
    if peers.len() < 3 {
        return None;
    }
    let med = |get: fn(&Peer) -> Option<f64>| {
        let mut v: Vec<f64> = peers.iter().filter_map(get).collect();
        if v.len() < 3 { None } else { median(&mut v).map(|m| round_to(m, 2)) }
    };
    Some(PeerComparison {
        group: group.to_string(),
        median_per: med(|p| p.per),
        median_ps: med(|p| p.ps),
        median_operating_margin: med(|p| p.operating_margin),
        median_net_margin: med(|p| p.net_margin),
        median_revenue_growth: med(|p| p.revenue_growth),
        peers,
        date,
        source: "Nasdaq (activité, cours), SEC EDGAR (comptes de chaque société)".into(),
    })
}

/// One sentence comparing the stock with its peers' medians.
pub fn peer_note(s: &StockFundamentals, c: &PeerComparison) -> String {
    let fr1 = |v: f64| fr(v, 0, 1);
    let mut parts = vec![];
    if let (Some(a), Some(b)) = (s.per, c.median_per) {
        parts.push(format!("PER {} contre {}", fr1(a), fr1(b)));
    }
    if let (Some(a), Some(b)) = (s.ps, c.median_ps) {
        parts.push(format!("P/S {} contre {}", fr1(a), fr1(b)));
    }
    if let (Some(a), Some(b)) = (s.operating_margin, c.median_operating_margin) {
        parts.push(format!("marge opérationnelle {} % contre {} %", fr1(a), fr1(b)));
    }
    if let (Some(a), Some(b)) = (s.revenue_growth, c.median_revenue_growth) {
        parts.push(format!("croissance du chiffre d'affaires {} % contre {} %", fr1(a), fr1(b)));
    }
    let head = format!("Comparée à {} sociétés de même activité ({}, classement Nasdaq)", c.peers.len(), c.group);
    if parts.is_empty() {
        return format!("{head} : aucun ratio comparable");
    }
    format!("{head}, en médiane : {}", parts.join(", "))
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

/// SEC submissions of a symbol (SIC code, fund or not), cached a week (a SIC code hardly ever changes). Ok(None):
/// no SEC filer under this symbol.
pub async fn sec_filer(symbol: &str) -> Result<Option<SecFiler>> {
    let Some(cik) = cik(symbol).await? else { return Ok(None) };
    let f = cached(&format!("sec:filer:{cik}"), 7 * 24 * HOUR, move || async move {
        Ok(parse::filer(&sec_json(&format!("https://data.sec.gov/submissions/CIK{cik:010}.json")).await?))
    })
    .await?;
    Ok(Some((*f).clone()))
}

/// Sector from the SEC submissions (SIC code).
async fn sector(symbol: &str) -> Option<Sector> {
    sec_filer(symbol).await.ok().flatten()?.sector
}

/// Nasdaq screener (≈ 2 MB, every US listing), cached for the day, with the time it was read.
pub async fn screener() -> Result<Arc<(Vec<Listing>, i64)>> {
    cached("nasdaq:screener:industry", 24 * HOUR, || async {
        let d = get_json_with(
            "https://api.nasdaq.com/api/screener/stocks?tableonly=true&download=true",
            &[("User-Agent", BROWSER_UA), ("Accept-Language", "en-US,en;q=0.9")],
            Duration::from_secs(30),
        )
        .await?;
        let rows = parse::screener(&d);
        if rows.len() < 1000 {
            return Err(Error("screener Nasdaq incomplet".into()));
        }
        Ok((rows, now_ms()))
    })
    .await
}

/// Up to 6 peers of the same activity whose filings were read (cached for the day). None under 3.
async fn peers(symbol: &str) -> Option<PeerComparison> {
    let sym = sec_symbol(symbol);
    cached(&format!("sec:peers:{sym}"), 24 * HOUR, move || async move {
        let list = screener().await?;
        let Some((group, candidates)) = pick_peers(&list.0, &sym, 6) else { return Ok(None) };
        let own = cik(&sym).await.ok().flatten();
        let reads = futures::future::join_all(candidates.iter().map(|l| async move {
            let c = cik(&l.symbol).await.ok().flatten()?;
            let facts = company_facts(&l.symbol).await.ok().flatten()?;
            filed(&facts).map(|f| (c, peer(l, &f)))
        }))
        .await;
        // One company per CIK (share classes), never the company itself.
        let mut seen = vec![];
        let mut out = vec![];
        for (c, p) in reads.into_iter().flatten() {
            if Some(c) != own && !seen.contains(&c) {
                seen.push(c);
                out.push(p);
            }
        }
        Ok(compare(group, out, list.1))
    })
    .await
    .ok()
    .and_then(|v| (*v).clone())
}

/// Up to 5 years of daily closes (StockAnalysis, else Robinhood), cached for the day; `fallback` (the decision's own
/// daily candles, ≈ 3 years) when both fail.
async fn long_closes(symbol: &str, fallback: &[Candle]) -> (Vec<(i64, f64)>, String) {
    let sym = symbol.to_string();
    let r = cached(&format!("stock:closes5y:{sym}"), 24 * HOUR, move || async move {
        for name in ["StockAnalysis", "Robinhood"] {
            let Some(s) = crate::market::STOCK_SOURCES.iter().find(|s| s.name == name) else { continue };
            if let Ok(c) = (s.fetch)(sym.clone(), Interval::D1).await {
                let c = crate::market::stock_closed(c, Interval::D1);
                if c.len() >= MIN_HISTORY {
                    return Ok((c.iter().map(|x| (x.time, x.close)).collect::<Vec<_>>(), name.to_string()));
                }
            }
        }
        Err(Error("historique 5 ans indisponible".into()))
    })
    .await;
    match r {
        Ok(v) => (*v).clone(),
        Err(_) => (fallback.iter().map(|x| (x.time, x.close)).collect(), "consensus des plateformes".to_string()),
    }
}

/// Daily P/E of a stock over up to 5 years (SEC EDGAR twelve-month EPS, daily closes) with its sources, for the
/// strategy comparator. Err with the reason when the filings or a usable EPS are missing.
pub async fn per_history(symbol: &str, daily: &[Candle]) -> Result<(Vec<(i64, f64)>, String)> {
    let facts = company_facts(symbol).await?.ok_or_else(|| Error("aucun état financier déposé à la SEC sous ce symbole".into()))?;
    let (closes, source) = long_closes(symbol, daily).await;
    let per = daily_per(&facts, &closes);
    if per.is_empty() {
        return Err(Error("aucun PER calculable (pertes, ou BPA absent des comptes)".into()));
    }
    Ok((per, format!("SEC EDGAR (BPA dilué 12 mois), cours de clôture {source}")))
}

/// Fundamentals of a US-listed stock or fund. An error only when neither the SEC nor Nasdaq answered; a fund
/// without filings gets `None` figures and a `period` saying why. `daily`: the decision's daily candles, used for
/// the valuation history when no 5-year history answers.
pub async fn stock_fundamentals(symbol: &str, price: Option<f64>, daily: &[Candle]) -> Result<StockFundamentals> {
    let (facts, street, sector, peers) = tokio::join!(company_facts(symbol), street(symbol), sector(symbol), peers(symbol));
    if let (Err(e), Err(_)) = (&facts, &street) {
        return Err(Error(format!("SEC EDGAR et Nasdaq indisponibles : {e}")));
    }
    let why = match &facts {
        Err(e) => format!("Comptes non disponibles : SEC EDGAR n'a pas répondu ({e})"),
        Ok(None) => "Comptes non disponibles : aucun état financier (10-K, 10-Q) déposé à la SEC sous ce symbole (fonds, ETF ou émetteur étranger)"
            .to_string(),
        Ok(Some(_)) => "Comptes non disponibles : aucun compte de résultat dans les dépôts SEC (fonds ou ETF)".to_string(),
    };
    let facts = facts.ok().flatten();
    let filed = facts.as_deref().and_then(filed);
    let street = street.ok();
    let mut out = assemble(filed.as_ref(), street.as_deref(), price, now_ms(), &why);
    out.sector = sector;
    if let (Some(f), true) = (&facts, filed.is_some() && (out.per.is_some() || out.ps.is_some())) {
        let (closes, source) = long_closes(symbol, daily).await;
        out.valuation_history = valuation_history(f, &closes, out.per, out.ps, &source);
        out.valuation_verdict = valuation_verdict(out.valuation_history.as_ref(), out.peg, out.eps_growth);
    }
    if filed.is_some() {
        if let Some(c) = peers {
            out.sector_note = peer_note(&out, &c);
            out.peers = Some(c);
        }
    }
    Ok(out)
}

// ---------- Change of the growth at the last filing (opportunities scan) ----------

/// Growth of the last filed period against the period before it (trailing twelve months, %).
#[derive(Debug, Clone, PartialEq)]
pub struct FilingTrend {
    /// "10-Q" | "10-K", its filing date and the end of its period (days since 1970-01-01).
    pub form: String,
    pub filed: i64,
    pub end: i64,
    pub revenue_growth: Option<f64>,
    pub revenue_growth_before: Option<f64>,
    pub eps_growth: Option<f64>,
    pub eps_growth_before: Option<f64>,
}

fn growth_at(f: &Facts, names: &[&str], end: i64) -> Option<f64> {
    let prev = year_before(f, &[REVENUE, NET_INCOME].concat(), end)?;
    growth(concept_ttm(f, names, end), concept_ttm(f, names, prev))
}

/// Last filed period (its filing date) and the revenue / EPS growth there and one period earlier.
pub fn filing_trend(f: &Facts) -> Option<FilingTrend> {
    let facts: Vec<&Fact> = NET_INCOME.iter().filter_map(|n| f.concepts.get(*n)).flatten().filter(|x| x.start.is_some()).collect();
    let last = facts.iter().max_by_key(|x| (x.end, x.filed))?;
    // The period before: the latest end at least two months earlier.
    let before = facts.iter().map(|x| x.end).filter(|e| *e <= last.end - 60).max();
    let at = |end: i64| (growth_at(f, REVENUE, end), growth_at(f, EPS, end));
    let (rg, eg) = at(last.end);
    let (rb, eb) = before.map(at).unwrap_or((None, None));
    Some(FilingTrend {
        form: if last.form.starts_with("10-K") { "10-K" } else { "10-Q" }.to_string(),
        filed: last.filed,
        end: last.end,
        revenue_growth: rg,
        revenue_growth_before: rb,
        eps_growth: eg,
        eps_growth_before: eb,
    })
}

/// `filing_trend` of a listed company (cached filings); Ok(None) for a fund or a non-filer.
pub async fn filing_trend_for(symbol: &str) -> Result<Option<FilingTrend>> {
    Ok(company_facts(symbol).await?.and_then(|f| filing_trend(&f)))
}

/// Consensus EPS revisions over a month (Nasdaq, Zacks data; cached).
pub async fn revisions_for(symbol: &str) -> Result<Option<Revisions>> {
    Ok(street(symbol).await?.revisions.clone())
}

/// A date in days since 1970-01-01, in words ("01/08/2026").
pub fn day_label(days: i64) -> String {
    fr_date(days)
}

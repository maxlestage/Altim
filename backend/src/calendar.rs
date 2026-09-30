//! Calendar of upcoming events (`/api/calendar`): major economic releases, central bank decisions, earnings,
//! dividends, splits and IPOs. Everything comes from a named free source (Nasdaq's calendars, the Federal Reserve's
//! and the ECB's official meeting schedules), fetched per day with bounded concurrency and cached per day; a source
//! or a day that fails is reported, the rest still comes. What no free source covers is listed in `notCovered`.
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone};
use chrono_tz::America::New_York;
use chrono_tz::Europe::Paris;
use futures::StreamExt;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::cache::cached;
use crate::http::{Error, Result, err, get_json_with, get_text_with};
use crate::js::now_ms;
use crate::types::Kind;

const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const TIMEOUT: Duration = Duration::from_secs(10);
/// Parallel upstream requests of one calendar (30 days × 4 Nasdaq calendars = 120 requests on a cold cache, about
/// 2 s each at Nasdaq).
const CONCURRENCY: usize = 16;
/// The answer is given after this long whatever is missing (the Heroku router cuts at 30 s): the requests still
/// running go on and land in the cache, the days they cover are reported as failed.
const DEADLINE: Duration = Duration::from_secs(20);
pub const DEFAULT_DAYS: u32 = 14;
pub const MAX_DAYS: u32 = 30;
/// Without a symbol filter, earnings, dividends and splits are kept for the largest US companies only.
const TOP_RANK: i64 = 500;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;

pub const FED_URL: &str = "https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm";
pub const ECB_URL: &str = "https://www.ecb.europa.eu/press/calendars/mgcgc/html/index.en.html";
const NASDAQ_ECO_PAGE: &str = "https://www.nasdaq.com/market-activity/economic-calendar";
const NASDAQ_DIVIDENDS_PAGE: &str = "https://www.nasdaq.com/market-activity/dividends";
const NASDAQ_SPLITS_PAGE: &str = "https://www.nasdaq.com/market-activity/stock-splits";
const NASDAQ_IPO_PAGE: &str = "https://www.nasdaq.com/market-activity/ipos";

pub use altim_core::calendar::{CalendarEvent, Category, EventKind, Importance};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub name: String,
    pub ok: bool,
    /// Days ("YYYY-MM-DD") or months ("YYYY-MM") that could not be read.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Calendar {
    pub as_of: i64,
    pub days: u32,
    /// First and last day of the window, "YYYY-MM-DD" (Paris time).
    pub from: String,
    pub to: String,
    pub events: Vec<CalendarEvent>,
    pub sources: Vec<SourceStatus>,
    pub not_covered: Vec<String>,
}

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn day_ms(d: NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0).map(|t| t.and_utc().timestamp_millis()).unwrap_or(0)
}

/// Today in Paris (the user's day).
pub fn today_paris(now: i64) -> NaiveDate {
    chrono::DateTime::from_timestamp_millis(now).map(|d| d.with_timezone(&Paris).date_naive()).unwrap_or_default()
}

/// "9/30/2026" (Nasdaq) → date.
/// A Nasdaq dollar amount ("$31.24", "($0.05)" for a negative one) converted like every server amount; kept as
/// written when it is not a plain amount.
fn dollars(s: &str) -> String {
    let t = s.trim();
    let neg = t.starts_with('(') && t.ends_with(')') || t.starts_with('-');
    let n = t.trim_matches(|c| c == '(' || c == ')' || c == '-').replace(['$', ','], "").parse::<f64>().ok().filter(|v| v.is_finite());
    match n {
        Some(v) => format!("{}{}", if neg { "−" } else { "" }, crate::fx::money_with(v, |v| crate::js::fr(v, 2, 2))),
        None => t.to_string(),
    }
}

/// IPO price or range as Nasdaq writes it ("18.00" or "18.00-20.00", dollars), converted like every server amount;
/// kept as written, in dollars, when it is not plain numbers.
fn ipo_price(p: &str) -> String {
    let parts: Vec<Option<f64>> = p.split('-').map(|x| x.trim().replace(['$', ','], "").parse::<f64>().ok().filter(|v| v.is_finite())).collect();
    if parts.iter().all(Option::is_some) {
        let fmt = |v: f64| crate::js::fr(v, 2, 2);
        let (_, sym) = crate::fx::convert(0.0);
        let vals: Vec<String> = parts.iter().map(|v| fmt(crate::fx::convert(v.unwrap()).0)).collect();
        return format!("{} {sym}", vals.join(" – "));
    }
    format!("{p} $")
}

fn us_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%m/%d/%Y").ok()
}

/// A day-level event (no time).
fn at_day(d: NaiveDate) -> (i64, String) {
    (day_ms(d), ymd(d))
}

/// Nasdaq's economic calendar gives New York times ("08:30" for the payrolls): instant, Paris day and Paris time.
fn at_new_york(d: NaiveDate, hhmm: &str) -> Option<(i64, String, String)> {
    let t = NaiveTime::parse_from_str(hhmm.trim(), "%H:%M").ok()?;
    let ny = New_York.from_local_datetime(&d.and_time(t)).earliest()?;
    let paris = ny.with_timezone(&Paris);
    Some((ny.timestamp_millis(), ymd(paris.date_naive()), paris.format("%H:%M").to_string()))
}

/// Value of a Nasdaq cell: "&nbsp;", blanks and "N/A" are no value.
fn cell(v: &Value) -> Option<String> {
    let s = match v {
        Value::String(s) => s.replace("&nbsp;", " ").replace("&amp;", "&"),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let s = s.trim();
    if s.is_empty() || s == "N/A" || s == "-" { None } else { Some(s.to_string()) }
}

fn get<'a>(row: &'a Value, key: &str) -> &'a Value {
    row.get(key).unwrap_or(&Value::Null)
}

/// Symbols compared the way the app writes them ("BRK-B", Nasdaq writes "BRK.B" or "BRK/B").
pub fn norm_symbol(s: &str) -> String {
    s.trim().to_uppercase().replace(['.', '/'], "-")
}

fn country_fr(c: &str) -> Option<&'static str> {
    Some(match c {
        "United States" => "États-Unis",
        "Euro Zone" => "Zone euro",
        "Germany" => "Allemagne",
        "France" => "France",
        "United Kingdom" => "Royaume-Uni",
        "Japan" => "Japon",
        "China" => "Chine",
        _ => return None,
    })
}

/// What an economic release is: kind, category, importance and French title. None = not a major event (left out).
pub fn classify(country: &str, name: &str) -> Option<(EventKind, Category, Importance, String)> {
    use Category::*;
    use EventKind::{CentralBank, Macro};
    use Importance::{High, Medium};
    country_fr(country)?;
    let major = matches!(country, "United States" | "Euro Zone");
    let n = name.trim().to_lowercase();
    let n = n.as_str();
    let bank = match country {
        "United States" => "la Fed",
        "Euro Zone" => "la BCE",
        "United Kingdom" => "la Banque d'Angleterre",
        "Japan" => "la Banque du Japon",
        "China" => "la Banque populaire de Chine",
        _ => "",
    };
    // Central banks.
    if !bank.is_empty() {
        if n.contains("interest rate decision") || n.contains("deposit facility rate") || n.contains("main refinancing") {
            return Some((CentralBank, TauxDirecteurs, High, format!("Décision de taux de {bank}")));
        }
        if n.contains("loan prime rate") {
            return Some((CentralBank, TauxDirecteurs, Medium, format!("Taux de référence des prêts de {bank}")));
        }
    }
    if major {
        if n == "fomc statement" || n == "monetary policy statement" {
            return Some((CentralBank, TauxDirecteurs, High, format!("Communiqué de politique monétaire de {bank}")));
        }
        if n.contains("press conference") && (n.contains("fomc") || n.contains("ecb")) {
            return Some((CentralBank, TauxDirecteurs, High, format!("Conférence de presse de {bank}")));
        }
        if n.contains("fomc meeting minutes") || n == "fomc minutes" || (n.contains("ecb") && n.contains("accounts")) {
            return Some((CentralBank, TauxDirecteurs, Medium, format!("Compte rendu de la réunion de {bank}")));
        }
        static CHAIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(fed chair|ecb president) .*speaks$").expect("regex"));
        if CHAIR.is_match(n) {
            return Some((CentralBank, Discours, Medium, format!("Prise de parole de la présidence de {bank}")));
        }
    }
    // Inflation: headline indexes only (not the n.s.a. index levels, regional or expectation series).
    let inflation = match n {
        "cpi" => Some(("Inflation (CPI)", true)),
        "core cpi" => Some(("Inflation sous-jacente (Core CPI)", true)),
        "pce price index" => Some(("Inflation PCE", true)),
        "core pce price index" => Some(("Inflation PCE sous-jacente", true)),
        "hicp" => Some(("Inflation harmonisée (HICP)", true)),
        "ppi" => Some(("Prix à la production (PPI)", false)),
        "core ppi" => Some(("Prix à la production sous-jacents (Core PPI)", false)),
        "german cpi" | "french cpi" | "national cpi" | "tokyo cpi" => Some(("Inflation (CPI)", false)),
        "german hicp" | "french hicp" => Some(("Inflation harmonisée (HICP)", false)),
        "national core cpi" | "tokyo core cpi" => Some(("Inflation sous-jacente (Core CPI)", false)),
        _ => None,
    };
    if let Some((title, headline)) = inflation {
        let ppi = n.contains("ppi");
        if major || !ppi {
            return Some((Macro, Inflation, if major && headline { High } else { Medium }, title.into()));
        }
        return None;
    }
    // Employment.
    let jobs = match n {
        "nonfarm payrolls" if country == "United States" => Some(("Créations d'emplois (NFP)", High)),
        "unemployment rate" | "chinese unemployment rate" | "german unemployment rate" => {
            Some(("Taux de chômage", if country == "United States" { High } else { Medium }))
        }
        "german unemployment change" => Some(("Variation du nombre de chômeurs", Medium)),
        "initial jobless claims" if major => Some(("Inscriptions hebdomadaires au chômage", Medium)),
        "jolts job openings" if major => Some(("Offres d'emploi (JOLTS)", Medium)),
        "adp nonfarm employment change" if major => Some(("Emplois privés (ADP)", Medium)),
        "average hourly earnings" if major => Some(("Salaire horaire moyen", Medium)),
        _ => None,
    };
    if let Some((title, imp)) = jobs {
        return Some((Macro, Emploi, imp, title.into()));
    }
    // Growth.
    static GDP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(german |french )?gdp( \((qoq|yoy)\))?$").expect("regex"));
    if GDP.is_match(n) {
        return Some((Macro, Pib, if major && n.starts_with("gdp") { High } else { Medium }, "Croissance (PIB)".into()));
    }
    // Activity and confidence (United States and euro area).
    if major {
        let activity = match n {
            "ism manufacturing pmi" => Some("Indice ISM de l'industrie"),
            "ism non-manufacturing pmi" | "ism services pmi" => Some("Indice ISM des services"),
            "retail sales" => Some("Ventes au détail"),
            "michigan consumer sentiment" => Some("Confiance des consommateurs (Michigan)"),
            "cb consumer confidence" => Some("Confiance des consommateurs (Conference Board)"),
            _ => None,
        };
        if let Some(title) = activity {
            return Some((Macro, Activite, Medium, title.into()));
        }
    }
    None
}

fn event(date: (i64, String), kind: EventKind, category: Category, importance: Importance, title: String, source: &str, url: &str) -> CalendarEvent {
    CalendarEvent {
        date: date.0,
        day: date.1,
        time: None,
        kind,
        category,
        importance,
        title,
        original_name: None,
        country: None,
        symbol: None,
        actual: None,
        consensus: None,
        previous: None,
        detail: None,
        note: None,
        source: source.into(),
        url: url.into(),
    }
}

/// Rows of a Nasdaq calendar: `data.rows` (or `data.calendar.rows`); `data: null` with "No record found" is an empty
/// day, any other status an error.
fn nasdaq_rows<'a>(d: &'a Value, path: &[&str]) -> Result<Vec<&'a Value>> {
    if let Some(code) = d.pointer("/status/rCode").and_then(Value::as_i64) {
        if code != 200 {
            return err(format!("Nasdaq : statut {code}"));
        }
    }
    let mut node = d.get("data").unwrap_or(&Value::Null);
    if node.is_null() {
        return Ok(vec![]);
    }
    for k in path {
        node = node.get(*k).unwrap_or(&Value::Null);
    }
    match node.get("rows") {
        Some(Value::Array(rows)) => Ok(rows.iter().collect()),
        Some(Value::Null) | None => Ok(vec![]),
        _ => err("Nasdaq : format inattendu"),
    }
}

pub mod parse {
    use super::*;

    pub const ECO_SOURCE: &str = "Nasdaq (calendrier économique)";
    pub const EARNINGS_SOURCE: &str = "Nasdaq (calendrier des résultats)";
    pub const DIVIDENDS_SOURCE: &str = "Nasdaq (calendrier des dividendes)";
    pub const SPLITS_SOURCE: &str = "Nasdaq (calendrier des splits)";
    pub const IPO_SOURCE: &str = "Nasdaq (calendrier des introductions en bourse)";
    pub const FED_SOURCE: &str = "Réserve fédérale (calendrier officiel du FOMC)";
    pub const ECB_SOURCE: &str = "BCE (calendrier officiel du Conseil des gouverneurs)";

    /// Joins the values of rows published under the same name at the same time (Nasdaq lists the monthly and
    /// yearly rates of a CPI as two unlabelled rows): "0.3% · 3.4%", "—" where one row has no value.
    fn join(values: &[Option<String>]) -> Option<String> {
        if values.iter().all(Option::is_none) {
            return None;
        }
        Some(values.iter().map(|v| v.clone().unwrap_or_else(|| "—".into())).collect::<Vec<_>>().join(" · "))
    }

    /// Nasdaq economic calendar of `day` (the answer to `?date=` the day after, see `economic_query_date`): major
    /// releases of the followed countries only, same-name rows merged.
    pub fn economic(d: &Value, day: NaiveDate) -> Result<Vec<CalendarEvent>> {
        let rows = nasdaq_rows(d, &[])?;
        // Weekly jobless claims come out on Thursdays, on the Wednesday only when that Thursday is a federal holiday
        // (Thanksgiving, Christmas, New Year…): anywhere else, the day offset of the source has changed and every
        // date would be wrong, so the day fails instead of being shown.
        let claims = rows.iter().any(|r| get(r, "eventName").as_str().is_some_and(|n| n.trim() == "Initial Jobless Claims"));
        let claims_day = match day.weekday() {
            chrono::Weekday::Thu => true,
            chrono::Weekday::Wed => day.succ_opt().is_some_and(us_federal_holiday),
            _ => false,
        };
        if claims && !claims_day {
            return err(format!("calendrier économique Nasdaq : dates décalées pour le {}", ymd(day)));
        }
        let mut groups: indexmap::IndexMap<(String, String, String), Vec<&Value>> = indexmap::IndexMap::new();
        for r in rows {
            let country = get(r, "country").as_str().unwrap_or("").trim().to_string();
            let name = get(r, "eventName").as_str().unwrap_or("").trim().to_string();
            let time = get(r, "gmt").as_str().unwrap_or("").trim().to_string();
            // Case differs between rows of the same release ("PCE price index", "PCE Price index").
            groups.entry((country, name.to_lowercase(), time)).or_default().push(r);
        }
        let mut out = vec![];
        for ((country, _, time), rows) in groups {
            let name = get(rows[0], "eventName").as_str().unwrap_or("").trim().to_string();
            let Some((kind, category, importance, title)) = classify(&country, &name) else { continue };
            // "gmt" is New York time despite its name (payrolls at 08:30, Fed decisions at 14:00).
            let (date, paris_time) = match at_new_york(day, &time) {
                Some((ms, d, t)) => ((ms, d), Some(t)),
                None => (at_day(day), None),
            };
            let mut e = event(date, kind, category, importance, title, ECO_SOURCE, NASDAQ_ECO_PAGE);
            e.time = paris_time;
            e.original_name = Some(name);
            e.country = country_fr(&country).map(String::from);
            let col = |k: &str| rows.iter().map(|r| cell(get(r, k))).collect::<Vec<_>>();
            e.actual = join(&col("actual"));
            e.consensus = join(&col("consensus"));
            e.previous = join(&col("previous"));
            out.push(e);
        }
        Ok(out)
    }

    /// US federal holidays that can fall on a Thursday and move the jobless claims to the Wednesday: New Year's Day,
    /// Juneteenth, Independence Day, Veterans Day, Christmas (fixed dates, not moved when on a Thursday) and
    /// Thanksgiving (fourth Thursday of November).
    pub fn us_federal_holiday(d: NaiveDate) -> bool {
        let (m, day) = (d.month(), d.day());
        let fixed = matches!((m, day), (1, 1) | (6, 19) | (7, 4) | (11, 11) | (12, 25));
        let thanksgiving = m == 11 && d.weekday() == chrono::Weekday::Thu && (22..=28).contains(&day);
        fixed || thanksgiving
    }

    /// Nasdaq's economic calendar answers `?date=D` with the releases of the day before D (checked on 28/09/2026:
    /// `?date=2026-09-29` gave Monday's releases with their figures, `?date=2026-10-03` Friday's payrolls, and the
    /// Fed decision of 28/10 came under 29/10), unlike its earnings, dividends and splits calendars.
    pub fn economic_query_date(day: NaiveDate) -> NaiveDate {
        day.succ_opt().unwrap_or(day)
    }

    /// Nasdaq earnings calendar of `day`. `market_cap` in dollars when given.
    pub fn earnings(d: &Value, day: NaiveDate) -> Result<Vec<(CalendarEvent, Option<f64>)>> {
        let rows = nasdaq_rows(d, &[])?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let symbol = cell(get(r, "symbol"))?;
                let name = cell(get(r, "name")).unwrap_or_else(|| symbol.clone());
                let url = format!("https://www.nasdaq.com/market-activity/stocks/{}/earnings", symbol.to_lowercase());
                let mut e = event(
                    at_day(day),
                    EventKind::Earnings,
                    Category::Resultats,
                    Importance::High,
                    format!("Résultats {}", de(&name)),
                    EARNINGS_SOURCE,
                    &url,
                );
                e.time = match get(r, "time").as_str() {
                    Some("time-pre-market") => Some("avant l'ouverture".into()),
                    Some("time-after-hours") => Some("après la clôture".into()),
                    _ => None,
                };
                e.consensus = cell(get(r, "epsForecast")).map(|v| format!("BPA {}", dollars(&v)));
                e.previous = cell(get(r, "lastYearEPS")).map(|v| format!("BPA {} il y a un an", dollars(&v)));
                let mut detail = vec![];
                if let Some(q) = cell(get(r, "fiscalQuarterEnding")) {
                    detail.push(format!("trimestre clos {q}"));
                }
                if let Some(n) = cell(get(r, "noOfEsts")) {
                    detail.push(format!("{n} estimation{}", if n == "1" { "" } else { "s" }));
                }
                e.detail = (!detail.is_empty()).then(|| capitalize(&detail.join(", ")));
                e.symbol = Some(symbol);
                let cap = cell(get(r, "marketCap")).and_then(|c| c.replace(['$', ','], "").parse::<f64>().ok());
                Some((e, cap))
            })
            .collect())
    }

    /// "Mondelez International, Inc. Class A Common Stock" → "Mondelez International, Inc." (share class wording of
    /// the dividends and splits calendars).
    pub fn company(name: &str) -> String {
        static CLASS: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?i)\s+(class [a-z] )?(common stock|ordinary shares|american depositary shares|depositary shares)\b.*$").expect("regex")
        });
        let short = CLASS.replace(name.trim(), "").trim().to_string();
        if short.is_empty() { name.trim().to_string() } else { short }
    }

    /// "de Nike" / "d'Accenture".
    pub fn de(name: &str) -> String {
        if name.chars().next().is_some_and(|c| "AEIOUYaeiouyÉé".contains(c)) { format!("d'{name}") } else { format!("de {name}") }
    }

    fn capitalize(s: &str) -> String {
        let mut c = s.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }

    fn fr_date(d: NaiveDate) -> String {
        d.format("%d/%m/%Y").to_string()
    }

    /// Nasdaq dividends calendar: one event on the ex-dividend date.
    pub fn dividends(d: &Value) -> Result<Vec<CalendarEvent>> {
        let rows = nasdaq_rows(d, &["calendar"])?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let symbol = cell(get(r, "symbol"))?;
                let ex = us_date(get(r, "dividend_Ex_Date").as_str()?)?;
                let name = cell(get(r, "companyName")).map(|n| company(&n)).unwrap_or_else(|| symbol.clone());
                let mut e = event(
                    at_day(ex),
                    EventKind::Dividend,
                    Category::Dividende,
                    Importance::Medium,
                    format!("Détachement du dividende {}", de(&name)),
                    DIVIDENDS_SOURCE,
                    NASDAQ_DIVIDENDS_PAGE,
                );
                let mut detail = vec![];
                if let Some(rate) = get(r, "dividend_Rate").as_f64().or_else(|| cell(get(r, "dividend_Rate")).and_then(|s| s.parse().ok())) {
                    detail.push(format!("{} par action", crate::fx::money_with(rate, |v| crate::js::fr(v, 2, 4))));
                }
                if let Some(p) = get(r, "payment_Date").as_str().and_then(us_date) {
                    detail.push(format!("versé le {}", fr_date(p)));
                }
                e.detail = (!detail.is_empty()).then(|| capitalize(&detail.join(", ")));
                e.symbol = Some(symbol);
                Some(e)
            })
            .collect())
    }

    /// Nasdaq splits calendar: one event on the effective date ("3 : 1" division, "1 : 10" reverse split).
    pub fn splits(d: &Value) -> Result<Vec<CalendarEvent>> {
        let rows = nasdaq_rows(d, &[])?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let symbol = cell(get(r, "symbol"))?;
                let date = us_date(get(r, "executionDate").as_str()?)?;
                let ratio = cell(get(r, "ratio"))?;
                let name = cell(get(r, "name")).map(|n| company(&n)).unwrap_or_else(|| symbol.clone());
                let parts: Vec<f64> = ratio.split(':').filter_map(|p| p.trim().parse().ok()).collect();
                let title = match parts.as_slice() {
                    [a, b] if a < b => format!("Regroupement d'actions {}", de(&name)),
                    _ => format!("Division d'actions {}", de(&name)),
                };
                let mut e = event(at_day(date), EventKind::Split, Category::Split, Importance::Medium, title, SPLITS_SOURCE, NASDAQ_SPLITS_PAGE);
                e.detail = Some(match parts.as_slice() {
                    [a, b] => format!("{} nouvelle(s) action(s) pour {} (ratio {ratio})", crate::js::fr(*a, 0, 4), crate::js::fr(*b, 0, 4)),
                    _ => format!("Ratio {ratio}"),
                });
                e.symbol = Some(symbol);
                Some(e)
            })
            .collect())
    }

    /// Nasdaq IPO calendar of a month: upcoming deals on their expected date, priced deals on their pricing date.
    pub fn ipos(d: &Value) -> Result<Vec<CalendarEvent>> {
        if let Some(code) = d.pointer("/status/rCode").and_then(Value::as_i64) {
            if code != 200 {
                return err(format!("Nasdaq : statut {code}"));
            }
        }
        let rows = |p: &str| d.pointer(p).and_then(Value::as_array).cloned().unwrap_or_default();
        let mut out = vec![];
        for (rows, date_key, priced) in
            [(rows("/data/upcoming/upcomingTable/rows"), "expectedPriceDate", false), (rows("/data/priced/rows"), "pricedDate", true)]
        {
            for r in &rows {
                let Some(date) = get(r, date_key).as_str().and_then(us_date) else { continue };
                let Some(name) = cell(get(r, "companyName")) else { continue };
                let title = if priced {
                    format!("Introduction en bourse {} (prix fixé)", de(&name))
                } else {
                    format!("Introduction en bourse {}", de(&name))
                };
                let mut e = event(at_day(date), EventKind::Ipo, Category::Ipo, Importance::Medium, title, IPO_SOURCE, NASDAQ_IPO_PAGE);
                let mut detail = vec![];
                if let Some(p) = cell(get(r, "proposedSharePrice")) {
                    detail.push(format!("{} {}", if priced { "prix" } else { "fourchette" }, ipo_price(&p)));
                }
                if let Some(v) = cell(get(r, "dollarValueOfSharesOffered")) {
                    detail.push(format!("montant {v}"));
                }
                if let Some(x) = cell(get(r, "proposedExchange")) {
                    detail.push(x);
                }
                e.detail = (!detail.is_empty()).then(|| capitalize(&detail.join(", ")));
                e.symbol = cell(get(r, "proposedTickerSymbol"));
                out.push(e);
            }
        }
        Ok(out)
    }

    /// A monetary policy meeting of an official schedule: the decision day and what the schedule says of it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Meeting {
        pub decision: NaiveDate,
        /// French description ("Réunion des 27 et 28 oct.", "avec projections économiques").
        pub detail: String,
    }

    fn month(name: &str) -> Option<u32> {
        let m = name.trim().get(..3)?.to_lowercase();
        ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"].iter().position(|x| *x == m).map(|i| i as u32 + 1)
    }

    const MONTHS_FR: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

    /// Federal Reserve FOMC calendars page: one panel per year ("2026 FOMC Meetings"), each meeting a month
    /// ("October", "Apr/May") and days ("27-28", "30-1", "16-17*" with projections). Notation votes and
    /// unscheduled meetings are left out.
    pub fn fomc(html: &str) -> Vec<Meeting> {
        static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d{4}) FOMC Meetings").expect("regex"));
        static MEETING: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"(?s)fomc-meeting__month[^>]*>\s*<strong>([^<]+)</strong>.*?fomc-meeting__date[^>]*>([^<]+)<"#).expect("regex")
        });
        static DAYS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d{1,2})(?:-(\d{1,2}))?(\*)?$").expect("regex"));
        let heads: Vec<(usize, i32)> = YEAR.captures_iter(html).filter_map(|c| Some((c.get(0)?.start(), c[1].parse().ok()?))).collect();
        let mut out = vec![];
        for (i, (start, year)) in heads.iter().enumerate() {
            let end = heads.get(i + 1).map_or(html.len(), |h| h.0);
            for c in MEETING.captures_iter(&html[*start..end]) {
                let months: Vec<u32> = c[1].split('/').filter_map(month).collect();
                let raw = c[2].trim();
                let Some(d) = DAYS.captures(raw) else { continue };
                let (Some(&m1), Some(&m2)) = (months.first(), months.last()) else { continue };
                let first: u32 = d[1].parse().unwrap_or(0);
                let last: u32 = d.get(2).and_then(|x| x.as_str().parse().ok()).unwrap_or(first);
                // "Dec/Jan" would end the next year (never seen, handled anyway).
                let y2 = if m2 < m1 { year + 1 } else { *year };
                let Some(decision) = NaiveDate::from_ymd_opt(y2, m2, last) else { continue };
                let when = if first == last {
                    format!("Réunion du {first} {}", MONTHS_FR[m2 as usize - 1])
                } else if m1 == m2 {
                    format!("Réunion des {first} et {last} {}", MONTHS_FR[m2 as usize - 1])
                } else {
                    format!("Réunion du {first} {} au {last} {}", MONTHS_FR[m1 as usize - 1], MONTHS_FR[m2 as usize - 1])
                };
                let detail = if d.get(3).is_some() { format!("{when}, avec projections économiques") } else { when };
                out.push(Meeting { decision, detail });
            }
        }
        out.sort_by_key(|m| m.decision);
        out.dedup_by_key(|m| m.decision);
        out
    }

    /// ECB schedule of Governing Council meetings: the monetary policy meetings' decision days (day 2 of a two-day
    /// meeting); non-monetary meetings and General Council meetings are left out.
    pub fn ecb(html: &str) -> Vec<Meeting> {
        static ITEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<dt>\s*(\d{2})/(\d{2})/(\d{4})\s*</dt>\s*<dd>(.*?)</dd>").expect("regex"));
        let mut out: Vec<Meeting> = ITEM
            .captures_iter(html)
            .filter_map(|c| {
                let text = c[4].to_lowercase();
                if !text.contains("monetary policy meeting") || text.contains("non-monetary") || text.contains("(day 1)") {
                    return None;
                }
                let decision = NaiveDate::from_ymd_opt(c[3].parse().ok()?, c[2].parse().ok()?, c[1].parse().ok()?)?;
                let mut detail = String::from("Réunion de politique monétaire");
                if text.contains("frankfurt") {
                    detail.push_str(" à Francfort");
                } else if text.contains("virtual") {
                    detail.push_str(" à distance");
                }
                if text.contains("press conference") {
                    detail.push_str(", suivie d'une conférence de presse");
                }
                Some(Meeting { decision, detail })
            })
            .collect();
        out.sort_by_key(|m| m.decision);
        out.dedup_by_key(|m| m.decision);
        out
    }

    /// Events of an official schedule within [from, to].
    pub fn meetings_events(meetings: &[Meeting], bank: Bank, from: NaiveDate, to: NaiveDate) -> Vec<CalendarEvent> {
        let (title, source, url, country) = match bank {
            Bank::Fed => ("Décision de taux de la Fed (FOMC)", FED_SOURCE, FED_URL, "États-Unis"),
            Bank::Ecb => ("Décision de taux de la BCE", ECB_SOURCE, ECB_URL, "Zone euro"),
        };
        meetings
            .iter()
            .filter(|m| m.decision >= from && m.decision <= to)
            .map(|m| {
                let mut e = event(at_day(m.decision), EventKind::CentralBank, Category::TauxDirecteurs, Importance::High, title.into(), source, url);
                e.country = Some(country.into());
                e.detail = Some(m.detail.clone());
                e
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bank {
    Fed,
    Ecb,
}

fn is_decision(e: &CalendarEvent, country: &str) -> bool {
    e.kind == EventKind::CentralBank
        && e.source == parse::ECO_SOURCE
        && e.country.as_deref() == Some(country)
        && e.original_name
            .as_deref()
            .is_some_and(|n| n.to_lowercase().contains("interest rate decision") || n.to_lowercase().contains("deposit facility"))
}

/// The official schedule is authoritative for Fed and ECB decision days: Nasdaq's row for the same meeting (within
/// 3 days) is folded into the official event (its time and figures kept), and a different date is said in `note`.
pub fn merge_official(events: &mut Vec<CalendarEvent>, official: Vec<CalendarEvent>) {
    let mut out = official;
    let mut keep = vec![];
    for e in events.drain(..) {
        let country = e.country.clone().unwrap_or_default();
        let target = out.iter_mut().find(|o| {
            o.country.as_deref() == Some(country.as_str()) && is_decision(&e, &country) && (o.date - day_ms_of(&e.day)).abs() <= 3 * 24 * HOUR
        });
        match target {
            Some(o) => {
                if o.day == e.day {
                    o.time = e.time.clone();
                    o.date = e.date;
                    o.note = Some("Date confirmée par Nasdaq et le calendrier officiel.".into());
                } else {
                    let d = NaiveDate::parse_from_str(&e.day, "%Y-%m-%d").map(|d| d.format("%d/%m").to_string()).unwrap_or_else(|_| e.day.clone());
                    o.note = Some(format!("Nasdaq l'annonce le {d} : la date du calendrier officiel est retenue."));
                }
                o.original_name = e.original_name.clone();
                o.actual = e.actual.clone();
                o.consensus = e.consensus.clone();
                o.previous = e.previous.clone();
            }
            None => keep.push(e),
        }
    }
    keep.extend(out);
    *events = keep;
}

fn day_ms_of(day: &str) -> i64 {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").map(day_ms).unwrap_or(0)
}

/// Keeps the symbol-specific events (earnings, dividends, splits) of `symbols` only; the others stay.
pub fn filter_symbols(events: Vec<CalendarEvent>, symbols: &[String]) -> Vec<CalendarEvent> {
    let wanted: HashSet<String> = symbols.iter().map(|s| norm_symbol(s)).collect();
    events
        .into_iter()
        .filter(|e| match e.kind {
            EventKind::Earnings | EventKind::Dividend | EventKind::Split => e.symbol.as_deref().is_some_and(|s| wanted.contains(&norm_symbol(s))),
            _ => true,
        })
        .collect()
}

/// Events that matter to a decision on `symbol`: high-importance macro and central bank events, plus the stock's
/// own earnings, dividends and splits (a crypto has none), within [today, today + within_days] and not already past
/// by more than an hour (`now`, ms).
pub fn select_for(events: &[CalendarEvent], symbol: &str, kind: Kind, now: i64) -> Vec<CalendarEvent> {
    let sym = norm_symbol(symbol);
    events
        .iter()
        .filter(|e| e.time.is_none() || e.kind == EventKind::Earnings || e.date >= now - HOUR)
        .filter(|e| match e.kind {
            EventKind::Macro | EventKind::CentralBank => e.importance == Importance::High,
            EventKind::Earnings | EventKind::Dividend | EventKind::Split => {
                kind == Kind::Stock && e.symbol.as_deref().is_some_and(|s| norm_symbol(s) == sym)
            }
            EventKind::Ipo => false,
        })
        .cloned()
        .collect()
}

pub fn sort_events(events: &mut [CalendarEvent]) {
    events.sort_by(|a, b| a.day.cmp(&b.day).then(a.date.cmp(&b.date)).then(a.importance.cmp(&b.importance)).then(a.title.cmp(&b.title)));
}

/// French list of what the calendar does not cover (always shown).
pub fn not_covered(symbol_filter: bool) -> Vec<String> {
    let mut v = vec![
        "Déblocages de jetons (token unlocks) : aucune source gratuite vérifiable.".to_string(),
        "Régulation crypto : suivie via l'actualité, pas de calendrier officiel.".to_string(),
        "Mises à jour des réseaux crypto (forks, mises à niveau) : pas de calendrier gratuit et vérifiable.".to_string(),
        "Économie : seuls les États-Unis, la zone euro, l'Allemagne, la France, le Royaume-Uni, le Japon et la Chine sont suivis, et seulement leurs publications majeures.".to_string(),
        "Banque d'Angleterre, Banque du Japon et Banque populaire de Chine : d'après Nasdaq seulement, sans confirmation officielle.".to_string(),
        "Résultats, dividendes, splits et introductions en bourse : sociétés cotées aux États-Unis seulement (Nasdaq).".to_string(),
    ];
    if !symbol_filter {
        v.push(format!(
            "Sans filtre « Mes actifs », résultats, dividendes et splits limités aux {TOP_RANK} plus grandes capitalisations américaines."
        ));
    }
    v
}

// ---------------------------------------------------------------------------------------------------------------
// Fetching

async fn nasdaq(url: &str) -> Result<Value> {
    get_json_with(url, &[("User-Agent", BROWSER_UA), ("Accept-Language", "en-US,en;q=0.9")], TIMEOUT).await
}

/// Per-day cache: the current day (figures released during the day) is refreshed every 15 minutes, a future day every
/// 3 hours.
fn day_ttl(day: NaiveDate, today: NaiveDate) -> i64 {
    if day <= today { 15 * MINUTE } else { 3 * HOUR }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Job {
    Economic,
    Earnings,
    Dividends,
    Splits,
    Ipo,
    Fed,
    Ecb,
}

impl Job {
    fn source(self) -> &'static str {
        match self {
            Job::Economic => parse::ECO_SOURCE,
            Job::Earnings => parse::EARNINGS_SOURCE,
            Job::Dividends => parse::DIVIDENDS_SOURCE,
            Job::Splits => parse::SPLITS_SOURCE,
            Job::Ipo => parse::IPO_SOURCE,
            Job::Fed => parse::FED_SOURCE,
            Job::Ecb => parse::ECB_SOURCE,
        }
    }
}

/// Events of one job and day; earnings carry their market cap (dollars) for the unfiltered view.
type Fetched = Vec<(CalendarEvent, Option<f64>)>;

async fn run(job: Job, day: NaiveDate, today: NaiveDate, from: NaiveDate, to: NaiveDate) -> Result<Arc<Fetched>> {
    let d = ymd(day);
    let ttl = day_ttl(day, today);
    let plain = |v: Vec<CalendarEvent>| v.into_iter().map(|e| (e, None)).collect::<Fetched>();
    match job {
        Job::Economic => {
            cached(&format!("calendar:eco:{d}"), ttl, move || async move {
                let q = ymd(parse::economic_query_date(day));
                let v = nasdaq(&format!("https://api.nasdaq.com/api/calendar/economicevents?date={q}")).await?;
                parse::economic(&v, day).map(plain)
            })
            .await
        }
        Job::Earnings => {
            cached(&format!("calendar:earnings:{d}"), ttl, move || async move {
                parse::earnings(&nasdaq(&format!("https://api.nasdaq.com/api/calendar/earnings?date={d}")).await?, day)
            })
            .await
        }
        Job::Dividends => {
            cached(&format!("calendar:dividends:{d}"), ttl, move || async move {
                parse::dividends(&nasdaq(&format!("https://api.nasdaq.com/api/calendar/dividends?date={d}")).await?).map(plain)
            })
            .await
        }
        Job::Splits => {
            cached(&format!("calendar:splits:{d}"), ttl, move || async move {
                parse::splits(&nasdaq(&format!("https://api.nasdaq.com/api/calendar/splits?date={d}")).await?).map(plain)
            })
            .await
        }
        Job::Ipo => {
            let m = day.format("%Y-%m").to_string();
            cached(&format!("calendar:ipo:{m}"), if day.month() == today.month() { HOUR } else { 3 * HOUR }, move || async move {
                parse::ipos(&nasdaq(&format!("https://api.nasdaq.com/api/ipo/calendar?date={m}")).await?).map(plain)
            })
            .await
        }
        Job::Fed | Job::Ecb => {
            let bank = if job == Job::Fed { Bank::Fed } else { Bank::Ecb };
            let meetings = cached(&format!("calendar:official:{job:?}"), 24 * HOUR, move || async move {
                let url = if bank == Bank::Fed { FED_URL } else { ECB_URL };
                let html = get_text_with(url, &[("User-Agent", BROWSER_UA), ("Accept", "text/html")], TIMEOUT).await?;
                let m = if bank == Bank::Fed { parse::fomc(&html) } else { parse::ecb(&html) };
                // A page whose layout changed parses to nothing: a failure, never "no meeting".
                if m.is_empty() { Err(Error("calendrier officiel illisible".into())) } else { Ok(m) }
            })
            .await?;
            Ok(Arc::new(plain(parse::meetings_events(&meetings, bank, from, to))))
        }
    }
}

/// What to fetch: the symbol-specific calendars (earnings, dividends, splits) and the IPOs are optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wants {
    pub companies: bool,
    pub ipo: bool,
}

/// The calendar of the next `days` days (today included, Paris time). `symbols`: when given, earnings, dividends and
/// splits of these symbols only; otherwise those of the largest US companies.
pub async fn calendar_with(days: u32, symbols: Option<&[String]>, wants: Wants) -> Calendar {
    calendar_inner(days, symbols, wants, false).await
}

/// Whether a company event (earnings, dividend, split) stays: with `symbols`, those of the symbols (plus, with `top`,
/// those of the largest companies); without, the largest companies only. `top_set`: the Nasdaq ranking when read,
/// else a stock's earnings count as large from 10 bn $ of market cap (and dividends and splits all stay).
pub fn keep_company(e: &CalendarEvent, cap: Option<f64>, symbols: Option<&HashSet<String>>, top: bool, top_set: Option<&HashSet<String>>) -> bool {
    let sym = e.symbol.as_deref().map(norm_symbol).unwrap_or_default();
    if symbols.is_some_and(|s| s.contains(&sym)) {
        return true;
    }
    if symbols.is_some() && !top {
        return false;
    }
    match top_set {
        Some(t) => t.contains(&sym),
        None => e.kind != EventKind::Earnings || cap.is_some_and(|c| c >= 10e9),
    }
}

async fn calendar_inner(days: u32, symbols: Option<&[String]>, wants: Wants, top_too: bool) -> Calendar {
    let days = days.clamp(1, MAX_DAYS);
    let now = now_ms();
    let today = today_paris(now);
    let from = today;
    let to = today + chrono::Days::new(u64::from(days - 1));
    let mut jobs: Vec<(Job, NaiveDate)> = vec![(Job::Fed, from), (Job::Ecb, from)];
    let mut months = vec![];
    for i in 0..days {
        let day = from + chrono::Days::new(u64::from(i));
        jobs.push((Job::Economic, day));
        if wants.companies {
            jobs.extend([(Job::Earnings, day), (Job::Dividends, day), (Job::Splits, day)]);
        }
        if wants.ipo && !months.contains(&(day.year(), day.month())) {
            months.push((day.year(), day.month()));
            jobs.push((Job::Ipo, day));
        }
    }
    let ranks = async {
        if wants.companies && (symbols.is_none() || top_too) {
            crate::universe::stock_universe()
                .await
                .ok()
                .map(|u| u.iter().filter(|e| e.2 > 0 && e.2 <= TOP_RANK).map(|e| norm_symbol(&e.0)).collect::<HashSet<String>>())
        } else {
            None
        }
    };
    let wanted: Option<HashSet<String>> = symbols.map(|s| s.iter().map(|x| norm_symbol(x)).collect());
    let deadline = tokio::time::Instant::now() + DEADLINE;
    let fetch = futures::stream::iter(jobs.into_iter().map(|(job, day)| async move {
        // Polled at least once: a late load is started (and cached) even when the answer cannot wait for it.
        let r =
            tokio::time::timeout_at(deadline, run(job, day, today, from, to)).await.unwrap_or_else(|_| Err(Error("délai dépassé, réessayez".into())));
        (job, day, r)
    }))
    .buffer_unordered(CONCURRENCY)
    .collect::<Vec<_>>();
    let (results, top) = tokio::join!(fetch, ranks);

    let mut status: BTreeMap<Job, (Vec<String>, Option<String>)> = BTreeMap::new();
    let mut events: Vec<CalendarEvent> = vec![];
    let mut official = vec![];
    let mut seen = HashSet::new();
    for (job, day, r) in results {
        let entry = status.entry(job).or_default();
        match r {
            Ok(list) => {
                for (e, cap) in list.iter() {
                    if e.day < ymd(from) || e.day > ymd(to) {
                        continue;
                    }
                    // The symbols asked and / or the largest US companies (the Nasdaq ranking, else the earnings row's own cap).
                    if matches!(e.kind, EventKind::Earnings | EventKind::Dividend | EventKind::Split)
                        && !keep_company(e, *cap, wanted.as_ref(), top_too, top.as_ref())
                    {
                        continue;
                    }
                    // Splits and IPOs come back on several days: one event each.
                    let key = (e.kind, e.day.clone(), e.symbol.clone(), e.title.clone(), e.time.clone());
                    if !seen.insert(key) {
                        continue;
                    }
                    if matches!(job, Job::Fed | Job::Ecb) { official.push(e.clone()) } else { events.push(e.clone()) }
                }
            }
            Err(e) => {
                entry.0.push(if job == Job::Ipo { day.format("%Y-%m").to_string() } else { ymd(day) });
                entry.1 = Some(crate::app::error::scrub(&e.0));
            }
        }
    }
    merge_official(&mut events, official);
    sort_events(&mut events);
    let sources = status
        .into_iter()
        .map(|(job, (mut failed, error))| {
            failed.sort();
            SourceStatus { name: job.source().into(), ok: failed.is_empty(), failed, error }
        })
        .collect();
    let mut not_covered = not_covered((symbols.is_some() && !top_too) || !wants.companies);
    if wants.companies && (symbols.is_none() || top_too) && top.is_none() {
        not_covered.push(format!(
            "Classement des capitalisations indisponible : résultats des sociétés de plus de {}, dividendes et splits sans filtre de taille.",
            crate::fx::money_with(10e9, |v| format!("{} Md", crate::js::fr(v / 1e9, 0, 1))).replace("Md ", "Md")
        ));
    }
    Calendar { as_of: now, days, from: ymd(from), to: ymd(to), events, sources, not_covered }
}

/// `/api/calendar`: everything, earnings / dividends / splits of `symbols` when given.
pub async fn calendar(days: u32, symbols: Option<&[String]>) -> Calendar {
    calendar_with(days, symbols, Wants { companies: true, ipo: true }).await
}

/// `/api/calendar?top=1&symbols=…`: the company events of `symbols` and those of the largest companies together (the
/// Agenda's risk view needs both: the user's own stocks, even small, and the large caps).
pub async fn calendar_top(days: u32, symbols: Option<&[String]>) -> Calendar {
    calendar_inner(days, symbols, Wants { companies: true, ipo: true }, true).await
}

/// For the decision's event veto: high-importance macro and central bank events plus the stock's own earnings,
/// dividends and splits within the next `within_days` days (today included; a crypto gets macro and central banks
/// only). None when a source failed: a partial calendar must not read as "no announcement".
pub async fn upcoming_for(symbol: &str, kind: Kind, within_days: u32) -> Option<Vec<CalendarEvent>> {
    let days = (within_days + 1).clamp(1, MAX_DAYS);
    let symbols = [symbol.to_string()];
    let stock = kind == Kind::Stock;
    let cal = calendar_with(days, stock.then_some(&symbols[..]), Wants { companies: stock, ipo: false }).await;
    cal.sources.iter().all(|s| s.ok).then(|| select_for(&cal.events, symbol, kind, cal.as_of))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: EventKind, importance: Importance, symbol: Option<&str>, date: i64, time: Option<&str>) -> CalendarEvent {
        let mut e = event((date, "2026-10-01".into()), kind, Category::Activite, importance, "t".into(), "s", "u");
        e.symbol = symbol.map(String::from);
        e.time = time.map(String::from);
        e
    }

    #[test]
    fn classification() {
        let c = |country: &str, name: &str| classify(country, name).map(|x| (x.0, x.1, x.2));
        use Category::*;
        use EventKind::*;
        use Importance::*;
        assert_eq!(c("United States", "Fed Interest Rate Decision"), Some((CentralBank, TauxDirecteurs, High)));
        assert_eq!(c("United States", "Nonfarm Payrolls"), Some((Macro, Emploi, High)));
        assert_eq!(c("United States", "CPI"), Some((Macro, Inflation, High)));
        assert_eq!(c("United States", "PPI"), Some((Macro, Inflation, Medium)));
        assert_eq!(c("United States", "PCE price index"), Some((Macro, Inflation, High)));
        assert_eq!(c("United States", "GDP"), Some((Macro, Pib, High)));
        assert_eq!(c("Euro Zone", "Unemployment Rate"), Some((Macro, Emploi, Medium)));
        assert_eq!(c("Japan", "BoJ Interest Rate Decision"), Some((CentralBank, TauxDirecteurs, High)));
        assert_eq!(c("Japan", "PPI"), None);
        assert_eq!(c("United States", "CPI Index, n.s.a."), None);
        assert_eq!(c("United States", "Atlanta Fed GDPNow"), None);
        assert_eq!(c("United States", "Fed Waller Speaks"), None);
        assert_eq!(c("Euro Zone", "Consumer Inflation Expectation"), None);
        assert_eq!(c("Australia", "RBA Interest Rate Decision"), None);
        assert_eq!(classify("United States", "Fed Interest Rate Decision").unwrap().3, "Décision de taux de la Fed");
    }

    #[test]
    fn select_for_decision() {
        let now = 1_790_000_000_000;
        let events = vec![
            ev(EventKind::Macro, Importance::High, None, now + HOUR, Some("14:30")),
            ev(EventKind::Macro, Importance::Medium, None, now + HOUR, Some("14:30")),
            ev(EventKind::Macro, Importance::High, None, now - 2 * HOUR, Some("08:00")),
            ev(EventKind::CentralBank, Importance::High, None, now + 3 * HOUR, None),
            ev(EventKind::Earnings, Importance::High, Some("AAPL"), now, Some("après la clôture")),
            ev(EventKind::Earnings, Importance::High, Some("MSFT"), now, None),
            ev(EventKind::Dividend, Importance::Medium, Some("BRK.B"), now, None),
            ev(EventKind::Ipo, Importance::Medium, Some("OURA"), now, None),
        ];
        let aapl = select_for(&events, "AAPL", Kind::Stock, now);
        assert_eq!(aapl.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![EventKind::Macro, EventKind::CentralBank, EventKind::Earnings]);
        let brk = select_for(&events, "BRK-B", Kind::Stock, now);
        assert!(brk.iter().any(|e| e.kind == EventKind::Dividend));
        let btc = select_for(&events, "AAPL", Kind::Crypto, now);
        assert!(btc.iter().all(|e| matches!(e.kind, EventKind::Macro | EventKind::CentralBank)));
        assert_eq!(btc.len(), 2);
    }

    #[test]
    fn symbols_filter() {
        let events = vec![
            ev(EventKind::Earnings, Importance::High, Some("AAPL"), 0, None),
            ev(EventKind::Earnings, Importance::High, Some("MSFT"), 0, None),
            ev(EventKind::Split, Importance::Medium, Some("BRK/B"), 0, None),
            ev(EventKind::Macro, Importance::High, None, 0, None),
            ev(EventKind::Ipo, Importance::Medium, Some("OURA"), 0, None),
        ];
        let kept = filter_symbols(events, &["aapl".into(), "BRK-B".into()]);
        let syms: Vec<_> = kept.iter().map(|e| e.symbol.clone().unwrap_or_default()).collect();
        assert_eq!(syms, vec!["AAPL", "BRK/B", "", "OURA"]);
    }

    #[test]
    fn company_events_kept() {
        let small = ev(EventKind::Earnings, Importance::High, Some("OURA"), 0, None);
        let big = ev(EventKind::Earnings, Importance::High, Some("MSFT"), 0, None);
        let div = ev(EventKind::Dividend, Importance::Medium, Some("KO"), 0, None);
        let top: HashSet<String> = ["MSFT".to_string()].into();
        let mine: HashSet<String> = ["OURA".to_string()].into();
        // Unfiltered: the ranking decides, else the cap for earnings (dividends all stay).
        assert!(keep_company(&big, None, None, false, Some(&top)) && !keep_company(&small, None, None, false, Some(&top)));
        assert!(keep_company(&small, Some(12e9), None, false, None) && !keep_company(&small, Some(2e9), None, false, None));
        assert!(keep_company(&div, None, None, false, None));
        // Symbols only: the user's small stock, not the large one.
        assert!(keep_company(&small, None, Some(&mine), false, Some(&top)) && !keep_company(&big, None, Some(&mine), false, Some(&top)));
        // Symbols and the largest companies together (the risk view).
        assert!(keep_company(&small, None, Some(&mine), true, Some(&top)) && keep_company(&big, None, Some(&mine), true, Some(&top)));
        assert!(!keep_company(&div, None, Some(&mine), true, Some(&top)));
    }

    #[test]
    fn new_york_times() {
        // 08:30 in New York during summer time = 14:30 in Paris.
        let d = NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
        let (_, day, t) = at_new_york(d, "08:30").unwrap();
        assert_eq!((day.as_str(), t.as_str()), ("2026-10-02", "14:30"));
        // 19:30 in New York = 01:30 the next day in Paris.
        let (_, day, t) = at_new_york(d, "19:30").unwrap();
        assert_eq!((day.as_str(), t.as_str()), ("2026-10-03", "01:30"));
        assert!(at_new_york(d, "All Day").is_none());
    }
}

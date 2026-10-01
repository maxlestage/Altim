//! Agenda (/api/calendar, `web/src/webapp/calendar.ts`): the report's contract around the shared `CalendarEvent`, and
//! the pure helpers of the view (day labels, grouping, filters, the 7-day risk calendar). Days are "YYYY-MM-DD"
//! (Paris time from the server); "today" is the viewer's local day, passed in by the caller.
use chrono::{Datelike, Months, NaiveDate};
use serde::{Deserialize, Serialize};

pub use crate::calendar::{CalendarEvent, Category, EventKind, Importance};

/// A source of the agenda and the days (or IPO months) it could not read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceStatus {
    pub name: String,
    pub ok: bool,
    #[serde(default)]
    pub failed: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// `GET /api/calendar`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarReport {
    pub as_of: f64,
    pub days: u32,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub events: Vec<CalendarEvent>,
    #[serde(default)]
    pub sources: Vec<SourceStatus>,
    #[serde(default)]
    pub not_covered: Vec<String>,
}

pub const FILTER_KEY: &str = "altim.agenda.filter";
pub const MINE_KEY: &str = "altim.agenda.mine";

/// Type filter of the list: "all" or one kind of event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgendaFilter {
    All,
    Kind(EventKind),
}

pub const AGENDA_FILTERS: [(&str, &str); 7] = [
    ("all", "Tout"),
    ("macro", "Macro"),
    ("centralBank", "Banques centrales"),
    ("earnings", "Résultats"),
    ("dividend", "Dividendes"),
    ("split", "Splits"),
    ("ipo", "IPO"),
];

impl AgendaFilter {
    /// The saved value ("all", "macro", "centralBank"…); anything else is "all".
    pub fn parse(s: &str) -> AgendaFilter {
        match s {
            "macro" => AgendaFilter::Kind(EventKind::Macro),
            "centralBank" => AgendaFilter::Kind(EventKind::CentralBank),
            "earnings" => AgendaFilter::Kind(EventKind::Earnings),
            "dividend" => AgendaFilter::Kind(EventKind::Dividend),
            "split" => AgendaFilter::Kind(EventKind::Split),
            "ipo" => AgendaFilter::Kind(EventKind::Ipo),
            _ => AgendaFilter::All,
        }
    }
}

/// `CATEGORY_LABEL`.
pub fn category_label(c: Category) -> &'static str {
    match c {
        Category::TauxDirecteurs => "Taux directeurs",
        Category::Inflation => "Inflation",
        Category::Emploi => "Emploi",
        Category::Pib => "PIB",
        Category::Activite => "Activité",
        Category::Discours => "Discours",
        Category::Resultats => "Résultats",
        Category::Dividende => "Dividende",
        Category::Split => "Split",
        Category::Ipo => "IPO",
    }
}

const WEEKDAYS: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
const MONTHS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

/// "YYYY-MM-DD" read like `new Date(...day.split("-").map(Number))` (out-of-range months and days roll over).
fn parse_day(day: &str) -> NaiveDate {
    let mut p = day.split('-').map(|x| x.parse::<i32>().ok());
    let (y, m, d) = (p.next().flatten().unwrap_or(1970), p.next().flatten().unwrap_or(1), p.next().flatten().unwrap_or(1));
    let base = NaiveDate::from_ymd_opt(y, 1, 1).unwrap_or_default();
    let first = if m >= 1 { base.checked_add_months(Months::new((m - 1) as u32)) } else { base.checked_sub_months(Months::new((1 - m) as u32)) };
    first.unwrap_or(base) + chrono::Duration::days((d - 1) as i64)
}

/// "mer. 30 sept." (`Intl.DateTimeFormat("fr-FR", { weekday: "short", day: "numeric", month: "short" })`).
fn short_label(d: NaiveDate) -> String {
    format!("{} {} {}", WEEKDAYS[d.weekday().num_days_from_monday() as usize], d.day(), MONTHS[d.month0() as usize])
}

/// "Aujourd'hui", "Demain", else "mer. 30 sept."; `today` is the viewer's local day ("YYYY-MM-DD").
pub fn day_label(day: &str, today: &str) -> String {
    let t = parse_day(today);
    let tomorrow = t.succ_opt().unwrap_or(t);
    if day == t.format("%Y-%m-%d").to_string() {
        return "Aujourd'hui".into();
    }
    if day == tomorrow.format("%Y-%m-%d").to_string() {
        return "Demain".into();
    }
    short_label(parse_day(day))
}

const COMPANY: [EventKind; 3] = [EventKind::Earnings, EventKind::Dividend, EventKind::Split];

fn norm(s: &str) -> String {
    s.to_uppercase().replace(['.', '/'], "-")
}

/// Events of the chosen filter. `mine` (the user's stock symbols) keeps the company events of these stocks only and
/// leaves the IPOs out; the economy and central banks concern every asset and always stay.
pub fn filter_events<'a>(events: &'a [CalendarEvent], filter: AgendaFilter, mine: Option<&[String]>) -> Vec<&'a CalendarEvent> {
    let own: Option<Vec<String>> = mine.map(|m| m.iter().map(|s| norm(s)).collect());
    events
        .iter()
        .filter(|e| {
            if let AgendaFilter::Kind(k) = filter {
                if e.kind != k {
                    return false;
                }
            }
            let Some(own) = &own else { return true };
            if e.kind == EventKind::Ipo {
                return false;
            }
            !COMPANY.contains(&e.kind) || e.symbol.as_deref().is_some_and(|s| !s.is_empty() && own.contains(&norm(s)))
        })
        .collect()
}

/// Days in order, each with its events in the server's order.
pub fn group_by_day<'a>(events: &[&'a CalendarEvent]) -> Vec<(String, Vec<&'a CalendarEvent>)> {
    let mut sorted: Vec<&CalendarEvent> = events.to_vec();
    sorted.sort_by(|a, b| a.day.cmp(&b.day));
    let mut out: Vec<(String, Vec<&CalendarEvent>)> = Vec::new();
    for e in sorted {
        match out.iter_mut().find(|(d, _)| *d == e.day) {
            Some((_, list)) => list.push(e),
            None => out.push((e.day.clone(), vec![e])),
        }
    }
    out
}

/// The stock symbols of the user's radar and holdings (company events exist for stocks only), 50 at most.
pub fn stock_symbols<'a>(items: impl IntoIterator<Item = (&'a str, crate::types::Kind)>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (s, k) in items {
        let s = s.to_uppercase();
        if k == crate::types::Kind::Stock && !out.contains(&s) {
            out.push(s);
        }
    }
    out.truncate(50);
    out
}

/// No symbol (or none given): the whole calendar, filtered on the device when "Mes actifs" is on. `top`: the company
/// events of `symbols` and of the largest companies together (the risk view).
pub fn calendar_url(days: u32, symbols: Option<&[String]>, top: bool) -> String {
    let list = symbols.filter(|s| !s.is_empty());
    let mut url = format!("/api/calendar?days={days}");
    if let Some(s) = list {
        url.push_str(&format!("&symbols={}", crate::web::bot::encode_uri_component(&s.join(","))));
        if top {
            url.push_str("&top=1");
        }
    }
    url
}

// ---------- Risk by day ----------

/// 🔴 high, 🟠 medium, 🟢 low.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn icon(self) -> &'static str {
        match self {
            RiskLevel::High => "🔴",
            RiskLevel::Medium => "🟠",
            RiskLevel::Low => "🟢",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            RiskLevel::High => "Risque élevé",
            RiskLevel::Medium => "Risque modéré",
            RiskLevel::Low => "Risque faible",
        }
    }
    /// CSS class ("high", "medium", "low").
    pub fn id(self) -> &'static str {
        match self {
            RiskLevel::High => "high",
            RiskLevel::Medium => "medium",
            RiskLevel::Low => "low",
        }
    }
}

/// The categories whose high-importance releases make a 🔴 day.
const MAJOR: [Category; 4] = [Category::TauxDirecteurs, Category::Inflation, Category::Emploi, Category::Pib];

/// Risk of one event, None when it does not count (IPOs, other companies' dividends and splits).
/// - 🔴: a high-importance central bank decision / inflation (CPI) / jobs / GDP release, or the earnings of a stock the
///   user holds or watches;
/// - 🟠: the other macro and central bank events, the earnings of other companies (the calendar keeps the largest US
///   ones only), a dividend or split of a held stock.
pub fn event_risk(e: &CalendarEvent, held: &[String], watched: &[String]) -> Option<RiskLevel> {
    let sym = e.symbol.as_deref().map(norm).unwrap_or_default();
    let is_held = !sym.is_empty() && held.iter().any(|s| norm(s) == sym);
    let is_mine = is_held || (!sym.is_empty() && watched.iter().any(|s| norm(s) == sym));
    match e.kind {
        EventKind::Macro | EventKind::CentralBank => {
            Some(if e.importance == Importance::High && MAJOR.contains(&e.category) { RiskLevel::High } else { RiskLevel::Medium })
        }
        EventKind::Earnings => Some(if is_mine { RiskLevel::High } else { RiskLevel::Medium }),
        EventKind::Dividend | EventKind::Split => is_held.then_some(RiskLevel::Medium),
        EventKind::Ipo => None,
    }
}

/// The acronym in brackets at the end of a title, when it holds two capitals in a row: "Inflation (CPI)" → "CPI"
/// (`/\(([^)]*[A-Z]{2,}[^)]*)\)\s*$/`).
fn acronym(title: &str) -> Option<&str> {
    let inner = crate::engine::news::js_trim(title).strip_suffix(')')?;
    // The leftmost "(" after the last ")" (the regex's first match), then two capitals in a row inside.
    let region = &inner[inner.rfind(')').map_or(0, |i| i + 1)..];
    let s = &region[region.find('(')? + 1..];
    s.as_bytes().windows(2).any(|w| w[0].is_ascii_uppercase() && w[1].is_ascii_uppercase()).then_some(s)
}

/// Short name of an event for the risk row: "CPI", "Décision de taux de la Fed", "Résultats AAPL"…
pub fn short_title(e: &CalendarEvent) -> String {
    if let Some(s) = e.symbol.as_deref().filter(|s| !s.is_empty()) {
        match e.kind {
            EventKind::Earnings => return format!("Résultats {s}"),
            EventKind::Dividend => return format!("Dividende {s}"),
            EventKind::Split => return format!("Split {s}"),
            _ => {}
        }
    }
    let paren = if e.kind == EventKind::Macro { acronym(&e.title) } else { None };
    let name = paren.unwrap_or(&e.title);
    match e.country.as_deref() {
        Some(c) if !c.is_empty() && c != "États-Unis" && e.kind == EventKind::Macro => format!("{name} ({c})"),
        _ => name.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskDay<'a> {
    /// "YYYY-MM-DD".
    pub day: String,
    /// "Lun. 28 sept."
    pub label: String,
    pub weekend: bool,
    pub level: RiskLevel,
    /// Short names of the events at the day's level, in the calendar's order, without repeats.
    pub main: Vec<String>,
    /// Every event that counts, with its risk.
    pub events: Vec<(&'a CalendarEvent, RiskLevel)>,
    /// A source could not be read for this day: 🟢 is then not asserted.
    pub incomplete: bool,
}

/// "Lun. 28 sept." (a calendar day, read without time zone).
pub fn risk_day_label(day: &str) -> String {
    let s = short_label(parse_day(day));
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

/// The `n` days from `from` ("YYYY-MM-DD", the report's first day, Paris time), weekends included, each with its
/// risk: the highest of its events' (`event_risk`), 🟢 when none counts. `failed`: days a source could not read.
pub fn risk_days<'a>(events: &'a [CalendarEvent], from: &str, held: &[String], watched: &[String], n: usize, failed: &[String]) -> Vec<RiskDay<'a>> {
    let start = parse_day(from);
    (0..n)
        .map(|i| {
            let date = start + chrono::Duration::days(i as i64);
            let day = date.format("%Y-%m-%d").to_string();
            let counted: Vec<(&CalendarEvent, RiskLevel)> =
                events.iter().filter(|e| e.day == day).filter_map(|e| event_risk(e, held, watched).map(|r| (e, r))).collect();
            let level = counted.iter().map(|x| x.1).max().unwrap_or(RiskLevel::Low);
            let mut main: Vec<String> = Vec::new();
            for (e, r) in &counted {
                let t = short_title(e);
                if *r == level && !main.contains(&t) {
                    main.push(t);
                }
            }
            let dow = date.weekday().num_days_from_monday();
            RiskDay { label: risk_day_label(&day), weekend: dow >= 5, level, main, events: counted, incomplete: failed.contains(&day), day }
        })
        .collect()
}

/// Days a source could not read ("YYYY-MM-DD" only: the IPO months do not change the risk).
pub fn failed_days(report: &CalendarReport) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in report.sources.iter().flat_map(|s| s.failed.iter()) {
        if d.encode_utf16().count() == 10 && !out.contains(d) {
            out.push(d.clone());
        }
    }
    out
}

/// "⚠ Sources incomplètes : Nasdaq (2 jours manquants), BCE." list of the failing sources.
pub fn failed_sources_text(report: &CalendarReport) -> Option<String> {
    let failed: Vec<String> = report
        .sources
        .iter()
        .filter(|s| !s.ok)
        .map(|s| {
            let n = s.failed.len();
            if n > 0 {
                format!("{} ({n} jour{} manquant{})", s.name, if n > 1 { "s" } else { "" }, if n > 1 { "s" } else { "" })
            } else {
                s.name.clone()
            }
        })
        .collect();
    (!failed.is_empty()).then(|| format!("⚠ Sources incomplètes : {}.", failed.join(", ")))
}

/// The "Mes actifs" note under the filters.
pub fn mine_text(stocks: usize) -> String {
    if stocks > 0 {
        format!(
            "Résultats, dividendes et splits de vos {stocks} action{} (radar et avoirs), plus l'économie et les banques centrales, qui concernent tous les actifs, cryptos compris.",
            if stocks > 1 { "s" } else { "" }
        )
    } else {
        "Aucune action dans votre radar ni vos avoirs : seules l'économie et les banques centrales, qui concernent aussi les cryptos, sont affichées."
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Kind;

    // A real /api/calendar answer (28/09/2026, 14 days, trimmed to a few events of each kind).
    fn report() -> CalendarReport {
        serde_json::from_str(include_str!("samples/calendar-sample.json")).unwrap()
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn day_labels() {
        assert_eq!(day_label("2026-09-28", "2026-09-28"), "Aujourd'hui");
        assert_eq!(day_label("2026-09-29", "2026-09-28"), "Demain");
        assert_eq!(day_label("2026-09-30", "2026-09-28"), "mer. 30 sept.");
        // Month end: tomorrow is the 1st of the next month.
        assert_eq!(day_label("2026-10-01", "2026-09-30"), "Demain");
        assert_eq!(day_label("2027-01-01", "2026-12-31"), "Demain");
    }

    #[test]
    fn grouped_by_day_in_order() {
        let r = report();
        let all: Vec<&CalendarEvent> = r.events.iter().collect();
        let groups = group_by_day(&all);
        let mut days: Vec<String> = Vec::new();
        for e in &r.events {
            if !days.contains(&e.day) {
                days.push(e.day.clone());
            }
        }
        days.sort();
        assert_eq!(groups.iter().map(|g| g.0.clone()).collect::<Vec<_>>(), days);
        assert_eq!(groups.iter().map(|g| g.1.len()).sum::<usize>(), r.events.len());
    }

    #[test]
    fn filters_by_kind_and_mine() {
        let r = report();
        let e = &r.events;
        assert!(filter_events(e, AgendaFilter::parse("ipo"), None).iter().all(|e| e.kind == EventKind::Ipo));
        assert!(!filter_events(e, AgendaFilter::parse("centralBank"), None).is_empty());
        // Mine: MU's earnings stay, other companies and IPOs go, the economy and central banks stay.
        let mine = filter_events(e, AgendaFilter::All, Some(&s(&["MU"])));
        let earnings: Vec<&str> = mine.iter().filter(|e| e.kind == EventKind::Earnings).filter_map(|e| e.symbol.as_deref()).collect();
        assert_eq!(earnings, ["MU"]);
        assert!(!mine.iter().any(|e| e.kind == EventKind::Ipo || e.kind == EventKind::Dividend));
        assert_eq!(mine.iter().filter(|e| e.kind == EventKind::Macro).count(), e.iter().filter(|e| e.kind == EventKind::Macro).count());
        // No stock at all: only the economy and central banks.
        assert!(filter_events(e, AgendaFilter::All, Some(&[])).iter().all(|e| matches!(e.kind, EventKind::Macro | EventKind::CentralBank)));
        assert_eq!(AgendaFilter::parse("oops"), AgendaFilter::All);
    }

    #[test]
    fn each_event_has_its_source_and_link() {
        let r = report();
        for e in &r.events {
            assert!(e.source.chars().count() > 3);
            assert!(e.url.starts_with("https://"));
        }
        assert!(!r.not_covered.is_empty());
    }

    #[test]
    fn symbols_and_url() {
        assert_eq!(stock_symbols([("aapl", Kind::Stock), ("BTC", Kind::Crypto), ("AAPL", Kind::Stock)]), ["AAPL"]);
        assert_eq!(calendar_url(14, None, false), "/api/calendar?days=14");
        assert_eq!(calendar_url(14, Some(&[]), false), "/api/calendar?days=14");
        assert_eq!(calendar_url(30, Some(&s(&["AAPL", "BRK-B"])), false), "/api/calendar?days=30&symbols=AAPL%2CBRK-B");
        // Risk view: top=1 only with symbols (without, the calendar already keeps the largest companies).
        assert_eq!(calendar_url(7, Some(&s(&["AAPL"])), true), "/api/calendar?days=7&symbols=AAPL&top=1");
        assert_eq!(calendar_url(7, None, true), "/api/calendar?days=7");
    }

    #[test]
    fn risk_calendar_over_7_days_weekend_included() {
        let r = report();
        // KDP held (its dividend counts), MU watched (its earnings count as high).
        let days = risk_days(&r.events, "2026-09-28", &s(&["KDP"]), &s(&["MU"]), 7, &s(&["2026-10-03"]));
        assert_eq!(
            days.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(),
            ["Lun. 28 sept.", "Mar. 29 sept.", "Mer. 30 sept.", "Jeu. 1 oct.", "Ven. 2 oct.", "Sam. 3 oct.", "Dim. 4 oct."]
        );
        use RiskLevel::*;
        assert_eq!(days.iter().map(|d| d.level).collect::<Vec<_>>(), [Medium, Medium, High, High, Medium, Low, Low]);
        assert_eq!(days.iter().map(|d| d.weekend).collect::<Vec<_>>(), [false, false, false, false, false, true, true]);
        assert_eq!(days[0].main, ["Dividende KDP"]);
        // Other companies' earnings (large caps) and a central bank speech: 🟠; the others' dividends do not count.
        assert_eq!(days[1].main, ["Résultats CCL", "Prise de parole de la présidence de la BCE"]);
        assert!(!days[1].events.iter().any(|x| x.0.symbol.as_deref() == Some("ERIC")));
        assert_eq!(days[2].main, ["Résultats MU"]);
        // US GDP and PCE (high); the UK's GDP (medium) is not among the main events.
        assert_eq!(days[3].main, ["PIB", "Inflation PCE"]);
        assert!(days[5].main.is_empty());
        assert!(days[5].incomplete);
        assert!(!days[6].incomplete);
    }

    #[test]
    fn risk_rule_per_event() {
        let ev = |f: &dyn Fn(&mut CalendarEvent)| {
            let mut e = CalendarEvent {
                date: 0,
                day: "2026-10-06".into(),
                time: None,
                kind: EventKind::Macro,
                category: Category::Inflation,
                importance: Importance::High,
                title: "Inflation (CPI)".into(),
                original_name: None,
                country: None,
                symbol: None,
                actual: None,
                consensus: None,
                previous: None,
                detail: None,
                note: None,
                source: "s".into(),
                url: "https://x".into(),
            };
            f(&mut e);
            e
        };
        let sym = |k: EventKind, c: Category, s: &str| {
            let s = s.to_string();
            ev(&move |e: &mut CalendarEvent| {
                e.kind = k;
                e.category = c;
                e.symbol = Some(s.clone());
            })
        };
        assert_eq!(event_risk(&ev(&|_| {}), &[], &[]), Some(RiskLevel::High));
        assert_eq!(
            event_risk(
                &ev(&|e| {
                    e.kind = EventKind::CentralBank;
                    e.category = Category::TauxDirecteurs
                }),
                &[],
                &[]
            ),
            Some(RiskLevel::High)
        );
        assert_eq!(
            event_risk(
                &ev(&|e| {
                    e.category = Category::Activite;
                    e.importance = Importance::Medium
                }),
                &[],
                &[]
            ),
            Some(RiskLevel::Medium)
        );
        assert_eq!(event_risk(&sym(EventKind::Earnings, Category::Resultats, "BRK.B"), &s(&["BRK-B"]), &[]), Some(RiskLevel::High));
        assert_eq!(event_risk(&sym(EventKind::Earnings, Category::Resultats, "NVDA"), &[], &[]), Some(RiskLevel::Medium));
        assert_eq!(event_risk(&sym(EventKind::Dividend, Category::Dividende, "KO"), &[], &s(&["KO"])), None);
        assert_eq!(event_risk(&sym(EventKind::Split, Category::Split, "KO"), &s(&["KO"]), &[]), Some(RiskLevel::Medium));
        assert_eq!(event_risk(&sym(EventKind::Ipo, Category::Ipo, "OURA"), &s(&["OURA"]), &[]), None);
        assert_eq!(short_title(&ev(&|_| {})), "CPI");
        assert_eq!(short_title(&ev(&|e| e.country = Some("Allemagne".into()))), "CPI (Allemagne)");
        assert_eq!(
            short_title(&ev(&|e| e.title = "Confiance des consommateurs (Conference Board)".into())),
            "Confiance des consommateurs (Conference Board)"
        );
        let mut r = report();
        r.sources = vec![SourceStatus { name: "a".into(), ok: false, failed: s(&["2026-10-01", "2026-10"]), error: None }];
        assert_eq!(failed_days(&r), ["2026-10-01"]);
        assert_eq!(failed_sources_text(&r).unwrap(), "⚠ Sources incomplètes : a (2 jours manquants).");
        assert_eq!(category_label(Category::Pib), "PIB");
    }
}

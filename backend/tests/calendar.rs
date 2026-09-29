//! Calendar parsers on responses saved from the real sources (`tests/samples/calendar/`, 28/09/2026, trimmed), and
//! the live sources (ignored: network). The Nasdaq economic samples are named after the `?date=` asked: they hold the
//! releases of the day before (`parse::economic_query_date`).
use altim::calendar::parse::{self, Meeting};
use altim::calendar::{Bank, Category, EventKind, Importance, merge_official, sort_events};
use chrono::NaiveDate;
use serde_json::Value;

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/samples/calendar/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn json(name: &str) -> Value {
    serde_json::from_str(&sample(name)).unwrap()
}

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn economic_releases() {
    // `?date=2026-10-03` (a Saturday) answers with Friday 2/10: payrolls on the first Friday of the month.
    let day = date(2026, 10, 2);
    assert_eq!(parse::economic_query_date(day), date(2026, 10, 3));
    let ev = parse::economic(&json("nasdaq-economic-2026-10-03.json"), day).unwrap();
    let titles: Vec<(&str, &str)> = ev.iter().map(|e| (e.country.as_deref().unwrap(), e.original_name.as_deref().unwrap())).collect();
    // Payrolls, unemployment and hourly earnings for the US, CPI for the euro area; CFTC, rig counts, n.s.a. left out.
    assert!(titles.contains(&("États-Unis", "Nonfarm Payrolls")));
    assert!(titles.contains(&("Zone euro", "CPI")));
    assert!(!titles.iter().any(|(_, n)| n.contains("CFTC") || n.contains("n.s.a") || n.contains("U6")));
    let nfp = ev.iter().find(|e| e.original_name.as_deref() == Some("Nonfarm Payrolls")).unwrap();
    assert_eq!((nfp.kind, nfp.category, nfp.importance), (EventKind::Macro, Category::Emploi, Importance::High));
    // 08:30 in New York = 14:30 in Paris.
    assert_eq!((nfp.day.as_str(), nfp.time.as_deref()), ("2026-10-02", Some("14:30")));
    assert_eq!((nfp.consensus.as_deref(), nfp.previous.as_deref(), nfp.actual.as_deref()), (Some("98K"), Some("162K"), None));
    assert_eq!(nfp.title, "Créations d'emplois (NFP)");
    // The two unlabelled CPI rows (year on year, month on month) merged, aligned, "—" where a row has no value.
    let cpi = ev.iter().find(|e| e.country.as_deref() == Some("Zone euro") && e.original_name.as_deref() == Some("CPI")).unwrap();
    assert_eq!((cpi.consensus.as_deref(), cpi.previous.as_deref()), (Some("3.7% · —"), Some("3.2% · 0.4%")));
    assert_eq!(cpi.time.as_deref(), Some("11:00"));
    assert_eq!(cpi.url, "https://www.nasdaq.com/market-activity/economic-calendar");

    // `?date=2026-10-01` answers with Wednesday 30/09 (ADP and mortgage applications come out on Wednesdays).
    let ev = parse::economic(&json("nasdaq-economic-2026-10-01.json"), date(2026, 9, 30)).unwrap();
    let pce: Vec<_> = ev.iter().filter(|e| e.title == "Inflation PCE").collect();
    assert_eq!(pce.len(), 1, "same release written with two cases: one event");
    assert!(ev.iter().any(|e| e.title == "Croissance (PIB)" && e.country.as_deref() == Some("États-Unis")));
    assert!(ev.iter().all(|e| e.original_name.as_deref() != Some("Atlanta Fed GDPNow")));
}

#[test]
fn a_shifted_economic_calendar_fails_instead_of_showing_wrong_dates() {
    // Read without the one-day offset, Thursday's jobless claims of `?date=2026-10-02` would land on Friday 2/10.
    let shifted = parse::economic(&json("nasdaq-economic-2026-10-02.json"), date(2026, 10, 2));
    assert!(shifted.unwrap_err().0.contains("dates décalées"));
    // Nasdaq back to answering with the day itself: Thursday's claims read for a plain Wednesday → refused too.
    assert!(parse::economic(&json("nasdaq-economic-2026-10-02.json"), date(2026, 9, 30)).is_err());
    // A real holiday week: claims on the Wednesday before Thanksgiving (26/11/2026) are accepted.
    assert!(parse::economic(&json("nasdaq-economic-2026-10-02.json"), date(2026, 11, 25)).is_ok());
    assert!(parse::us_federal_holiday(date(2026, 11, 26)) && !parse::us_federal_holiday(date(2026, 11, 19)));
    assert!(parse::us_federal_holiday(date(2025, 12, 25)) && parse::us_federal_holiday(date(2026, 1, 1)));
    let ok = parse::economic(&json("nasdaq-economic-2026-10-02.json"), date(2026, 10, 1)).unwrap();
    assert!(ok.iter().any(|e| e.original_name.as_deref() == Some("Initial Jobless Claims") && e.day == "2026-10-01"));
}

#[test]
fn nasdaq_company_calendars() {
    let e = parse::earnings(&json("nasdaq-earnings-2026-09-30.json"), date(2026, 9, 30)).unwrap();
    let (mu, cap) = e.iter().find(|(e, _)| e.symbol.as_deref() == Some("MU")).unwrap();
    assert_eq!(mu.title, "Résultats de Micron Technology, Inc.");
    assert_eq!(mu.time.as_deref(), Some("après la clôture"));
    assert_eq!(mu.consensus.as_deref(), Some("BPA $31.24"));
    assert_eq!(*cap, Some(1_222_319_670_000.0));
    assert_eq!(mu.url, "https://www.nasdaq.com/market-activity/stocks/mu/earnings");

    let d = parse::dividends(&json("nasdaq-dividends-2026-09-30.json")).unwrap();
    let agnc = d.iter().find(|e| e.symbol.as_deref() == Some("AGNC")).unwrap();
    assert_eq!(agnc.day, "2026-09-30");
    assert_eq!(agnc.title, "Détachement du dividende d'AGNC Investment Corp.");
    assert_eq!(parse::company("Mondelez International, Inc. Class A Common Stock"), "Mondelez International, Inc.");
    assert_eq!(agnc.detail.as_deref(), Some("0,12 $ par action, versé le 09/10/2026"));

    let s = parse::splits(&json("nasdaq-splits-2026-09-28.json")).unwrap();
    assert_eq!(s.len(), 20);
    let dxj = s.iter().find(|e| e.symbol.as_deref() == Some("DXJ")).unwrap();
    assert_eq!((dxj.day.as_str(), dxj.title.starts_with("Division")), ("2026-10-09", true));
    let dhy = s.iter().find(|e| e.symbol.as_deref() == Some("DHY")).unwrap();
    assert!(dhy.title.starts_with("Regroupement"), "1 : 10 is a reverse split");
    // "No record found": an empty day, not an error.
    assert!(parse::splits(&json("nasdaq-splits-none.json")).unwrap().is_empty());

    let i = parse::ipos(&json("nasdaq-ipo-2026-09.json")).unwrap();
    let oura = i.iter().find(|e| e.symbol.as_deref() == Some("OURA")).unwrap();
    assert_eq!(oura.day, "2026-09-30");
    assert!(oura.detail.as_deref().unwrap().starts_with("Fourchette 40.00-44.00 $"));
    assert!(i.iter().any(|e| e.title.ends_with("(prix fixé)")));
}

#[test]
fn official_schedules() {
    let fomc = parse::fomc(&sample("fed-fomccalendars.html"));
    let days: Vec<NaiveDate> = fomc.iter().map(|m| m.decision).collect();
    // 2025, 2026 and 2027: 8 meetings each (the 2025 notation vote left out).
    assert_eq!(days.len(), 24, "{days:?}");
    assert!(days.contains(&date(2026, 10, 28)));
    assert!(days.contains(&date(2026, 12, 9)));
    assert!(!days.contains(&date(2025, 8, 22)));
    let oct = fomc.iter().find(|m| m.decision == date(2026, 10, 28)).unwrap();
    assert_eq!(oct.detail, "Réunion des 27 et 28 oct.");
    let dec = fomc.iter().find(|m| m.decision == date(2026, 12, 9)).unwrap();
    assert!(dec.detail.ends_with("avec projections économiques"));

    let ecb = parse::ecb(&sample("ecb-mgcgc.html"));
    let days: Vec<NaiveDate> = ecb.iter().map(|m| m.decision).collect();
    assert_eq!(&days[..3], &[date(2026, 10, 29), date(2026, 12, 17), date(2027, 2, 4)]);
    assert!(!days.contains(&date(2026, 9, 30)), "non-monetary meeting");
    assert!(!days.contains(&date(2026, 10, 28)), "day 1");
    assert_eq!(ecb[0].detail, "Réunion de politique monétaire à Francfort, suivie d'une conférence de presse");
    // A changed page layout parses to nothing (the fetch then reports the source as failed).
    assert!(parse::fomc("<html></html>").is_empty() && parse::ecb("<html></html>").is_empty());
}

#[test]
fn official_date_wins_over_nasdaq() {
    // `?date=2026-10-29` holds the Fed decision of 28/10, the day the Fed's own calendar gives: both agree.
    let mut ev = parse::economic(&json("nasdaq-economic-2026-10-29.json"), date(2026, 10, 28)).unwrap();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].day, "2026-10-28");
    // Were they to disagree (here a made-up official 27/10), the official date wins and the gap is told.
    let meetings = vec![Meeting { decision: date(2026, 10, 27), detail: "Réunion des 26 et 27 oct.".into() }];
    let official = parse::meetings_events(&meetings, Bank::Fed, date(2026, 10, 20), date(2026, 11, 5));
    merge_official(&mut ev, official);
    sort_events(&mut ev);
    assert_eq!(ev.len(), 1);
    let fed = &ev[0];
    assert_eq!((fed.day.as_str(), fed.kind, fed.importance), ("2026-10-27", EventKind::CentralBank, Importance::High));
    assert_eq!(fed.source, parse::FED_SOURCE);
    assert_eq!(fed.note.as_deref(), Some("Nasdaq l'annonce le 28/10 : la date du calendrier officiel est retenue."));
    assert_eq!(fed.previous.as_deref(), Some("4.00%"));
    // Same day: the time given by Nasdaq is kept.
    let mut ev = parse::economic(&json("nasdaq-economic-2026-10-29.json"), date(2026, 10, 28)).unwrap();
    let meetings = vec![Meeting { decision: date(2026, 10, 28), detail: "Réunion des 27 et 28 oct.".into() }];
    merge_official(&mut ev, parse::meetings_events(&meetings, Bank::Fed, date(2026, 10, 20), date(2026, 11, 5)));
    assert_eq!((ev.len(), ev[0].time.as_deref()), (1, Some("19:00")));
}

/// `/api/calendar`: a bad parameter is a 400 with a French message, before any upstream call.
#[tokio::test]
async fn calendar_parameter_validation() {
    use altim::app::{AppState, router};
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    let many = (0..51).map(|i| format!("A{i}")).collect::<Vec<_>>().join(",");
    for (path, message) in [
        ("/api/calendar?days=0".to_string(), "days invalide"),
        ("/api/calendar?days=31".to_string(), "days invalide"),
        ("/api/calendar?days=abc".to_string(), "days invalide"),
        ("/api/calendar?days=7.5".to_string(), "days invalide"),
        ("/api/calendar?symbols=..%2Fx".to_string(), "symbole invalide"),
        ("/api/calendar?symbols=AAPL:bond".to_string(), "kind invalide"),
        (format!("/api/calendar?symbols={many}"), "symbols invalide"),
    ] {
        let app = router(
            AppState::new(altim::live::LiveHub::with(vec![], vec![], std::time::Duration::from_secs(3600))),
            altim::auth::Auth::new(None, false),
        );
        let r = app.oneshot(Request::get(&path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{path}");
        let b = String::from_utf8_lossy(&to_bytes(r.into_body(), 1 << 16).await.unwrap()).to_string();
        let v: Value = serde_json::from_str(&b).unwrap();
        assert!(v["error"].as_str().is_some_and(|e| e.starts_with(message)), "{path}: {b}");
    }
}

#[tokio::test]
#[ignore]
async fn calendar_live() {
    let cal = altim::calendar::calendar(7, None).await;
    for s in &cal.sources {
        println!("{} ok={} {:?} {:?}", s.name, s.ok, s.failed, s.error);
    }
    println!("{} événements", cal.events.len());
    assert!(cal.sources.iter().filter(|s| s.ok).count() >= 5, "{:?}", cal.sources);
    assert!(cal.events.iter().any(|e| e.kind == EventKind::Macro));
    let aapl = altim::calendar::upcoming_for("AAPL", altim::types::Kind::Stock, 30).await.unwrap_or_default();
    println!("AAPL : {} événements", aapl.len());
    assert!(aapl.iter().all(|e| e.importance == Importance::High || e.symbol.as_deref() == Some("AAPL")));
}

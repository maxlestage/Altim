//! Sector of each stock held (`/api/sectors`), for the sector exposure of the portfolio: the Nasdaq screener's
//! sector when it has one, else the SIC code filed at the SEC, grouped by division. ETFs (flag of the Nasdaq Trader
//! directory, or a SEC filer without SIC code that is not an operating company) are never assigned a sector: they
//! span several. Nothing is guessed: without a source, the sector is null and the reason says why.
use std::collections::HashSet;

use futures::StreamExt;
use serde::Serialize;

use crate::engine::decision_types::Sector;
use crate::fundamentals::{Listing, SecFiler, sec_filer};
use crate::js::now_ms;

pub const ETF_LABEL: &str = "ETF / fonds indiciel (plusieurs secteurs)";
const SEC_SOURCE: &str = "SEC EDGAR (code SIC déclaré)";
const NASDAQ_SOURCE: &str = "Nasdaq (secteur du screener des actions US)";
const DIRECTORY_SOURCE: &str = "Nasdaq Trader (annuaire des cotations : ETF)";
/// Parallel requests to the SEC (its limit is 10 per second).
const PARALLEL: usize = 5;

/// Nasdaq classification of a stock.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NasdaqClass {
    /// As published: "Technology".
    pub sector: String,
    /// French label: "Technologie".
    pub sector_fr: String,
    /// Activity: "Semiconductors".
    pub industry: String,
}

/// Sector of one symbol. `classification`: "nasdaq" | "sec" | "etf", null when unknown (then `reason` says why).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorItem {
    pub symbol: String,
    pub sector: Option<String>,
    pub classification: Option<&'static str>,
    pub source: Option<String>,
    pub reason: Option<String>,
    pub etf: bool,
    /// SIC division filed at the SEC (also given when the Nasdaq sector is the one used).
    pub sec: Option<Sector>,
    pub nasdaq: Option<NasdaqClass>,
}

/// State of one source over the request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceState {
    pub name: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorsReport {
    pub as_of: i64,
    pub items: Vec<SectorItem>,
    pub sources: Vec<SourceState>,
}

/// The sector of a symbol from what each source answered (Err: the source failed, with its message; Ok(None): the
/// source has no entry for it). `etf`: flag of the Nasdaq Trader directory, None when the directory failed.
pub fn classify(symbol: &str, etf: Option<bool>, sec: &Result<Option<SecFiler>, String>, nasdaq: &Result<Option<Listing>, String>) -> SectorItem {
    let filer = sec.as_ref().ok().and_then(Option::as_ref);
    let listing = nasdaq.as_ref().ok().and_then(Option::as_ref);
    let nasdaq_class = listing.filter(|l| !l.sector.is_empty()).map(|l| NasdaqClass {
        sector: l.sector.clone(),
        sector_fr: crate::screener::sector_fr(&l.sector),
        industry: l.industry.clone(),
    });
    let sec_sector = filer.and_then(|f| f.sector.clone());
    let mut item = SectorItem {
        symbol: symbol.to_string(),
        sector: None,
        classification: None,
        source: None,
        reason: None,
        etf: false,
        sec: sec_sector.clone(),
        nasdaq: nasdaq_class.clone(),
    };
    if etf == Some(true) || filer.is_some_and(|f| f.fund) {
        item.etf = true;
        item.sector = Some(ETF_LABEL.to_string());
        item.classification = Some("etf");
        item.source = Some(if etf == Some(true) {
            DIRECTORY_SOURCE.to_string()
        } else {
            format!("SEC EDGAR (pas de code SIC d'activité : {})", filer.map_or("", |f| f.name.as_str()))
        });
    } else if let Some(n) = nasdaq_class {
        item.sector = Some(n.sector_fr);
        item.classification = Some("nasdaq");
        item.source = Some(format!("{NASDAQ_SOURCE}, activité : {}", n.industry));
    } else if let Some(s) = sec_sector {
        item.sector = Some(s.label.clone());
        item.classification = Some("sec");
        item.source = Some(format!("{SEC_SOURCE} {} : {}", s.sic, s.sic_description));
    } else {
        let sec_why = match sec {
            Err(e) => format!("SEC EDGAR indisponible ({e})"),
            Ok(None) => "aucun dépôt à la SEC sous ce symbole (société étrangère ou fonds ?)".to_string(),
            Ok(Some(_)) => "pas de code SIC d'activité à la SEC".to_string(),
        };
        let nasdaq_why = match nasdaq {
            Err(e) => format!("screener Nasdaq indisponible ({e})"),
            Ok(_) => "pas de secteur au screener Nasdaq".to_string(),
        };
        item.reason = Some(format!("Secteur non couvert : {sec_why} ; {nasdaq_why}."));
    }
    item
}

fn msg(e: &crate::http::Error) -> String {
    e.0.clone()
}

/// Symbols as the directories write them (BRK.B → BRK-B).
fn listing_symbol(s: &str) -> String {
    s.trim().to_uppercase().replace(['.', '/'], "-")
}

/// Sectors of stock symbols (already validated, 50 at most). The SEC submissions are cached a week per company,
/// the Nasdaq screener and the directory for the day: a portfolio read again costs no request.
pub async fn sectors(symbols: &[String]) -> SectorsReport {
    if symbols.is_empty() {
        return SectorsReport { as_of: now_ms(), items: vec![], sources: vec![] };
    }
    let (universe, screener) = tokio::join!(crate::universe::stock_universe(), crate::fundamentals::screener());
    let etfs: Option<HashSet<String>> = universe.as_ref().ok().map(|u| u.iter().filter(|e| e.3 == 1).map(|e| e.0.clone()).collect());
    let screener = screener.map_err(|e| msg(&e));
    let mut unique: Vec<String> = Vec::new();
    for s in symbols {
        if !unique.contains(s) {
            unique.push(s.clone());
        }
    }
    let jobs: Vec<(String, Option<bool>, Result<Option<Listing>, String>)> = unique
        .into_iter()
        .map(|sym| {
            let key = listing_symbol(&sym);
            let etf = etfs.as_ref().map(|e| e.contains(&key));
            let nasdaq = screener.as_ref().map(|l| l.0.iter().find(|x| x.symbol == key).cloned()).map_err(Clone::clone);
            (sym, etf, nasdaq)
        })
        .collect();
    let results: Vec<(SectorItem, Option<String>)> = futures::stream::iter(jobs.into_iter().map(|(sym, etf, nasdaq)| async move {
        // An ETF known from the directory needs no SEC request.
        let sec = if etf == Some(true) { Ok(None) } else { sec_filer(&sym).await.map_err(|e| msg(&e)) };
        (classify(&sym, etf, &sec, &nasdaq), sec.err())
    }))
    .buffered(PARALLEL)
    .collect()
    .await;
    let sec_err = results.iter().find_map(|r| r.1.clone());
    let state = |name: &str, error: Option<String>| SourceState { name: name.to_string(), ok: error.is_none(), error };
    SectorsReport {
        as_of: now_ms(),
        items: results.into_iter().map(|r| r.0).collect(),
        sources: vec![state(NASDAQ_SOURCE, screener.err()), state(SEC_SOURCE, sec_err), state(DIRECTORY_SOURCE, universe.err().map(|e| msg(&e)))],
    }
}

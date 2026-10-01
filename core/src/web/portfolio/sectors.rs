//! Sector exposure of the portfolio held (`web/src/engine/sectors.ts`): stocks grouped by sector (as returned by
//! /api/sectors), cryptos and cash as their own blocks, stocks without a known sector as "Secteur inconnu".
//!
//! JSON contract of GET /api/sectors?symbols=AAPL,NVDA (≤ 50 stocks, ":crypto" entries ignored):
//! `{ asOf: ms, items: SectorItem[], sources: { name, ok, error: string | null }[] }`.
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::holdings::{Insight, InsightLevel, pc};
use crate::engine::decision_types::Sector;
use crate::js::fr;
use crate::types::Kind;
use crate::web::sorting::Sorting;

/// "nasdaq": sector of the Nasdaq screener; "sec": SIC division filed at the SEC; "etf": fund spanning several sectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SectorClassification {
    Nasdaq,
    Sec,
    Etf,
}

pub const CLASSIFICATIONS: [SectorClassification; 3] = [SectorClassification::Nasdaq, SectorClassification::Sec, SectorClassification::Etf];

impl SectorClassification {
    /// `CLASSIFICATION_SOURCE`.
    pub fn source(self) -> &'static str {
        match self {
            SectorClassification::Nasdaq => "Nasdaq (secteur du screener)",
            SectorClassification::Sec => "SEC EDGAR (code SIC, grandes divisions)",
            SectorClassification::Etf => "Nasdaq Trader / SEC (ETF)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NasdaqSector {
    pub sector: String,
    pub sector_fr: String,
    pub industry: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorItem {
    pub symbol: String,
    /// French label ("Technologie", "Finance et immobilier", "ETF / fonds indiciel (plusieurs secteurs)"), None when unknown.
    #[serde(default)]
    pub sector: Option<String>,
    #[serde(default)]
    pub classification: Option<SectorClassification>,
    /// Named source of the label, None when unknown.
    #[serde(default)]
    pub source: Option<String>,
    /// Why no source covers it (None when classified).
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub etf: bool,
    #[serde(default)]
    pub sec: Option<Sector>,
    #[serde(default)]
    pub nasdaq: Option<NasdaqSector>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorSource {
    pub name: String,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorsReport {
    #[serde(default)]
    pub as_of: i64,
    pub items: Vec<SectorItem>,
    #[serde(default)]
    pub sources: Vec<SectorSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockKind {
    Sector,
    Etf,
    Crypto,
    Cash,
    Unknown,
}

impl BlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockKind::Sector => "sector",
            BlockKind::Etf => "etf",
            BlockKind::Crypto => "crypto",
            BlockKind::Cash => "cash",
            BlockKind::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposureBlock {
    pub key: String,
    pub label: String,
    pub kind: BlockKind,
    pub value: f64,
    /// % of the whole portfolio (cash included).
    pub weight: f64,
    /// % of the stock part (stocks only, else None).
    pub stock_weight: Option<f64>,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownSector {
    pub symbol: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BySource {
    pub nasdaq: usize,
    pub sec: usize,
    pub etf: usize,
}

impl BySource {
    pub fn get(&self, c: SectorClassification) -> usize {
        match c {
            SectorClassification::Nasdaq => self.nasdaq,
            SectorClassification::Sec => self.sec,
            SectorClassification::Etf => self.etf,
        }
    }
    fn bump(&mut self, c: SectorClassification) {
        match c {
            SectorClassification::Nasdaq => self.nasdaq += 1,
            SectorClassification::Sec => self.sec += 1,
            SectorClassification::Etf => self.etf += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorExposure {
    pub total: f64,
    pub stock_value: f64,
    /// Sorted by weight, heaviest first.
    pub blocks: Vec<ExposureBlock>,
    /// 1 / Σ w² over the stocks with an operating sector (ETFs and unknown left out); None without any.
    pub effective_sectors: Option<f64>,
    /// % of the stock part whose operating sector is known (ETFs and unknown excluded).
    pub classified_share: f64,
    pub unknown: Vec<UnknownSector>,
    /// Number of stocks per classification, for the source line.
    pub by_source: BySource,
    pub insights: Vec<Insight>,
}

/// A sector above this share of the whole portfolio is flagged.
pub const SECTOR_MAX_WEIGHT: f64 = 35.0;
/// A sector above this share of the stock part is flagged (from 2 stock lines).
pub const SECTOR_MAX_STOCK_SHARE: f64 = 50.0;

pub const ETF_BLOCK_LABEL: &str = "ETF / fonds (plusieurs secteurs)";
pub const UNKNOWN_LABEL: &str = "Secteur inconnu";
pub const DEFAULT_FAILURE: &str = "classement sectoriel indisponible";

/// A line of the portfolio with its current value (USD).
#[derive(Debug, Clone, PartialEq)]
pub struct ExposureLine {
    pub symbol: String,
    pub kind: Kind,
    pub value: f64,
}

/// French collation close to `localeCompare(…, "fr")` for the block labels: accents and case only break ties.
pub fn fr_compare(a: &str, b: &str) -> Ordering {
    fn base(c: char) -> char {
        match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'À' | 'Â' | 'Ä' | 'Á' => 'a',
            'ç' | 'Ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'î' | 'ï' | 'í' | 'Î' | 'Ï' => 'i',
            'ô' | 'ö' | 'ó' | 'Ô' | 'Ö' => 'o',
            'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ÿ' | 'Ÿ' => 'y',
            c => c.to_lowercase().next().unwrap_or(c),
        }
    }
    let fold = |s: &str| s.chars().map(base).collect::<String>();
    fold(a).cmp(&fold(b)).then_with(|| a.to_lowercase().cmp(&b.to_lowercase())).then_with(|| b.cmp(a))
}

/// Exposure by sector. `sectors`: items of /api/sectors by symbol, None when the call failed (`failure` then says
/// why: every stock is "Secteur inconnu").
pub fn sector_exposure(lines: &[ExposureLine], cash: f64, sectors: Option<&HashMap<String, SectorItem>>, failure: &str) -> SectorExposure {
    let safe_cash = cash.max(0.0);
    let total = lines.iter().map(|l| l.value.max(0.0)).sum::<f64>() + safe_cash;
    struct Block {
        label: String,
        kind: BlockKind,
        value: f64,
        symbols: Vec<String>,
    }
    let mut blocks: IndexMap<String, Block> = IndexMap::new();
    let mut unknown: IndexMap<String, String> = IndexMap::new();
    let mut by_source = BySource::default();
    let mut counted: HashSet<String> = HashSet::new();
    let mut add = |key: String, label: String, kind: BlockKind, value: f64, symbol: Option<&str>| {
        let b = blocks.entry(key).or_insert(Block { label, kind, value: 0.0, symbols: Vec::new() });
        b.value += value;
        if let Some(s) = symbol {
            if !b.symbols.iter().any(|x| x == s) {
                b.symbols.push(s.into());
            }
        }
    };
    let mut stock_value = 0.0;
    for l in lines {
        let value = l.value.max(0.0);
        if l.kind == Kind::Crypto {
            add("crypto".into(), "Crypto".into(), BlockKind::Crypto, value, Some(&l.symbol));
            continue;
        }
        stock_value += value;
        let s = sectors.and_then(|m| m.get(&l.symbol));
        match s.and_then(|s| Some((s.sector.as_ref().filter(|x| !x.is_empty())?, s.classification?))) {
            Some((sector, classification)) => {
                if !counted.contains(&l.symbol) {
                    by_source.bump(classification);
                }
                counted.insert(l.symbol.clone());
                match classification {
                    SectorClassification::Etf => add("etf".into(), ETF_BLOCK_LABEL.into(), BlockKind::Etf, value, Some(&l.symbol)),
                    // Nasdaq and SEC are two classifications: never merged, the SEC's labelled "(SIC)".
                    SectorClassification::Sec => add(format!("sec:{sector}"), format!("{sector} (SIC)"), BlockKind::Sector, value, Some(&l.symbol)),
                    SectorClassification::Nasdaq => add(format!("nasdaq:{sector}"), sector.clone(), BlockKind::Sector, value, Some(&l.symbol)),
                }
            }
            None => {
                add("unknown".into(), UNKNOWN_LABEL.into(), BlockKind::Unknown, value, Some(&l.symbol));
                let reason = s
                    .and_then(|s| s.reason.clone())
                    .unwrap_or_else(|| if sectors.is_some() { "absent de la réponse du serveur".into() } else { failure.into() });
                unknown.insert(l.symbol.clone(), reason);
            }
        }
    }
    if safe_cash > 0.0 {
        add("cash".into(), "Liquidités".into(), BlockKind::Cash, safe_cash, None);
    }

    let pct = |v: f64, of: f64| if of > 0.0 { v / of * 100.0 } else { 0.0 };
    let mut out: Vec<ExposureBlock> = blocks
        .into_iter()
        .filter(|(_, b)| b.value > 0.0)
        .map(|(key, b)| ExposureBlock {
            weight: pct(b.value, total),
            stock_weight: matches!(b.kind, BlockKind::Sector | BlockKind::Etf | BlockKind::Unknown).then(|| pct(b.value, stock_value)),
            key,
            label: b.label,
            kind: b.kind,
            value: b.value,
            symbols: b.symbols,
        })
        .collect();
    out.sort_by_dyn(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(Ordering::Equal).then_with(|| fr_compare(&a.label, &b.label)));

    let sector_blocks: Vec<&ExposureBlock> = out.iter().filter(|b| b.kind == BlockKind::Sector).collect();
    let classified: f64 = sector_blocks.iter().map(|b| b.value).sum();
    let hhi: f64 = if classified > 0.0 { sector_blocks.iter().map(|b| (b.value / classified).powi(2)).sum() } else { 0.0 };
    let effective_sectors = (hhi > 0.0).then(|| 1.0 / hhi);
    let stock_lines = lines.iter().filter(|l| l.kind == Kind::Stock && l.value > 0.0).map(|l| l.symbol.as_str()).collect::<HashSet<_>>().len();

    let mut insights = Vec::new();
    if let Some(top) = sector_blocks.first() {
        if top.weight > SECTOR_MAX_WEIGHT {
            insights.push(Insight::new(InsightLevel::Warning, "sector_heavy", &[("sector", json!(top.label)), ("weight", json!(top.weight))]));
        } else if stock_lines >= 2 && top.stock_weight.unwrap_or(0.0) > SECTOR_MAX_STOCK_SHARE {
            insights.push(Insight::new(
                InsightLevel::Warning,
                "sector_heavy_stocks",
                &[("sector", json!(top.label)), ("share", json!(top.stock_weight.unwrap_or(0.0)))],
            ));
        }
    }
    if let Some(e) = effective_sectors {
        if sector_blocks.iter().map(|b| b.symbols.len()).sum::<usize>() >= 2 && e < 2.0 {
            insights.push(Insight::new(InsightLevel::Info, "sector_effective", &[("effective", json!(e))]));
        }
    }
    if let Some(etf) = out.iter().find(|b| b.kind == BlockKind::Etf) {
        insights.push(Insight::new(InsightLevel::Info, "sector_etf", &[("weight", json!(etf.weight)), ("symbols", json!(etf.symbols.join(", ")))]));
    }
    if let Some(unk) = out.iter().find(|b| b.kind == BlockKind::Unknown) {
        insights.push(Insight::new(
            InsightLevel::Info,
            "sector_unknown",
            &[("weight", json!(unk.weight)), ("symbols", json!(unk.symbols.join(", ")))],
        ));
    }

    SectorExposure {
        total,
        stock_value,
        classified_share: pct(classified, stock_value),
        effective_sectors,
        blocks: out,
        unknown: unknown.into_iter().map(|(symbol, reason)| UnknownSector { symbol, reason }).collect(),
        by_source,
        insights,
    }
}

pub fn sector_insight_text(i: &Insight) -> String {
    match i.code.as_str() {
        "sector_heavy" => format!(
            "{} pèse {} de votre patrimoine : une mauvaise passe de ce secteur pourrait toucher plusieurs lignes à la fois.",
            i.text("sector"),
            pc(i.num("weight"))
        ),
        "sector_heavy_stocks" => {
            format!("{} représente {} de vos actions : leur diversification sectorielle est faible.", i.text("sector"), pc(i.num("share")))
        }
        "sector_effective" => format!("Vos actions classées équivalent à {} secteur(s) de même poids.", fr(i.num("effective"), 0, 1)),
        "sector_etf" => {
            format!("ETF ({}, {}) : leur répartition par secteur n'est pas couverte (composition non lue).", i.text("symbols"), pc(i.num("weight")))
        }
        "sector_unknown" => format!("Secteur non couvert pour {} ({}).", i.text("symbols"), pc(i.num("weight"))),
        other => other.into(),
    }
}

/// "Nasdaq (secteur du screener) · 3 actions ; Nasdaq Trader / SEC (ETF) · 1 action" (SectorCard's source line).
pub fn source_line(by: &BySource) -> String {
    CLASSIFICATIONS
        .iter()
        .filter(|k| by.get(**k) > 0)
        .map(|k| format!("{} · {} action{}", k.source(), by.get(*k), if by.get(*k) > 1 { "s" } else { "" }))
        .collect::<Vec<_>>()
        .join(" ; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, digits: i32) {
        assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≠ {b}");
    }

    fn item(symbol: &str, sector: Option<&str>, classification: Option<SectorClassification>, reason: Option<&str>) -> SectorItem {
        SectorItem {
            symbol: symbol.into(),
            sector: sector.map(String::from),
            classification,
            source: classification.map(|_| "test".into()),
            reason: reason.map(String::from),
            etf: classification == Some(SectorClassification::Etf),
            sec: None,
            nasdaq: None,
        }
    }

    fn sectors() -> HashMap<String, SectorItem> {
        use SectorClassification::*;
        [
            item("AAPL", Some("Technologie"), Some(Nasdaq), None),
            item("NVDA", Some("Technologie"), Some(Nasdaq), None),
            item("JPM", Some("Finance"), Some(Nasdaq), None),
            item("BRK-B", Some("Finance et immobilier"), Some(Sec), None),
            item("SPY", Some("ETF / fonds indiciel (plusieurs secteurs)"), Some(Etf), None),
            item("ABCD", None, None, Some("Secteur non couvert : aucun dépôt à la SEC")),
        ]
        .into_iter()
        .map(|i| (i.symbol.clone(), i))
        .collect()
    }

    fn l(symbol: &str, kind: Kind, value: f64) -> ExposureLine {
        ExposureLine { symbol: symbol.into(), kind, value }
    }

    #[test]
    fn blocks_heaviest_first() {
        let s = sectors();
        let e = sector_exposure(
            &[
                l("AAPL", Kind::Stock, 3000.0),
                l("NVDA", Kind::Stock, 1000.0),
                l("JPM", Kind::Stock, 1000.0),
                l("SPY", Kind::Stock, 1000.0),
                l("ABCD", Kind::Stock, 500.0),
                l("BTC", Kind::Crypto, 2000.0),
                l("ETH", Kind::Crypto, 500.0),
            ],
            1000.0,
            Some(&s),
            DEFAULT_FAILURE,
        );
        assert_eq!(e.total, 10000.0);
        assert_eq!(e.stock_value, 6500.0);
        assert_eq!(
            e.blocks.iter().map(|b| (b.label.as_str(), b.weight)).collect::<Vec<_>>(),
            vec![("Technologie", 40.0), ("Crypto", 25.0), (ETF_BLOCK_LABEL, 10.0), ("Finance", 10.0), ("Liquidités", 10.0), (UNKNOWN_LABEL, 5.0)]
        );
        let tech = &e.blocks[0];
        close(tech.stock_weight.unwrap(), 4000.0 / 6500.0 * 100.0, 9);
        assert_eq!(tech.symbols, vec!["AAPL", "NVDA"]);
        assert_eq!(e.blocks.iter().find(|b| b.kind == BlockKind::Crypto).unwrap().stock_weight, None);
        // Weights add up to 100 %.
        close(e.blocks.iter().map(|b| b.weight).sum(), 100.0, 9);
        // Effective sectors over Technologie 4000 / Finance 1000: 1 / (0.8² + 0.2²).
        close(e.effective_sectors.unwrap(), 1.0 / 0.68, 9);
        close(e.classified_share, 5000.0 / 6500.0 * 100.0, 9);
        assert_eq!(e.by_source, BySource { nasdaq: 3, sec: 0, etf: 1 });
        assert_eq!(e.unknown, vec![UnknownSector { symbol: "ABCD".into(), reason: "Secteur non couvert : aucun dépôt à la SEC".into() }]);
        let codes: Vec<&str> = e.insights.iter().map(|i| i.code.as_str()).collect();
        assert_eq!(codes, vec!["sector_heavy", "sector_effective", "sector_etf", "sector_unknown"]);
        assert_eq!(
            sector_insight_text(&e.insights[0]),
            "Technologie pèse 40 % de votre patrimoine : une mauvaise passe de ce secteur pourrait toucher plusieurs lignes à la fois."
        );
        assert_eq!(source_line(&e.by_source), "Nasdaq (secteur du screener) · 3 actions ; Nasdaq Trader / SEC (ETF) · 1 action");
    }

    #[test]
    fn stock_part_concentration_sec_and_nasdaq_apart() {
        let s = sectors();
        let e = sector_exposure(
            &[l("AAPL", Kind::Stock, 2000.0), l("JPM", Kind::Stock, 1000.0), l("BRK-B", Kind::Stock, 500.0), l("BTC", Kind::Crypto, 6500.0)],
            0.0,
            Some(&s),
            DEFAULT_FAILURE,
        );
        assert_eq!(
            e.blocks.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(),
            vec!["Crypto", "Technologie", "Finance", "Finance et immobilier (SIC)"]
        );
        assert_eq!(
            e.insights[0],
            Insight::new(
                InsightLevel::Warning,
                "sector_heavy_stocks",
                &[("sector", json!("Technologie")), ("share", json!(2000.0 / 3500.0 * 100.0))]
            )
        );
        close(e.effective_sectors.unwrap(), 1.0 / ((2.0f64 / 3.5).powi(2) + (1.0f64 / 3.5).powi(2) + (0.5f64 / 3.5).powi(2)), 9);
        // One stock line only: 100 % of the stock part is not flagged (the line concentration is, elsewhere).
        let one = sector_exposure(&[l("AAPL", Kind::Stock, 1000.0), l("BTC", Kind::Crypto, 9000.0)], 0.0, Some(&s), DEFAULT_FAILURE);
        assert!(one.insights.is_empty());
    }

    #[test]
    fn server_unreachable() {
        let e = sector_exposure(&[l("AAPL", Kind::Stock, 1000.0), l("BTC", Kind::Crypto, 1000.0)], 0.0, None, "HTTP 502");
        assert_eq!(e.blocks.iter().map(|b| (b.kind, b.weight)).collect::<Vec<_>>(), vec![(BlockKind::Crypto, 50.0), (BlockKind::Unknown, 50.0)]);
        assert_eq!(e.effective_sectors, None);
        assert_eq!(e.classified_share, 0.0);
        assert_eq!(e.unknown, vec![UnknownSector { symbol: "AAPL".into(), reason: "HTTP 502".into() }]);
        // Empty portfolio: no block, no division by zero.
        let empty = sector_exposure(&[], 0.0, Some(&HashMap::new()), DEFAULT_FAILURE);
        assert_eq!((empty.total, empty.blocks.len(), empty.effective_sectors), (0.0, 0, None));
    }

    #[test]
    fn server_reply_reads() {
        let raw = r#"{"asOf":1,"items":[{"symbol":"AAPL","sector":"Technologie","classification":"nasdaq","source":"Nasdaq","reason":null,"etf":false,
            "sec":{"label":"Industrie manufacturière","sic":"3571","sicDescription":"Electronic Computers","source":"SEC"},
            "nasdaq":{"sector":"Technology","sectorFr":"Technologie","industry":"Computer Manufacturing"}},
            {"symbol":"ZZZZ","sector":null,"classification":null,"source":null,"reason":"inconnu","etf":false,"sec":null,"nasdaq":null}],
            "sources":[{"name":"SEC","ok":true,"error":null}]}"#;
        let r: SectorsReport = serde_json::from_str(raw).unwrap();
        assert_eq!(r.items[0].classification, Some(SectorClassification::Nasdaq));
        assert_eq!(r.items[1].reason.as_deref(), Some("inconnu"));
    }
}

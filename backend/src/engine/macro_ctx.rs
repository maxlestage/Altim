//! Macro-economic and geopolitical context (`web/src/engine/macro.ts`): an asset can sit in a buy zone while
//! everything collapses because of a war, a central bank or a crisis. No one can predict such an event; what can be
//! measured is the stress it causes in the markets, as soon as it starts:
//! - fear (VIX), fall of the S&P 500 from its recent high;
//! - unusual moves (compared with their own last year) of oil, gold, the dollar and US 10-year yields;
//! - escalation headlines (war declared, invasion, nuclear threat, bank run…), which cannot be checked on history.
//!
//! The market part is checked on each asset: does high stress precede a fall of the asset more often than usual?
use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::Evidence;
use super::guard::NewsItem;
use super::signal::{Candle, atr, sanitize};
use crate::js::{fr, iso_date};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacroLevel {
    Calm,
    Tense,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacroKey {
    Vix,
    Spx,
    Oil,
    Gold,
    Dollar,
    Rates,
}

impl MacroKey {
    /// Same order as the TypeScript (`["vix", "spx", "oil", "gold", "dollar", "rates"]`).
    pub const ALL: [MacroKey; 6] = [MacroKey::Vix, MacroKey::Spx, MacroKey::Oil, MacroKey::Gold, MacroKey::Dollar, MacroKey::Rates];

    pub fn as_str(self) -> &'static str {
        match self {
            MacroKey::Vix => "vix",
            MacroKey::Spx => "spx",
            MacroKey::Oil => "oil",
            MacroKey::Gold => "gold",
            MacroKey::Dollar => "dollar",
            MacroKey::Rates => "rates",
        }
    }
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MacroPoint {
    pub time: i64,
    pub close: f64,
}

/// `MacroSeries = Partial<Record<MacroKey, { time, close }[]>>`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MacroSeries {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vix: Option<Vec<MacroPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx: Option<Vec<MacroPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oil: Option<Vec<MacroPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold: Option<Vec<MacroPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dollar: Option<Vec<MacroPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rates: Option<Vec<MacroPoint>>,
}

impl MacroSeries {
    pub fn get(&self, k: MacroKey) -> Option<&Vec<MacroPoint>> {
        self.slot(k).as_ref()
    }
    fn slot(&self, k: MacroKey) -> &Option<Vec<MacroPoint>> {
        match k {
            MacroKey::Vix => &self.vix,
            MacroKey::Spx => &self.spx,
            MacroKey::Oil => &self.oil,
            MacroKey::Gold => &self.gold,
            MacroKey::Dollar => &self.dollar,
            MacroKey::Rates => &self.rates,
        }
    }
    pub fn set(&mut self, k: MacroKey, v: Option<Vec<MacroPoint>>) {
        let slot = match k {
            MacroKey::Vix => &mut self.vix,
            MacroKey::Spx => &mut self.spx,
            MacroKey::Oil => &mut self.oil,
            MacroKey::Gold => &mut self.gold,
            MacroKey::Dollar => &mut self.dollar,
            MacroKey::Rates => &mut self.rates,
        };
        *slot = v;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroFactor {
    pub code: String,
    pub points: f64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacroThemeKind {
    Geopolitics,
    Monetary,
    Trade,
    Stress,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroTheme {
    pub theme: MacroThemeKind,
    pub label: String,
    pub count: usize,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroValue {
    pub value: f64,
    pub change5d: f64,
}

/// `Partial<Record<MacroKey, { value, change5d }>>`, keys in `MacroKey::ALL` order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MacroValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vix: Option<MacroValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spx: Option<MacroValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oil: Option<MacroValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold: Option<MacroValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dollar: Option<MacroValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rates: Option<MacroValue>,
}

impl MacroValues {
    pub fn get(&self, k: MacroKey) -> Option<&MacroValue> {
        match k {
            MacroKey::Vix => self.vix.as_ref(),
            MacroKey::Spx => self.spx.as_ref(),
            MacroKey::Oil => self.oil.as_ref(),
            MacroKey::Gold => self.gold.as_ref(),
            MacroKey::Dollar => self.dollar.as_ref(),
            MacroKey::Rates => self.rates.as_ref(),
        }
    }
    fn set(&mut self, k: MacroKey, v: MacroValue) {
        let slot = match k {
            MacroKey::Vix => &mut self.vix,
            MacroKey::Spx => &mut self.spx,
            MacroKey::Oil => &mut self.oil,
            MacroKey::Gold => &mut self.gold,
            MacroKey::Dollar => &mut self.dollar,
            MacroKey::Rates => &mut self.rates,
        };
        *slot = Some(v);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroReport {
    pub score: f64,
    pub level: MacroLevel,
    /// Market part only (measurable, checked on history).
    pub market_score: f64,
    pub factors: Vec<MacroFactor>,
    pub themes: Vec<MacroTheme>,
    /// Latest values, for display.
    pub values: MacroValues,
    pub as_of: Option<i64>,
}

/// `MACRO` thresholds.
pub struct MacroThresholds {
    pub tense: f64,
    pub high: f64,
    pub window: usize,
}
pub const MACRO: MacroThresholds = MacroThresholds { tense: 25.0, high: 50.0, window: 250 };

fn one(v: f64) -> String {
    fr(v, 0, 1)
}
fn signed(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, one(v.abs()))
}

/// Values of each series on each date (last known), indexed by `MacroKey::index()`.
pub type MacroColumns = [Vec<Option<f64>>; 6];

#[derive(Debug, Clone, PartialEq)]
pub struct Aligned {
    pub dates: Vec<String>,
    pub times: Vec<i64>,
    pub values: MacroColumns,
}

/// Series aligned on the dates of the VIX (or the S&P 500): value of each series on each date (last known).
pub fn align(series: &MacroSeries) -> Aligned {
    let empty = Vec::new();
    let reference = match series.vix.as_ref() {
        Some(v) if !v.is_empty() => v,
        _ => series.spx.as_ref().unwrap_or(&empty),
    };
    let dates: Vec<String> = reference.iter().map(|p| iso_date(p.time)).collect();
    let times: Vec<i64> = reference.iter().map(|p| p.time).collect();
    let values: MacroColumns = std::array::from_fn(|idx| {
        let k = MacroKey::ALL[idx];
        let mut by_day: HashMap<String, f64> = HashMap::new();
        for p in series.get(k).unwrap_or(&empty) {
            by_day.insert(iso_date(p.time), p.close);
        }
        let mut last_v: Option<f64> = None;
        dates
            .iter()
            .map(|d| {
                if let Some(v) = by_day.get(d) {
                    last_v = Some(*v);
                }
                last_v
            })
            .collect()
    });
    Aligned { dates, times, values }
}

fn at(v: &[Option<f64>], j: i64) -> Option<f64> {
    if j < 0 { None } else { v.get(j as usize).copied().flatten() }
}

/// z-score of the 5-day change at index i against the previous `window` 5-day changes (no look-ahead).
fn z5(v: &[Option<f64>], i: usize, window: usize) -> Option<(f64, f64)> {
    let ch = |j: i64| -> Option<f64> {
        match (at(v, j), at(v, j - 5)) {
            (Some(a), Some(b)) if b > 0.0 => Some((a / b - 1.0) * 100.0),
            _ => None,
        }
    };
    let i = i as i64;
    let now = ch(i)?;
    let mut past: Vec<f64> = Vec::new();
    let mut j = 5.max(i - window as i64);
    while j < i - 5 {
        if let Some(c) = ch(j) {
            past.push(c);
        }
        j += 1;
    }
    if past.len() < 60 {
        return None;
    }
    let n = past.len() as f64;
    let m = past.iter().fold(0.0, |a, b| a + b) / n;
    let sd = (past.iter().fold(0.0, |a, b| a + (b - m) * (b - m)) / n).sqrt();
    if sd > 0.0 { Some(((now - m) / sd, now)) } else { None }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketStress {
    pub score: f64,
    pub factors: Vec<MacroFactor>,
}

fn factor(code: &str, points: f64, text: String) -> MacroFactor {
    MacroFactor { code: code.into(), points, text }
}

/// Market stress on day i (0–100) and its factors.
pub fn market_stress(values: &MacroColumns, i: usize) -> MarketStress {
    let mut f: Vec<MacroFactor> = Vec::new();
    let vixs = &values[MacroKey::Vix.index()];
    if let Some(vix) = vixs.get(i).copied().flatten() {
        if vix >= 30.0 {
            f.push(factor("vixHigh", 25.0, format!("Peur généralisée : VIX à {} (au-delà de 30).", one(vix))));
        } else if vix >= 25.0 {
            f.push(factor("vixHigh", 15.0, format!("Nervosité élevée : VIX à {}.", one(vix))));
        }
    }
    if let Some((z, change)) = z5(vixs, i, MACRO.window) {
        if z >= 2.0 {
            f.push(factor("vixJump", 15.0, format!("Le VIX a bondi de {} en 5 séances, inhabituel.", signed(change))));
        }
    }
    let spx = &values[MacroKey::Spx.index()];
    if let Some(now) = spx.get(i).copied().flatten() {
        let mut peak: f64 = 0.0;
        for x in spx.iter().take(i + 1).skip(i.saturating_sub(20)).flatten() {
            peak = peak.max(*x);
        }
        let dd = if peak > 0.0 { (1.0 - now / peak) * 100.0 } else { 0.0 };
        if dd >= 7.0 {
            f.push(factor("spxDrawdown", 20.0, format!("Le S&P 500 a perdu {} % depuis son plus haut du mois.", one(dd))));
        } else if dd >= 4.0 {
            f.push(factor("spxDrawdown", 10.0, format!("Le S&P 500 recule de {} % depuis son plus haut du mois.", one(dd))));
        }
    }
    let unusual: [(MacroKey, bool, f64, fn(f64) -> String); 4] = [
        (MacroKey::Oil, false, 15.0, |c| format!("Pétrole {} en 5 séances : choc d'offre possible (tensions géopolitiques).", signed(c))),
        (MacroKey::Gold, false, 10.0, |c| format!("Or {} en 5 séances : les investisseurs cherchent un refuge.", signed(c))),
        (MacroKey::Dollar, false, 10.0, |c| format!("Dollar {} en 5 séances : fuite vers la sécurité.", signed(c))),
        (MacroKey::Rates, true, 10.0, |c| {
            format!("Taux américains à 10 ans {} en 5 séances : banques centrales / inflation sous tension.", signed(c))
        }),
    ];
    for (k, both, pts, text) in unusual {
        if let Some((z, change)) = z5(&values[k.index()], i, MACRO.window) {
            if if both { z.abs() >= 2.0 } else { z >= 2.0 } {
                f.push(factor(k.as_str(), pts, text(change)));
            }
        }
    }
    let score = 100f64.min(f.iter().fold(0.0, |a, x| a + x.points));
    MarketStress { score, factors: f }
}

// ---------- Headlines ----------

/// JavaScript `/…/i` on ASCII patterns: ASCII case folding and ASCII word boundaries.
fn re(p: &str) -> Regex {
    Regex::new(&format!("(?i-u){p}")).expect("regex")
}

struct ThemeDef {
    theme: MacroThemeKind,
    label: &'static str,
    re: Regex,
}

static THEMES: LazyLock<Vec<ThemeDef>> = LazyLock::new(|| {
    vec![
        ThemeDef {
            theme: MacroThemeKind::Geopolitics,
            label: "Géopolitique / guerre",
            re: re(
                r"\b(wars?|invasion|invades?|invaded|missiles?|air ?strikes?|drone strikes?|military|troops|nuclear|sanctions?|ceasefire|hostages?|coup|blockade)\b",
            ),
        },
        ThemeDef {
            theme: MacroThemeKind::Monetary,
            label: "Banques centrales / inflation",
            re: re(r"\b(fed|federal reserve|fomc|powell|ecb|rate (hikes?|cuts?)|interest rates?|inflation|cpi|treasury yields?)\b"),
        },
        ThemeDef {
            theme: MacroThemeKind::Trade,
            label: "Commerce / droits de douane",
            re: re(r"\b(tariffs?|trade war|export (ban|controls?)|embargo)\b"),
        },
        ThemeDef {
            theme: MacroThemeKind::Stress,
            label: "Crise financière",
            re: re(r"\b(recession|default(s|ed)?|bank (runs?|collapse|failures?)|financial crisis|market crash|sell-?off|bankruptcy|contagion)\b"),
        },
    ]
});

/// Rare escalation events: each distinct one in the last 12 h adds points (not checkable on history).
static ESCALATION: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (re(r"\bdeclar(es|ed|ing) war\b"), "déclaration de guerre"),
        (re(r"\b(invades?|invaded|invasion of)\b"), "invasion"),
        (re(r"\bnuclear (strike|attack|threat|test)\b"), "menace nucléaire"),
        (re(r"\b(missile|air) ?strikes? on\b"), "frappes militaires"),
        (re(r"\b(closes?|closed|blockade of) (the )?strait\b"), "blocage d'un détroit (pétrole)"),
        (re(r"\bmartial law|state of emergency\b"), "état d'urgence"),
        (re(r"\bbank runs?|bank collapse\b"), "panique bancaire"),
        (re(r"\bcircuit breaker|trading halted\b"), "cotations suspendues"),
        (re(r"\bdefaults? on (its )?debt\b"), "défaut de paiement d'un État"),
    ]
});

/// A question ("Is a bank run coming?") is speculation, not an event.
static QUESTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\?\s*(-\s*[^-]+)?$").expect("regex"));

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeadlineThemes {
    pub themes: Vec<MacroTheme>,
    pub factors: Vec<MacroFactor>,
}

pub fn headline_themes(items: &[NewsItem], now: i64) -> HeadlineThemes {
    let recent: Vec<&NewsItem> = items.iter().filter(|n| n.time <= now && n.time >= now - 86_400_000).collect();
    let mut themes: Vec<MacroTheme> = THEMES
        .iter()
        .map(|t| {
            let hits: Vec<&&NewsItem> = recent.iter().filter(|n| t.re.is_match(&n.title)).collect();
            MacroTheme { theme: t.theme, label: t.label.into(), count: hits.len(), examples: hits.iter().take(3).map(|n| n.title.clone()).collect() }
        })
        .filter(|t| t.count > 0)
        .collect();
    themes.sort_by(|a, b| b.count.cmp(&a.count));
    let last12: Vec<&&NewsItem> = recent.iter().filter(|n| n.time >= now - 12 * 3_600_000 && !QUESTION.is_match(&n.title)).collect();
    let found: Vec<&str> = ESCALATION.iter().filter(|(r, _)| last12.iter().any(|n| r.is_match(&n.title))).map(|(_, t)| *t).collect();
    let factors = if found.is_empty() {
        vec![]
    } else {
        vec![MacroFactor {
            code: "escalation".into(),
            points: 30f64.min(10.0 * found.len() as f64),
            text: format!("Actualité : {} (dernières 12 h, non vérifiable sur l'historique).", found.join(", ")),
        }]
    };
    HeadlineThemes { themes, factors }
}

/// `macroReport(series, news = [], now = Date.now())`.
pub fn macro_report(series: &MacroSeries, news: &[NewsItem], now: i64) -> MacroReport {
    let a = align(series);
    let last = a.dates.len().checked_sub(1);
    let market = match last {
        Some(i) => market_stress(&a.values, i),
        None => MarketStress { score: 0.0, factors: vec![] },
    };
    let h = headline_themes(news, now);
    let score = 100f64.min(market.score + h.factors.iter().fold(0.0, |s, f| s + f.points));
    let mut values = MacroValues::default();
    if let Some(i) = last {
        if i >= 5 {
            for k in MacroKey::ALL {
                let v = &a.values[k.index()];
                if let (Some(now_v), Some(prev)) = (v[i], v[i - 5]) {
                    values.set(k, MacroValue { value: now_v, change5d: (now_v / prev - 1.0) * 100.0 });
                }
            }
        }
    }
    let mut factors = market.factors;
    factors.extend(h.factors);
    MacroReport {
        score,
        level: if score >= MACRO.high {
            MacroLevel::High
        } else if score >= MACRO.tense {
            MacroLevel::Tense
        } else {
            MacroLevel::Calm
        },
        market_score: market.score,
        factors,
        themes: h.themes,
        values,
        as_of: last.map(|i| a.times[i]),
    }
}

/// Does market stress precede a fall of this asset? Days with stress ≥ `MACRO.tense`: share followed within 5 days
/// by a fall of at least twice the asset's usual daily range (median ATR of the whole history), against all days.
pub fn macro_evidence(asset_daily: &[Candle], series: &MacroSeries) -> Option<Evidence> {
    let c = sanitize(asset_daily);
    let a = align(series);
    let mut idx: HashMap<&str, usize> = HashMap::new();
    for (i, d) in a.dates.iter().enumerate() {
        idx.insert(d.as_str(), i);
    }
    let at = atr(&c, 14);
    let mut ratios: Vec<f64> = at.iter().enumerate().filter_map(|(j, r)| r.map(|r| r / c[j].close)).collect();
    ratios.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    if ratios.is_empty() {
        return None;
    }
    let usual = ratios[ratios.len() / 2];
    let (mut n, mut hits, mut base_n, mut base_hits) = (0.0, 0.0, 0.0, 0.0);
    let mut j = 20;
    while j + 5 < c.len() {
        let Some(&k) = idx.get(iso_date(c[j].time).as_str()) else {
            j += 1;
            continue;
        };
        let mut low = f64::INFINITY;
        for x in &c[j + 1..=j + 5] {
            low = low.min(x.low);
        }
        let fell = if 1.0 - low / c[j].close >= 2.0 * usual { 1.0 } else { 0.0 };
        base_n += 1.0;
        base_hits += fell;
        if market_stress(&a.values, k).score >= MACRO.tense {
            n += 1.0;
            hits += fell;
        }
        j += 1;
    }
    if n == 0.0 || base_n == 0.0 {
        return None;
    }
    let rate = (hits / n) * 100.0;
    let base = (base_hits / base_n) * 100.0;
    Some(Evidence { samples: n, rate, base, lift: if base > 0.0 { rate / base } else { 1.0 } })
}

/// What the macro context changes for each horizon ("short" | "medium" | "long"; anything else → None, as the
/// TypeScript `undefined`).
pub fn macro_advice(level: MacroLevel, horizon: &str) -> Option<&'static str> {
    if level == MacroLevel::Calm {
        return None;
    }
    let high = level == MacroLevel::High;
    match horizon {
        "short" => Some(if high {
            "Contexte macro très tendu : éviter d'entrer à court terme, les zones techniques sautent facilement."
        } else {
            "Contexte macro tendu : réduire la taille et garder un stop serré."
        }),
        "medium" => Some(if high {
            "Contexte macro très tendu : attendre le bas de la zone, ou entrer par petites tranches."
        } else {
            "Contexte macro tendu : entrer en deux ou trois fois plutôt qu'en une."
        }),
        "long" => Some(if high {
            "Contexte macro très tendu : les crises offrent parfois de bons points d'entrée long terme, mais seulement par achats échelonnés sur plusieurs semaines."
        } else {
            "Contexte macro tendu : échelonner les achats long terme."
        }),
        _ => None,
    }
}

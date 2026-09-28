//! Summaries of a decision (`/api/decision`), all derived deterministically from what the decision engine already
//! measured; none of them changes the verdict:
//! - `rating`: one of 6 levels (ACHAT FORT … VENTE FORTE) from the verdict, its level, the confidence and the trend;
//! - `score`: composite multi-factor score −100 … +100 over the families, with weights the user can tune (`w=`),
//!   renormalised over the factors that could be measured;
//! - `degraded`: "signal dégradé" when the evidence contradicts itself, the data are unreliable or the signal lost
//!   money on this asset;
//! - `marketRegime`: risk-on / risk-off / neutre from the macro stress and the benchmark's trend;
//! - `horizon`: scalping … long terme, from the plan's candles and the distance to its first target in ATR.
use serde::{Deserialize, Serialize};

use super::decision_types::{Family, Level, Status, Verdict};
use super::macro_ctx::{MACRO, MacroLevel, MacroReport};
use super::signal::sma;
use crate::js::{fr, round};

// ---------- Rating ----------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rating {
    StrongBuy,
    Buy,
    #[default]
    Hold,
    Reduce,
    Sell,
    StrongSell,
}

impl Rating {
    pub fn label(self) -> &'static str {
        match self {
            Rating::StrongBuy => "ACHAT FORT",
            Rating::Buy => "ACHAT",
            Rating::Hold => "ATTENDRE",
            Rating::Reduce => "ALLÉGER",
            Rating::Sell => "VENDRE",
            Rating::StrongSell => "VENTE FORTE",
        }
    }
}

/// Confidence from which a buy or a sell is "strong".
pub const STRONG_CONFIDENCE: f64 = 70.0;

/// Rules, in order:
/// - a degraded signal never gives a buy rating (ATTENDRE instead);
/// - ACHETER: ACHAT FORT with the "Signal fort" level and a confidence ≥ 70, else ACHAT;
/// - ZONE D'ACHAT: ACHAT (the zone is reached, a limit order is possible);
/// - ATTENDRE: ATTENDRE; ALLÉGER: ALLÉGER;
/// - VENDRE (held): VENTE FORTE when the background trend is down and the confidence ≥ 70, else VENDRE;
/// - AUCUNE POSITION: VENDRE when the background trend is down (stay out, sell what is held), else ATTENDRE (a
///   blocking veto that is not a downtrend: nothing to do until it clears).
pub fn rating(verdict: Verdict, level: Level, confidence: f64, trend_down: bool, degraded: bool) -> Rating {
    match verdict {
        Verdict::Buy | Verdict::BuyZone if degraded => Rating::Hold,
        Verdict::Buy if level == Level::Strong && confidence >= STRONG_CONFIDENCE => Rating::StrongBuy,
        Verdict::Buy | Verdict::BuyZone => Rating::Buy,
        Verdict::Wait => Rating::Hold,
        Verdict::Trim => Rating::Reduce,
        Verdict::Sell if trend_down && confidence >= STRONG_CONFIDENCE => Rating::StrongSell,
        Verdict::Sell => Rating::Sell,
        Verdict::NoPosition if trend_down => Rating::Sell,
        Verdict::NoPosition => Rating::Hold,
    }
}

// ---------- Composite score ----------

/// Factors: key of `w=`, label, default weight (%), families averaged into it (plus the technical structure for
/// "tech").
pub const FACTORS: [(&str, &str, f64, &[&str]); 6] = [
    ("tech", "Technique", 32.0, &["trend", "volume", "volatility"]),
    ("mom", "Momentum", 18.0, &["momentum"]),
    ("fund", "Fondamentaux", 20.0, &["valuation", "fundamentals", "onchain"]),
    ("sent", "Sentiment", 10.0, &["sentiment"]),
    ("news", "Actualités", 10.0, &["news"]),
    ("macro", "Macro", 10.0, &["macro"]),
];

/// Weights of the 6 factors, in `FACTORS` order (0 – 100 each, any total above 0: they are renormalised).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreWeights(pub [f64; 6]);

impl Default for ScoreWeights {
    fn default() -> Self {
        ScoreWeights(FACTORS.map(|f| f.2))
    }
}

/// `w=tech:32,mom:18,fund:20,sent:10,news:10,macro:10`: integers 0 – 100, each factor at most once, a factor left
/// out keeps its default weight, the total must be above 0. Absent or empty → None (default weights).
pub fn parse_weights(v: Option<&str>) -> Result<Option<ScoreWeights>, String> {
    let Some(v) = v.filter(|s| !s.trim().is_empty()) else { return Ok(None) };
    let usage = "w invalide (tech|mom|fund|sent|news|macro:entier de 0 à 100, séparés par des virgules)";
    let mut w = ScoreWeights::default();
    let mut seen = [false; 6];
    for item in v.split(',') {
        let Some((key, value)) = item.trim().split_once(':') else { return Err(usage.into()) };
        let Some(i) = FACTORS.iter().position(|f| f.0 == key.trim()) else { return Err(usage.into()) };
        let value = value.trim();
        let n: u32 = match value.parse() {
            Ok(n) if value.bytes().all(|b| b.is_ascii_digit()) && n <= 100 => n,
            _ => return Err("w invalide : chaque poids est un entier de 0 à 100".into()),
        };
        if std::mem::replace(&mut seen[i], true) {
            return Err(format!("w invalide : « {} » donné deux fois", FACTORS[i].0));
        }
        w.0[i] = n as f64;
    }
    if w.0.iter().sum::<f64>() <= 0.0 {
        return Err("w invalide : la somme des poids doit être supérieure à 0".into());
    }
    Ok(Some(w))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreFactor {
    /// "tech" | "mom" | "fund" | "sent" | "news" | "macro"
    pub key: String,
    pub label: String,
    /// Weight asked (default or `w=`), %.
    pub weight: f64,
    /// Weight used after renormalisation over the measured factors, % (0 when not measured).
    pub applied: f64,
    /// −100 … +100: mean of its measured families; None when none was measured.
    pub value: Option<f64>,
    /// value × applied ÷ 100: the contributions add up to the score.
    pub contribution: Option<f64>,
    /// Families (and "structure") that were measured and averaged.
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeScore {
    /// −100 … +100; None when no factor could be measured.
    pub value: Option<f64>,
    /// "Plutôt favorable"
    pub label: String,
    pub factors: Vec<ScoreFactor>,
    /// Labels of the factors without any measured family (their weight is spread over the others).
    pub missing: Vec<String>,
    /// true when the weights come from `w=`.
    pub custom: bool,
    pub text: String,
}

fn score_label(v: f64) -> &'static str {
    if v >= 40.0 {
        "Nettement favorable"
    } else if v >= 15.0 {
        "Plutôt favorable"
    } else if v > -15.0 {
        "Neutre"
    } else if v > -40.0 {
        "Plutôt défavorable"
    } else {
        "Nettement défavorable"
    }
}

fn signed0(v: f64) -> String {
    format!(
        "{}{}",
        if v > 0.0 {
            "+"
        } else if v < 0.0 {
            "−"
        } else {
            ""
        },
        fr(v.abs(), 0, 0)
    )
}

/// Each factor is the mean of its measured families (the technique also averages the structure score); the score
/// is the weighted mean of the measured factors, the weights renormalised over them.
pub fn composite(fams: &[Family], structure: Option<f64>, weights: Option<ScoreWeights>) -> CompositeScore {
    let custom = weights.is_some();
    let w = weights.unwrap_or_default();
    let mut factors: Vec<ScoreFactor> = FACTORS
        .iter()
        .zip(w.0)
        .map(|((key, label, _, keys), weight)| {
            let mut values = vec![];
            let mut sources = vec![];
            for f in fams.iter().filter(|f| keys.contains(&f.key.as_str()) && f.status != Status::Unavailable) {
                if let Some(s) = f.score {
                    values.push(s);
                    sources.push(f.key.clone());
                }
            }
            if *key == "tech" {
                if let Some(s) = structure {
                    values.push(s);
                    sources.push("structure".into());
                }
            }
            let value = (!values.is_empty()).then(|| round(values.iter().sum::<f64>() / values.len() as f64));
            ScoreFactor { key: (*key).into(), label: (*label).into(), weight, applied: 0.0, value, contribution: None, sources }
        })
        .collect();
    let total: f64 = factors.iter().filter(|f| f.value.is_some()).map(|f| f.weight).sum();
    let missing: Vec<String> = factors.iter().filter(|f| f.value.is_none()).map(|f| f.label.clone()).collect();
    let mut value = None;
    if total > 0.0 {
        let mut sum = 0.0;
        for f in factors.iter_mut() {
            if let Some(v) = f.value {
                f.applied = round(f.weight / total * 1000.0) / 10.0;
                let c = v * f.weight / total;
                f.contribution = Some(round(c * 10.0) / 10.0);
                sum += c;
            }
        }
        value = Some(round(sum));
    }
    let measured = factors.iter().filter(|f| f.value.is_some() && f.weight > 0.0).count();
    let (label, text) = match value {
        Some(v) => (
            score_label(v).to_string(),
            format!(
                "Score composite {} sur {measured} facteur(s) mesuré(s){}{}{}",
                signed0(v),
                if custom { ", poids personnalisés" } else { ", poids par défaut" },
                if missing.is_empty() {
                    String::new()
                } else {
                    format!(" ; non mesuré(s) : {} (poids réparti sur les autres)", missing.join(", "))
                },
                "."
            ),
        ),
        None => ("Non disponible".into(), "Aucun facteur pondéré n'a pu être mesuré : pas de score composite.".into()),
    };
    CompositeScore { value, label, factors, missing, custom, text }
}

// ---------- Degraded signal ----------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Degraded {
    pub active: bool,
    /// "⚠️ Signal dégradé — …" when active, empty otherwise.
    pub headline: String,
    pub reasons: Vec<String>,
}

pub const DEGRADED_HEADLINE: &str = "⚠️ Signal dégradé — Le modèle détecte des signaux contradictoires. Aucune entrée privilégiée actuellement.";
/// Headlines when nothing contradicts itself but the data or the signal's record can't be trusted: the headline
/// names the actual cause (the reasons list gives the detail).
pub const DEGRADED_DATA_HEADLINE: &str = "⚠️ Signal dégradé — Données peu fiables. Aucune entrée privilégiée actuellement.";
pub const DEGRADED_RECORD_HEADLINE: &str =
    "⚠️ Signal dégradé — Le signal n'a pas fait ses preuves sur cet actif. Aucune entrée privilégiée actuellement.";
/// Families on each side from which the evidence contradicts itself.
pub const CONTRADICTION_FAMILIES: usize = 3;
/// Technique and fundamentals both beyond this score, on opposite sides: conflict.
pub const CONFLICT_SCORE: f64 = 30.0;

/// Active when: at least 3 favourable and 3 unfavourable families; technique and fundamentals strongly opposed;
/// unreliable data (low reliability or sources in conflict); the signal lost money on this asset (its own
/// walk-forward history, fees included).
pub fn degraded(fams: &[Family], score: &CompositeScore, unreliable: Option<String>, signal_lost: Option<String>) -> Degraded {
    let mut reasons = vec![];
    let pos: Vec<&str> = fams.iter().filter(|f| f.status == Status::Positive).map(|f| f.label.as_str()).collect();
    let neg: Vec<&str> = fams.iter().filter(|f| f.status == Status::Negative).map(|f| f.label.as_str()).collect();
    if pos.len() >= CONTRADICTION_FAMILIES && neg.len() >= CONTRADICTION_FAMILIES {
        reasons.push(format!(
            "Familles contradictoires : {} favorables ({}) et {} défavorables ({})",
            pos.len(),
            pos.join(", ").to_lowercase(),
            neg.len(),
            neg.join(", ").to_lowercase()
        ));
    }
    let value = |k: &str| score.factors.iter().find(|f| f.key == k).and_then(|f| f.value);
    if let (Some(t), Some(f)) = (value("tech"), value("fund")) {
        if t.abs() >= CONFLICT_SCORE && f.abs() >= CONFLICT_SCORE && t.signum() != f.signum() {
            reasons.push(format!(
                "Technique {} ({}) mais fondamentaux {} ({})",
                if t > 0.0 { "favorable" } else { "défavorable" },
                signed0(t),
                if f > 0.0 { "favorables" } else { "défavorables" },
                signed0(f)
            ));
        }
    }
    let headline = if !reasons.is_empty() {
        DEGRADED_HEADLINE
    } else if unreliable.is_some() {
        DEGRADED_DATA_HEADLINE
    } else if signal_lost.is_some() {
        DEGRADED_RECORD_HEADLINE
    } else {
        ""
    };
    reasons.extend(unreliable);
    reasons.extend(signal_lost);
    Degraded { active: !reasons.is_empty(), headline: headline.into(), reasons }
}

// ---------- Market regime ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegimeKind {
    RiskOn,
    RiskOff,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRegime {
    pub kind: RegimeKind,
    /// "Risk-on" | "Risk-off" | "Neutre"
    pub label: String,
    /// Benchmark whose trend was read ("S&P 500", "Bitcoin"), when its history was available.
    pub benchmark: Option<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchTrend {
    Up,
    Down,
    Range,
}

/// Up: close above its 200-day average and the 50-day above the 200-day; down: both below; else range.
fn bench_trend(closes: &[f64]) -> Option<(BenchTrend, String)> {
    let last = *closes.last()?;
    let s50 = sma(closes, 50).last().copied().flatten()?;
    let s200 = sma(closes, 200).last().copied().flatten()?;
    let gap = (last / s200 - 1.0) * 100.0;
    let where_ = format!(
        "{} de sa moyenne 200 jours ({}{} %)",
        if last > s200 { "au-dessus" } else { "sous" },
        if gap >= 0.0 { "+" } else { "−" },
        fr(gap.abs(), 0, 1)
    );
    Some(if last > s200 && s50 > s200 {
        (BenchTrend::Up, format!("{where_}, moyenne 50 jours au-dessus de la 200 : tendance haussière"))
    } else if last < s200 && s50 < s200 {
        (BenchTrend::Down, format!("{where_}, moyenne 50 jours sous la 200 : tendance baissière"))
    } else {
        (BenchTrend::Range, format!("{where_} : tendance indécise"))
    })
}

/// Rules: risk-off when the macro stress is "très tendu" (≥ 50), the VIX is at 30 or above, or the stress is
/// "tendu" (≥ 25) while the benchmark trends down; risk-on when the stress is calm and the benchmark trends up;
/// neutre otherwise. Without the macro report, only a benchmark trending down gives risk-off, never risk-on (the
/// stress is not known). None when neither is available.
pub fn market_regime(report: Option<&MacroReport>, benchmark: &str, closes: &[f64]) -> Option<MarketRegime> {
    let trend = bench_trend(closes);
    if report.is_none() && trend.is_none() {
        return None;
    }
    let mut reasons = vec![];
    if let Some(r) = report {
        let word = match r.level {
            MacroLevel::Calm => "calme",
            MacroLevel::Tense => "tendu",
            MacroLevel::High => "très tendu",
        };
        reasons.push(format!("Stress macro {}/100 ({word})", fr(r.score, 0, 0)));
        if let Some(v) = r.values.vix {
            reasons.push(format!("VIX {}", fr(v.value, 0, 1)));
        }
    } else {
        reasons.push("Stress macro indisponible".into());
    }
    match &trend {
        Some((_, text)) => reasons.push(format!("{benchmark} {text}")),
        None => reasons.push(format!("{benchmark} : moins de 200 jours d'historique, tendance non mesurée")),
    }
    let t = trend.map(|t| t.0);
    let vix_high = report.and_then(|r| r.values.vix).is_some_and(|v| v.value >= 30.0);
    let kind = match report {
        Some(r) if r.score >= MACRO.high || vix_high => RegimeKind::RiskOff,
        Some(r) if r.score >= MACRO.tense && t == Some(BenchTrend::Down) => RegimeKind::RiskOff,
        Some(r) if r.level == MacroLevel::Calm && t == Some(BenchTrend::Up) => RegimeKind::RiskOn,
        None if t == Some(BenchTrend::Down) => RegimeKind::RiskOff,
        _ => RegimeKind::Neutral,
    };
    let label = match kind {
        RegimeKind::RiskOn => "Risk-on",
        RegimeKind::RiskOff => "Risk-off",
        RegimeKind::Neutral => "Neutre",
    };
    Some(MarketRegime { kind, label: label.into(), benchmark: t.map(|_| benchmark.to_string()), reasons })
}

// ---------- Horizon ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HorizonKind {
    Scalping,
    DayTrading,
    Swing,
    MediumTerm,
    LongTerm,
}

impl HorizonKind {
    pub fn label(self) -> &'static str {
        match self {
            HorizonKind::Scalping => "Scalping",
            HorizonKind::DayTrading => "Day trading",
            HorizonKind::Swing => "Swing",
            HorizonKind::MediumTerm => "Moyen terme",
            HorizonKind::LongTerm => "Long terme",
        }
    }
    fn span(self) -> &'static str {
        match self {
            HorizonKind::Scalping => "quelques minutes à quelques heures",
            HorizonKind::DayTrading => "dans la journée",
            HorizonKind::Swing => "quelques jours à quelques semaines",
            HorizonKind::MediumTerm => "quelques semaines à quelques mois",
            HorizonKind::LongTerm => "plusieurs mois et plus",
        }
    }
}

/// Candles the plan is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanCandles {
    H1,
    H4,
    D1,
    W1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HorizonClass {
    pub kind: HorizonKind,
    pub label: String,
    /// Distance from the entry to target 1, in ATR of the plan's candles.
    pub atr_distance: f64,
    pub detail: String,
}

/// Rule (distance = (target 1 − entry) ÷ ATR 14 of the plan's candles):
/// - 1 h candles: scalping up to 3 ATR, day trading beyond;
/// - 4 h: day trading up to 3 ATR, swing beyond;
/// - daily: swing up to 4 ATR, medium term up to 12, long term beyond;
/// - weekly: medium term up to 4 ATR, long term beyond.
pub fn horizon(candles: PlanCandles, entry: f64, target1: f64, atr: f64) -> Option<HorizonClass> {
    if atr <= 0.0 || target1 <= entry {
        return None;
    }
    let d = (target1 - entry) / atr;
    let (kind, unit) = match candles {
        PlanCandles::H1 => (if d <= 3.0 { HorizonKind::Scalping } else { HorizonKind::DayTrading }, "1 h"),
        PlanCandles::H4 => (if d <= 3.0 { HorizonKind::DayTrading } else { HorizonKind::Swing }, "4 h"),
        PlanCandles::D1 => (
            if d <= 4.0 {
                HorizonKind::Swing
            } else if d <= 12.0 {
                HorizonKind::MediumTerm
            } else {
                HorizonKind::LongTerm
            },
            "journalières",
        ),
        PlanCandles::W1 => (if d <= 4.0 { HorizonKind::MediumTerm } else { HorizonKind::LongTerm }, "hebdomadaires"),
    };
    let d = round(d * 10.0) / 10.0;
    Some(HorizonClass {
        kind,
        label: kind.label().into(),
        atr_distance: d,
        detail: format!("Plan sur bougies {unit}, objectif 1 à {} ATR de l'entrée : {} ({})", fr(d, 0, 1), kind.label().to_lowercase(), kind.span()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::macro_ctx::{MacroValue, MacroValues};

    fn fam(key: &str, score: Option<f64>) -> Family {
        let status = match score {
            None => Status::Unavailable,
            Some(s) if s >= 20.0 => Status::Positive,
            Some(s) if s <= -20.0 => Status::Negative,
            _ => Status::Neutral,
        };
        Family { key: key.into(), label: key.into(), score, status, summary: String::new(), points: vec![], source: String::new() }
    }

    #[test]
    fn rating_rules() {
        use Verdict::*;
        assert_eq!(rating(Buy, Level::Strong, 72.0, false, false), Rating::StrongBuy);
        assert_eq!(rating(Buy, Level::Strong, 69.0, false, false), Rating::Buy);
        assert_eq!(rating(Buy, Level::Moderate, 90.0, false, false), Rating::Buy);
        assert_eq!(rating(Buy, Level::Strong, 90.0, false, true), Rating::Hold, "degraded: no buy rating");
        assert_eq!(rating(BuyZone, Level::Moderate, 50.0, false, false), Rating::Buy);
        assert_eq!(rating(Wait, Level::Waiting, 50.0, false, false), Rating::Hold);
        assert_eq!(rating(Trim, Level::Moderate, 50.0, false, false), Rating::Reduce);
        assert_eq!(rating(Sell, Level::Exit, 75.0, true, false), Rating::StrongSell);
        assert_eq!(rating(Sell, Level::Exit, 75.0, false, false), Rating::Sell);
        assert_eq!(rating(NoPosition, Level::Exit, 80.0, true, false), Rating::Sell);
        assert_eq!(rating(NoPosition, Level::HighRisk, 80.0, false, false), Rating::Hold);
        assert_eq!(Rating::StrongSell.label(), "VENTE FORTE");
    }

    #[test]
    fn weights_are_validated() {
        assert_eq!(parse_weights(None), Ok(None));
        assert_eq!(parse_weights(Some(" ")), Ok(None));
        let w = parse_weights(Some("tech:50,macro:0")).unwrap().unwrap();
        assert_eq!(w.0, [50.0, 18.0, 20.0, 10.0, 10.0, 0.0]);
        for bad in ["tech", "foo:10", "tech:101", "tech:-1", "tech:1.5", "tech:+5", "tech:10,tech:20", "tech:"] {
            assert!(parse_weights(Some(bad)).is_err(), "{bad}");
        }
        assert!(parse_weights(Some("tech:0,mom:0,fund:0,sent:0,news:0,macro:0")).unwrap_err().contains("supérieure à 0"));
    }

    #[test]
    fn composite_by_hand() {
        // Technique: (40 + 20 + −30) / 3 families + structure 50 → (40 + 20 − 30 + 50) / 4 = 20. Momentum 60.
        // Fundamentals, sentiment, news, macro unavailable: weights 32 and 18 renormalised to 64 % and 36 %.
        let fams = vec![
            fam("trend", Some(40.0)),
            fam("volume", Some(20.0)),
            fam("volatility", Some(-30.0)),
            fam("momentum", Some(60.0)),
            fam("fundamentals", None),
            fam("valuation", None),
            fam("macro", None),
            fam("sentiment", None),
            fam("news", None),
            fam("liquidity", Some(90.0)),
        ];
        let s = composite(&fams, Some(50.0), None);
        let tech = &s.factors[0];
        assert_eq!((tech.value, tech.applied, tech.contribution), (Some(20.0), 64.0, Some(12.8)));
        assert_eq!(tech.sources, ["trend", "volume", "volatility", "structure"]);
        assert_eq!((s.factors[1].applied, s.factors[1].contribution), (36.0, Some(21.6)));
        // 20 × 0.64 + 60 × 0.36 = 34.4 → 34 (liquidity is not a factor).
        assert_eq!(s.value, Some(34.0));
        assert_eq!(s.label, "Plutôt favorable");
        assert_eq!(s.missing, ["Fondamentaux", "Sentiment", "Actualités", "Macro"]);
        assert!(!s.custom && s.text.contains("non mesuré(s) : Fondamentaux"), "{}", s.text);
        // Custom weights: momentum only.
        let s = composite(&fams, Some(50.0), Some(ScoreWeights([0.0, 10.0, 0.0, 0.0, 0.0, 0.0])));
        assert_eq!((s.value, s.custom), (Some(60.0), true));
        // Nothing measured.
        assert_eq!(composite(&[fam("trend", None)], None, None).value, None);
    }

    #[test]
    fn degraded_when_the_evidence_contradicts_itself() {
        let mut fams: Vec<Family> = ["trend", "momentum", "volume"].iter().map(|k| fam(k, Some(50.0))).collect();
        fams.extend(["valuation", "fundamentals", "macro"].iter().map(|k| fam(k, Some(-50.0))));
        let s = composite(&fams, None, None);
        let d = degraded(&fams, &s, None, None);
        assert!(d.active && d.headline == DEGRADED_HEADLINE);
        assert!(d.reasons[0].starts_with("Familles contradictoires : 3 favorables"), "{:?}", d.reasons);
        assert!(d.reasons[1].starts_with("Technique favorable (+50) mais fondamentaux défavorables (−50)"), "{:?}", d.reasons);
        let calm: Vec<Family> = fams.iter().take(3).cloned().collect();
        let d = degraded(&calm, &composite(&calm, None, None), None, None);
        assert!(!d.active && d.headline.is_empty() && d.reasons.is_empty());
        let d = degraded(&calm, &composite(&calm, None, None), Some("Données peu fiables".into()), None);
        assert!(d.active && d.headline == DEGRADED_DATA_HEADLINE, "the headline names the actual cause");
        let d = degraded(&calm, &composite(&calm, None, None), None, Some("Le signal a perdu de l'argent".into()));
        assert!(d.active && d.headline == DEGRADED_RECORD_HEADLINE);
    }

    fn report(score: f64, vix: f64) -> MacroReport {
        MacroReport {
            score,
            level: if score >= 50.0 {
                MacroLevel::High
            } else if score >= 25.0 {
                MacroLevel::Tense
            } else {
                MacroLevel::Calm
            },
            market_score: score,
            factors: vec![],
            themes: vec![],
            values: MacroValues { vix: Some(MacroValue { value: vix, change5d: 0.0 }), ..Default::default() },
            as_of: None,
        }
    }

    #[test]
    fn regime_rules() {
        let up: Vec<f64> = (0..250).map(|i| 100.0 + i as f64).collect();
        let down: Vec<f64> = up.iter().rev().copied().collect();
        let k = |r: Option<&MacroReport>, c: &[f64]| market_regime(r, "S&P 500", c).map(|x| x.kind);
        assert_eq!(k(Some(&report(10.0, 15.0)), &up), Some(RegimeKind::RiskOn));
        assert_eq!(k(Some(&report(10.0, 15.0)), &down), Some(RegimeKind::Neutral));
        assert_eq!(k(Some(&report(30.0, 22.0)), &down), Some(RegimeKind::RiskOff));
        assert_eq!(k(Some(&report(30.0, 22.0)), &up), Some(RegimeKind::Neutral));
        assert_eq!(k(Some(&report(55.0, 20.0)), &up), Some(RegimeKind::RiskOff));
        assert_eq!(k(Some(&report(10.0, 31.0)), &up), Some(RegimeKind::RiskOff));
        assert_eq!(k(None, &up), Some(RegimeKind::Neutral));
        assert_eq!(k(None, &down), Some(RegimeKind::RiskOff));
        assert_eq!(k(None, &up[..100]), None);
        let r = market_regime(Some(&report(10.0, 15.0)), "S&P 500", &up).unwrap();
        assert_eq!(r.label, "Risk-on");
        assert_eq!(r.reasons[0], "Stress macro 10/100 (calme)");
        assert!(r.reasons[2].starts_with("S&P 500 au-dessus de sa moyenne 200 jours"), "{:?}", r.reasons);
    }

    #[test]
    fn horizon_rules() {
        let h = |c, d: f64| horizon(c, 100.0, 100.0 + d, 1.0).map(|x| x.kind);
        assert_eq!(h(PlanCandles::H1, 2.0), Some(HorizonKind::Scalping));
        assert_eq!(h(PlanCandles::H1, 5.0), Some(HorizonKind::DayTrading));
        assert_eq!(h(PlanCandles::H4, 3.0), Some(HorizonKind::DayTrading));
        assert_eq!(h(PlanCandles::H4, 6.0), Some(HorizonKind::Swing));
        assert_eq!(h(PlanCandles::D1, 4.0), Some(HorizonKind::Swing));
        assert_eq!(h(PlanCandles::D1, 8.0), Some(HorizonKind::MediumTerm));
        assert_eq!(h(PlanCandles::D1, 13.0), Some(HorizonKind::LongTerm));
        assert_eq!(h(PlanCandles::W1, 3.0), Some(HorizonKind::MediumTerm));
        assert_eq!(h(PlanCandles::W1, 5.0), Some(HorizonKind::LongTerm));
        assert_eq!(h(PlanCandles::D1, 0.0), None);
        let x = horizon(PlanCandles::D1, 100.0, 108.4, 2.0).unwrap();
        assert_eq!(x.atr_distance, 4.2);
        assert_eq!(x.detail, "Plan sur bougies journalières, objectif 1 à 4,2 ATR de l'entrée : moyen terme (quelques semaines à quelques mois)");
    }
}

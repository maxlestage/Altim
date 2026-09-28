//! Candle-series quality check and reliability score (quality capped by the number of independent sources).
use serde::{Deserialize, Serialize};

use super::signal::{Action, Candle, Signal, sanitize};
use crate::js::round;
pub use crate::types::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReliabilityLevel {
    High,
    Medium,
    Low,
}

impl ReliabilityLevel {
    pub fn label(self) -> &'static str {
        match self {
            ReliabilityLevel::High => "Fiabilité élevée",
            ReliabilityLevel::Medium => "Fiabilité moyenne",
            ReliabilityLevel::Low => "Fiabilité faible",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub score: f64,
    pub issues: Vec<String>,
    pub gaps: u32,
    pub bad_ticks: u32,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reliability {
    pub score: f64,
    pub level: ReliabilityLevel,
    pub independent: usize,
    pub conflict: bool,
}

fn median0(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { crate::js::median(v) }
}

pub fn assess_quality(raw: &[Candle], interval_ms: i64, kind: Kind, now: i64) -> QualityReport {
    let candles = sanitize(raw);
    let mut score = 100.0;
    let mut issues: Vec<String> = Vec::new();
    let invalid = raw.len() - candles.len();
    if invalid > 0 {
        score -= (invalid as f64 * 2.0).min(20.0);
        issues.push(format!("{invalid} bougie(s) incohérente(s) écartée(s)."));
    }
    if candles.len() < 2 {
        issues.push("Aucune donnée exploitable.".into());
        return QualityReport { score: 0.0, issues, gaps: 0, bad_ticks: 0, stale: true };
    }
    if candles.len() < 60 {
        score -= 40.0;
        issues.push(format!("Historique trop court ({} bougies).", candles.len()));
    }
    let recent = &candles[candles.len().saturating_sub(300)..];
    let iv = interval_ms as f64;
    let mut gaps = 0u32;
    for w in recent.windows(2) {
        let delta = (w[1].time - w[0].time) as f64;
        let too_far = match kind {
            Kind::Crypto => delta > iv * 1.5,
            Kind::Stock => delta > (iv * 1.5).max(4.5 * 86_400_000.0),
        };
        if too_far {
            gaps += 1;
        }
    }
    if gaps > 0 {
        score -= (gaps as f64 * 5.0).min(30.0);
        issues.push(format!("{gaps} trou(s) dans l'historique."));
    }
    let returns: Vec<f64> = recent.windows(2).map(|w| (w[1].close / w[0].close).ln()).collect();
    let mut bad_ticks = 0u32;
    if returns.len() > 20 {
        let med = median0(&returns);
        let dev: Vec<f64> = returns.iter().map(|r| (r - med).abs()).collect();
        let mad = median0(&dev) * 1.4826;
        if mad > 0.0 {
            for i in 0..returns.len() - 1 {
                let r = returns[i];
                let next = returns[i + 1];
                if (r - med).abs() > 12.0 * mad && r.abs() > 0.03 && next * r < 0.0 && next.abs() > r.abs() * 0.6 {
                    bad_ticks += 1;
                }
            }
        }
    }
    if bad_ticks > 0 {
        score -= (bad_ticks as f64 * 10.0).min(30.0);
        issues.push(format!("{bad_ticks} pic(s) de prix aberrant(s) aussitôt annulé(s)."));
    }
    let last = candles[candles.len() - 1].time;
    let allowed = match kind {
        Kind::Crypto => interval_ms * 3,
        Kind::Stock => (interval_ms * 3).max(4 * 86_400_000),
    };
    let stale = now - last > allowed + interval_ms;
    if stale {
        score -= 40.0;
        issues.push("Données périmées.".into());
    }
    let with_volume = candles.iter().filter(|c| c.volume > 0.0).count();
    if with_volume > 0 && (candles.len() - with_volume) as f64 / candles.len() as f64 > 0.2 {
        score -= 10.0;
        issues.push("Volume absent sur une partie de l'historique.".into());
    }
    if recent.iter().filter(|c| c.high == c.low).count() as f64 / recent.len() as f64 > 0.3 {
        score -= 15.0;
        issues.push("Marché très peu liquide (prix figés).".into());
    }
    QualityReport { score: score.max(0.0), issues, gaps, bad_ticks, stale }
}

/// Score cap by number of independent agreeing sources (index = sources, 5 and more = 100).
pub const RELIABILITY_CAP: [f64; 6] = [40.0, 40.0, 60.0, 75.0, 90.0, 100.0];

pub fn reliability(quality: f64, independent_sources: usize, conflict: bool) -> Reliability {
    // A decision needs several independent confirmations: 1 source = no advice, 3 minimum for "high".
    let cap = RELIABILITY_CAP[independent_sources.min(5)];
    let score = if conflict { quality.min(30.0) } else { quality.min(cap) };
    let level = if score >= 75.0 {
        ReliabilityLevel::High
    } else if score >= 50.0 {
        ReliabilityLevel::Medium
    } else {
        ReliabilityLevel::Low
    };
    Reliability { score, level, independent: independent_sources, conflict }
}

/// Never a BUY/SELL on doubtful data.
pub fn gate(signal: &Signal, rel: &Reliability, quality_issues: &[String]) -> Signal {
    let mut action = signal.action;
    let mut warnings = signal.warnings.clone();
    match rel.level {
        ReliabilityLevel::Low => {
            if action != Action::Hold {
                action = Action::Hold;
                warnings.insert(0, format!("Données insuffisamment fiables ({}/100) : signal suspendu.", round(rel.score)));
            }
        }
        ReliabilityLevel::Medium => {
            if action == Action::StrongBuy {
                action = Action::Buy;
            }
            if action == Action::StrongSell {
                action = Action::Sell;
            }
            warnings.push(format!("Fiabilité moyenne des données ({} source(s) indépendante(s)).", rel.independent));
        }
        ReliabilityLevel::High => {}
    }
    warnings.extend(quality_issues.iter().cloned());
    Signal { action, warnings, confidence: signal.confidence * rel.score / 100.0, ..signal.clone() }
}

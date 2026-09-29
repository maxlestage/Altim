//! "Can I buy now?" — one rule shared by the notifications of the iPhone, the Apple Watch and Android
//! (`web/src/engine/alerts.ts`).
//!
//! An asset is buyable when at least one measured reason says so and nothing blocks it:
//! - reasons: the 4 h signal is ACHAT / ACHAT FORT, or the price is inside a Fibonacci buy zone (or its golden pocket);
//! - blockers: data sources disagree (low reliability), the guard sees a shock, or the move that drew the zone has
//!   been broken (price under its low).
//!
//! Cautions (downtrend, very tense macro context) do not block, they are written in the notification.
use serde::{Deserialize, Serialize};

use super::fibonacci::{Band, FibZone, Horizon, ZoneStatus};
use super::format::format_price;
use super::reliability::ReliabilityLevel;
use super::signal::Action;
use crate::js::{number_to_string, round};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AlertSignal {
    pub action: Action,
    pub confidence: f64,
}

/// The fields of a `FibZone` the rule reads (`Pick<FibZone, "horizon" | "label" | "status" | "zone" | "golden" |
/// "invalidation">`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertZone {
    pub horizon: Horizon,
    pub label: String,
    pub status: ZoneStatus,
    pub zone: Option<Band>,
    pub golden: Option<Band>,
    pub invalidation: Option<f64>,
}

impl From<&FibZone> for AlertZone {
    fn from(z: &FibZone) -> Self {
        AlertZone { horizon: z.horizon, label: z.label.clone(), status: z.status, zone: z.zone, golden: z.golden, invalidation: z.invalidation }
    }
}

/// Shock level of the guard ("calm" | "agitated" | "shock").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertShock {
    Calm,
    Agitated,
    Shock,
}

/// Underlying trend of the guard ("up" | "down" | "range").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertTrend {
    Up,
    Down,
    Range,
}

/// Macro level ("calm" | "tense" | "high").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertMacro {
    Calm,
    Tense,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertInput {
    pub symbol: String,
    pub name: String,
    pub price: Option<f64>,
    pub signal: Option<AlertSignal>,
    pub reliability: Option<ReliabilityLevel>,
    pub zones: Vec<AlertZone>,
    pub shock: Option<AlertShock>,
    pub trend: Option<AlertTrend>,
    #[serde(rename = "macro")]
    pub macro_level: Option<AlertMacro>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuyAlert {
    pub buy: bool,
    /// Both a buy signal and the price in a zone.
    pub strong: bool,
    pub reasons: Vec<String>,
    pub blockers: Vec<String>,
    pub cautions: Vec<String>,
    /// Stable identifier of the situation: a new notification only when it changes (e.g. "signal+zone:medium").
    pub key: String,
    pub title: String,
    pub body: String,
}

fn usd(v: f64) -> String {
    format!("{} $", format_price(v))
}

fn order(h: Horizon) -> usize {
    match h {
        Horizon::Long => 0,
        Horizon::Medium => 1,
        Horizon::Short => 2,
    }
}

pub fn buy_alert(a: &AlertInput) -> BuyAlert {
    let mut reasons: Vec<String> = Vec::new();
    let mut blockers: Vec<String> = Vec::new();
    let mut cautions: Vec<String> = Vec::new();
    let mut parts: Vec<String> = Vec::new();

    let signal_buy = a.signal.is_some_and(|s| matches!(s.action, Action::Buy | Action::StrongBuy));
    if signal_buy {
        let s = a.signal.unwrap();
        reasons.push(format!(
            "Signal {} en 4 h (confiance {} %).",
            if s.action == Action::StrongBuy { "ACHAT FORT" } else { "ACHAT" },
            number_to_string(round(s.confidence))
        ));
        parts.push("signal".into());
    }
    // Longest horizon first: a long-term zone matters more than a short one.
    let mut sorted: Vec<&AlertZone> = a.zones.iter().collect();
    sorted.sort_by_key(|z| order(z.horizon));
    let hit = sorted.into_iter().find(|z| matches!(z.status, ZoneStatus::InZone | ZoneStatus::Golden) && z.zone.is_some());
    if let Some(hit) = hit {
        let band = match (hit.status, hit.golden) {
            (ZoneStatus::Golden, Some(g)) => g,
            _ => hit.zone.unwrap(),
        };
        reasons.push(format!(
            "Prix dans la {} {} ({} – {}).",
            if hit.status == ZoneStatus::Golden { "zone d'or" } else { "zone d'achat" },
            hit.label.to_lowercase(),
            usd(band.to),
            usd(band.from)
        ));
        parts.push(format!("zone:{}", hit.horizon.as_str()));
    }

    if a.reliability == Some(ReliabilityLevel::Low) {
        blockers.push("Sources de prix en désaccord : données peu fiables.".into());
    }
    if a.shock == Some(AlertShock::Shock) {
        blockers.push("Garde-fou : risque de choc élevé, mieux vaut attendre.".into());
    }
    if let (Some(hit), Some(price)) = (hit, a.price) {
        if hit.invalidation.is_some_and(|inv| price < inv) {
            blockers.push("Le plus bas du mouvement est cassé : zone invalidée.".into());
        }
    }

    if a.trend == Some(AlertTrend::Down) {
        cautions.push("Tendance de fond baissière : taille réduite.".into());
    }
    if a.macro_level == Some(AlertMacro::High) {
        cautions.push("Contexte macro très tendu : taille réduite.".into());
    }
    if a.shock == Some(AlertShock::Agitated) {
        cautions.push("Marché agité : stop plus large.".into());
    }

    let buy = !reasons.is_empty() && blockers.is_empty();
    let strong = buy && signal_buy && hit.is_some();
    let stop = match hit.and_then(|h| h.invalidation) {
        Some(inv) => format!(" Invalidé sous {}.", usd(inv)),
        None => String::new(),
    };
    let price = a.price.map(|p| format!(" à {}", usd(p))).unwrap_or_default();
    let title = if buy {
        format!("{} : achat {}{}", a.symbol, if strong { "conseillé" } else { "possible" }, price)
    } else {
        format!("{} : pas d'achat pour l'instant", a.symbol)
    };
    let body = if buy {
        let all: Vec<&str> = reasons.iter().chain(cautions.iter()).map(|s| s.as_str()).collect();
        all.join(" ") + &stop
    } else {
        blockers.first().cloned().unwrap_or_else(|| "Ni signal d'achat ni prix dans une zone d'achat.".into())
    };
    BuyAlert { buy, strong, key: if buy { parts.join("+") } else { String::new() }, reasons, blockers, cautions, title, body }
}

/// What the full decision (`/api/decision`) says of the asset, to keep the notifications and the brief consistent with
/// the decision card: its verdict, label ("ATTENDRE") and first reason to wait (or its headline).
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionGate {
    pub verdict: super::decision_types::Verdict,
    pub label: String,
    pub reason: String,
}

/// The alert once checked against the full decision. The quick rule above (4 h signal, buy zone, guard) only
/// proposes; a buy is announced only when the decision itself is ACHETER or ZONE D'ACHAT — otherwise the decision's
/// own reason becomes the first blocker. Without a decision (not computed in time), no buy is announced either.
pub fn with_decision(mut alert: BuyAlert, symbol: &str, gate: Option<&DecisionGate>) -> BuyAlert {
    use super::decision_types::Verdict;
    let blocker = match gate {
        Some(g) if matches!(g.verdict, Verdict::Buy | Verdict::BuyZone) => {
            // The badge and the title follow the decision: ACHETER (conseillé) or ZONE D'ACHAT (possible).
            let strong = g.verdict == Verdict::Buy;
            let from = if alert.strong { "achat conseillé" } else { "achat possible" };
            alert.title = alert.title.replacen(from, if strong { "achat conseillé" } else { "achat possible" }, 1);
            alert.strong = strong;
            alert.key = format!("{}:{}", alert.key, if strong { "acheter" } else { "zone" });
            alert.reasons.push(format!("Décision complète : {}.", g.label));
            return alert;
        }
        Some(g) => format!("La décision complète dit {} : {}.", g.label, g.reason.trim().trim_end_matches('.')),
        // Never announce a buy the full decision has not confirmed: the next pass (decision warmed by then) decides.
        None => "Décision complète pas encore calculée : aucun achat n'est annoncé tant qu'elle ne l'a pas confirmé.".into(),
    };
    alert.blockers.insert(0, blocker.clone());
    alert.buy = false;
    alert.strong = false;
    alert.key = String::new();
    alert.title = format!("{symbol} : pas d'achat pour l'instant");
    alert.body = blocker;
    alert
}

#[cfg(test)]
mod decision_gate_tests {
    use super::*;
    use crate::engine::decision_types::Verdict;

    fn buy() -> BuyAlert {
        BuyAlert {
            buy: true,
            strong: true,
            reasons: vec!["Signal ACHAT en 4 h (confiance 70 %).".into()],
            blockers: vec![],
            cautions: vec![],
            key: "signal+zone:medium".into(),
            title: "BTC : achat conseillé à 83 000 $".into(),
            body: "…".into(),
        }
    }

    #[test]
    fn a_wait_decision_cancels_the_buy() {
        let g = DecisionGate { verdict: Verdict::Wait, label: "ATTENDRE".into(), reason: "Prix au-dessus de la zone d'achat".into() };
        let a = with_decision(buy(), "BTC", Some(&g));
        assert!(!a.buy && !a.strong && a.key.is_empty());
        assert_eq!(a.title, "BTC : pas d'achat pour l'instant");
        assert_eq!(a.body, "La décision complète dit ATTENDRE : Prix au-dessus de la zone d'achat.");
        assert_eq!(a.blockers[0], a.body);
    }

    #[test]
    fn a_buy_decision_confirms_and_a_missing_one_blocks() {
        let g = DecisionGate { verdict: Verdict::BuyZone, label: "ZONE D'ACHAT".into(), reason: String::new() };
        let a = with_decision(buy(), "BTC", Some(&g));
        assert!(a.buy && a.reasons.last().unwrap() == "Décision complète : ZONE D'ACHAT.");
        assert!(!a.strong && a.title == "BTC : achat possible à 83 000 $" && a.key == "signal+zone:medium:zone");
        let g = DecisionGate { verdict: Verdict::Buy, label: "ACHETER".into(), reason: String::new() };
        let a = with_decision(BuyAlert { strong: false, title: "BTC : achat possible à 83 000 $".into(), ..buy() }, "BTC", Some(&g));
        assert!(a.strong && a.title == "BTC : achat conseillé à 83 000 $");
        let a = with_decision(buy(), "BTC", None);
        assert!(!a.buy && a.blockers[0].starts_with("Décision complète pas encore calculée"));
    }
}

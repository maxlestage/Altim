//! French formats and labels of the decision card (decision.ts, DecisionCard.tsx): fr-FR numbers with narrow
//! no-break spaces, amounts in the display currency (`MoneyDisplay`), dates in Paris or New York time, and the
//! labels that never leave colour alone (an icon and a word each time).
use chrono::{Datelike, TimeZone, Timelike};
use chrono_tz::Tz;

use crate::engine::decision_types::{ExitKind, Family, Level, Plan, Rating, ScenarioKind, Status, StepState, Uncertainty, Verdict, Veto};
use crate::engine::structure::Bias;
use crate::engine::synthesis::RegimeKind;
use crate::js::{fr, round};
use crate::web::money::MoneyDisplay;

/// Narrow no-break space (fr-FR before "%" and the currency symbol of the card).
pub const NNBSP: &str = "\u{202f}";

/// A dollar price in the display currency: no decimals when whole, else 2 (4 or 8 below 1).
pub fn usd(v: Option<f64>, m: &MoneyDisplay) -> String {
    let Some(v) = v.filter(|v| v.is_finite()) else { return "—".into() };
    let x = m.to_display(v);
    let a = x.abs();
    let digits = if a >= 1.0 {
        if x.fract() == 0.0 { 0 } else { 2 }
    } else if a >= 0.01 {
        4
    } else {
        8
    };
    format!("{}{NNBSP}{}", fr(x, digits, digits), m.symbol())
}

/// Large dollar amounts in the display currency: 421 Md€, 3,16 Md€, 850 M€.
pub fn usd_compact(v: Option<f64>, m: &MoneyDisplay) -> String {
    match v.filter(|v| v.is_finite()) {
        None => "—".into(),
        Some(v) => m.money_compact(v, NNBSP),
    }
}

/// Percentage; `sign` adds + for positive values (− is the typographic minus).
pub fn pct(v: Option<f64>, digits: usize, sign: bool) -> String {
    let Some(v) = v.filter(|v| v.is_finite()) else { return "—".into() };
    let s = if v < 0.0 {
        "−"
    } else if sign && v > 0.0 {
        "+"
    } else {
        ""
    };
    format!("{s}{}{NNBSP}%", fr(v.abs(), 0, digits))
}

/// Plain number (ratios, counts).
pub fn num(v: Option<f64>, digits: usize) -> String {
    let Some(v) = v.filter(|v| v.is_finite()) else { return "—".into() };
    format!("{}{}", if v < 0.0 { "−" } else { "" }, fr(v.abs(), 0, digits))
}

/// Large counts: 19,93 millions.
pub fn count(v: Option<f64>) -> String {
    let Some(v) = v.filter(|v| v.is_finite()) else { return "—".into() };
    if v >= 1e9 {
        format!("{} milliards", fr(v / 1e9, 0, 2))
    } else if v >= 1e6 {
        format!("{} millions", fr(v / 1e6, 0, 2))
    } else {
        fr(v, 0, 0)
    }
}

/// Hash rate (H/s) in EH/s.
pub fn hash_rate(v: Option<f64>) -> String {
    match v.filter(|v| v.is_finite()) {
        None => "—".into(),
        Some(v) => format!("{}{NNBSP}EH/s", fr(v / 1e18, 0, 0)),
    }
}

/// "+34", "−12", "0" (scores −100 … +100).
pub fn signed_score(v: f64) -> String {
    let s = if v > 0.0 {
        "+"
    } else if v < 0.0 {
        "−"
    } else {
        ""
    };
    format!("{s}{}", round(v).abs())
}

/// "0 %" with no decimal (`pct0` of model-bot.ts, used by the bot's line of the card).
pub fn pct0(v: Option<f64>) -> String {
    match v.filter(|v| v.is_finite()) {
        None => "—".into(),
        Some(v) => format!("{}{NNBSP}%", fr(v, 0, 0)),
    }
}

/// (label, tone) of the bot's action in a decision (`ACTION_UI` of model-bot.ts).
pub fn bot_action_ui(a: crate::engine::bot::BotAction) -> (&'static str, &'static str) {
    use crate::engine::bot::BotAction;
    match a {
        BotAction::Buy => ("ACHETER", "buy"),
        BotAction::Wait => ("ATTENDRE", "wait"),
        BotAction::Sell => ("VENDRE", "sell"),
    }
}

// ---------- Dates (`new Date(ms)` then fr-FR in a fixed time zone) ----------

const MONTHS_LONG: [&str; 12] =
    ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
const MONTHS_SHORT: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

fn at(ms: f64, tz: Tz) -> Option<chrono::DateTime<Tz>> {
    let ms = ms.trunc();
    if !ms.is_finite() {
        return None;
    }
    tz.timestamp_millis_opt(ms as i64).single()
}

fn long_in(ms: f64, tz: Tz) -> String {
    match at(ms, tz) {
        Some(d) => format!("{} {} {}", d.day(), MONTHS_LONG[d.month0() as usize], d.year()),
        None => "Invalid Date".into(),
    }
}

/// "28 septembre 2026" (Paris time).
pub fn long_date(ms: f64) -> String {
    long_in(ms, chrono_tz::Europe::Paris)
}

/// "28/09 à 14:02" (Paris time).
pub fn short_date_time(ms: f64) -> String {
    match at(ms, chrono_tz::Europe::Paris) {
        Some(d) => format!("{:02}/{:02} à {:02}:{:02}", d.day(), d.month(), d.hour(), d.minute()),
        None => "Invalid Date à Invalid Date".into(),
    }
}

/// Earnings and filing dates: New York calendar day (a timestamp at 00:00 UTC is a bare date, read as such).
pub fn ny_date(ms: f64) -> String {
    long_in(ms, if ms % 86_400_000.0 == 0.0 { chrono_tz::UTC } else { chrono_tz::America::New_York })
}

/// "3 janv. 2024" (UTC): dates of the strategy comparator.
pub fn short_date_utc(ms: f64) -> String {
    match at(ms, chrono_tz::UTC) {
        Some(d) => format!("{} {} {}", d.day(), MONTHS_SHORT[d.month0() as usize], d.year()),
        None => "Invalid Date".into(),
    }
}

// ---------- Labels (colour never alone: an icon and a word each time) ----------

/// (icon, label) of a level.
pub fn level_ui(l: Level) -> (&'static str, &'static str) {
    match l {
        Level::Strong => ("🟢", "Signal fort"),
        Level::Moderate => ("🟡", "Signal modéré"),
        Level::Waiting => ("⚪", "Attente"),
        Level::HighRisk => ("🟠", "Risque élevé"),
        Level::Exit => ("🔴", "Sortie / risque d'invalidation"),
    }
}

/// CSS name of a level ("lv-highRisk", `data-tone`).
pub fn level_key(l: Level) -> &'static str {
    match l {
        Level::Strong => "strong",
        Level::Moderate => "moderate",
        Level::Waiting => "waiting",
        Level::HighRisk => "highRisk",
        Level::Exit => "exit",
    }
}

/// 6-level rating: (icon, label, tone = the matching level).
pub fn rating_ui(r: Rating) -> (&'static str, &'static str, Level) {
    match r {
        Rating::StrongBuy => ("🟢", "ACHAT FORT", Level::Strong),
        Rating::Buy => ("🟢", "ACHAT", Level::Strong),
        Rating::Hold => ("⚪", "ATTENDRE", Level::Waiting),
        Rating::Reduce => ("🟠", "ALLÉGER", Level::HighRisk),
        Rating::Sell => ("🔴", "VENDRE", Level::Exit),
        Rating::StrongSell => ("🔴", "VENTE FORTE", Level::Exit),
    }
}

pub fn rating_key(r: Rating) -> &'static str {
    match r {
        Rating::StrongBuy => "strongBuy",
        Rating::Buy => "buy",
        Rating::Hold => "hold",
        Rating::Reduce => "reduce",
        Rating::Sell => "sell",
        Rating::StrongSell => "strongSell",
    }
}

pub fn verdict_key(v: Verdict) -> &'static str {
    match v {
        Verdict::Buy => "buy",
        Verdict::BuyZone => "buyZone",
        Verdict::Wait => "wait",
        Verdict::NoPosition => "noPosition",
        Verdict::Trim => "trim",
        Verdict::Sell => "sell",
    }
}

pub fn bias_ui(b: Bias) -> (&'static str, &'static str) {
    match b {
        Bias::Bullish => ("↗", "haussier"),
        Bias::Neutral => ("→", "neutre"),
        Bias::Bearish => ("↘", "baissier"),
    }
}

pub fn bias_key(b: Bias) -> &'static str {
    match b {
        Bias::Bullish => "bullish",
        Bias::Neutral => "neutral",
        Bias::Bearish => "bearish",
    }
}

pub fn regime_ui(r: RegimeKind) -> (&'static str, &'static str) {
    match r {
        RegimeKind::RiskOn => ("🟢", "Risk-on (appétit pour le risque)"),
        RegimeKind::Neutral => ("⚪", "Neutre"),
        RegimeKind::RiskOff => ("🔴", "Risk-off (aversion au risque)"),
    }
}

pub fn step_ui(s: StepState) -> (&'static str, &'static str) {
    match s {
        StepState::Ok => ("✓", "fait"),
        StepState::No => ("✕", "pas encore"),
        StepState::Unknown => ("?", "inconnu"),
    }
}

pub fn step_key(s: StepState) -> &'static str {
    match s {
        StepState::Ok => "ok",
        StepState::No => "no",
        StepState::Unknown => "unknown",
    }
}

pub fn scenario_ui(s: ScenarioKind) -> (&'static str, &'static str) {
    match s {
        ScenarioKind::Bull => ("↗", "Haussier"),
        ScenarioKind::Neutral => ("→", "Neutre"),
        ScenarioKind::Bear => ("↘", "Baissier"),
    }
}

pub fn scenario_key(s: ScenarioKind) -> &'static str {
    match s {
        ScenarioKind::Bull => "bull",
        ScenarioKind::Neutral => "neutral",
        ScenarioKind::Bear => "bear",
    }
}

pub fn uncertainty_label(u: Uncertainty) -> &'static str {
    match u {
        Uncertainty::Low => "faible",
        Uncertainty::Medium => "moyenne",
        Uncertainty::High => "élevée",
    }
}

pub fn exit_kind_label(k: ExitKind) -> &'static str {
    match k {
        ExitKind::Profit => "Prise de bénéfices",
        ExitKind::Defensive => "Défensive",
        ExitKind::Macro => "Choc de marché",
    }
}

pub fn exit_kind_key(k: ExitKind) -> &'static str {
    match k {
        ExitKind::Profit => "profit",
        ExitKind::Defensive => "defensive",
        ExitKind::Macro => "macro",
    }
}

/// Tone, icon and word of a family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tone {
    /// "strong" | "moderate" | "highRisk" | "exit" | "na"
    pub tone: &'static str,
    pub icon: &'static str,
    pub text: &'static str,
}

/// Family status → tone, icon and word. A negative score beyond −50 is "très défavorable".
pub fn family_tone(status: Status, score: Option<f64>) -> Tone {
    match status {
        Status::Positive => Tone { tone: "strong", icon: "🟢", text: "favorable" },
        Status::Neutral => Tone { tone: "moderate", icon: "🟡", text: "neutre" },
        Status::Negative if score.is_some_and(|s| s <= -50.0) => Tone { tone: "exit", icon: "🔴", text: "très défavorable" },
        Status::Negative => Tone { tone: "highRisk", icon: "🟠", text: "défavorable" },
        Status::Unavailable => Tone { tone: "na", icon: "⚪", text: "non disponible" },
    }
}

/// The summary row: Tendance, Momentum, Volume, Fondamentaux, Macro, Risque, Valorisation (when present):
/// (key, name, family).
pub fn summary_families(families: &[Family]) -> Vec<(&'static str, &'static str, &Family)> {
    const KEYS: [(&str, &str); 7] = [
        ("trend", "Tendance"),
        ("momentum", "Momentum"),
        ("volume", "Volume"),
        ("fundamentals", "Fondamentaux"),
        ("macro", "Macro"),
        ("volatility", "Risque"),
        ("valuation", "Valorisation"),
    ];
    KEYS.iter().filter_map(|(key, name)| families.iter().find(|f| f.key == *key).map(|f| (*key, *name, f))).collect()
}

/// Active vetoes first, then the checks that pass, then those that cannot be verified.
pub fn sort_vetoes(v: &[Veto]) -> Vec<&Veto> {
    let rank = |x: &Veto| {
        if x.active {
            0
        } else if x.verifiable {
            1
        } else {
            2
        }
    };
    let mut out: Vec<&Veto> = v.iter().collect();
    out.sort_by_key(|x| rank(x));
    out
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// `x.charAt(0).toLowerCase() + x.slice(1)`.
pub fn uncapitalize(s: &str) -> String {
    lower_first(s)
}

/// "Vendre 20 % si objectif 1 atteint (338 $)"
pub fn exit_text(share: f64, trigger: &str) -> String {
    format!("Vendre {}{NNBSP}% si {}", num(Some(share), 0), lower_first(trigger))
}

/// "1,2 (minimum 2)"
pub fn risk_reward_text(p: &Plan) -> String {
    format!("{} (minimum {})", num(Some(p.risk_reward), 1), num(Some(p.min_risk_reward), 1))
}

pub fn mode_text(mode: &str) -> &'static str {
    if mode == "personal" {
        "Mode personnel : calculé avec votre prix d'achat et vos pondérations, jamais conservés"
    } else {
        "Mode informationnel : données de marché uniquement, sans votre position"
    }
}

/// Rank of a rating for sorting (buy side first); unknown last (9).
pub fn rating_rank(r: Option<Rating>) -> u8 {
    match r {
        Some(Rating::StrongBuy) => 0,
        Some(Rating::Buy) => 1,
        Some(Rating::Hold) => 2,
        Some(Rating::Reduce) => 3,
        Some(Rating::Sell) => 4,
        Some(Rating::StrongSell) => 5,
        None => 9,
    }
}

/// Colour class of a rating on the Radar (the badge and the card's border): "buy" | "sell" | "hold".
pub fn rating_tone(r: Option<Rating>, verdict: Verdict) -> &'static str {
    match r {
        Some(Rating::StrongBuy | Rating::Buy) => "buy",
        Some(Rating::Sell | Rating::StrongSell | Rating::Reduce) => "sell",
        Some(Rating::Hold) => "hold",
        None if matches!(verdict, Verdict::Buy | Verdict::BuyZone) => "buy",
        None if matches!(verdict, Verdict::Sell | Verdict::Trim) => "sell",
        None => "hold",
    }
}

#[cfg(test)]
mod tests {
    use super::super::doc::tests::{aapl, btc};
    use super::*;

    const N: &str = NNBSP;

    // decision.test.ts "French formatting"
    #[test]
    fn prices_amounts_percentages() {
        let m = MoneyDisplay::usd();
        assert_eq!(usd(Some(80100.0), &m), format!("80{N}100{N}$"));
        assert_eq!(usd(Some(340.99), &m), format!("340,99{N}$"));
        assert_eq!(usd(None, &m), "—");
        assert_eq!(usd_compact(Some(421e9), &m), format!("421{N}Md$"));
        assert_eq!(usd_compact(Some(3.16e9), &m), format!("3,16{N}Md$"));
        assert_eq!(usd_compact(Some(1654e9), &m), format!("1{N}654{N}Md$"));
        assert_eq!(pct(Some(-2.6), 1, true), format!("−2,6{N}%"));
        assert_eq!(pct(Some(7.9), 1, true), format!("+7,9{N}%"));
        assert_eq!(pct(Some(0.0069), 4, true), format!("+0,0069{N}%"));
        assert_eq!(num(Some(1.2), 1), "1,2");
        assert_eq!(count(Some(19_930_000.0)), "19,93 millions");
        assert_eq!(hash_rate(Some(1.069e21)), format!("1{N}069{N}EH/s"));
        assert_eq!(long_date(crate::jstime::date_utc(2026.0, 8.0, 28.0, 12.0, 0.0)), "28 septembre 2026");
        assert_eq!(short_date_time(crate::jstime::date_utc(2026.0, 8.0, 28.0, 12.0, 2.0)), "28/09 à 14:02");
        assert_eq!(ny_date(crate::jstime::date_utc(2026.0, 9.0, 29.0, 0.0, 0.0)), "29 octobre 2026");
        assert_eq!(short_date_utc(crate::jstime::date_utc(2024.0, 0.0, 3.0, 0.0, 0.0)), "3 janv. 2024");
        assert_eq!(signed_score(34.4), "+34");
        assert_eq!(signed_score(-12.0), "−12");
    }

    #[test]
    fn decision_texts() {
        let d = aapl();
        let e = &d.d.position.as_ref().unwrap().exits[0];
        assert_eq!(exit_text(e.share, &e.trigger), format!("Vendre 20{N}% si objectif 1 atteint (338 $)"));
        assert_eq!(risk_reward_text(btc().d.plan.as_ref().unwrap()), "1,2 (minimum 2)");
    }

    #[test]
    fn levels_and_families() {
        assert_eq!(family_tone(Status::Negative, Some(-55.0)), Tone { tone: "exit", icon: "🔴", text: "très défavorable" });
        assert_eq!(family_tone(Status::Negative, Some(-20.0)).text, "défavorable");
        assert_eq!(family_tone(Status::Unavailable, None).text, "non disponible");
        let d = btc();
        let names: Vec<&str> = summary_families(&d.d.families).iter().map(|x| x.1).collect();
        assert_eq!(names, ["Tendance", "Momentum", "Volume", "Fondamentaux", "Macro", "Risque", "Valorisation"]);
        let v = sort_vetoes(&d.d.vetoes);
        assert_eq!(v[0].code, "riskReward");
        assert!(!v[v.len() - 1].verifiable);
    }

    #[test]
    fn radar_tones() {
        assert_eq!(rating_rank(Some(Rating::StrongBuy)), 0);
        assert_eq!(rating_rank(None), 9);
        assert_eq!(rating_tone(Some(Rating::Reduce), Verdict::Buy), "sell");
        assert_eq!(rating_tone(None, Verdict::BuyZone), "buy");
        assert_eq!(rating_tone(None, Verdict::Trim), "sell");
        assert_eq!(rating_tone(None, Verdict::Wait), "hold");
    }
}

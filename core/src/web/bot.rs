//! « Bot Altim » JSON contract (`web/src/webapp/model-bot.ts`), the part the presentation site reads: the report's
//! v3 headline (/api/bot) and today's views (/api/bot/views). Fields are optional or defaulted where the TypeScript
//! allows an older server; the Bot screen (phase 2, batch A) adds the rest ADDITIVELY here.
use serde::{Deserialize, Serialize};

use crate::js::fr;
use crate::types::Kind;

/// Narrow no-break space (fr-FR before "%").
pub const NNBSP: &str = "\u{202f}";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BotAction {
    Buy,
    Wait,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Insufficient,
    Edge,
    Negative,
    Unproven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Absolute,
    Peers,
    V2,
}

/// (label, tone) of an action (`ACTION_UI`).
pub fn action_ui(a: BotAction) -> (&'static str, &'static str) {
    match a {
        BotAction::Buy => ("ACHETER", "buy"),
        BotAction::Wait => ("ATTENDRE", "wait"),
        BotAction::Sell => ("VENDRE", "sell"),
    }
}

pub fn family_label(f: Family) -> &'static str {
    match f {
        Family::Absolute => "Hausse ou baisse de l'actif",
        Family::Peers => "Classement entre pairs",
        Family::V2 => "Sélection v2 (log-loss)",
    }
}

/// One view of /api/bot/views (the fields the site reads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BotView {
    pub symbol: String,
    pub kind: Kind,
    pub available: bool,
    #[serde(default)]
    pub action: Option<BotAction>,
    #[serde(default)]
    pub counts: bool,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BotViews {
    #[serde(default)]
    pub as_of: Option<f64>,
    #[serde(default)]
    pub views: Vec<BotView>,
}

pub fn bot_views_url(items: &[(String, Kind)]) -> String {
    let list: Vec<String> = items.iter().take(20).map(|(s, k)| format!("{s}:{}", k.as_str())).collect();
    format!("/api/bot/views?symbols={}", encode_uri_component(&list.join(",")))
}

/// `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// One side of a configuration (`SideStats`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SideStats {
    #[serde(default)]
    pub signals: u32,
    #[serde(default)]
    pub excess: Option<f64>,
    #[serde(default)]
    pub t: Option<f64>,
    #[serde(default)]
    pub verdict: Option<Verdict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigStats {
    pub buy: SideStats,
    pub sell: SideStats,
    #[serde(default)]
    pub buy_vs_mean: Option<SideStats>,
    #[serde(default)]
    pub sell_vs_mean: Option<SideStats>,
}

impl ConfigStats {
    pub fn side(&self, buy: bool) -> &SideStats {
        if buy { &self.buy } else { &self.sell }
    }
    fn vs_mean(&self, buy: bool) -> Option<&SideStats> {
        if buy { self.buy_vs_mean.as_ref() } else { self.sell_vs_mean.as_ref() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V3Config {
    pub id: String,
    pub family: Family,
    #[serde(default)]
    pub headline: bool,
    pub main: ConfigStats,
    pub forward: ConfigStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V3Horizon {
    pub horizon: u32,
    #[serde(default)]
    pub configs: Vec<V3Config>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V3Group {
    pub id: String,
    #[serde(default)]
    pub horizons: Vec<V3Horizon>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KCount {
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V3Report {
    pub prereg_date: String,
    pub k: KCount,
    pub t_required: f64,
    pub headline: String,
    #[serde(default)]
    pub forward_headline: String,
    #[serde(default)]
    pub groups: Vec<V3Group>,
}

/// /api/bot (the fields the site reads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotReport {
    pub as_of: f64,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub v3: Option<V3Report>,
}

/// "−1,2" (fr-FR, true minus sign), "—" when missing.
pub fn plain(v: Option<f64>, digits: usize) -> String {
    match v {
        Some(v) if v.is_finite() => format!("{}{}", if v < 0.0 { "−" } else { "" }, fr(v.abs(), 0, digits)),
        _ => "—".into(),
    }
}

/// "12/10/2026" from "2026-10-12".
pub fn fr_iso(s: &str) -> String {
    s.split('-').rev().collect::<Vec<_>>().join("/")
}

/// The 4 headline configurations of a group: (horizon, config).
pub fn headline_configs(g: &V3Group) -> Vec<(u32, &V3Config)> {
    g.horizons.iter().flat_map(|h| h.configs.iter().filter(|c| c.headline).map(move |c| (h.horizon, c))).collect()
}

/// The figures that judge a side: family B's control against the group's mean when measured, else the side itself.
pub fn judged(c: &ConfigStats, buy: bool) -> &SideStats {
    c.vs_mean(buy).unwrap_or(c.side(buy))
}

/// A side proven at the corrected threshold (family B: against the median and the mean).
pub fn proven(c: &ConfigStats, buy: bool) -> bool {
    c.side(buy).verdict == Some(Verdict::Edge) && judged(c, buy).verdict == Some(Verdict::Edge)
}

/// A side proven on the past and confirmed by the forward test (≥ 30 signals, excess ≥ 0 against each reference).
pub fn confirmed(c: &V3Config, buy: bool) -> bool {
    let refs = [Some(c.forward.side(buy)), c.forward.vs_mean(buy)];
    proven(&c.main, buy) && refs.iter().flatten().all(|s| s.signals >= 30 && s.excess.is_some_and(|e| e >= 0.0))
}

/// Signals judged so far in the forward test (headline configurations).
pub fn forward_signals(r: &V3Report) -> u32 {
    r.groups.iter().map(|g| headline_configs(g).iter().map(|(_, c)| c.forward.buy.signals + c.forward.sell.signals).sum::<u32>()).sum()
}

/// "non démontré (t = −1,2)" for a side at the corrected threshold (the presentation site's `verdictShort`).
pub fn verdict_short(cfg: &V3Config, buy: bool) -> String {
    let c = &cfg.main;
    let s = judged(c, buy);
    let t = match s.t {
        None => "t non calculable".to_string(),
        Some(t) => format!("t = {}", plain(Some(t), 1)),
    };
    if confirmed(cfg, buy) {
        return format!("avantage démontré et confirmé sur l'avenir ({t})");
    }
    if proven(c, buy) {
        return format!("avantage mesuré sur le passé ({t}) mais fragile : ne comptera qu'après 30 signaux sur l'avenir qui le confirment");
    }
    if c.side(buy).verdict == Some(Verdict::Edge) {
        return format!("non démontré face à la moyenne du groupe ({t})");
    }
    match s.verdict {
        Some(Verdict::Negative) => format!("moins bien que la référence ({t})"),
        Some(Verdict::Insufficient) => "trop peu de signaux".into(),
        _ => format!("non démontré ({t})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(signals: u32, excess: f64, t: f64, v: Verdict) -> SideStats {
        SideStats { signals, excess: Some(excess), t: Some(t), verdict: Some(v) }
    }

    #[test]
    fn verdicts() {
        let mut c = V3Config {
            id: "x".into(),
            family: Family::Absolute,
            headline: true,
            main: ConfigStats { buy: side(100, 0.5, 3.7, Verdict::Edge), sell: side(10, -0.2, -1.24, Verdict::Unproven), ..Default::default() },
            forward: ConfigStats { buy: side(12, 0.1, 0.4, Verdict::Insufficient), ..Default::default() },
        };
        assert!(verdict_short(&c, true).starts_with("avantage mesuré sur le passé (t = 3,7) mais fragile"));
        assert_eq!(verdict_short(&c, false), "non démontré (t = −1,2)");
        c.forward.buy.signals = 30;
        assert_eq!(verdict_short(&c, true), "avantage démontré et confirmé sur l'avenir (t = 3,7)");
        c.main.buy_vs_mean = Some(side(100, 0.1, 1.0, Verdict::Unproven));
        assert_eq!(verdict_short(&c, true), "non démontré face à la moyenne du groupe (t = 1)");
        assert_eq!(fr_iso("2026-09-30"), "30/09/2026");
        assert_eq!(plain(None, 1), "—");
    }

    #[test]
    fn parses_partial_report_and_views() {
        let r: BotReport = serde_json::from_str(r#"{"asOf":1,"headline":"h","v3":{"preregDate":"2026-09-30","k":{"total":9,"v1":1},"tRequired":3.52,"headline":"H","forwardHeadline":"F","groups":[{"id":"stock","horizons":[{"horizon":20,"configs":[{"id":"a","family":"peers","headline":true,"main":{"buy":{"signals":1},"sell":{"signals":2}},"forward":{"buy":{"signals":3},"sell":{"signals":4}}}]}]}]}}"#).unwrap();
        assert_eq!(forward_signals(r.v3.as_ref().unwrap()), 7);
        let v: BotViews = serde_json::from_str(
            r#"{"asOf":null,"views":[{"symbol":"BTC","kind":"crypto","available":true,"action":"buy","counts":false,"text":""}]}"#,
        )
        .unwrap();
        assert_eq!(v.views[0].action, Some(BotAction::Buy));
        assert_eq!(
            bot_views_url(&[("BTC".into(), Kind::Crypto), ("AAPL".into(), Kind::Stock)]),
            "/api/bot/views?symbols=BTC%3Acrypto%2CAAPL%3Astock"
        );
    }
}

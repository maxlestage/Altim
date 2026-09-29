//! « Preuve du modèle » inside each decision (`modelEvidence` of `/api/decision`): what the cross-asset validation
//! (`validation.rs`, `/api/validation`) says about the asset's class and about the market regime the asset is in
//! now. Pure: the route passes an already-computed report (never computed for a decision) and the asset's candles.
//!
//! Effect on the decision (the verdict never changes; validation never raises anything):
//! - "weak" evidence (class verdict "negative", or the signal beat buy-and-hold on less than a third of the class's
//!   assets): confidence capped at `EVIDENCE_CAP` and a con "Le signal n'a pas démontré d'avantage…";
//! - class verdict "insufficient" or "unproven" (and not weak): the same con, no cap;
//! - class verdict "edge" and not weak: nothing;
//! - a strong rating (ACHAT FORT / VENTE FORTE) is downgraded to ACHAT / VENDRE unless the evidence is "edge" and
//!   not weak, with the reason in `ratingReason`;
//! - no cached report, or no tested asset of the class: `available: false`, nothing changes.
use serde::{Deserialize, Serialize};

use super::backtest::{Regime, regime_at};
use super::signal::sanitize;
use super::synthesis::Rating;
use super::validation::{AssetClass, GroupStat, ValidationReport, Verdict};
use crate::js::fr;
use crate::types::{Candle, DAY_MS, Kind};

/// Confidence ceiling when the class's evidence is weak (0-100).
pub const EVIDENCE_CAP: f64 = 60.0;
/// Where the web app shows the validation report.
pub const LINK: &str = "/app/validation";
pub const NOT_COMPUTED: &str = "Validation pas encore calculée : ouvrez l'écran « Validation du modèle » pour la lancer (quelques minutes).";

/// What the cross-asset validation says about this asset's class and current regime. Every figure is copied from
/// the cached report (`asOf`); the regime is computed here from the asset's closed daily candles with the
/// validation's rule (`backtest::regime_at`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelEvidence {
    /// False when no validation report is cached yet, or none of the class's assets could be tested.
    pub available: bool,
    pub asset_class: AssetClass,
    pub class_label: String,
    /// Verdict of the class's pooled trades ("insufficient" | "edge" | "negative" | "unproven"); None when unavailable.
    pub class_verdict: Option<Verdict>,
    pub class_verdict_label: Option<String>,
    /// Assets of the class tested, and among them those where the signal beat buy-and-hold ("n/m" in `beatHold`).
    pub assets: usize,
    pub beat_hold_count: usize,
    pub beat_hold: Option<String>,
    /// Pooled trades of the class and their t statistic (mean ÷ standard error of the net trade returns).
    pub trades: usize,
    pub t_stat: Option<f64>,
    /// Current regime of the asset ("bull" | "bear" | "range" | "crisis" | "unknown") and its French label.
    pub regime: Regime,
    pub regime_label: String,
    /// Verdict of the class's trades taken in that regime; None when unavailable or the regime is unknown.
    pub regime_verdict: Option<Verdict>,
    pub regime_trades: usize,
    pub regime_t_stat: Option<f64>,
    /// True when the class verdict is "negative" or the signal beat buy-and-hold on less than a third of the class:
    /// the confidence is then capped at `EVIDENCE_CAP`.
    pub weak: bool,
    /// Plain-French summary.
    pub text: String,
    /// Time of the validation report (ms); None when unavailable.
    pub as_of: Option<i64>,
    pub link: String,
}

impl Default for ModelEvidence {
    fn default() -> Self {
        ModelEvidence {
            available: false,
            asset_class: AssetClass::Stock,
            class_label: AssetClass::Stock.label().into(),
            class_verdict: None,
            class_verdict_label: None,
            assets: 0,
            beat_hold_count: 0,
            beat_hold: None,
            trades: 0,
            t_stat: None,
            regime: Regime::Unknown,
            regime_label: regime_phrase(Regime::Unknown).into(),
            regime_verdict: None,
            regime_trades: 0,
            regime_t_stat: None,
            weak: false,
            text: NOT_COMPUTED.into(),
            as_of: None,
            link: LINK.into(),
        }
    }
}

/// BTC → Bitcoin, ETH → Ethereum, any other crypto → altcoins, stocks and ETFs → stocks.
pub fn asset_class(symbol: &str, kind: Kind) -> AssetClass {
    match (kind, symbol.to_ascii_uppercase().as_str()) {
        (Kind::Stock, _) => AssetClass::Stock,
        (Kind::Crypto, "BTC") => AssetClass::Btc,
        (Kind::Crypto, "ETH") => AssetClass::Eth,
        _ => AssetClass::Altcoin,
    }
}

/// Regime on the last closed daily candle (a candle is closed one day after its open time), with the validation's
/// rule; unknown with fewer than 220 closed candles.
pub fn current_regime(daily: &[Candle], now: i64) -> Regime {
    let closed: Vec<Candle> = sanitize(daily).into_iter().filter(|c| c.time + DAY_MS <= now).collect();
    match closed.len() {
        0 => Regime::Unknown,
        n => regime_at(&closed, n - 1),
    }
}

fn regime_phrase(r: Regime) -> &'static str {
    match r {
        Regime::Bull => "marché haussier",
        Regime::Bear => "marché baissier",
        Regime::Range => "marché sans tendance",
        Regime::Crisis => "crise (plus de 30 % sous le plus haut d'un an)",
        Regime::Unknown => "régime inconnu (historique trop court)",
    }
}

fn t_fr(t: Option<f64>) -> String {
    t.map(|t| format!(" (t = {}{})", if t < 0.0 { "−" } else { "" }, fr(t.abs(), 0, 1))).unwrap_or_default()
}

fn assets_word(n: usize) -> String {
    format!("{n} actif{}", if n > 1 { "s" } else { "" })
}

fn trades_phrase(v: Verdict, trades: usize, t: Option<f64>, expectancy: Option<f64>) -> String {
    match v {
        Verdict::Insufficient => format!("trop peu de trades ({trades}) pour conclure"),
        Verdict::Edge => format!("gain moyen positif par trade{}", t_fr(t)),
        Verdict::Negative => format!("perte moyenne par trade{}", t_fr(t)),
        Verdict::Unproven if expectancy.is_some_and(|e| e > 0.0) => format!("gain moyen par trade positif mais non démontré{}", t_fr(t)),
        Verdict::Unproven => format!("avantage par trade non démontré{}", t_fr(t)),
    }
}

fn subject(class: AssetClass, n: usize) -> String {
    match class {
        AssetClass::Stock => format!("Sur les actions et ETF testés ({n})"),
        AssetClass::Btc => "Sur le Bitcoin".into(),
        AssetClass::Eth => "Sur l'Ethereum".into(),
        AssetClass::Altcoin => format!("Sur les altcoins testés ({n})"),
    }
}

fn class_phrase(class: AssetClass) -> &'static str {
    match class {
        AssetClass::Stock => "les actions et ETF américains",
        AssetClass::Btc => "le Bitcoin",
        AssetClass::Eth => "l'Ethereum",
        AssetClass::Altcoin => "les altcoins",
    }
}

fn is_weak(g: &GroupStat) -> bool {
    g.verdict == Verdict::Negative || g.beat_hold * 3 < g.assets
}

/// The evidence for one asset: its class's group in `report` (None: not computed yet) and its current regime.
pub fn model_evidence(report: Option<&ValidationReport>, symbol: &str, kind: Kind, daily: &[Candle], now: i64) -> ModelEvidence {
    let class = asset_class(symbol, kind);
    let regime = current_regime(daily, now);
    let base = ModelEvidence {
        asset_class: class,
        class_label: class.label().into(),
        regime,
        regime_label: regime_phrase(regime).into(),
        ..ModelEvidence::default()
    };
    let Some(report) = report else { return base };
    let Some(g) = report.classes.iter().find(|g| g.id == class.id()) else {
        return ModelEvidence {
            text: format!("Validation du {} : aucun actif de cette classe n'a pu être testé, rien à conclure.", crate::js::iso_date(report.as_of)),
            as_of: Some(report.as_of),
            ..base
        };
    };
    let n = g.assets;
    let mut text = subject(class, n);
    let trade = trades_phrase(g.verdict, g.pooled.trades, g.pooled.t_stat, g.pooled.expectancy);
    let hold_better = n - g.beat_hold;
    let hold = match n {
        1 if g.beat_hold == 1 => "le signal a fait mieux que la simple détention".to_string(),
        1 => "la simple détention a fait mieux".to_string(),
        _ if hold_better * 2 > n => format!("la simple détention a fait mieux dans {hold_better} cas sur {n}"),
        _ => format!("le signal a battu la simple détention dans {} cas sur {n}", g.beat_hold),
    };
    let positive = matches!(g.verdict, Verdict::Edge) || (g.verdict == Verdict::Unproven && g.pooled.expectancy.is_some_and(|e| e > 0.0));
    let contrast = positive && hold_better * 2 > n;
    text.push_str(&format!(", {trade}{}{hold}", if contrast { " mais " } else { " ; " }));
    let rg = (regime != Regime::Unknown).then(|| g.regimes.iter().find(|r| r.regime == regime)).flatten();
    match rg {
        Some(r) => text.push_str(&format!(
            " ; en {}, régime actuel de l'actif : {}",
            regime_phrase(regime),
            match r.verdict {
                Verdict::Insufficient => format!("trop peu de trades ({}) pour conclure", r.pooled.trades),
                Verdict::Edge => format!("gain moyen positif{}, à confirmer", t_fr(r.pooled.t_stat)),
                Verdict::Negative => format!("perte moyenne par trade{}", t_fr(r.pooled.t_stat)),
                Verdict::Unproven => format!("avantage non démontré{}", t_fr(r.pooled.t_stat)),
            }
        )),
        None if regime == Regime::Unknown => text.push_str(" ; régime actuel de l'actif inconnu (historique trop court)"),
        None => text.push_str(&format!(" ; en {}, régime actuel de l'actif : aucun trade testé", regime_phrase(regime))),
    }
    text.push_str(". Résultats passés, sans garantie pour la suite.");
    ModelEvidence {
        available: true,
        class_verdict: Some(g.verdict),
        class_verdict_label: Some(g.verdict_label.clone()),
        assets: n,
        beat_hold_count: g.beat_hold,
        beat_hold: Some(format!("{}/{n}", g.beat_hold)),
        trades: g.pooled.trades,
        t_stat: g.pooled.t_stat,
        regime_verdict: rg.map(|r| r.verdict),
        regime_trades: rg.map_or(0, |r| r.pooled.trades),
        regime_t_stat: rg.and_then(|r| r.pooled.t_stat),
        weak: is_weak(g),
        text,
        as_of: Some(report.as_of),
        ..base
    }
}

impl ModelEvidence {
    /// True when the validation shows an edge on the class that is not contradicted by buy-and-hold.
    pub fn is_edge(&self) -> bool {
        self.available && self.class_verdict == Some(Verdict::Edge) && !self.weak
    }

    /// The confidence, capped at `EVIDENCE_CAP` when the evidence is weak (never raised).
    pub fn cap_confidence(&self, confidence: f64) -> f64 {
        if self.available && self.weak { confidence.min(EVIDENCE_CAP) } else { confidence }
    }

    /// The con to add: whenever the validation is available and does not show an edge.
    pub fn con(&self) -> Option<String> {
        if !self.available || self.is_edge() {
            return None;
        }
        let mut s = format!("Le signal n'a pas démontré d'avantage sur cette classe d'actifs (validation sur {}", assets_word(self.assets));
        if self.assets > 1 && self.weak && self.beat_hold_count < self.assets {
            s.push_str(&format!(" : la simple détention a fait mieux dans {} cas", self.assets - self.beat_hold_count));
        }
        s.push(')');
        Some(s)
    }

    /// A strong rating downgraded when the evidence is not an edge, with the reason; otherwise unchanged.
    pub fn cap_rating(&self, r: Rating) -> (Rating, Option<String>) {
        if !self.available || self.is_edge() {
            return (r, None);
        }
        let (to, from) = match r {
            Rating::StrongBuy => (Rating::Buy, "ACHAT FORT"),
            Rating::StrongSell => (Rating::Sell, "VENTE FORTE"),
            _ => return (r, None),
        };
        let why = match self.class_verdict {
            Some(Verdict::Insufficient) => "trop peu de trades pour conclure",
            Some(Verdict::Negative) => "perte moyenne par trade",
            _ if self.weak => "la simple détention a fait mieux sur la plupart des actifs",
            _ => "avantage non démontré",
        };
        (
            to,
            Some(format!(
                "{} plutôt que {from} : la validation du modèle sur {} ({}) ne montre pas d'avantage du signal ({why}).",
                to.label(),
                class_phrase(self.asset_class),
                assets_word(self.assets)
            )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert_eq!(asset_class("BTC", Kind::Crypto), AssetClass::Btc);
        assert_eq!(asset_class("eth", Kind::Crypto), AssetClass::Eth);
        assert_eq!(asset_class("SOL", Kind::Crypto), AssetClass::Altcoin);
        assert_eq!(asset_class("SPY", Kind::Stock), AssetClass::Stock);
        // A stock ticker that looks like a crypto one is still a stock.
        assert_eq!(asset_class("ETH", Kind::Stock), AssetClass::Stock);
    }

    #[test]
    fn unavailable_changes_nothing() {
        let e = model_evidence(None, "BTC", Kind::Crypto, &[], 0);
        assert!(!e.available && e.text == NOT_COMPUTED && e.as_of.is_none());
        assert_eq!(e.asset_class, AssetClass::Btc);
        assert_eq!(e.regime, Regime::Unknown);
        assert_eq!(e.cap_confidence(85.0), 85.0);
        assert_eq!(e.con(), None);
        assert_eq!(e.cap_rating(Rating::StrongBuy), (Rating::StrongBuy, None));
    }

    fn ev(verdict: Verdict, beat: usize, assets: usize) -> ModelEvidence {
        ModelEvidence {
            available: true,
            class_verdict: Some(verdict),
            assets,
            beat_hold_count: beat,
            beat_hold: Some(format!("{beat}/{assets}")),
            weak: verdict == Verdict::Negative || beat * 3 < assets,
            ..ModelEvidence::default()
        }
    }

    #[test]
    fn caps_and_downgrades() {
        // Edge not contradicted by holding: nothing.
        let e = ev(Verdict::Edge, 5, 10);
        assert!(e.is_edge() && e.con().is_none());
        assert_eq!(e.cap_confidence(80.0), 80.0);
        assert_eq!(e.cap_rating(Rating::StrongBuy), (Rating::StrongBuy, None));
        // Unproven: the con, no cap, no strong rating.
        let e = ev(Verdict::Unproven, 5, 10);
        assert_eq!(e.con().as_deref(), Some("Le signal n'a pas démontré d'avantage sur cette classe d'actifs (validation sur 10 actifs)"));
        assert_eq!(e.cap_confidence(80.0), 80.0);
        let (r, why) = e.cap_rating(Rating::StrongBuy);
        assert_eq!(r, Rating::Buy);
        assert_eq!(
            why.as_deref(),
            Some(
                "ACHAT plutôt que ACHAT FORT : la validation du modèle sur les actions et ETF américains (10 actifs) ne montre pas d'avantage du signal (avantage non démontré)."
            )
        );
        let (r, why) = e.cap_rating(Rating::StrongSell);
        assert!(r == Rating::Sell && why.unwrap().starts_with("VENDRE plutôt que VENTE FORTE"));
        assert_eq!(e.cap_rating(Rating::Buy), (Rating::Buy, None));
        assert_eq!(e.cap_rating(Rating::Hold), (Rating::Hold, None));
        // Weak (edge per trade, holding better on 20 of 22): capped at 60, never raised.
        let e = ev(Verdict::Edge, 2, 22);
        assert!(e.weak && !e.is_edge());
        assert_eq!((e.cap_confidence(72.0), e.cap_confidence(41.0)), (EVIDENCE_CAP, 41.0));
        assert!(e.con().unwrap().ends_with("(validation sur 22 actifs : la simple détention a fait mieux dans 20 cas)"));
        assert!(e.cap_rating(Rating::StrongBuy).1.unwrap().contains("la simple détention a fait mieux"));
        // Insufficient on one asset (Bitcoin): weak (0 of 1), singular.
        let e = ModelEvidence { asset_class: AssetClass::Btc, ..ev(Verdict::Insufficient, 0, 1) };
        assert!(e.weak);
        assert_eq!(e.con().as_deref(), Some("Le signal n'a pas démontré d'avantage sur cette classe d'actifs (validation sur 1 actif)"));
        assert!(
            e.cap_rating(Rating::StrongSell)
                .1
                .unwrap()
                .contains("sur le Bitcoin (1 actif) ne montre pas d'avantage du signal (trop peu de trades pour conclure)")
        );
        // Negative, even if it beat holding everywhere.
        assert!(ev(Verdict::Negative, 10, 10).weak);
    }

    #[test]
    fn regime_on_closed_candles_only() {
        // 300 rising days: bull; the candle still open (today) is ignored.
        let day = |i: i64, close: f64| Candle { time: i * DAY_MS, open: close, high: close, low: close, close, volume: 1.0 };
        let mut c: Vec<Candle> = (0..300).map(|i| day(i, 100.0 + i as f64)).collect();
        let now = 300 * DAY_MS;
        assert_eq!(current_regime(&c, now), Regime::Bull);
        // An open candle 50 % lower does not count before it closes.
        c.push(day(300, 150.0));
        assert_eq!(current_regime(&c, now + 3_600_000), Regime::Bull);
        assert_eq!(current_regime(&c, now + DAY_MS), Regime::Crisis);
        assert_eq!(current_regime(&c[..100], now), Regime::Unknown);
    }
}

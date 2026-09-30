//! Unusual readings on one asset (`/api/anomalies`): volume spike, price/volume divergence, distance to the 20-day
//! mean (z-score) on the last closed daily candles, and for cryptos the derivatives (open interest, funding,
//! long/short ratio, OKX). Every measure is returned, triggered or not, with its value and threshold; what it "may
//! mean" is always phrased as a possibility to confirm, never as a certain signal.
use serde::Serialize;

use crate::js::fr;
use crate::types::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Below the threshold.
    Normal,
    Warning,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anomaly {
    /// "volume" | "priceVolume" | "zScore" | "openInterest" | "funding" | "longShort".
    pub code: &'static str,
    pub severity: Severity,
    pub triggered: bool,
    /// "Volume ×4,2 par rapport à la moyenne 20 j".
    pub title: String,
    /// Measured value and the threshold it is compared with, in `unit`.
    pub value: f64,
    pub threshold: f64,
    /// "×" (ratio) | "%" | "σ" (standard deviations).
    pub unit: &'static str,
    /// "×4,2 (seuil ×3)".
    pub measured: String,
    /// What it can mean, with the uncertainty and what would confirm it.
    pub meaning: String,
    pub source: String,
}

/// Volume ratio that triggers the alert (warning, high).
pub const VOLUME_X: (f64, f64) = (3.0, 5.0);
/// |z| of the close against its 20-day mean (warning, high).
pub const Z_LIMIT: (f64, f64) = (2.5, 3.5);
/// Price/volume divergence: 5-session volume ÷ the 20 sessions before, at or under (warning, high).
pub const FADING_VOLUME: (f64, f64) = (0.7, 0.5);
/// |open interest change over 24 h|, % (warning, high).
pub const OI_24H: (f64, f64) = (15.0, 30.0);
/// Funding: outside the 5–95 % range of its recent settlements, and at least this far from zero (fraction per
/// period, 0.02 % = twice OKX's neutral rate); from 0.1 % per period, always "high".
pub const FUNDING_MIN: f64 = 0.0002;
pub const FUNDING_HIGH: f64 = 0.001;
/// Share of the recent distribution strictly on one side of the value to call it extreme (%).
pub const TAIL: f64 = 95.0;

fn x(v: f64) -> String {
    format!("×{}", fr(v, 0, 1))
}

fn signed(v: f64, digits: usize) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, digits))
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn ok(c: &Candle) -> bool {
    c.close.is_finite() && c.close > 0.0 && c.volume.is_finite() && c.volume >= 0.0
}

/// Below its threshold, a measure says so instead of what an alert would mean.
fn finish(mut a: Anomaly) -> Anomaly {
    if !a.triggered {
        a.meaning = "Rien d'inhabituel : sous le seuil d'alerte.".into();
    }
    a
}

fn level(triggered: bool, high: bool) -> Severity {
    match (triggered, high) {
        (false, _) => Severity::Normal,
        (true, false) => Severity::Warning,
        (true, true) => Severity::High,
    }
}

/// Last closed session's volume ÷ the average of the 20 sessions before it.
pub fn volume_spike(c: &[Candle], source: &str) -> Option<Anomaly> {
    if c.len() < 21 {
        return None;
    }
    let last = c[c.len() - 1];
    let prior = &c[c.len() - 21..c.len() - 1];
    if !ok(&last) || !prior.iter().all(ok) {
        return None;
    }
    let avg = mean(&prior.iter().map(|x| x.volume).collect::<Vec<_>>());
    if avg <= 0.0 {
        return None;
    }
    let r = last.volume / avg;
    let t = r >= VOLUME_X.0;
    let up = last.close >= last.open;
    Some(finish(Anomaly {
        code: "volume",
        severity: level(t, r >= VOLUME_X.1),
        triggered: t,
        title: format!("Volume {} par rapport à la moyenne 20 j", x(r)),
        value: r,
        threshold: VOLUME_X.0,
        unit: "×",
        measured: format!("{} (seuil {})", x(r), x(VOLUME_X.0)),
        meaning: format!(
            "Des échanges inhabituels sur une séance {} peuvent signaler une information nouvelle (résultats, annonce) ou un gros intervenant, sans certitude sur la suite ; à confirmer par l'actualité du jour et la clôture des séances suivantes.",
            if up { "haussière" } else { "baissière" }
        ),
        source: source.to_string(),
    }))
}

/// Price/volume divergence, precisely: the last close is a 20-session closing high (or low) while the average
/// volume of the last 5 sessions is at most 70 % of the average of the 20 sessions before them.
pub fn price_volume_divergence(c: &[Candle], source: &str) -> Option<Anomaly> {
    if c.len() < 25 || !c[c.len() - 25..].iter().all(ok) {
        return None;
    }
    let n = c.len();
    let last = c[n - 1].close;
    let window: Vec<f64> = c[n - 20..n - 1].iter().map(|x| x.close).collect();
    let high = window.iter().all(|v| last >= *v);
    let low = window.iter().all(|v| last <= *v);
    let v5 = mean(&c[n - 5..].iter().map(|x| x.volume).collect::<Vec<_>>());
    let v20 = mean(&c[n - 25..n - 5].iter().map(|x| x.volume).collect::<Vec<_>>());
    if v20 <= 0.0 {
        return None;
    }
    let r = v5 / v20;
    let t = (high || low) && r <= FADING_VOLUME.0;
    let pct = r * 100.0;
    let (title, meaning) = if t && high {
        (
            format!("Forte divergence prix/volume : plus haut de 20 séances sur un volume à {} % de la normale", fr(pct, 0, 0)),
            "Une hausse portée par de moins en moins d'échanges peut signaler un essoufflement des acheteurs, sans certitude ; à confirmer par la cassure du dernier creux ou, à l'inverse, par un retour du volume.",
        )
    } else if t && low {
        (
            format!("Forte divergence prix/volume : plus bas de 20 séances sur un volume à {} % de la normale", fr(pct, 0, 0)),
            "Une baisse sur des échanges qui se tarissent peut signaler une pression vendeuse qui s'épuise, sans certitude ; à confirmer par un rebond au-dessus du dernier sommet avec du volume.",
        )
    } else {
        (
            format!("Pas de divergence prix/volume (volume 5 j à {} % des 20 j précédents)", fr(pct, 0, 0)),
            "Le prix n'est pas à un extrême de 20 séances : pas de divergence à signaler.",
        )
    };
    Some(finish(Anomaly {
        code: "priceVolume",
        severity: level(t, r <= FADING_VOLUME.1),
        triggered: t,
        title,
        value: pct,
        threshold: FADING_VOLUME.0 * 100.0,
        unit: "%",
        measured: format!(
            "volume 5 j = {} % des 20 j précédents (seuil ≤ {} %){}",
            fr(pct, 0, 0),
            fr(FADING_VOLUME.0 * 100.0, 0, 0),
            if high || low { ", prix à un extrême de 20 séances" } else { ", prix pas à un extrême de 20 séances" }
        ),
        meaning: meaning.to_string(),
        source: source.to_string(),
    }))
}

/// Distance of the last close to its 20-session mean, in standard deviations of those 20 closes.
pub fn z_score(c: &[Candle], source: &str) -> Option<Anomaly> {
    if c.len() < 20 {
        return None;
    }
    let closes: Vec<f64> = c[c.len() - 20..].iter().map(|x| x.close).collect();
    if closes.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return None;
    }
    let m = mean(&closes);
    let sd = (closes.iter().map(|v| (v - m).powi(2)).sum::<f64>() / closes.len() as f64).sqrt();
    if sd <= 0.0 {
        return None;
    }
    let z = (closes[closes.len() - 1] - m) / sd;
    let t = z.abs() >= Z_LIMIT.0;
    let above = z >= 0.0;
    Some(finish(Anomaly {
        code: "zScore",
        severity: level(t, z.abs() >= Z_LIMIT.1),
        triggered: t,
        title: format!(
            "Écart {} à la moyenne 20 j : {}{} σ",
            if t { "inhabituel" } else { "normal" },
            if above { "+" } else { "−" },
            fr(z.abs(), 1, 1)
        ),
        value: z,
        threshold: if above { Z_LIMIT.0 } else { -Z_LIMIT.0 },
        unit: "σ",
        measured: format!("{}{} σ (seuil ±{} σ)", if above { "+" } else { "−" }, fr(z.abs(), 1, 1), fr(Z_LIMIT.0, 1, 1)),
        meaning: if above {
            "Un prix aussi loin au-dessus de sa moyenne peut signaler un excès à court terme et un retour vers la moyenne, sans certitude : une tendance forte peut le prolonger ; à confirmer par le RSI et la tenue des derniers niveaux."
        } else {
            "Un prix aussi loin sous sa moyenne peut signaler une vente excessive et un rebond technique, sans certitude : une baisse de fond peut continuer ; à confirmer par un arrêt de la baisse et le RSI."
        }
        .to_string(),
        source: source.to_string(),
    }))
}

/// Change (%) between the last value and the one `back` steps before; None when too short or not positive.
pub fn change(series: &[f64], back: usize) -> Option<f64> {
    let n = series.len();
    if n <= back {
        return None;
    }
    let (a, b) = (series[n - 1 - back], series[n - 1]);
    (a > 0.0 && b.is_finite()).then(|| (b / a - 1.0) * 100.0)
}

/// Open interest change over 24 h from hourly values (oldest first).
pub fn open_interest(oi_hourly: &[f64], source: &str) -> Option<Anomaly> {
    let d = change(oi_hourly, 24)?;
    let t = d.abs() >= OI_24H.0;
    Some(finish(Anomaly {
        code: "openInterest",
        severity: level(t, d.abs() >= OI_24H.1),
        triggered: t,
        title: format!("Open interest {} en 24 h", signed(d, if d.abs() >= 10.0 { 0 } else { 1 })),
        value: d,
        threshold: if d >= 0.0 { OI_24H.0 } else { -OI_24H.0 },
        unit: "%",
        measured: format!("{} (seuil ±{} %)", signed(d, 1), fr(OI_24H.0, 0, 0)),
        meaning: if d >= 0.0 {
            "Beaucoup de nouvelles positions à effet de levier : peut annoncer un mouvement plus brutal, dans un sens ou dans l'autre, voire des liquidations en chaîne, sans certitude ; à confirmer par le funding et le sens du prix."
        } else {
            "Des positions à levier se ferment (prises de bénéfices ou liquidations) : peut signaler la fin d'un excès, sans certitude ; à confirmer par le prix et les liquidations."
        }
        .to_string(),
        source: source.to_string(),
    }))
}

/// Share (%) of `history` strictly below and strictly above `v`.
pub fn tails(history: &[f64], v: f64) -> (f64, f64) {
    let n = history.len() as f64;
    let below = history.iter().filter(|h| **h < v).count() as f64 / n * 100.0;
    let above = history.iter().filter(|h| **h > v).count() as f64 / n * 100.0;
    (below, above)
}

/// Percentile `p` (0–100) of `v`, nearest rank.
pub fn percentile(v: &[f64], p: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let k = ((p / 100.0) * (s.len() - 1) as f64).round() as usize;
    s.get(k).copied()
}

/// Current funding rate (fraction per period) against its recent settled rates (at least 30).
pub fn funding(rate: f64, history: &[f64], source: &str) -> Option<Anomaly> {
    if history.len() < 30 || !rate.is_finite() {
        return None;
    }
    let (below, above) = tails(history, rate);
    let (p5, p95) = (percentile(history, 5.0)?, percentile(history, 95.0)?);
    let extreme_share = below >= TAIL || above >= TAIL;
    let t = (extreme_share && rate.abs() >= FUNDING_MIN) || rate.abs() >= FUNDING_HIGH;
    let pct = rate * 100.0;
    let positive = rate >= 0.0;
    Some(finish(Anomaly {
        code: "funding",
        severity: level(t, rate.abs() >= FUNDING_HIGH),
        triggered: t,
        title: format!("Funding {} : {} par période", if t { "extrême" } else { "dans sa plage habituelle" }, signed(pct, 4)),
        value: pct,
        threshold: if positive { p95 * 100.0 } else { p5 * 100.0 },
        unit: "%",
        measured: format!(
            "{} ; plage 5–95 % des {} derniers règlements : {} à {} (et |taux| ≥ {} % requis)",
            signed(pct, 4),
            history.len(),
            signed(p5 * 100.0, 4),
            signed(p95 * 100.0, 4),
            fr(FUNDING_MIN * 100.0, 2, 2)
        ),
        meaning: if positive {
            "Les acheteurs à levier paient cher pour garder leurs positions : peut signaler un marché trop optimiste, exposé à une purge des positions acheteuses, sans certitude ; à confirmer par l'open interest et les liquidations."
        } else {
            "Les vendeurs à découvert paient pour garder leurs positions : peut signaler un pessimisme excessif et un risque de rachat forcé des ventes (short squeeze), sans certitude ; à confirmer par l'open interest et le prix."
        }
        .to_string(),
        source: source.to_string(),
    }))
}

/// Long/short account ratio (last hourly value) against its own last 30 days of hourly values.
pub fn long_short(series: &[f64], source: &str) -> Option<Anomaly> {
    let (&last, hist) = series.split_last()?;
    if hist.len() < 48 || !last.is_finite() {
        return None;
    }
    let (below, above) = tails(hist, last);
    let (p5, p95) = (percentile(hist, 5.0)?, percentile(hist, 95.0)?);
    let t = below >= TAIL || above >= TAIL;
    let high_side = below >= TAIL;
    Some(finish(Anomaly {
        code: "longShort",
        severity: level(t, false),
        triggered: t,
        title: format!("Ratio acheteurs/vendeurs {} : {}", if t { "inhabituel" } else { "habituel" }, fr(last, 2, 2)),
        value: last,
        threshold: if high_side || last >= p95 { p95 } else { p5 },
        unit: "×",
        measured: format!("{} ; plage 5–95 % sur {} h : {} à {}", fr(last, 2, 2), hist.len(), fr(p5, 2, 2), fr(p95, 2, 2)),
        meaning: if last >= p95 {
            "Une part inhabituelle de comptes parie à la hausse : peut signaler un positionnement encombré, sans certitude ; ce ratio compte des comptes, pas des montants — à confirmer par le funding."
        } else {
            "Une part inhabituelle de comptes parie à la baisse : peut signaler un pessimisme encombré (risque de rachat forcé), sans certitude ; ce ratio compte des comptes, pas des montants — à confirmer par le funding."
        }
        .to_string(),
        source: source.to_string(),
    }))
}

/// The daily-candle measures of an asset (closed sessions only), in display order.
pub fn candle_anomalies(daily: &[Candle], source: &str) -> Vec<Anomaly> {
    [volume_spike(daily, source), price_volume_divergence(daily, source), z_score(daily, source)].into_iter().flatten().collect()
}

/// Triggered first (most severe first), then the normal measures.
pub fn sort(list: &mut [Anomaly]) {
    list.sort_by_key(|a| match a.severity {
        Severity::High => 0,
        Severity::Warning => 1,
        Severity::Normal => 2,
    });
}

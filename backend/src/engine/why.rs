//! « Pourquoi ça bouge ? » (`/api/why`): what was observed AT THE SAME TIME as today's move of an asset, from sources
//! the server already has in cache (candles, the market guard's inputs, news, macro stress and market regime, today's
//! calendar, the last decision computed). A structured list of factors, each with a direction, a magnitude, a source
//! and an uncertainty level, and a deterministic French summary. These are co-occurring observations, never proven
//! causes: only the move and the volume are "observé"; a link with anything else is at best a possible correlation.
//! Pure, deterministic functions.
use serde::Serialize;

use crate::engine::decision_types::{Family, Status};
use crate::engine::guard::GUARD;
use crate::engine::macro_ctx::MacroLevel;
use crate::engine::signal::atr;
use crate::engine::synthesis::{MarketRegime, RegimeKind};
use crate::js::fr;
use crate::types::{Candle, Kind};

pub const DISCLAIMER: &str = "Observations simultanées, pas des causes prouvées : un mouvement peut avoir une cause non observée ici (ordre important, rumeur, \
     information non publique…). Ce n'est ni un conseil ni une prévision.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Up,
    Down,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Magnitude {
    Low,
    Medium,
    High,
}

/// How far the factor can be linked to the move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Certainty {
    /// Measured on the asset itself (the move, its volume).
    Observed,
    /// Measured elsewhere at the same time: a link is possible, not shown.
    PossibleCorrelation,
    /// A link cannot be checked with the data available (headlines, scheduled events not yet published).
    Unverifiable,
}

impl Certainty {
    pub fn label(self) -> &'static str {
        match self {
            Certainty::Observed => "observé",
            Certainty::PossibleCorrelation => "corrélation possible",
            Certainty::Unverifiable => "non vérifiable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Factor {
    pub key: String,
    pub label: String,
    /// Direction of the move or of the pressure the factor is usually associated with.
    pub direction: Direction,
    pub magnitude: Magnitude,
    pub detail: String,
    pub source: String,
    pub certainty: Certainty,
    pub certainty_label: String,
    /// Short form used in the summary ("Fear & Greed 74/100").
    pub brief: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotCovered {
    pub key: String,
    pub label: String,
    pub reason: String,
}

/// Change since the last closed daily candle.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// %.
    pub pct: f64,
    pub price: f64,
    pub previous_close: f64,
    /// Start of the last closed daily candle (ms).
    pub previous_close_time: i64,
    /// Daily ATR (14) in % of the price, on the closed candles.
    pub atr_pct: Option<f64>,
}

/// Volume of the last closed day against the 20 days before it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeObs {
    pub ratio: f64,
    pub day: i64,
    pub volume: f64,
    pub average: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceState {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhyReport {
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    pub as_of: i64,
    pub change: Option<Change>,
    pub volume: Option<VolumeObs>,
    pub factors: Vec<Factor>,
    pub not_covered: Vec<NotCovered>,
    pub summary: String,
    pub disclaimer: String,
    /// The optional question box (`POST /api/ask`) is available (server key configured).
    pub ask_enabled: bool,
    pub sources: Vec<SourceState>,
}

/// Benchmark's own move over the same period.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchMove {
    pub name: String,
    pub pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewsInput {
    pub count24h: usize,
    pub negative: usize,
    pub positive: usize,
    pub headlines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventInput {
    pub title: String,
    pub time: Option<String>,
    pub high: bool,
    /// Published figure (the event already happened today).
    pub released: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionInfo {
    pub as_of: i64,
    pub label: String,
    pub families: Vec<Family>,
}

/// Everything gathered for one asset; `None` = source not loaded (it lands in `notCovered` when it applies).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WhyInput {
    pub symbol: String,
    pub name: String,
    pub kind: Option<Kind>,
    pub now: i64,
    /// Consensus live price and "agreeing/total" sources.
    pub price: Option<f64>,
    pub price_sources: Option<(usize, usize)>,
    /// Closed daily candles of the asset.
    pub daily: Vec<Candle>,
    pub benchmark: Option<BenchMove>,
    /// Name of the benchmark, when the asset is not itself the benchmark (for the "not covered" line).
    pub benchmark_name: Option<String>,
    pub guard_loaded: bool,
    pub funding_rate: Option<f64>,
    pub long_short: Option<f64>,
    pub fear_greed: Option<f64>,
    pub news: Option<NewsInput>,
    pub macro_score: Option<f64>,
    pub macro_level: Option<MacroLevel>,
    pub regime: Option<MarketRegime>,
    /// Today's events; None: calendar not loaded (or a source failed).
    pub events_today: Option<Vec<EventInput>>,
    pub decision: Option<DecisionInfo>,
}

fn signed(v: f64, d: usize) -> String {
    format!(
        "{}{}",
        if v > 0.0 {
            "+"
        } else if v < 0.0 {
            "−"
        } else {
            ""
        },
        fr(v.abs(), 0, d)
    )
}

fn ddmm(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.format("%d/%m").to_string()).unwrap_or_default()
}

fn dir_of(v: f64) -> Direction {
    if v > 0.0 {
        Direction::Up
    } else if v < 0.0 {
        Direction::Down
    } else {
        Direction::Neutral
    }
}

fn factor(key: &str, label: String, direction: Direction, magnitude: Magnitude, detail: String, source: &str, certainty: Certainty) -> Factor {
    Factor {
        key: key.into(),
        label,
        direction,
        magnitude,
        detail,
        source: source.into(),
        certainty,
        certainty_label: certainty.label().into(),
        brief: String::new(),
    }
}

/// Short form of a factor for the summary.
fn brief_of(f: &Factor, inp: &WhyInput, change: Option<&Change>, volume: Option<&VolumeObs>) -> String {
    let n = |v: usize, w: &str| format!("{v} {w}{}", if v > 1 { "s" } else { "" });
    match f.key.as_str() {
        "move" => change.map_or_else(String::new, |c| format!("variation {} %", signed(c.pct, 1))),
        "volume" => volume.map_or_else(String::new, |v| format!("volume {}× la moyenne", fr(v.ratio, 0, 1))),
        "market" => inp.benchmark.as_ref().map_or_else(String::new, |b| format!("{} {} %", b.name, signed(b.pct, 1))),
        "funding" => inp.funding_rate.map_or_else(String::new, |x| format!("financement des perpétuels {} % par 8 h", fr(x * 100.0, 0, 4))),
        "longShort" => inp.long_short.map_or_else(String::new, |x| format!("ratio acheteurs / vendeurs à levier {}", fr(x, 0, 2))),
        "fearGreed" => inp.fear_greed.map_or_else(String::new, |x| format!("Fear & Greed {}/100", fr(x, 0, 0))),
        "news" => inp
            .news
            .as_ref()
            .map_or_else(String::new, |x| format!("{} en 24 h ({}, {})", n(x.count24h, "titre"), n(x.negative, "négatif"), n(x.positive, "positif"))),
        "macro" => inp.macro_score.map_or_else(String::new, |x| format!("stress macro {}/100", fr(x, 0, 0))),
        "regime" => inp.regime.as_ref().map_or_else(String::new, |r| format!("régime {}", r.label)),
        "events" => inp.events_today.as_ref().map_or_else(String::new, |e| format!("{} aujourd'hui", n(e.len(), "annonce"))),
        "decision" => inp.decision.as_ref().map_or_else(String::new, |d| format!("décision « {} »", d.label)),
        _ => f.label.clone(),
    }
}

/// Change of `price` against the close of the last closed daily candle, with the daily ATR % of those candles.
pub fn change_since_close(daily: &[Candle], price: Option<f64>) -> Option<Change> {
    let last = daily.last()?;
    let price = price.filter(|p| p.is_finite() && *p > 0.0)?;
    if last.close <= 0.0 {
        return None;
    }
    let atr_pct = if daily.len() >= 15 { atr(daily, 14).last().copied().flatten().map(|a| a / last.close * 100.0) } else { None };
    Some(Change { pct: (price / last.close - 1.0) * 100.0, price, previous_close: last.close, previous_close_time: last.time, atr_pct })
}

/// Volume of the last closed day ÷ average of the 20 closed days before it (None under 21 days or without volume).
pub fn volume_vs_average(daily: &[Candle]) -> Option<VolumeObs> {
    if daily.len() < 21 {
        return None;
    }
    let last = daily.last()?;
    let prev: Vec<f64> = daily[daily.len() - 21..daily.len() - 1].iter().map(|c| c.volume).filter(|v| v.is_finite() && *v > 0.0).collect();
    if prev.len() < 20 || last.volume.is_nan() || last.volume <= 0.0 {
        return None;
    }
    let average = prev.iter().sum::<f64>() / prev.len() as f64;
    Some(VolumeObs { ratio: last.volume / average, day: last.time, volume: last.volume, average })
}

/// Size of a move against the asset's own daily volatility (ATR), else fixed steps (1 % / 3 %).
fn move_magnitude(pct: f64, atr_pct: Option<f64>) -> Magnitude {
    let x = match atr_pct.filter(|a| *a > 0.0) {
        Some(a) => pct.abs() / a,
        None => pct.abs() / 2.0,
    };
    if x < 0.5 {
        Magnitude::Low
    } else if x < 1.5 {
        Magnitude::Medium
    } else {
        Magnitude::High
    }
}

fn family_bias(families: &[Family]) -> (Direction, Vec<String>) {
    let keys = ["trend", "momentum", "volume"];
    let mut score = 0i32;
    let mut parts = vec![];
    for f in families.iter().filter(|f| keys.contains(&f.key.as_str())) {
        let (s, w) = match f.status {
            Status::Positive => (1, "favorable"),
            Status::Negative => (-1, "défavorable"),
            Status::Neutral => (0, "neutre"),
            Status::Unavailable => continue,
        };
        score += s;
        parts.push(format!("{} {w}", f.label.to_lowercase()));
    }
    (dir_of(score as f64), parts)
}

/// The report (without `askEnabled` nor `sources`, set by the route).
pub fn why_report(inp: &WhyInput) -> WhyReport {
    let kind = inp.kind.unwrap_or(Kind::Crypto);
    let change = change_since_close(&inp.daily, inp.price);
    let volume = volume_vs_average(&inp.daily);
    let mut factors = vec![];
    let mut not_covered = vec![];
    let nc = |key: &str, label: &str, reason: &str| NotCovered { key: key.into(), label: label.into(), reason: reason.into() };

    match &change {
        Some(c) => {
            let src = match inp.price_sources {
                Some((a, t)) => format!("Cours consensus ({a}/{t} sources) et bougies journalières closes"),
                None => "Cours consensus et bougies journalières closes".into(),
            };
            let atr = c.atr_pct.map(|a| format!(" ; volatilité journalière habituelle (ATR 14 j) {} %", fr(a, 0, 1))).unwrap_or_default();
            factors.push(factor(
                "move",
                "Variation du cours".into(),
                dir_of(c.pct),
                move_magnitude(c.pct, c.atr_pct),
                format!("{} % depuis la clôture du {}{atr}.", signed(c.pct, 1), ddmm(c.previous_close_time)),
                &src,
                Certainty::Observed,
            ))
        }
        None => not_covered.push(nc("move", "Variation du cours", "Cours actuel ou bougies journalières indisponibles.")),
    }

    match &volume {
        Some(v) => {
            let (mag, word) = if v.ratio >= 2.0 {
                (Magnitude::High, "nettement au-dessus de")
            } else if v.ratio >= 1.3 {
                (Magnitude::Medium, "au-dessus de")
            } else if v.ratio <= 0.7 {
                (Magnitude::Medium, "sous")
            } else {
                (Magnitude::Low, "proche de")
            };
            factors.push(factor(
                "volume",
                "Volume".into(),
                Direction::Neutral,
                mag,
                format!("Dernière séance close ({}) : {}× la moyenne des 20 précédentes, {word} l'habitude.", ddmm(v.day), fr(v.ratio, 0, 1)),
                "Bougies journalières closes (consensus multi-sources)",
                Certainty::Observed,
            ));
        }
        None => not_covered.push(nc("volume", "Volume", "Moins de 21 séances closes avec un volume.")),
    }

    match (&inp.benchmark, &inp.benchmark_name) {
        (Some(b), _) => {
            let same = change.map(|c| c.pct * b.pct > 0.0);
            let rel = match same {
                Some(true) => " : même sens, un mouvement commun au marché est possible",
                Some(false) => " : sens opposé, le mouvement semble propre à l'actif",
                None => "",
            };
            let mag = if b.pct.abs() < 0.5 {
                Magnitude::Low
            } else if b.pct.abs() < 1.5 {
                Magnitude::Medium
            } else {
                Magnitude::High
            };
            factors.push(factor(
                "market",
                format!("Marché ({})", b.name),
                dir_of(b.pct),
                mag,
                format!("{} {} % sur la même période{rel}.", b.name, signed(b.pct, 1)),
                "Cours consensus et bougies journalières closes de la référence",
                Certainty::PossibleCorrelation,
            ));
        }
        (None, Some(n)) => not_covered.push(nc("market", &format!("Marché ({n})"), "Cours de la référence indisponible.")),
        (None, None) => {}
    }

    if kind == Kind::Crypto {
        if let Some(f) = inp.funding_rate {
            let (dir, mag, txt) = if f >= GUARD.funding_very_hot {
                (Direction::Down, Magnitude::High, "très élevé : acheteurs à levier surchargés, des liquidations peuvent amplifier une baisse")
            } else if f >= GUARD.funding_hot {
                (Direction::Down, Magnitude::Medium, "élevé : beaucoup d'acheteurs à levier")
            } else if f <= GUARD.funding_very_cold {
                (Direction::Up, Magnitude::High, "très négatif : vendeurs à découvert surchargés, un rachat brutal peut amplifier une hausse")
            } else if f <= GUARD.funding_cold {
                (Direction::Up, Magnitude::Medium, "négatif : beaucoup de vendeurs à découvert")
            } else {
                (Direction::Neutral, Magnitude::Low, "normal")
            };
            factors.push(factor(
                "funding",
                "Financement des contrats perpétuels".into(),
                dir,
                mag,
                format!("{} % par 8 h, {txt}.", fr(f * 100.0, 0, 4)),
                "OKX (contrats perpétuels)",
                Certainty::PossibleCorrelation,
            ));
        } else {
            not_covered.push(nc(
                "funding",
                "Financement des contrats perpétuels",
                "Pas de contrat perpétuel OKX pour cet actif, ou source indisponible.",
            ));
        }
        if let Some(r) = inp.long_short {
            let (dir, mag, txt) = if r >= 2.0 {
                (Direction::Down, Magnitude::Medium, "foule très acheteuse (risque de débouclage)")
            } else if r <= 0.7 {
                (Direction::Up, Magnitude::Medium, "foule vendeuse (risque de rachat)")
            } else {
                (Direction::Neutral, Magnitude::Low, "équilibré")
            };
            factors.push(factor(
                "longShort",
                "Ratio acheteurs / vendeurs à levier".into(),
                dir,
                mag,
                format!("{} : {txt}.", fr(r, 0, 2)),
                "OKX (comptes à levier)",
                Certainty::PossibleCorrelation,
            ));
        }
        if let Some(v) = inp.fear_greed {
            let (dir, mag, txt) = if v < 25.0 {
                (Direction::Down, Magnitude::High, "peur extrême")
            } else if v < 45.0 {
                (Direction::Down, Magnitude::Medium, "peur")
            } else if v <= 55.0 {
                (Direction::Neutral, Magnitude::Low, "neutre")
            } else if v <= 75.0 {
                (Direction::Up, Magnitude::Medium, "avidité")
            } else {
                (Direction::Up, Magnitude::High, "avidité extrême")
            };
            factors.push(factor(
                "fearGreed",
                "Sentiment crypto (Fear & Greed)".into(),
                dir,
                mag,
                format!("{}/100 : {txt} (indice quotidien, tout le marché crypto).", fr(v, 0, 0)),
                "alternative.me",
                Certainty::PossibleCorrelation,
            ));
        } else if inp.guard_loaded {
            not_covered.push(nc("fearGreed", "Sentiment crypto (Fear & Greed)", "Indice indisponible pour l'instant."));
        }
    }

    match &inp.news {
        Some(n) if n.count24h > 0 => {
            let dir = dir_of(n.positive as f64 - n.negative as f64);
            let mag = if n.count24h >= 10 {
                Magnitude::High
            } else if n.count24h >= 3 {
                Magnitude::Medium
            } else {
                Magnitude::Low
            };
            let heads: Vec<String> = n.headlines.iter().take(2).map(|h| format!("« {h} »")).collect();
            factors.push(factor(
                "news",
                "Actualités de l'actif (24 h)".into(),
                dir,
                mag,
                format!(
                    "{} titre{} en 24 h : {} négatif{}, {} positif{} (ton lu sur les titres){}.",
                    n.count24h,
                    if n.count24h > 1 { "s" } else { "" },
                    n.negative,
                    if n.negative > 1 { "s" } else { "" },
                    n.positive,
                    if n.positive > 1 { "s" } else { "" },
                    if heads.is_empty() { String::new() } else { format!(" ; ex. {}", heads.join(", ")) }
                ),
                "Google Actualités (titres)",
                Certainty::Unverifiable,
            ));
        }
        Some(_) => factors.push(factor(
            "news",
            "Actualités de l'actif (24 h)".into(),
            Direction::Neutral,
            Magnitude::Low,
            "Aucun titre sur cet actif en 24 h.".into(),
            "Google Actualités (titres)",
            Certainty::Unverifiable,
        )),
        None => not_covered.push(nc("news", "Actualités de l'actif", "Flux d'actualités indisponible.")),
    }

    match (inp.macro_score, inp.macro_level) {
        (Some(s), Some(l)) => {
            let (dir, mag, word) = match l {
                MacroLevel::High => (Direction::Down, Magnitude::High, "très tendu"),
                MacroLevel::Tense => (Direction::Down, Magnitude::Medium, "tendu"),
                MacroLevel::Calm => (Direction::Neutral, Magnitude::Low, "calme"),
            };
            factors.push(factor(
                "macro",
                "Stress macro".into(),
                dir,
                mag,
                format!("{}/100, climat {word} (VIX, S&P 500, pétrole, or, dollar, taux, titres d'escalade).", fr(s, 0, 0)),
                "Altim, contexte macro (Yahoo Finance, Cboe, actualités)",
                Certainty::PossibleCorrelation,
            ));
        }
        _ => not_covered.push(nc("macro", "Stress macro", "Contexte macro indisponible.")),
    }

    if let Some(r) = &inp.regime {
        let dir = match r.kind {
            RegimeKind::RiskOn => Direction::Up,
            RegimeKind::RiskOff => Direction::Down,
            RegimeKind::Neutral => Direction::Neutral,
        };
        let mag = if r.kind == RegimeKind::Neutral { Magnitude::Low } else { Magnitude::Medium };
        let reasons = if r.reasons.is_empty() { String::new() } else { format!(" : {}", r.reasons.join(" ; ")) };
        factors.push(factor(
            "regime",
            "Régime de marché".into(),
            dir,
            mag,
            format!("{}{reasons}.", r.label),
            "Altim (stress macro et tendance du S&P 500)",
            Certainty::PossibleCorrelation,
        ));
    }

    match &inp.events_today {
        Some(ev) if !ev.is_empty() => {
            let released = ev.iter().any(|e| e.released);
            let high = ev.iter().filter(|e| e.high).count();
            let list: Vec<String> = ev.iter().take(3).map(|e| e.time.as_ref().map_or(e.title.clone(), |t| format!("{} ({t})", e.title))).collect();
            factors.push(factor(
                "events",
                "Annonces du jour".into(),
                Direction::Neutral,
                if high > 0 { Magnitude::High } else { Magnitude::Medium },
                format!(
                    "{} annonce{} aujourd'hui{} : {}{}.",
                    ev.len(),
                    if ev.len() > 1 { "s" } else { "" },
                    if high > 0 { format!(", dont {high} importante{}", if high > 1 { "s" } else { "" }) } else { String::new() },
                    list.join(", "),
                    if released { " ; au moins un chiffre déjà publié" } else { " ; pas encore de chiffre publié" }
                ),
                "Calendrier Altim (Nasdaq, Fed, BCE…)",
                if released { Certainty::PossibleCorrelation } else { Certainty::Unverifiable },
            ));
        }
        Some(_) => factors.push(factor(
            "events",
            "Annonces du jour".into(),
            Direction::Neutral,
            Magnitude::Low,
            "Aucune annonce importante prévue aujourd'hui pour cet actif.".into(),
            "Calendrier Altim (Nasdaq, Fed, BCE…)",
            Certainty::PossibleCorrelation,
        )),
        None => not_covered.push(nc("events", "Annonces du jour", "Calendrier non chargé ou source en échec.")),
    }

    match &inp.decision {
        Some(d) => {
            let (dir, parts) = family_bias(&d.families);
            if !parts.is_empty() {
                factors.push(factor(
                    "decision",
                    "Lecture technique".into(),
                    dir,
                    Magnitude::Low,
                    format!("Décision « {} » calculée le {} : {}.", d.label, ddmm(d.as_of), parts.join(", ")),
                    "Décision Altim (dernière calculée)",
                    Certainty::PossibleCorrelation,
                ));
            }
        }
        None => {
            not_covered.push(nc("decision", "Lecture technique", "Aucune décision calculée récemment pour cet actif (ouvrez sa carte « Décision »)."))
        }
    }

    let briefs: Vec<String> = factors.iter().map(|f| brief_of(f, inp, change.as_ref(), volume.as_ref())).collect();
    for (f, b) in factors.iter_mut().zip(briefs) {
        f.brief = b;
    }
    let summary = summary(&inp.symbol, change.as_ref(), volume.as_ref(), &factors, &not_covered);
    WhyReport {
        symbol: inp.symbol.clone(),
        kind,
        name: inp.name.clone(),
        as_of: inp.now,
        change,
        volume,
        factors,
        not_covered,
        summary,
        disclaimer: DISCLAIMER.into(),
        ask_enabled: false,
        sources: vec![],
    }
}

/// "BTC −3,2 % aujourd'hui (volume 1,8× la moyenne). Éléments observés en même temps : … ; Incertitude : …"
pub fn summary(symbol: &str, change: Option<&Change>, volume: Option<&VolumeObs>, factors: &[Factor], not_covered: &[NotCovered]) -> String {
    let mut s = match change {
        Some(c) => format!("{symbol} {} % aujourd'hui (depuis la clôture du {}", signed(c.pct, 1), ddmm(c.previous_close_time)),
        None => format!("{symbol} : variation du jour inconnue ("),
    };
    match volume {
        Some(v) => {
            s.push_str(&format!("{}volume de la dernière séance close {}× la moyenne)", if change.is_some() { " ; " } else { "" }, fr(v.ratio, 0, 1)))
        }
        None => s.push_str(if change.is_some() { ")" } else { "volume inconnu)" }),
    }
    s.push('.');
    let mut others: Vec<&Factor> =
        factors.iter().filter(|f| f.certainty != Certainty::Observed && f.magnitude >= Magnitude::Medium && f.key != "decision").collect();
    others.sort_by_key(|f| std::cmp::Reverse(f.magnitude));
    if others.is_empty() {
        s.push_str(" Éléments observés en même temps : rien de marquant dans les données disponibles.");
    } else {
        let parts: Vec<String> = others.iter().take(4).map(|f| format!("{} ({})", f.brief, f.certainty.label())).collect();
        s.push_str(&format!(" Éléments observés en même temps : {}.", parts.join(" ; ")));
    }
    s.push_str(" Incertitude : ce sont des observations simultanées, pas des causes prouvées");
    if !not_covered.is_empty() {
        let labels: Vec<String> = not_covered.iter().map(|n| n.label.to_lowercase()).collect();
        s.push_str(&format!(" ; non couvert : {}", labels.join(", ")));
    }
    s.push('.');
    s
}

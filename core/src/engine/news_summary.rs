//! "Résumé intelligent" of the news section: the day's few important events (merged stories), each with a transparent
//! impact rule, the user's assets it names, the agreement of its sources' headlines and, when the data is already in
//! the server's cache, the move of those assets since publication and how the story sits with their technical trend.
//! Pure functions: the market data comes through a lookup that never fetches anything.
//!
//! Impact rule (points, `impactPoints`):
//! - independent sources (distinct headlines: one article syndicated word for word counts once): 2–3 → +1, 4+ → +2;
//! - theme: serious escalation (`alert`) → +2; central banks / rates, regulation, hack → +1 (the other themes 0);
//! - names one of the user's assets → +1.
//!
//! 0–1 point = faible, 2–3 = moyen, 4 and more = important ("impact estimé par règle"). When a named asset's hourly
//! candles are cached and at least one full candle closed after publication, the level comes from the largest move
//! since the close preceding publication instead ("impact mesuré"): below 1 % (crypto 2 %) faible, below 3 %
//! (crypto 5 %) moyen, beyond important. A move after a story is not proof that the story caused it.
use serde::{Deserialize, Serialize};

use crate::engine::guard::Trend;
use crate::engine::news::{AssetMatcher, FeedItems, NewsCategory, NewsItem, NewsTheme, NewsTone, RawNews, WatchAsset, same_story, tone_of};

/// Events kept in the summary.
pub const MAX_EVENTS: usize = 5;
/// "Aujourd'hui": stories of the last 24 hours.
const DAY_MS: i64 = 86_400_000;
/// Headlines listed per event.
const MAX_LINKS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Impact {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImpactBasis {
    /// From the move of the named assets since publication.
    Measured,
    /// From the rule (sources, theme, the user's assets).
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agreement {
    /// No two sources with opposite tones.
    Convergent,
    /// At least one negative and one positive headline.
    Divergent,
    /// One source only: nothing to compare.
    Single,
}

/// One source's version of the story.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryLink {
    pub source: String,
    pub title: String,
    pub link: String,
    pub time: i64,
    pub tone: NewsTone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConsensus {
    pub agreement: Agreement,
    /// Over the distinct headlines. Majority tone of the headlines that take a side (neutral on a tie or when none does).
    pub tone: NewsTone,
    pub negative: usize,
    pub positive: usize,
    pub neutral: usize,
}

/// Move of a named asset since the story (hourly closes of the server's cache).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMove {
    /// "stock:AAPL".
    pub asset: String,
    /// Close of the last candle closed before publication (ms, closing time) and its price.
    pub from_time: i64,
    pub from_price: f64,
    /// Close of the last cached candle.
    pub to_time: i64,
    pub to_price: f64,
    /// Percent.
    pub change_pct: f64,
    /// Candle source (consensus of the market data).
    pub source: String,
}

/// How the story sits with a named asset's technical trend (the guard's regime, when cached).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TechnicalNote {
    pub asset: String,
    pub trend: Trend,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorySummary {
    /// Id of the merged story in `items`.
    pub id: String,
    pub title: String,
    pub link: String,
    pub source: String,
    pub time: i64,
    pub category: NewsCategory,
    pub themes: Vec<NewsTheme>,
    pub alert: bool,
    /// Distinct sources that told the story (the first one included).
    pub sources: usize,
    /// Distinct headlines among them (a headline repeated word for word is one syndicated article): the rule's count.
    pub independent_sources: usize,
    pub links: Vec<SummaryLink>,
    /// The user's assets named in any of the headlines ("crypto:BTC"…).
    pub assets: Vec<String>,
    pub impact: Impact,
    pub impact_basis: ImpactBasis,
    /// The rule's level and points, given even when the impact is measured.
    pub rule_impact: Impact,
    pub impact_points: u32,
    /// French, one line per point of the rule (and the measured move).
    pub impact_reasons: Vec<String>,
    pub consensus: SourceConsensus,
    pub moves: Vec<AssetMove>,
    pub technical: Vec<TechnicalNote>,
}

/// What the server already holds for an asset: never fetched for the summary.
#[derive(Debug, Clone, Default)]
pub struct CachedMarket {
    /// Closed hourly candles: (open time ms, close).
    pub closes: Vec<(i64, f64)>,
    pub step_ms: i64,
    pub source: String,
    pub crypto: bool,
    pub trend: Option<Trend>,
}

static HACK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?i)(?-u:\b)(hack(s|ed|er|ers)?|exploit(ed)?|drained|breach|stolen|piratage|piraté|pirates?)(?-u:\b)").expect("regex")
});

/// Independent sources: 2–3 → 1, 4+ → 2.
fn source_points(n: usize) -> u32 {
    match n {
        0 | 1 => 0,
        2 | 3 => 1,
        _ => 2,
    }
}

fn level(points: u32) -> Impact {
    match points {
        0 | 1 => Impact::Low,
        2 | 3 => Impact::Medium,
        _ => Impact::High,
    }
}

/// Measured level of a move (percent): thresholds 1 % / 3 % for a stock, 2 % / 5 % for a crypto.
pub fn measured_level(change_pct: f64, crypto: bool) -> Impact {
    let (mid, high) = if crypto { (2.0, 5.0) } else { (1.0, 3.0) };
    let a = change_pct.abs();
    if a >= high {
        Impact::High
    } else if a >= mid {
        Impact::Medium
    } else {
        Impact::Low
    }
}

/// Move since `published`: from the close of the last candle closed at or before it, to the last candle, provided a
/// full candle closed after that reference (else nothing is measurable yet).
pub fn move_since(m: &CachedMarket, asset: &str, published: i64) -> Option<AssetMove> {
    if m.step_ms <= 0 {
        return None;
    }
    let from = m.closes.iter().rev().find(|(t, _)| t + m.step_ms <= published)?;
    let to = m.closes.last()?;
    if to.0 <= from.0 || from.1.is_nan() || from.1 <= 0.0 || !to.1.is_finite() {
        return None;
    }
    Some(AssetMove {
        asset: asset.to_string(),
        from_time: from.0 + m.step_ms,
        from_price: from.1,
        to_time: to.0 + m.step_ms,
        to_price: to.1,
        change_pct: (to.1 / from.1 - 1.0) * 100.0,
        source: m.source.clone(),
    })
}

/// The technical line: the story's tone against the trend, phrased as something to watch, never as a signal.
pub fn technical_text(tone: NewsTone, trend: Trend) -> String {
    match (tone, trend) {
        (NewsTone::Negative, Trend::Up) => "Actualité négative alors que la tendance technique est haussière : à surveiller.",
        (NewsTone::Negative, Trend::Down) => {
            "Actualité négative dans une tendance technique déjà baissière : elle pourrait prolonger la baisse, sans certitude."
        }
        (NewsTone::Positive, Trend::Down) => {
            "Actualité positive alors que la tendance technique est baissière : un retournement n'est pas confirmé pour autant."
        }
        (NewsTone::Positive, Trend::Up) => {
            "Actualité positive dans une tendance technique haussière : les deux vont dans le même sens, sans garantie."
        }
        (NewsTone::Neutral, Trend::Up) => "Ton neutre ; tendance technique haussière.",
        (NewsTone::Neutral, Trend::Down) => "Ton neutre ; tendance technique baissière.",
        (_, Trend::Range) => "Tendance technique sans direction nette : l'actualité pourrait peser sur la suite, sans certitude.",
    }
    .to_string()
}

fn consensus(links: &[SummaryLink]) -> SourceConsensus {
    let count = |t: NewsTone| links.iter().filter(|l| l.tone == t).count();
    let (negative, positive, neutral) = (count(NewsTone::Negative), count(NewsTone::Positive), count(NewsTone::Neutral));
    let agreement = if links.len() < 2 {
        Agreement::Single
    } else if negative > 0 && positive > 0 {
        Agreement::Divergent
    } else {
        Agreement::Convergent
    };
    // Tone of the non-neutral headlines' majority (neutral on a tie or when none takes a side).
    let tone = match negative.cmp(&positive) {
        std::cmp::Ordering::Greater => NewsTone::Negative,
        std::cmp::Ordering::Less => NewsTone::Positive,
        std::cmp::Ordering::Equal => NewsTone::Neutral,
    };
    SourceConsensus { agreement, tone, negative, positive, neutral }
}

/// Headline compared without case, accents aside, punctuation or spacing.
fn headline_key(t: &str) -> String {
    t.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

fn short_id(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, s)| s)
}

/// The day's important events (at most [`MAX_EVENTS`]): stories of the last 24 h told by 2 independent sources at least,
/// or a serious escalation, or naming one of the user's assets with 2 points of the rule at least; the highest impact first, then the most sources, then the
/// most recent. `feeds` are the raw items `items` was aggregated from (each source's own headline and link), `market`
/// what the server has cached for an asset id ("stock:AAPL").
pub fn summarize(
    items: &[NewsItem],
    feeds: &[FeedItems],
    assets: &[WatchAsset],
    now: i64,
    market: &dyn Fn(&str) -> Option<CachedMarket>,
) -> Vec<StorySummary> {
    let matchers: Vec<(&WatchAsset, AssetMatcher)> = assets.iter().map(|a| (a, AssetMatcher::new(a))).collect();
    let raw: Vec<&RawNews> = feeds.iter().flat_map(|f| f.items.iter()).collect();
    let mut out: Vec<StorySummary> = items
        .iter()
        .filter(|i| i.time >= now - DAY_MS && i.time <= now + 600_000)
        .filter(|i| !i.also_in.is_empty() || i.alert || !i.assets.is_empty())
        .map(|i| summarize_one(i, &raw, &matchers, market))
        .filter(|s| s.independent_sources >= 2 || s.alert || (!s.assets.is_empty() && s.impact_points >= 2))
        .collect();
    out.sort_by(|a, b| {
        b.impact
            .cmp(&a.impact)
            .then(b.impact_points.cmp(&a.impact_points))
            .then(b.independent_sources.cmp(&a.independent_sources))
            .then(b.time.cmp(&a.time))
    });
    out.truncate(MAX_EVENTS);
    out
}

fn summarize_one(
    i: &NewsItem,
    raw: &[&RawNews],
    matchers: &[(&WatchAsset, AssetMatcher)],
    market: &dyn Fn(&str) -> Option<CachedMarket>,
) -> StorySummary {
    let kept = RawNews { title: i.title.clone(), link: i.link.clone(), time: i.time, source: i.source.clone(), summary: i.summary.clone() };
    // Each source's own headline: the kept one first, then the twins found the way `aggregate` merges them.
    let mut links = vec![SummaryLink { source: i.source.clone(), title: i.title.clone(), link: i.link.clone(), time: i.time, tone: i.tone }];
    let mut twins: Vec<&RawNews> = raw.iter().copied().filter(|r| r.link == kept.link || same_story(&kept, r)).collect();
    twins.sort_by_key(|r| r.time);
    for r in twins {
        if links.iter().any(|l| l.source == r.source) {
            continue;
        }
        links.push(SummaryLink { source: r.source.clone(), title: r.title.clone(), link: r.link.clone(), time: r.time, tone: tone_of(&r.title) });
    }
    let sources = links.len();
    // The same headline word for word at several outlets is one article syndicated, not independent reporting.
    let mut distinct: Vec<SummaryLink> = vec![];
    for l in &links {
        if !distinct.iter().any(|d| headline_key(&d.title) == headline_key(&l.title)) {
            distinct.push(l.clone());
        }
    }
    let independent = distinct.len();
    let mut asset_ids: Vec<String> = i.assets.clone();
    for (a, m) in matchers {
        if !asset_ids.contains(&a.id) && links.iter().any(|l| m.is_match(&l.title)) {
            asset_ids.push(a.id.clone());
        }
    }
    let consensus = consensus(&distinct);
    links.truncate(MAX_LINKS);

    let mut points = 0;
    let mut reasons = vec![];
    let sp = source_points(independent);
    points += sp;
    reasons.push(match (sources, independent) {
        (1, _) => "1 seule source (+0)".to_string(),
        (n, 1) => format!("{n} sources reprenant le même titre, comptées comme une (+0)"),
        (n, k) if n == k => format!("{n} sources indépendantes (+{sp})"),
        (n, k) => format!("{n} sources, {k} titres distincts (+{sp})"),
    });
    let hack = links.iter().any(|l| HACK.is_match(&l.title));
    if i.alert {
        points += 2;
        reasons.push("Escalade grave (+2)".into());
    } else if i.themes.contains(&NewsTheme::Monetary) {
        points += 1;
        reasons.push("Banques centrales / taux (+1)".into());
    } else if i.themes.contains(&NewsTheme::Regulation) {
        points += 1;
        reasons.push("Régulation (+1)".into());
    } else if hack {
        points += 1;
        reasons.push("Piratage (+1)".into());
    }
    if !asset_ids.is_empty() {
        points += 1;
        reasons.push(format!("Concerne {} (+1)", asset_ids.iter().map(|a| short_id(a)).collect::<Vec<_>>().join(", ")));
    }
    let rule_impact = level(points);

    let mut moves = vec![];
    let mut technical = vec![];
    let mut measured: Option<Impact> = None;
    for id in &asset_ids {
        let Some(m) = market(id) else { continue };
        if let Some(mv) = move_since(&m, id, i.time) {
            let l = measured_level(mv.change_pct, m.crypto);
            reasons.push(format!("{} {} % depuis la publication", short_id(id), format!("{:+.1}", mv.change_pct).replace('.', ",")));
            measured = Some(measured.map_or(l, |x| x.max(l)));
            moves.push(mv);
        }
        if let Some(trend) = m.trend {
            technical.push(TechnicalNote { asset: id.clone(), trend, text: technical_text(consensus.tone, trend) });
        }
    }

    StorySummary {
        id: i.id.clone(),
        title: i.title.clone(),
        link: i.link.clone(),
        source: i.source.clone(),
        time: i.time,
        category: i.category,
        themes: i.themes.clone(),
        alert: i.alert,
        sources,
        independent_sources: independent,
        links,
        assets: asset_ids,
        impact: measured.unwrap_or(rule_impact),
        impact_basis: if measured.is_some() { ImpactBasis::Measured } else { ImpactBasis::Rule },
        rule_impact,
        impact_points: points,
        impact_reasons: reasons,
        consensus,
        moves,
        technical,
    }
}

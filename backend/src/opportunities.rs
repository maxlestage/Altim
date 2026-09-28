//! "Opportunités du moment" (`/api/opportunities`): the selection's universe scanned by category on the daily
//! candles the selection already caches (`screener.rs`). Fundamentals need one request per company (SEC filings,
//! Nasdaq revisions): they are read for the most liquid stocks only, and the page says so. Cryptos' TVL comes from
//! the DefiLlama index already cached for the decision (one request for every coin).
use std::sync::Arc;

use futures::StreamExt;
use serde::Serialize;

use crate::cache::cached;
use crate::derivatives::NotCovered;
use crate::engine::opportunities::{CATEGORIES, Category, Hit, Metrics, metrics, technical_hits};
use crate::engine::screener::{CandleInterval, Horizon, is_pegged};
use crate::http::{Error, Result};
use crate::js::{fr, now_ms};
use crate::screener::{Listed, crypto_daily_for, crypto_listed, daily_for, listed};
use crate::types::{Candle, Kind};

/// Fundamentals are read for this many stocks (the most traded ones).
pub const TOP_N: usize = 30;
/// A filing counts as recent this many days after it was filed.
pub const FILING_DAYS: i64 = 45;
/// Change of the twelve-month growth from one period to the next, points (revenue, EPS).
pub const REVENUE_SHIFT: f64 = 5.0;
pub const EPS_SHIFT: f64 = 10.0;
/// Consensus EPS revised by at least this much over a month, %.
pub const REVISION_PCT: f64 = 3.0;
/// TVL change over 30 days, %, for a TVL of at least TVL_MIN USD.
pub const TVL_PCT: f64 = 20.0;
pub const TVL_MIN: f64 = 10_000_000.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    pub id: Category,
    pub label: &'static str,
    /// The rule, in words, with its thresholds.
    pub rule: String,
    /// How many assets were analysed for this category.
    pub analyzed: usize,
    /// "analysé sur les 30 plus liquides" and similar limits; None when the whole universe was read.
    pub note: Option<String>,
    /// Why the category is empty for a reason other than "no hit" (source down).
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub symbol: String,
    pub name: String,
    pub sector: String,
    /// Stocks: Nasdaq market cap (USD). Cryptos: None (the rank is given instead).
    pub market_cap: Option<f64>,
    /// Cryptos: CoinGecko market-cap rank.
    pub rank: Option<i64>,
    #[serde(flatten)]
    pub metrics: Metrics,
    pub hits: Vec<Hit>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityReport {
    pub kind: Kind,
    pub as_of: i64,
    pub scanned: usize,
    pub universe: String,
    pub top_n: usize,
    pub categories: Vec<CategoryInfo>,
    /// Assets with at least one hit, the most hits first.
    pub items: Vec<Item>,
    pub not_covered: Vec<NotCovered>,
    pub source: String,
}

fn rule(c: Category, kind: Kind) -> String {
    match c {
        Category::Setup => "La sélection d'Altim (horizon 1 mois) : classée sur ce qui a le mieux marché par le passé, finalistes vérifiés.".into(),
        Category::Reversal => "RSI 14 remonté au-dessus de 30 dans les 5 dernières séances, avec une divergence haussière RSI ou une clôture repassée au-dessus de la moyenne 20 ou 50 j.".into(),
        Category::Breakout => "Clôture au-dessus du plus haut des 20 ou 55 séances précédentes, volume ≥ 1,5 × la moyenne 20 j.".into(),
        Category::Volume => "Volume de la dernière séance ≥ 3 × la moyenne des 20 précédentes.".into(),
        Category::Oversold => "RSI 14 journalier < 25, ou clôture ≥ 2,5 ATR sous la moyenne 20 j.".into(),
        Category::Fundamentals => match kind {
            Kind::Stock => format!(
                "Dépôt 10-Q / 10-K à la SEC depuis moins de {FILING_DAYS} j avec une croissance sur 12 mois qui change d'au moins {} points (CA) ou {} points (BPA), ou consensus BPA révisé d'au moins {} % en un mois (Nasdaq, données Zacks).",
                fr(REVENUE_SHIFT, 0, 0),
                fr(EPS_SHIFT, 0, 0),
                fr(REVISION_PCT, 0, 0)
            ),
            Kind::Crypto => format!(
                "TVL (valeur déposée dans les protocoles du jeton, DefiLlama) en hausse ou en baisse d'au moins {} % sur 30 j, pour une TVL ≥ 10 M$.",
                fr(TVL_PCT, 0, 0)
            ),
        },
    }
}

/// "+49 %", "−75 %".
fn sg(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}

/// The stock hit of a filing trend and revisions (None: nothing notable). `today` in days since 1970-01-01.
pub fn stock_fundamental_hit(
    t: Option<&crate::fundamentals::FilingTrend>,
    r: Option<&crate::engine::decision_types::Revisions>,
    today: i64,
) -> Option<Hit> {
    let mut why = Vec::new();
    let mut strength = 0.0;
    if let Some(t) = t.filter(|t| today - t.filed <= FILING_DAYS && t.filed <= today) {
        let mut shift = |label: &str, now: Option<f64>, before: Option<f64>, min: f64| {
            if let (Some(n), Some(b)) = (now, before) {
                if (n - b).abs() >= min {
                    why.push(format!("{label} {} sur 12 mois (contre {} à la période précédente)", sg(n), sg(b)));
                    strength += (n - b).abs();
                }
            }
        };
        shift("croissance du CA", t.revenue_growth, t.revenue_growth_before, REVENUE_SHIFT);
        shift("croissance du BPA", t.eps_growth, t.eps_growth_before, EPS_SHIFT);
        if !why.is_empty() {
            why.insert(0, format!("{} déposé le {}", t.form, crate::fundamentals::day_label(t.filed)));
        }
    }
    if let Some(r) = r.filter(|r| r.change_pct.abs() >= REVISION_PCT) {
        why.push(format!(
            "consensus BPA de l'exercice révisé de {}{} % en un mois ({} → {})",
            if r.change_pct >= 0.0 { "+" } else { "−" },
            fr(r.change_pct.abs(), 0, 1),
            fr(r.month_ago, 2, 2),
            fr(r.now, 2, 2)
        ));
        strength += r.change_pct.abs();
    }
    (!why.is_empty()).then(|| Hit { category: Category::Fundamentals, reason: why.join(" ; "), strength })
}

/// The crypto hit of a TVL change over 30 days.
pub fn tvl_hit(t: &crate::tokenomics::TvlMonth) -> Option<Hit> {
    let (tvl, month_ago) = (t.tvl, t.month_ago);
    if tvl < TVL_MIN || month_ago <= 0.0 {
        return None;
    }
    let d = (tvl / month_ago - 1.0) * 100.0;
    (d.abs() >= TVL_PCT).then(|| Hit {
        category: Category::Fundamentals,
        reason: format!(
            "TVL {}{} % sur 30 j ({} M$ → {} M$ ; protocoles DefiLlama portant le jeton : {})",
            if d >= 0.0 { "+" } else { "−" },
            fr(d.abs(), 0, 0),
            fr(month_ago / 1e6, 0, 0),
            fr(tvl / 1e6, 0, 0),
            t.protocols.join(", ")
        ),
        strength: d.abs(),
    })
}

struct Row {
    s: Listed,
    rank: Option<i64>,
    m: Metrics,
    hits: Vec<Hit>,
}

async fn scan(kind: Kind) -> Result<OpportunityReport> {
    let crypto = kind == Kind::Crypto;
    let list = if crypto { crypto_listed().await? } else { listed().await? };
    let universe: Vec<Listed> = list.iter().take(if crypto { 120 } else { 150 }).cloned().collect();
    let ranks = if crypto { crate::universe::crypto_universe().await.ok() } else { None };
    // Same candles (and cache keys) as the selection's daily horizons.
    let symbols: Vec<String> = universe.iter().map(|s| s.symbol.clone()).collect();
    let candles: Vec<Option<Arc<Vec<Candle>>>> = futures::stream::iter(
        symbols.into_iter().map(|sym| async move { if crypto { crypto_daily_for(&sym).await.ok() } else { daily_for(&sym).await.ok() } }),
    )
    .buffered(10)
    .collect()
    .await;
    let mut rows: Vec<Row> = universe
        .iter()
        .zip(candles)
        .filter_map(|(s, c)| {
            let c = c?;
            let m = metrics(&c)?;
            if crypto && is_pegged(m.volatility, CandleInterval::D1) {
                return None;
            }
            let rank = ranks.as_ref().and_then(|u| u.iter().find(|e| e.0 == s.symbol)).map(|e| e.2).filter(|r| *r > 0);
            Some(Row { s: s.clone(), rank, m, hits: technical_hits(&c) })
        })
        .collect();
    let scanned = rows.len();
    // Every candle source failing (throttled, network): an error, not an empty scan kept for 30 minutes.
    if scanned == 0 {
        return Err(Error("aucune bougie journalière disponible pour le moment (sources injoignables)".into()));
    }

    // Configurations: the selection (cached, warmed every 25 minutes).
    let mut infos: Vec<CategoryInfo> = CATEGORIES
        .iter()
        .map(|c| CategoryInfo { id: *c, label: c.label(), rule: rule(*c, kind), analyzed: scanned, note: None, error: None })
        .collect();
    match crate::app::data::selection(Horizon::Mo1, kind).await {
        Ok(sel) => {
            for c in &sel.buy {
                if let Some(r) = rows.iter_mut().find(|r| r.s.symbol == c.symbol) {
                    r.hits.insert(
                        0,
                        Hit {
                            category: Category::Setup,
                            reason: format!("n° {} de la sélection d'Altim (1 mois) ; {}", c.rank, c.why.get(sel.rank_by)),
                            strength: 100.0 - c.rank as f64,
                        },
                    );
                }
            }
            infos[0].analyzed = sel.scanned;
        }
        Err(e) => infos[0].error = Some(format!("sélection indisponible : {e}")),
    }

    // Fundamentals.
    let fi = CATEGORIES.iter().position(|c| *c == Category::Fundamentals).unwrap();
    if crypto {
        match crate::tokenomics::tvl_month().await {
            Ok(tvl) => {
                let mut found = 0;
                for r in rows.iter_mut() {
                    if let Some(t) = tvl.iter().find(|t| t.symbol == r.s.symbol) {
                        found += 1;
                        r.hits.extend(tvl_hit(t));
                    }
                }
                infos[fi].analyzed = found;
                infos[fi].note = Some(format!(
                    "{found} cryptos sur {scanned} ont une TVL de protocole suivie par DefiLlama ; pour les blockchains elles-mêmes (ETH, SOL…) la TVL d'il y a 30 j n'est pas dans l'index gratuit lu : non couvert."
                ));
            }
            Err(e) => infos[fi].error = Some(format!("DefiLlama indisponible : {e}")),
        }
    } else {
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|a, b| rows[*b].m.liquidity.unwrap_or(0.0).total_cmp(&rows[*a].m.liquidity.unwrap_or(0.0)));
        order.truncate(TOP_N);
        let today = now_ms().div_euclid(86_400_000);
        let wanted: Vec<(usize, String)> = order.iter().map(|&i| (i, rows[i].s.symbol.clone())).collect();
        let found: Vec<(usize, Option<Hit>, bool)> = futures::stream::iter(wanted.into_iter().map(|(i, sym)| async move {
            let (t, r) = tokio::join!(crate::fundamentals::filing_trend_for(&sym), crate::fundamentals::revisions_for(&sym));
            let answered = t.is_ok() || r.is_ok();
            let (t, r) = (t.ok().flatten(), r.ok().flatten());
            (i, stock_fundamental_hit(t.as_ref(), r.as_ref(), today), answered)
        }))
        .buffered(4)
        .collect()
        .await;
        let answered = found.iter().filter(|x| x.2).count();
        for (i, hit, _) in found {
            rows[i].hits.extend(hit);
        }
        infos[fi].analyzed = answered;
        infos[fi].note =
            Some(format!("analysé sur les {} plus liquides (volume échangé moyen sur 20 j) : une requête SEC et Nasdaq par société.", order.len()));
        if answered == 0 && !order.is_empty() {
            infos[fi].error = Some("SEC EDGAR et Nasdaq n'ont pas répondu".into());
        }
    }

    let mut items: Vec<Item> = rows
        .into_iter()
        .filter(|r| !r.hits.is_empty())
        .map(|r| Item {
            symbol: r.s.symbol.clone(),
            name: r.s.name.clone(),
            sector: if crypto { "Crypto".into() } else { crate::screener::sector_fr(&r.s.sector) },
            market_cap: (r.s.market_cap > 0.0).then_some(r.s.market_cap),
            rank: r.rank,
            metrics: r.m,
            hits: r.hits,
        })
        .collect();
    items.sort_by(|a, b| {
        let s = |i: &Item| i.hits.iter().map(|h| h.strength).fold(0.0, f64::max);
        b.hits.len().cmp(&a.hits.len()).then(s(b).total_cmp(&s(a)))
    });
    let mut not_covered = vec![NotCovered {
        label: "Révisions d'analystes et dépôts SEC au-delà des 30 plus liquides".into(),
        reason: "une requête par société : lu sur les 30 plus liquides seulement pour garder le scan rapide".into(),
    }];
    if crypto {
        not_covered = vec![NotCovered {
            label: "TVL 30 j des blockchains (ETH, SOL…)".into(),
            reason: "l'index gratuit de DefiLlama lu ici ne donne la TVL d'il y a un mois que pour les protocoles".into(),
        }];
    }
    Ok(OpportunityReport {
        kind,
        as_of: now_ms(),
        scanned,
        universe: if crypto {
            "Les 120 plus grandes cryptos (hors stablecoins et jetons adossés)".into()
        } else {
            "Les 150 plus grandes sociétés cotées aux États-Unis (Nasdaq)".into()
        },
        top_n: TOP_N,
        categories: infos,
        items,
        not_covered,
        source: if crypto {
            "Bougies journalières Gate.io (repli MEXC, Kraken) ; TVL DefiLlama ; sélection d'Altim".into()
        } else {
            "Bougies journalières Finviz (repli Yahoo) ; SEC EDGAR ; Nasdaq (données Zacks) ; sélection d'Altim".into()
        },
    })
}

/// Cached 30 minutes (the daily candles change once a day; the selection every 25 minutes).
pub async fn opportunities(kind: Kind) -> Result<Arc<OpportunityReport>> {
    cached(&format!("opportunities:{}", kind.as_str()), 30 * 60_000, move || scan(kind)).await
}

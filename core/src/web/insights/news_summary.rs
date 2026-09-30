//! "Résumé intelligent" of the news (`summary` of /api/news, computed by the server in `engine::news_summary`): the
//! pure helpers of the card (`web/src/webapp/news-summary.ts`), over the server's own `StorySummary`.
use crate::engine::news::NewsTone;
use crate::engine::news_summary::{Agreement, AssetMove, Impact, ImpactBasis, StorySummary};

/// `IMPACT_LABEL`.
pub fn impact_label(i: Impact) -> &'static str {
    match i {
        Impact::Low => "faible",
        Impact::Medium => "moyen",
        Impact::High => "important",
    }
}

/// CSS class of an impact ("low", "medium", "high").
pub fn impact_id(i: Impact) -> &'static str {
    match i {
        Impact::Low => "low",
        Impact::Medium => "medium",
        Impact::High => "high",
    }
}

pub fn basis_label(s: &StorySummary) -> &'static str {
    if s.impact_basis == ImpactBasis::Measured { "impact mesuré" } else { "impact estimé par règle" }
}

/// Counts the events of moyen or important impact; the faible ones are "autres sujets": (title, others).
pub fn summary_heading(list: &[StorySummary]) -> (String, Option<String>) {
    let n = list.iter().filter(|s| s.impact != Impact::Low).count();
    let rest = list.len() - n;
    let title = match n {
        0 => "Aucun événement important aujourd'hui".to_string(),
        1 => "1 événement important aujourd'hui".to_string(),
        n => format!("{n} événements importants aujourd'hui"),
    };
    let others = (rest > 0).then(|| {
        format!("{}{rest} sujet{} repris par plusieurs sources, à impact faible.", if n == 0 { "" } else { "Et " }, if rest > 1 { "s" } else { "" })
    });
    (title, others)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

/// "Convergent · 3 sources · ton des titres : 2 négatifs, 1 neutre" (tones counted over the distinct headlines).
pub fn consensus_text(s: &StorySummary) -> String {
    let c = &s.consensus;
    if c.agreement == Agreement::Single {
        return if s.sources > 1 {
            format!("Même titre repris par {} sources : pas de consensus mesurable", s.sources)
        } else {
            "Une seule source : pas de consensus mesurable".into()
        };
    }
    let parts: Vec<String> = [(c.negative, "négatif", "négatifs"), (c.positive, "positif", "positifs"), (c.neutral, "neutre", "neutres")]
        .iter()
        .filter(|(n, _, _)| *n > 0)
        .map(|(n, one, many)| plural(*n, one, many))
        .collect();
    let titles = c.negative + c.positive + c.neutral;
    let head =
        format!("{} · {}", if c.agreement == Agreement::Convergent { "Convergent" } else { "Divergent" }, plural(s.sources, "source", "sources"));
    let distinct = if titles < s.sources { format!(" ({titles} titres distincts)") } else { String::new() };
    format!("{head}{distinct} · ton des titres : {}", parts.join(", "))
}

/// "stock:AAPL" → (kind, symbol, href).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetLink {
    pub kind: String,
    pub symbol: String,
    pub href: String,
}

pub fn asset_link(id: &str) -> AssetLink {
    let (kind, symbol) = match id.split_once(':') {
        // `id.split(":")` then the first two parts, like the TypeScript destructuring.
        Some((k, rest)) => (k.to_string(), rest.split(':').next().unwrap_or("").to_string()),
        None => (String::new(), id.to_string()),
    };
    let href = format!("/app/actif/{kind}/{}", crate::web::bot::encode_uri_component(&symbol));
    AssetLink { kind, symbol, href }
}

/// "AAPL +4,0 % depuis la publication".
pub fn move_text(m: &AssetMove) -> String {
    let pct = format!("{}{} %", if m.change_pct >= 0.0 { "+" } else { "−" }, crate::js::to_fixed(m.change_pct.abs(), 1).replace('.', ","));
    format!("{} {pct} depuis la publication", asset_link(&m.asset).symbol)
}

/// "▼", "▲", "·" of a headline's tone (`TONE_MARK`), and its CSS class.
pub fn tone_mark(t: NewsTone) -> (&'static str, &'static str, &'static str) {
    match t {
        NewsTone::Negative => ("▼", "down", "negative"),
        NewsTone::Positive => ("▲", "up", "positive"),
        NewsTone::Neutral => ("·", "muted", "neutral"),
    }
}

/// The rule's line of "Sources et calcul": the points, the level, and the measured move when it decided.
pub fn rule_text(s: &StorySummary) -> String {
    let rule: Vec<&str> = s.impact_reasons.iter().filter(|r| r.contains("(+")).map(String::as_str).collect();
    let measured: Vec<&str> = s.impact_reasons.iter().filter(|r| !r.contains("(+")).map(String::as_str).collect();
    let tail = if s.impact_basis == ImpactBasis::Measured {
        format!(" ; retenu : {}, d'après la variation mesurée ({}).", impact_label(s.impact), measured.join(" ; "))
    } else {
        ".".into()
    };
    format!(
        "Règle : {} → {} point{}, impact {} par règle{tail}",
        rule.join(" ; "),
        s.impact_points,
        if s.impact_points > 1 { "s" } else { "" },
        impact_label(s.rule_impact)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::news_summary::SourceConsensus;

    // A real /api/news `summary` (28/09/2026, radar AAPL, BTC, NVDA; guard and hourly candles cached for them).
    fn list() -> Vec<StorySummary> {
        let v: serde_json::Value = serde_json::from_str(include_str!("samples/news-summary-sample.json")).unwrap();
        serde_json::from_value(v["summary"].clone()).unwrap()
    }

    #[test]
    fn real_sample_at_most_5_events_each_complete() {
        let list = list();
        assert!(!list.is_empty() && list.len() <= 5);
        for s in &list {
            assert_eq!(s.links[0].link, s.link);
            assert!(s.independent_sources <= s.sources);
            // Measured only with a move, and every move's asset is one of the story's.
            assert_eq!(s.impact_basis == ImpactBasis::Measured, !s.moves.is_empty());
            for m in &s.moves {
                assert!(s.assets.contains(&m.asset));
            }
            for t in &s.technical {
                assert!(s.assets.contains(&t.asset));
            }
        }
    }

    #[test]
    fn labels() {
        let list = list();
        let at = |impact: Impact| StorySummary { impact, ..list[0].clone() };
        assert_eq!(summary_heading(&[]), ("Aucun événement important aujourd'hui".into(), None));
        assert_eq!(
            summary_heading(&[at(Impact::Low), at(Impact::Low)]),
            ("Aucun événement important aujourd'hui".into(), Some("2 sujets repris par plusieurs sources, à impact faible.".into()))
        );
        assert_eq!(
            summary_heading(&[at(Impact::High), at(Impact::Low)]),
            ("1 événement important aujourd'hui".into(), Some("Et 1 sujet repris par plusieurs sources, à impact faible.".into()))
        );
        assert_eq!(summary_heading(&[at(Impact::High), at(Impact::Medium)]).0, "2 événements importants aujourd'hui");
        assert_eq!(impact_label(Impact::High), "important");
        let nvda = list.iter().find(|s| s.assets.iter().any(|a| a == "stock:NVDA")).unwrap();
        assert_eq!(basis_label(nvda), "impact mesuré");
        assert_eq!(consensus_text(nvda), "Convergent · 2 sources · ton des titres : 2 positifs");
        let m = AssetMove { change_pct: -0.4220148770699983, ..nvda.moves[0].clone() };
        assert_eq!(move_text(&m), "NVDA −0,4 % depuis la publication");
        assert_eq!(asset_link("stock:BRK-B"), AssetLink { kind: "stock".into(), symbol: "BRK-B".into(), href: "/app/actif/stock/BRK-B".into() });
        assert!(rule_text(nvda).starts_with("Règle : "));
        assert!(rule_text(nvda).contains("d'après la variation mesurée"));
    }

    #[test]
    fn consensus_divergent_single_repeated() {
        let base = list()[0].clone();
        let divergent = StorySummary {
            sources: 3,
            consensus: SourceConsensus { agreement: Agreement::Divergent, tone: NewsTone::Negative, negative: 2, positive: 1, neutral: 0 },
            ..base.clone()
        };
        assert_eq!(consensus_text(&divergent), "Divergent · 3 sources · ton des titres : 2 négatifs, 1 positif");
        let single =
            StorySummary { sources: 1, consensus: SourceConsensus { agreement: Agreement::Single, ..base.consensus.clone() }, ..base.clone() };
        assert_eq!(consensus_text(&single), "Une seule source : pas de consensus mesurable");
        assert_eq!(consensus_text(&StorySummary { sources: 4, ..single }), "Même titre repris par 4 sources : pas de consensus mesurable");
        let partly = StorySummary { sources: 4, ..divergent };
        assert_eq!(consensus_text(&partly), "Divergent · 4 sources (3 titres distincts) · ton des titres : 2 négatifs, 1 positif");
    }
}

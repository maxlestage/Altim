//! The news screen (`News.tsx`), the « Point du jour » (`BriefCard.tsx`) and the price quotes of the alerts: the JSON
//! contracts of /api/news, /api/brief and /api/tickers?symbols= around the server's own `NewsItem`, `NewsDigest` and
//! `StorySummary`, and the small pure helpers of the views (filters, "il y a 5 min").
use serde::{Deserialize, Serialize};

use crate::engine::brief::{MarketLevel, Mover};
use crate::engine::news::{NewsCategory, NewsDigest, NewsItem};
use crate::engine::news_summary::StorySummary;
use crate::types::Kind;

pub const FILTER_KEY: &str = "altim.news.filter";
pub const VIEW_KEY: &str = "altim.news.view";

/// A feed of /api/news and how many items it gave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceCount {
    pub name: String,
    pub ok: bool,
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub error: Option<String>,
}

/// `GET /api/news`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsReport {
    pub as_of: f64,
    #[serde(default)]
    pub items: Vec<NewsItem>,
    #[serde(default)]
    pub top: Vec<String>,
    pub digest: NewsDigest,
    #[serde(default)]
    pub sources: Vec<SourceCount>,
    /// The day's important events (at most 5); absent from an older server.
    #[serde(default)]
    pub summary: Option<Vec<StorySummary>>,
}

/// `GET /api/news` read for its items only (the alert checks, in every page of the app): the same required fields
/// as `NewsReport`, without reading the digest and the summaries, whose readers then stay out of the pages that do
/// not show them.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItems {
    #[allow(dead_code)]
    as_of: serde::de::IgnoredAny,
    #[allow(dead_code)]
    digest: serde::de::IgnoredAny,
    #[serde(default)]
    pub items: Vec<NewsItem>,
}

impl NewsReport {
    /// "À la une": the top stories found among the items.
    pub fn top_items(&self) -> Vec<&NewsItem> {
        self.top.iter().filter_map(|id| self.items.iter().find(|n| &n.id == id)).collect()
    }

    pub fn sources_up(&self) -> usize {
        self.sources.iter().filter(|s| s.ok).count()
    }

    /// Items of the chosen rubric ("all" or a category), French only when asked.
    pub fn shown(&self, filter: Option<NewsCategory>, french_only: bool) -> Vec<&NewsItem> {
        self.items.iter().filter(|n| filter.is_none_or(|f| n.category == f) && (!french_only || n.lang == crate::engine::news::Lang::Fr)).collect()
    }
}

/// The rubrics of the list: (saved value, label).
pub const FILTERS: [(&str, &str); 5] = [("all", "Tout"), ("actifs", "Mes actifs"), ("monde", "Monde"), ("marches", "Marchés"), ("crypto", "Crypto")];

/// The saved rubric: None = "all" (anything unknown too).
pub fn parse_filter(s: &str) -> Option<NewsCategory> {
    match s {
        "actifs" => Some(NewsCategory::Actifs),
        "monde" => Some(NewsCategory::Monde),
        "marches" => Some(NewsCategory::Marches),
        "crypto" => Some(NewsCategory::Crypto),
        _ => None,
    }
}

/// "à l'instant", "il y a 5 min", "il y a 3 h", "il y a 2 j".
pub fn ago(ms: f64, now: f64) -> String {
    let m = crate::js::round((now - ms) / 60_000.0).max(0.0);
    if m < 1.0 {
        return "à l'instant".into();
    }
    if m < 60.0 {
        return format!("il y a {m} min");
    }
    let h = crate::js::round(m / 60.0);
    if h < 24.0 { format!("il y a {h} h") } else { format!("il y a {} j", crate::js::round(h / 24.0)) }
}

/// "Point du jour" market climate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefMarket {
    pub level: MarketLevel,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub themes: Vec<String>,
}

/// `GET /api/brief`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BriefReport {
    pub as_of: f64,
    pub headline: String,
    #[serde(default)]
    pub market: Option<BriefMarket>,
    #[serde(default)]
    pub movers: Vec<Mover>,
    #[serde(default)]
    pub news: Vec<NewsItem>,
}

impl BriefReport {
    /// The moves worth showing: 0.05 % or more, four at most.
    pub fn moves(&self) -> Vec<&Mover> {
        self.movers.iter().filter(|m| m.change.abs() >= 0.05).take(4).collect()
    }
}

/// "+1,2 %", "−0,4 %" (BriefCard).
pub fn brief_pct(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, crate::js::fr(v.abs(), 0, 1))
}

/// A consensus quote of /api/tickers?symbols= (the alerts' prices).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    pub price: f64,
    #[serde(default)]
    pub change: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ago_and_filters() {
        let now = 1_000_000_000.0;
        assert_eq!(ago(now - 20_000.0, now), "à l'instant");
        assert_eq!(ago(now + 60_000.0, now), "à l'instant");
        assert_eq!(ago(now - 5.0 * 60_000.0, now), "il y a 5 min");
        assert_eq!(ago(now - 3.0 * 3_600_000.0, now), "il y a 3 h");
        assert_eq!(ago(now - 50.0 * 3_600_000.0, now), "il y a 2 j");
        assert_eq!(parse_filter("monde"), Some(NewsCategory::Monde));
        assert_eq!(parse_filter("all"), None);
        assert_eq!(brief_pct(-0.42), "−0,4 %");
        assert_eq!(brief_pct(1.25), "+1,3 %");
    }

    #[test]
    fn reads_a_news_report_without_summary() {
        let r: NewsReport = serde_json::from_value(serde_json::json!({
            "asOf": 1, "items": [], "top": ["x"], "digest": { "total": 0, "themes": [], "tone": { "negative": 0, "positive": 0, "neutral": 0 } },
            "sources": [{ "name": "Reuters", "ok": false, "count": 0, "error": "HTTP 503" }]
        }))
        .unwrap();
        assert!(r.summary.is_none() && r.top_items().is_empty());
        assert_eq!(r.sources_up(), 0);
        let b: BriefReport = serde_json::from_value(serde_json::json!({
            "asOf": 1, "headline": "Contexte calme", "market": { "level": "calm", "label": "Contexte calme", "score": 12, "themes": [] }, "buyable": [],
            "movers": [{ "symbol": "BTC", "kind": "crypto", "price": 1, "change": 0.01 }, { "symbol": "ETH", "kind": "crypto", "price": 1, "change": -2 }], "news": []
        }))
        .unwrap();
        assert_eq!(b.moves().iter().map(|m| m.symbol.as_str()).collect::<Vec<_>>(), ["ETH"]);
    }

    /// The alert checks' reading of /api/news: the same items as the full report, the same required fields.
    #[test]
    fn news_items_only() {
        let item = NewsItem {
            id: "a".into(),
            title: "Bitcoin".into(),
            link: "https://example.org/a".into(),
            time: 5,
            source: "Reuters".into(),
            summary: None,
            lang: crate::engine::news::Lang::Fr,
            category: NewsCategory::Crypto,
            themes: vec![],
            tone: crate::engine::news::NewsTone::Neutral,
            assets: vec!["crypto:BTC".into()],
            also_in: vec![],
            alert: true,
        };
        let body = serde_json::json!({
            "asOf": 1, "items": [item], "top": ["a"], "digest": { "total": 1, "themes": [], "tone": { "negative": 0, "positive": 0, "neutral": 1 } },
            "sources": [{ "name": "Reuters", "ok": true, "count": 1 }]
        })
        .to_string();
        let full: NewsReport = serde_json::from_str(&body).unwrap();
        let light: NewsItems = serde_json::from_str(&body).unwrap();
        assert_eq!(light.items, full.items);
        assert_eq!(light.items[0].assets, ["crypto:BTC"]);
        assert!(serde_json::from_str::<NewsItems>(r#"{"asOf":1,"items":[]}"#).is_err());
        assert!(serde_json::from_str::<NewsItems>(r#"{"asOf":1,"digest":{}}"#).unwrap().items.is_empty());
    }
}

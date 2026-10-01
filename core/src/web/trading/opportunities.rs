//! Client part of "Opportunités du moment" (web/src/engine/opportunities.ts, Opportunities.tsx): the
//! `/api/opportunities` report as the screen reads it, the numeric and category filters, and the saved choices
//! ("altim.opportunities.v1"). The scan itself is the server's (`engine::opportunities`, backend/src/opportunities.rs).
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::js::fr;
use crate::types::Kind;
use crate::web::money::MoneyDisplay;
use crate::web::sorting::Sorting;

pub const OPPORTUNITIES_KEY: &str = "altim.opportunities.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OppCategory {
    Setup,
    Reversal,
    Breakout,
    Volume,
    Oversold,
    Fundamentals,
    /// A category added by a newer server: shown without a chip label, never filtered on.
    #[serde(other)]
    Other,
}

pub const OPP_CATEGORIES: [OppCategory; 6] =
    [OppCategory::Setup, OppCategory::Reversal, OppCategory::Breakout, OppCategory::Volume, OppCategory::Oversold, OppCategory::Fundamentals];

impl OppCategory {
    /// `OPP_SHORT[c]`.
    pub fn short(self) -> &'static str {
        match self {
            OppCategory::Setup => "Configurations",
            OppCategory::Reversal => "Retournements",
            OppCategory::Breakout => "Cassures",
            OppCategory::Volume => "Volume anormal",
            OppCategory::Oversold => "Survendus",
            OppCategory::Fundamentals => "Fondamentaux",
            OppCategory::Other => "",
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            OppCategory::Setup => "setup",
            OppCategory::Reversal => "reversal",
            OppCategory::Breakout => "breakout",
            OppCategory::Volume => "volume",
            OppCategory::Oversold => "oversold",
            OppCategory::Fundamentals => "fundamentals",
            OppCategory::Other => "other",
        }
    }
    fn parse(s: &str) -> Option<OppCategory> {
        OPP_CATEGORIES.into_iter().find(|c| c.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OppHit {
    pub category: OppCategory,
    pub reason: String,
    #[serde(default)]
    pub strength: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OppItem {
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub sector: String,
    /// Stocks: Nasdaq market cap, USD. Cryptos: None (rank instead).
    #[serde(default)]
    pub market_cap: Option<f64>,
    /// Cryptos: CoinGecko market-cap rank.
    #[serde(default)]
    pub rank: Option<f64>,
    pub price: f64,
    #[serde(default)]
    pub time: f64,
    #[serde(default)]
    pub change1d: Option<f64>,
    #[serde(default)]
    pub rsi14: Option<f64>,
    #[serde(default)]
    pub volume_ratio: Option<f64>,
    /// ATR(14) ÷ price, % per day.
    #[serde(default)]
    pub volatility: Option<f64>,
    /// Average daily traded value over 20 sessions, USD.
    #[serde(default)]
    pub liquidity: Option<f64>,
    #[serde(default)]
    pub distance_atr: Option<f64>,
    pub hits: Vec<OppHit>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OppCategoryInfo {
    pub id: OppCategory,
    pub label: String,
    pub rule: String,
    #[serde(default)]
    pub analyzed: usize,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NotCovered {
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpportunityReport {
    pub kind: Kind,
    pub as_of: f64,
    pub scanned: usize,
    #[serde(default)]
    pub universe: String,
    #[serde(default)]
    pub top_n: usize,
    pub categories: Vec<OppCategoryInfo>,
    pub items: Vec<OppItem>,
    #[serde(default)]
    pub not_covered: Vec<NotCovered>,
    #[serde(default)]
    pub source: String,
}

/// `/api/opportunities?kind=…`.
pub fn opportunities_url(market: Kind) -> String {
    format!("/api/opportunities?kind={}", market.as_str())
}

#[derive(Debug, Clone, PartialEq)]
pub struct OppFilters {
    pub categories: Vec<OppCategory>,
    /// Stocks: minimum market cap (USD).
    pub min_cap: Option<f64>,
    /// Cryptos: best rank allowed (top N by market cap).
    pub max_rank: Option<f64>,
    /// Minimum average daily traded value (USD).
    pub min_liquidity: Option<f64>,
    /// Maximum daily volatility (ATR %).
    pub max_volatility: Option<f64>,
}

impl Default for OppFilters {
    /// `DEFAULT_FILTERS`: every category, no threshold.
    fn default() -> Self {
        OppFilters { categories: OPP_CATEGORIES.to_vec(), min_cap: None, max_rank: None, min_liquidity: None, max_volatility: None }
    }
}

/// Whether an asset passes the numeric filters (an unknown value fails a filter that is set).
pub fn passes(item: &OppItem, f: &OppFilters) -> bool {
    let at_least = |v: Option<f64>, min: Option<f64>| min.is_none_or(|m| v.is_some_and(|v| v >= m));
    let at_most = |v: Option<f64>, max: Option<f64>| max.is_none_or(|m| v.is_some_and(|v| v <= m));
    at_least(item.market_cap, f.min_cap)
        && at_most(item.rank, f.max_rank)
        && at_least(item.liquidity, f.min_liquidity)
        && at_most(item.volatility, f.max_volatility)
}

/// Assets that pass the filters, with only the hits of the chosen categories; the most hits (then strongest) first.
pub fn filter_items(items: &[OppItem], f: &OppFilters) -> Vec<OppItem> {
    let best = |i: &OppItem| i.hits.iter().fold(0.0_f64, |m, h| super::js_max(m, h.strength));
    let mut out: Vec<OppItem> = items
        .iter()
        .filter(|i| passes(i, f))
        .map(|i| OppItem { hits: i.hits.iter().filter(|h| f.categories.contains(&h.category)).cloned().collect(), ..i.clone() })
        .filter(|i| !i.hits.is_empty())
        .collect();
    out.sort_by_dyn(|a, b| b.hits.len().cmp(&a.hits.len()).then_with(|| best(b).partial_cmp(&best(a)).unwrap_or(std::cmp::Ordering::Equal)));
    out
}

/// Number of assets per category once the numeric filters are applied (for the chips), in `OPP_CATEGORIES` order.
pub fn count_by_category(items: &[OppItem], f: &OppFilters) -> [usize; 6] {
    let mut out = [0; 6];
    for i in items.iter().filter(|i| passes(i, f)) {
        for (k, c) in OPP_CATEGORIES.iter().enumerate() {
            if i.hits.iter().any(|h| h.category == *c) {
                out[k] += 1;
            }
        }
    }
    out
}

/// "14,4 M$", "3,1 Md€", "843 k$", "420 $" (display currency, plain space).
pub fn compact_usd(d: &MoneyDisplay, usd: f64) -> String {
    let v = d.to_display(usd);
    let s = d.symbol();
    let a = v.abs();
    if a >= 1e9 {
        format!("{} Md{s}", fr(v / 1e9, 0, 1))
    } else if a >= 1e6 {
        format!("{} M{s}", fr(v / 1e6, 0, 1))
    } else if a >= 1e3 {
        format!("{} k{s}", fr(v / 1e3, 0, 0))
    } else {
        format!("{} {s}", fr(v, 0, 0))
    }
}

/// Saved market and filters (`readSaved`): anything unreadable gives stocks and every category.
pub fn read_saved(raw: Option<&str>) -> (Kind, OppFilters) {
    let fallback = (Kind::Stock, OppFilters::default());
    let Some(v) = raw.and_then(|r| serde_json::from_str::<Value>(r).ok()) else { return fallback };
    let market = match v.get("market").and_then(Value::as_str) {
        Some("stock") => Kind::Stock,
        Some("crypto") => Kind::Crypto,
        _ => return fallback,
    };
    let Some(f) = v.get("filters").filter(|f| f.is_object()) else { return fallback };
    let Some(cats) = f.get("categories").and_then(Value::as_array) else { return fallback };
    let num = |k: &str| f.get(k).and_then(Value::as_f64);
    let filters = OppFilters {
        categories: cats.iter().filter_map(Value::as_str).filter_map(OppCategory::parse).collect(),
        min_cap: num("minCap"),
        max_rank: num("maxRank"),
        min_liquidity: num("minLiquidity"),
        max_volatility: num("maxVolatility"),
    };
    (market, filters)
}

/// `JSON.stringify({ market, filters })` with the TypeScript key order.
pub fn saved_json(market: Kind, f: &OppFilters) -> String {
    let n = |v: Option<f64>| crate::js::to_value(&v);
    json!({
        "market": market.as_str(),
        "filters": {
            "categories": f.categories.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
            "minCap": n(f.min_cap),
            "maxRank": n(f.max_rank),
            "minLiquidity": n(f.min_liquidity),
            "maxVolatility": n(f.max_volatility),
        },
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(symbol: &str, over: Value) -> OppItem {
        let mut v = json!({
            "symbol": symbol, "name": symbol, "sector": "Technologie", "marketCap": 1e11, "rank": null, "price": 100, "time": 0, "change1d": 1, "rsi14": 50,
            "volumeRatio": 1, "volatility": 2, "liquidity": 5e8, "distanceAtr": 0, "hits": [{ "category": "breakout", "reason": "r", "strength": 2 }],
        });
        for (k, x) in over.as_object().unwrap() {
            v[k] = x.clone();
        }
        serde_json::from_value(v).unwrap()
    }

    fn with(f: impl FnOnce(&mut OppFilters)) -> OppFilters {
        let mut x = OppFilters::default();
        f(&mut x);
        x
    }

    // opportunities.test.ts (client filters)
    #[test]
    fn numeric_filters() {
        let a = item("A", json!({}));
        assert!(passes(&a, &OppFilters::default()));
        assert!(!passes(&a, &with(|f| f.min_cap = Some(2e11))));
        assert!(!passes(&item("B", json!({ "marketCap": null })), &with(|f| f.min_cap = Some(1e10))));
        assert!(passes(&item("C", json!({ "rank": 12 })), &with(|f| f.max_rank = Some(20.0))));
        assert!(!passes(&item("C", json!({ "rank": 45 })), &with(|f| f.max_rank = Some(20.0))));
        assert!(!passes(&a, &with(|f| f.max_volatility = Some(1.5))));
        assert!(!passes(&a, &with(|f| f.min_liquidity = Some(1e9))));
    }

    #[test]
    fn categories() {
        let items = [
            item("ONE", json!({})),
            item(
                "TWO",
                json!({ "hits": [{ "category": "volume", "reason": "v", "strength": 5 }, { "category": "breakout", "reason": "b", "strength": 3 }] }),
            ),
            item("OVS", json!({ "hits": [{ "category": "oversold", "reason": "o", "strength": 9 }] })),
        ];
        let syms = |v: Vec<OppItem>| v.into_iter().map(|i| i.symbol).collect::<Vec<_>>();
        assert_eq!(syms(filter_items(&items, &OppFilters::default())), ["TWO", "OVS", "ONE"]);
        let only = filter_items(&items, &with(|f| f.categories = vec![OppCategory::Breakout]));
        assert_eq!(only[0].hits.iter().map(|h| h.category).collect::<Vec<_>>(), [OppCategory::Breakout]);
        assert_eq!(syms(only), ["TWO", "ONE"]);
        assert!(filter_items(&items, &with(|f| f.categories = vec![])).is_empty());
        assert_eq!(count_by_category(&items, &OppFilters::default()), [0, 0, 2, 1, 1, 0]);
        assert_eq!(count_by_category(&items, &with(|f| f.max_volatility = Some(1.0)))[2], 0);
    }

    #[test]
    fn amounts() {
        let d = MoneyDisplay::usd();
        assert_eq!(compact_usd(&d, 14_389_008.0), "14,4 M$");
        assert_eq!(compact_usd(&d, 3_060_890_723.0), "3,1 Md$");
        assert_eq!(compact_usd(&d, 843_414.0), "843 k$");
        assert_eq!(compact_usd(&d, 420.0), "420 $");
    }

    #[test]
    fn saved_choices() {
        assert_eq!(read_saved(None), (Kind::Stock, OppFilters::default()));
        let f = with(|f| {
            f.categories = vec![OppCategory::Volume, OppCategory::Oversold];
            f.min_liquidity = Some(1e7);
        });
        let raw = saved_json(Kind::Crypto, &f);
        assert_eq!(
            raw,
            r#"{"market":"crypto","filters":{"categories":["volume","oversold"],"minCap":null,"maxRank":null,"minLiquidity":10000000,"maxVolatility":null}}"#
        );
        assert_eq!(read_saved(Some(&raw)), (Kind::Crypto, f));
        // Unknown categories dropped, missing thresholds default.
        let (_, g) = read_saved(Some(r#"{"market":"stock","filters":{"categories":["volume","nope"]}}"#));
        assert_eq!(g, with(|f| f.categories = vec![OppCategory::Volume]));
        assert_eq!(read_saved(Some(r#"{"market":"forex","filters":{"categories":[]}}"#)), (Kind::Stock, OppFilters::default()));
    }
}

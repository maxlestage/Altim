//! Server replies read by Mes avoirs and its tools (types of `web/src/webapp/api.ts`), lenient like the TypeScript:
//! what an older server does not send takes its default. Prices in dollars, times in ms.
use serde::{Deserialize, Serialize};

use crate::engine::history::Close;
use crate::engine::reliability::ReliabilityLevel;
use crate::engine::signal::{Action, Candle};
use crate::types::Kind;

/// Signal of a radar row (`{ action, score, confidence }`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RowSignal {
    pub action: Action,
    pub score: f64,
    #[serde(default)]
    pub confidence: f64,
}

/// The part of `Reliability` the holdings read.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RowReliability {
    pub level: ReliabilityLevel,
}

/// A row of `GET /api/radar?symbols=…&interval=…` (an asset in error has no signal and an `error`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadarRow {
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub signal: Option<RowSignal>,
    #[serde(default)]
    pub reliability: Option<RowReliability>,
    #[serde(default)]
    pub error: Option<String>,
}

/// `GET /api/candles?symbol=…&kind=…&interval=…` (the candles only; quality and sources are not read here).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandlesSnapshot {
    #[serde(default)]
    pub candles: Vec<Candle>,
}

/// A row of `GET /api/search` and of the `items` of `GET /api/universe`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UniverseItem {
    pub symbol: String,
    pub name: String,
    pub kind: Kind,
    #[serde(default)]
    pub rank: Option<u32>,
    /// Stocks: listed as an ETF.
    #[serde(default)]
    pub etf: Option<bool>,
    /// Cryptos: number of exchanges quoting it.
    #[serde(default)]
    pub exchanges: Option<u32>,
}

impl UniverseItem {
    /// "crypto:BTC" (`assetKey`).
    pub fn key(&self) -> String {
        crate::web::store::asset_key(&self.symbol, self.kind)
    }
}

/// `GET /api/universe?kind=…&q=…&offset=…&limit=…`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniversePage {
    pub total: usize,
    #[serde(default)]
    pub offset: usize,
    pub items: Vec<UniverseItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistorySeries {
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub closes: Vec<Close>,
    #[serde(default)]
    pub error: Option<String>,
}

/// `GET /api/history?days=…&symbols=…`: daily closes `[time, close]` of the assets and of the benchmarks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryReply {
    #[serde(default)]
    pub as_of: i64,
    #[serde(default)]
    pub days: f64,
    pub series: Vec<HistorySeries>,
}

impl HistoryReply {
    /// Closes by "kind:symbol".
    pub fn by_id(&self) -> std::collections::HashMap<String, Vec<Close>> {
        self.series.iter().map(|s| (crate::web::store::asset_key(&s.symbol, s.kind), s.closes.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_read() {
        let rows: Vec<RadarRow> = serde_json::from_str(
            r#"[{"symbol":"BTC","kind":"crypto","signal":{"action":"buy","score":31.2,"confidence":55},"reliability":{"level":"high","score":90,"agreeing":3},
                 "agreeing":3,"sources":4,"sparkline":[1,2],"name":"Bitcoin","price":64000.5,"change":1.2,"priceSources":"3/4"},
                {"symbol":"ZZZ","kind":"stock","error":"introuvable","name":"ZZZ","price":null,"change":null,"priceSources":null}]"#,
        )
        .unwrap();
        assert_eq!(rows[0].signal.unwrap().action, Action::Buy);
        assert_eq!(rows[0].reliability.unwrap().level, ReliabilityLevel::High);
        assert_eq!((rows[1].price, rows[1].signal), (None, None));
        let h: HistoryReply =
            serde_json::from_str(r#"{"asOf":5,"days":90,"series":[{"symbol":"BTC","kind":"crypto","closes":[[1,2.5]]},{"symbol":"X","kind":"stock","closes":[],"error":"historique indisponible"}]}"#)
                .unwrap();
        assert_eq!(h.by_id()["crypto:BTC"], vec![(1, 2.5)]);
        let u: UniversePage =
            serde_json::from_str(r#"{"total":2,"offset":0,"items":[{"symbol":"SPY","name":"SPDR","kind":"stock","rank":null,"etf":true}]}"#).unwrap();
        assert_eq!((u.items[0].etf, u.items[0].key().as_str()), (Some(true), "stock:SPY"));
    }
}

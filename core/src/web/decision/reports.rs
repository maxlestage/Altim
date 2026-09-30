//! JSON contracts of the other routes the Radar and the asset screen read (api.ts, WhyCard.tsx, AnomaliesCard.tsx),
//! where the server's type has no `Deserialize` of its own. What the TypeScript treats as optional is defaulted, so
//! an older server stays readable. Units: prices and amounts in USD, times in ms, percentages as 46.8 = 46,8 %.
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::Evidence;
use crate::engine::fibonacci::FibZone;
use crate::engine::guard::GuardResult;
use crate::engine::macro_ctx::MacroReport;
use crate::engine::reliability::{QualityReport, Reliability};
use crate::engine::signal::Action;
use crate::engine::synthesis::MarketRegime;
use crate::types::{Candle, Interval, Kind};
use crate::web::market::SourceStatus;

/// 4 h technical signal of a Radar row (`/api/radar`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadarSignal {
    pub action: Action,
    pub score: f64,
    pub confidence: f64,
}

/// A row of `/api/radar?symbols=&interval=`.
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
    pub change: Option<f64>,
    /// "3/4" sources agreeing on the price.
    #[serde(default)]
    pub price_sources: Option<String>,
    #[serde(default)]
    pub signal: Option<RadarSignal>,
    #[serde(default)]
    pub reliability: Option<Reliability>,
    #[serde(default)]
    pub sparkline: Vec<f64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteSource {
    pub name: String,
    pub ok: bool,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub error: Option<String>,
}

/// A row of `/api/tickers?symbols=` (consensus of the price sources).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    pub price: f64,
    #[serde(default)]
    pub change: Option<f64>,
    #[serde(default)]
    pub agreeing: u32,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub sources: Vec<QuoteSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FearGreed {
    pub value: f64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Social {
    #[serde(default)]
    pub bullish_percent: Option<f64>,
    #[serde(default)]
    pub sample: f64,
}

/// `/api/sentiment?symbol=&kind=` (context, outside the score).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sentiment {
    #[serde(default)]
    pub fear_greed: Option<FearGreed>,
    #[serde(default)]
    pub social: Option<Social>,
}

/// `/api/macro` and the `macro` of the guard and zones reports: the report, what its stress announced on this asset,
/// and the market regime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroInfo {
    #[serde(flatten)]
    pub report: MacroReport,
    #[serde(default)]
    pub evidence: Option<Evidence>,
    #[serde(default)]
    pub regime: Option<MarketRegime>,
}

/// `/api/candles?symbol=&kind=&interval=`: candles validated by multi-source consensus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub symbol: String,
    pub kind: Kind,
    pub interval: Interval,
    pub candles: Vec<Candle>,
    pub source: String,
    #[serde(default)]
    pub sources: Vec<SourceStatus>,
    #[serde(default)]
    pub agreeing: u32,
    pub quality: QualityReport,
    pub reliability: Reliability,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Headline {
    pub title: String,
    pub time: f64,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardInputs {
    #[serde(default)]
    pub news24h: u32,
    #[serde(default)]
    pub headlines: Vec<Headline>,
}

/// `/api/guard?symbol=&kind=`: the market guard and the inputs shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardReport {
    #[serde(flatten)]
    pub result: GuardResult,
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub as_of: f64,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub inputs: GuardInputs,
    #[serde(default, rename = "macro")]
    pub macro_info: Option<MacroInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HorizonZone {
    #[serde(flatten)]
    pub zone: FibZone,
    #[serde(default)]
    pub macro_note: Option<String>,
}

/// `/api/zones?symbol=&kind=`: buy zones by horizon (Fibonacci) and the macro context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZonesReport {
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub as_of: f64,
    pub zones: Vec<HorizonZone>,
    #[serde(default, rename = "macro")]
    pub macro_info: Option<MacroInfo>,
}

/// A row of `/api/alerts?symbols=` ("can I buy now?", the rule of the apps' notifications).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuyAlertRow {
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub buy: bool,
    #[serde(default)]
    pub strong: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub cautions: Vec<String>,
}

// ---------- /api/anomalies ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anomaly {
    /// "volume" | "priceVolume" | "zScore" | "openInterest" | "funding" | "longShort".
    pub code: String,
    /// "normal" | "warning" | "high".
    pub severity: String,
    #[serde(default)]
    pub triggered: bool,
    pub title: String,
    #[serde(default)]
    pub measured: String,
    #[serde(default)]
    pub meaning: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Largest {
    pub usd: f64,
    pub long: bool,
    pub price: f64,
    pub time: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Liquidations {
    pub long_usd: f64,
    pub short_usd: f64,
    pub long_count: u32,
    pub short_count: u32,
    #[serde(default)]
    pub largest: Option<Largest>,
    #[serde(default)]
    pub hours: f64,
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub scope: String,
}

/// A dollar amount in the display currency: "12,3 M€", "850 k€", "420 €" (`compactUsd` of engine/opportunities.ts).
pub fn compact_usd(usd: f64, m: &crate::web::money::MoneyDisplay) -> String {
    let v = m.to_display(usd);
    let s = m.symbol();
    let a = v.abs();
    let f = |x: f64, d: usize| crate::js::fr(x, 0, d);
    if a >= 1e9 {
        format!("{} Md{s}", f(v / 1e9, 1))
    } else if a >= 1e6 {
        format!("{} M{s}", f(v / 1e6, 1))
    } else if a >= 1e3 {
        format!("{} k{s}", f(v / 1e3, 0))
    } else {
        format!("{} {s}", f(v, 0))
    }
}

impl Liquidations {
    /// Share of the buyers in the liquidated amount, % (`longShare`).
    pub fn long_share(&self) -> Option<f64> {
        let total = self.long_usd + self.short_usd;
        (total > 0.0).then(|| self.long_usd / total * 100.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInterest {
    pub usd: f64,
    #[serde(default)]
    pub change24h: Option<f64>,
    #[serde(default)]
    pub change7d: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Funding {
    /// Last settled rate and the 5–95 % range, % per period.
    pub rate: f64,
    pub p5: f64,
    pub p95: f64,
    #[serde(default)]
    pub period_hours: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongShort {
    pub ratio: f64,
    pub p5: f64,
    pub p95: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotCovered {
    pub label: String,
    pub reason: String,
}

/// Cryptos: OKX derivatives and big moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Derivatives {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub liquidations: Option<Liquidations>,
    #[serde(default)]
    pub open_interest: Option<OpenInterest>,
    #[serde(default)]
    pub funding: Option<Funding>,
    #[serde(default)]
    pub long_short: Option<LongShort>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub not_covered: Vec<NotCovered>,
}

/// `/api/anomalies?symbol=&kind=`: unusual readings on this asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnomalyReport {
    pub symbol: String,
    pub kind: Kind,
    /// Time of the last closed daily session the measures read (ms).
    #[serde(default)]
    pub session: Option<f64>,
    #[serde(default)]
    pub anomalies: Vec<Anomaly>,
    #[serde(default)]
    pub normal: Vec<Anomaly>,
    #[serde(default)]
    pub derivatives: Option<Derivatives>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub source: String,
}

// ---------- /api/why and /api/ask ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhyFactor {
    pub key: String,
    pub label: String,
    /// "up" | "down" | "neutral"
    pub direction: String,
    /// "low" | "medium" | "high"
    pub magnitude: String,
    pub detail: String,
    pub source: String,
    /// "observed" | "possibleCorrelation" | "unverifiable"
    pub certainty: String,
    pub certainty_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhyNotCovered {
    pub key: String,
    pub label: String,
    pub reason: String,
}

/// `/api/why?symbol=&kind=` (« Pourquoi ça bouge ? »: co-occurring observations, never presented as causes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhyReport {
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub factors: Vec<WhyFactor>,
    #[serde(default)]
    pub not_covered: Vec<WhyNotCovered>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub disclaimer: String,
    #[serde(default)]
    pub ask_enabled: bool,
}

pub fn why_url(symbol: &str, kind: Kind) -> String {
    format!("/api/why?symbol={}&kind={}", crate::web::bot::encode_uri_component(symbol), kind.as_str())
}

/// `POST /api/ask` body.
pub fn ask_body(symbol: &str, kind: Kind, question: &str) -> String {
    serde_json::json!({ "symbol": symbol, "kind": kind, "question": question }).to_string()
}

/// `POST /api/ask` answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskAnswer {
    pub answer: String,
    #[serde(default)]
    pub data: Value,
}

impl AskAnswer {
    /// The data sent included the last decision computed.
    pub fn used_decision(&self) -> bool {
        self.data.get("derniereDecision").is_some_and(|v| !matches!(v, Value::Null | Value::Bool(false)))
    }
}

/// "BTC:crypto,AAPL:stock" of the batch routes, as `encodeURIComponent` writes it.
pub fn symbols_param(items: &[(String, Kind)]) -> String {
    let s: Vec<String> = items.iter().map(|(sym, k)| format!("{sym}:{}", k.as_str())).collect();
    crate::web::bot::encode_uri_component(&s.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real answers of a local server (saved from `/api/…`, trimmed to what is read).
    #[test]
    fn contracts_decode() {
        let radar: Vec<RadarRow> = serde_json::from_str(
            r#"[{"symbol":"BTC","kind":"crypto","signal":{"action":"hold","score":-3.2,"confidence":41.5},"reliability":{"score":100,"level":"high","independent":3,"conflict":false},"agreeing":3,"sources":3,"sparkline":[1,2],"lastClose":64000.5,"name":"Bitcoin","price":64010,"change":-1.2,"priceSources":"3/4"},
                {"symbol":"X","kind":"stock","name":"X","price":null,"change":null,"priceSources":null,"error":"indisponible"}]"#,
        )
        .unwrap();
        assert_eq!((radar[0].signal.as_ref().unwrap().action, radar[0].price_sources.as_deref()), (Action::Hold, Some("3/4")));
        assert!(radar[1].sparkline.is_empty() && radar[1].error.is_some());
        let s: Sentiment = serde_json::from_str(r#"{"fearGreed":{"value":44,"label":"Peur"},"social":{"bullishPercent":61,"sample":30}}"#).unwrap();
        assert_eq!(s.fear_greed.unwrap().value, 44.0);
        assert_eq!(serde_json::from_str::<Sentiment>("{}").unwrap(), Sentiment::default());
        let a: AnomalyReport = serde_json::from_str(
            r#"{"symbol":"BTC","kind":"crypto","asOf":1,"price":1.5,"session":1759190400000,"anomalies":[],"normal":[{"code":"volume","severity":"normal","triggered":false,"title":"Volume","value":1.1,"threshold":3,"unit":"×","measured":"×1,1 (seuil ×3)","meaning":"…","source":"s"}],
                "derivatives":{"source":"OKX","liquidations":{"longUsd":3,"shortUsd":1,"longCount":2,"shortCount":1,"largest":{"usd":2,"long":true,"price":64000,"time":1},"from":0,"to":1,"hours":24,"complete":true,"scope":"OKX seulement"},"openInterest":null,"funding":{"rate":0.01,"p5":0,"p95":0.02,"samples":99,"periodHours":8,"time":1},"longShort":null,"errors":[],"notCovered":[{"label":"Baleines","reason":"payant"}]},"errors":[],"source":"Bougies"}"#,
        )
        .unwrap();
        assert_eq!(a.derivatives.as_ref().unwrap().liquidations.as_ref().unwrap().long_share(), Some(75.0));
        let m = crate::web::money::MoneyDisplay::usd();
        assert_eq!(compact_usd(12_345_678.0, &m), "12,3 M$");
        assert_eq!(compact_usd(850_400.0, &m), "850 k$");
        assert_eq!(compact_usd(420.4, &m), "420 $");
        let answer: AskAnswer =
            serde_json::from_str(r#"{"answer":"a","model":"m","question":"q","data":{"derniereDecision":{"x":1}},"disclaimer":"d"}"#).unwrap();
        assert!(answer.used_decision());
        assert_eq!(symbols_param(&[("BTC".into(), Kind::Crypto), ("BRK.B".into(), Kind::Stock)]), "BTC%3Acrypto%2CBRK.B%3Astock");
    }

    /// Live answers of a local server (optional: `ALTIM_DECISION_SAMPLES=<dir>` holding the saved `/api/…` answers).
    #[test]
    fn live_answers_decode() {
        let Ok(dir) = std::env::var("ALTIM_DECISION_SAMPLES") else { return };
        let read = |f: &str| std::fs::read_to_string(format!("{dir}/{f}")).unwrap();
        fn ok<T: serde::de::DeserializeOwned>(s: &str) -> T {
            serde_json::from_str(s).unwrap()
        }
        assert!(!ok::<Vec<RadarRow>>(&read("radar.json")).is_empty());
        assert!(!ok::<Vec<Quote>>(&read("tickers.json")).is_empty());
        assert!(ok::<Sentiment>(&read("sentiment.json")).fear_greed.is_some());
        assert!(ok::<MacroInfo>(&read("macro.json")).regime.is_some());
        assert!(!ok::<Vec<BuyAlertRow>>(&read("alerts.json")).is_empty());
        assert_eq!(ok::<ZonesReport>(&read("zones-BTC.json")).zones.len(), 3);
        assert!(!ok::<GuardReport>(&read("guard-BTC.json")).inputs.headlines.is_empty());
        assert!(ok::<AnomalyReport>(&read("anomalies-BTC.json")).derivatives.is_some());
        assert!(!ok::<WhyReport>(&read("why-AAPL.json")).factors.is_empty());
        assert!(!ok::<Snapshot>(&read("candles-BTC.json")).candles.is_empty());
        assert_eq!(ok::<crate::engine::strategies::StrategiesReport>(&read("strategies-AAPL.json")).strategies.len(), 8);
    }
}

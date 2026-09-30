//! Web app state saved in the browser (`web/src/webapp/store.ts`): the SAME localStorage keys and JSON as the
//! TypeScript app, so existing users keep their settings and holdings. Reading is lenient like the TypeScript's
//! (a bad field takes its default, a bad holding line is dropped, never the whole state); writing keeps the field
//! order of `JSON.stringify` on the TypeScript objects.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::money::{Currency, MoneyDisplay, stored_currency};
use crate::types::{Interval, Kind};

pub const STATE_KEY: &str = "altim.webapp.v1";
pub const HOLDINGS_KEY: &str = "altim.holdings.v1";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WatchItem {
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
}

impl WatchItem {
    fn new(symbol: &str, kind: Kind, name: &str) -> Self {
        WatchItem { symbol: symbol.into(), kind, name: name.into() }
    }
    /// "crypto:BTC" (`assetKey`).
    pub fn key(&self) -> String {
        asset_key(&self.symbol, self.kind)
    }
}

/// `assetKey`: "crypto:BTC".
pub fn asset_key(symbol: &str, kind: Kind) -> String {
    format!("{}:{symbol}", kind.as_str())
}

pub fn default_watchlist() -> Vec<WatchItem> {
    vec![
        WatchItem::new("BTC", Kind::Crypto, "Bitcoin"),
        WatchItem::new("ETH", Kind::Crypto, "Ethereum"),
        WatchItem::new("SOL", Kind::Crypto, "Solana"),
        WatchItem::new("BNB", Kind::Crypto, "BNB"),
        WatchItem::new("XRP", Kind::Crypto, "XRP"),
        WatchItem::new("AAPL", Kind::Stock, "Apple"),
        WatchItem::new("NVDA", Kind::Stock, "NVIDIA"),
        WatchItem::new("MSFT", Kind::Stock, "Microsoft"),
    ]
}

/// Investment horizon of the user: which buy zones come first (Fibonacci on 4 h, daily or weekly candles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HorizonPref {
    Short,
    #[default]
    Medium,
    Long,
}

/// Position sizing settings (`RiskSettings` of engine/risk.ts).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskSettings {
    pub risk_per_trade_percent: f64,
    pub max_position_percent: f64,
    pub daily_loss_limit_percent: f64,
    pub min_risk_reward: f64,
    pub fee_rate: f64,
    /// Cap of the crypto share of the portfolio (%, cash included).
    pub max_crypto_percent: f64,
}

pub const DEFAULT_RISK: RiskSettings = RiskSettings {
    risk_per_trade_percent: 1.0,
    max_position_percent: 20.0,
    daily_loss_limit_percent: 3.0,
    min_risk_reward: 1.5,
    fee_rate: 0.001,
    max_crypto_percent: 60.0,
};

impl RiskSettings {
    /// `{ ...DEFAULT_RISK, ...saved }`: each numeric field saved overrides its default.
    fn merged(v: Option<&Value>) -> Self {
        let mut r = DEFAULT_RISK;
        if let Some(o) = v.and_then(Value::as_object) {
            let f = |k: &str, d: f64| o.get(k).and_then(Value::as_f64).unwrap_or(d);
            r.risk_per_trade_percent = f("riskPerTradePercent", r.risk_per_trade_percent);
            r.max_position_percent = f("maxPositionPercent", r.max_position_percent);
            r.daily_loss_limit_percent = f("dailyLossLimitPercent", r.daily_loss_limit_percent);
            r.min_risk_reward = f("minRiskReward", r.min_risk_reward);
            r.fee_rate = f("feeRate", r.fee_rate);
            r.max_crypto_percent = f("maxCryptoPercent", r.max_crypto_percent);
        }
        r
    }
}

/// Weights of the decision's composite score (Réglages), same factors and defaults as the server (synthesis.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreWeights {
    pub tech: u32,
    pub mom: u32,
    pub fund: u32,
    pub sent: u32,
    pub news: u32,
    pub macro_: u32,
}

/// (key, label, hint, default) of each factor (`SCORE_FACTORS`).
pub const SCORE_FACTORS: [(&str, &str, &str, u32); 6] = [
    ("tech", "Technique", "tendance, volume, volatilité, structure", 32),
    ("mom", "Momentum", "MACD, RSI, variation sur 1 mois", 18),
    ("fund", "Fondamentaux", "valorisation, comptes ou réseau", 20),
    ("sent", "Sentiment", "Fear & Greed, financement, StockTwits", 10),
    ("news", "Actualités", "ton des titres sur 24 h", 10),
    ("macro", "Macro", "stress des marchés, VIX, taux", 10),
];

pub const DEFAULT_SCORE_WEIGHTS: ScoreWeights = ScoreWeights { tech: 32, mom: 18, fund: 20, sent: 10, news: 10, macro_: 10 };

impl ScoreWeights {
    pub fn get(&self, key: &str) -> u32 {
        match key {
            "tech" => self.tech,
            "mom" => self.mom,
            "fund" => self.fund,
            "sent" => self.sent,
            "news" => self.news,
            _ => self.macro_,
        }
    }
    fn set(&mut self, key: &str, v: u32) {
        match key {
            "tech" => self.tech = v,
            "mom" => self.mom = v,
            "fund" => self.fund = v,
            "sent" => self.sent = v,
            "news" => self.news = v,
            _ => self.macro_ = v,
        }
    }
    /// Whole numbers 0 – 100 (a bad or missing value takes its default); all at 0 → the defaults.
    pub fn sanitize(raw: Option<&Value>) -> Self {
        let mut w = DEFAULT_SCORE_WEIGHTS;
        let o = raw.and_then(Value::as_object);
        for (k, _, _, def) in SCORE_FACTORS {
            let v = o.and_then(|o| o.get(k)).and_then(Value::as_f64).filter(|v| v.is_finite());
            w.set(k, v.map(|v| crate::js::round(v).clamp(0.0, 100.0) as u32).unwrap_or(def));
        }
        if SCORE_FACTORS.iter().any(|(k, ..)| w.get(k) > 0) { w } else { DEFAULT_SCORE_WEIGHTS }
    }
    /// "tech:40,mom:18,…", or None for the default weights (the URL stays the same).
    pub fn param(&self) -> Option<String> {
        let s = Self::sanitize(Some(&self.to_json()));
        if s == DEFAULT_SCORE_WEIGHTS {
            return None;
        }
        Some(SCORE_FACTORS.iter().map(|(k, ..)| format!("{k}:{}", s.get(k))).collect::<Vec<_>>().join(","))
    }
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        for (k, ..) in SCORE_FACTORS {
            m.insert(k.into(), json!(self.get(k)));
        }
        Value::Object(m)
    }
}

/// `AppState` of store.ts (version 1).
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub accepted_disclaimer: bool,
    pub interval: Interval,
    pub watchlist: Vec<WatchItem>,
    pub risk: RiskSettings,
    pub horizon: HorizonPref,
    pub score_weights: ScoreWeights,
    /// Display currency of every amount (Réglages): euros by default, dollars on request.
    pub currency: Currency,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            accepted_disclaimer: false,
            interval: Interval::H4,
            watchlist: default_watchlist(),
            risk: DEFAULT_RISK,
            horizon: HorizonPref::Medium,
            score_weights: DEFAULT_SCORE_WEIGHTS,
            currency: Currency::Eur,
        }
    }
}

fn watch_item(v: &Value) -> Option<WatchItem> {
    let symbol = v.get("symbol")?.as_str()?;
    let kind = Kind::parse(v.get("kind")?.as_str()?)?;
    let name = v.get("name").and_then(Value::as_str).unwrap_or(symbol);
    Some(WatchItem::new(symbol, kind, name))
}

impl AppState {
    /// The saved text of "altim.webapp.v1" (`load` of store.ts): anything unreadable gives the initial state.
    pub fn parse(raw: Option<&str>) -> Self {
        let Some(p) = raw.and_then(|r| serde_json::from_str::<Value>(r).ok()) else { return Self::default() };
        if p.get("version").and_then(Value::as_f64) != Some(1.0) {
            return Self::default();
        }
        AppState {
            accepted_disclaimer: p.get("acceptedDisclaimer").is_some_and(truthy),
            interval: p.get("interval").and_then(Value::as_str).and_then(Interval::parse).unwrap_or(Interval::H4),
            watchlist: match p.get("watchlist").and_then(Value::as_array) {
                Some(a) => a.iter().filter_map(watch_item).collect(),
                None => default_watchlist(),
            },
            risk: RiskSettings::merged(p.get("risk")),
            horizon: p.get("horizon").and_then(|h| serde_json::from_value(h.clone()).ok()).unwrap_or_default(),
            score_weights: ScoreWeights::sanitize(p.get("scoreWeights")),
            currency: if p.get("currency").and_then(Value::as_str) == Some("USD") { Currency::Usd } else { Currency::Eur },
        }
    }

    /// `JSON.stringify(state)` with the TypeScript key order.
    pub fn to_json(&self) -> Value {
        crate::js::to_value(&json!({
            "version": 1,
            "acceptedDisclaimer": self.accepted_disclaimer,
            "interval": self.interval,
            "watchlist": self.watchlist,
            "risk": self.risk,
            "horizon": self.horizon,
            "scoreWeights": self.score_weights.to_json(),
            "currency": self.currency,
        }))
    }
}

/// JavaScript truthiness of a JSON value (`!!x`).
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

// ---------- My holdings (real portfolio entered by the user) ----------

/// A line as saved: `averagePrice` and `stop` are in the currency they were typed in (`costCurrency`, `stopCurrency`;
/// absent = dollars, the only currency before the euro display). Unknown fields are kept as they were.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredHolding {
    pub id: String,
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    pub quantity: f64,
    /// Average cost price (PRU), in `cost_currency`.
    pub average_price: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_currency: Option<Currency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_currency: Option<Currency>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A holding in dollars, as the engines and the server get it (`Holding` of engine/holdings.ts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub id: String,
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    pub quantity: f64,
    /// Average cost price (PRU) in USD.
    pub average_price: f64,
    /// Stop set by the user (USD), optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<f64>,
}

impl StoredHolding {
    /// `isValidHolding` then `cleanStop` on a saved line: None when the line itself is invalid.
    pub fn from_value(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let id = o.get("id")?.as_str()?;
        let symbol = o.get("symbol")?.as_str()?;
        let kind = Kind::parse(o.get("kind")?.as_str()?)?;
        let quantity = o.get("quantity")?.as_f64().filter(|q| q.is_finite() && *q > 0.0)?;
        let average_price = o.get("averagePrice")?.as_f64().filter(|p| p.is_finite() && *p >= 0.0)?;
        let cur = |k: &str| match o.get(k).and_then(Value::as_str) {
            Some("EUR") => Some(Currency::Eur),
            Some("USD") => Some(Currency::Usd),
            _ => None,
        };
        let mut extra = o.clone();
        for k in ["id", "symbol", "kind", "name", "quantity", "averagePrice", "stop", "costCurrency", "stopCurrency"] {
            extra.remove(k);
        }
        let mut h = StoredHolding {
            id: id.into(),
            symbol: symbol.into(),
            kind,
            name: o.get("name").and_then(Value::as_str).unwrap_or_default().into(),
            quantity,
            average_price,
            stop: None,
            cost_currency: cur("costCurrency"),
            stop_currency: cur("stopCurrency"),
            extra,
        };
        // The optional stop is dropped when it is not a positive number (the line itself stays).
        match o.get("stop").and_then(Value::as_f64).filter(|s| s.is_finite() && *s > 0.0) {
            Some(s) => h.stop = Some(s),
            None => h.stop_currency = None,
        }
        Some(h)
    }
}

/// "altim.holdings.v1".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HoldingsState {
    /// Cash in `cash_currency` (absent = dollars).
    pub cash: f64,
    pub cash_currency: Option<Currency>,
    pub holdings: Vec<StoredHolding>,
    pub updated_at: f64,
}

impl HoldingsState {
    pub fn parse(raw: Option<&str>) -> Self {
        let Some(p) = raw.and_then(|r| serde_json::from_str::<Value>(r).ok()) else { return Self::default() };
        let Some(lines) = p.get("holdings").and_then(Value::as_array) else { return Self::default() };
        if p.get("version").and_then(Value::as_f64) != Some(1.0) {
            return Self::default();
        }
        HoldingsState {
            cash: p.get("cash").and_then(Value::as_f64).unwrap_or(0.0),
            cash_currency: match p.get("cashCurrency").and_then(Value::as_str) {
                Some("EUR") => Some(Currency::Eur),
                Some("USD") => Some(Currency::Usd),
                _ => None,
            },
            holdings: lines.iter().filter_map(StoredHolding::from_value).collect(),
            updated_at: p.get("updatedAt").and_then(Value::as_f64).unwrap_or(0.0),
        }
    }

    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("version".into(), json!(1));
        m.insert("cash".into(), json!(self.cash));
        if let Some(c) = self.cash_currency {
            m.insert("cashCurrency".into(), json!(c));
        }
        m.insert("holdings".into(), json!(self.holdings));
        m.insert("updatedAt".into(), json!(self.updated_at));
        crate::js::to_value(&Value::Object(m))
    }

    /// `exportHoldings`: `{ app: "altim", ...state }`, indented.
    pub fn export(&self) -> String {
        let mut m = Map::new();
        m.insert("app".into(), json!("altim"));
        if let Value::Object(o) = self.to_json() {
            m.extend(o);
        }
        serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default()
    }

    /// `importHoldings`: the new state, or the French error to show.
    pub fn import(json: &str, now: f64) -> Result<Self, &'static str> {
        let p: Value = serde_json::from_str(json).map_err(|_| "Fichier illisible.")?;
        let lines = p.get("holdings").and_then(Value::as_array).ok_or("Fichier invalide.")?;
        let cash = p.get("cash").and_then(Value::as_f64).filter(|c| c.is_finite() && *c >= 0.0).unwrap_or(0.0);
        Ok(HoldingsState {
            cash,
            cash_currency: (p.get("cashCurrency").and_then(Value::as_str) == Some("EUR")).then_some(Currency::Eur),
            holdings: lines.iter().filter_map(StoredHolding::from_value).collect(),
            updated_at: now,
        })
    }

    /// Assets held (the alert checks).
    pub fn assets(&self) -> Vec<WatchItem> {
        self.holdings.iter().map(|h| WatchItem::new(&h.symbol, h.kind, &h.name)).collect()
    }
}

/// Dollars view for the engines and the server, plus what could not be converted.
#[derive(Debug, Clone, PartialEq)]
pub struct UsdHoldings {
    pub holdings: Vec<Holding>,
    pub cash: f64,
    pub updated_at: f64,
    /// Symbols whose euro cost could not be converted (no rate): their cost is left at 0 and their P&L is not shown.
    pub unconverted: Vec<String>,
    /// The euro cash could not be converted (no rate): counted as 0.
    pub cash_unconverted: bool,
}

/// Saved amounts in dollars at the current rate: a euro cost basis becomes `cost ÷ rate`, so that the dollar P&L,
/// converted back at the same rate, is exactly the euro P&L (currency effect included).
pub fn to_usd_holdings(s: &HoldingsState, d: &MoneyDisplay) -> UsdHoldings {
    let mut unconverted = Vec::new();
    let holdings = s
        .holdings
        .iter()
        .map(|h| {
            let mut average_price = d.convert(h.average_price, stored_currency(h.cost_currency), Currency::Usd);
            if !average_price.is_finite() {
                unconverted.push(h.symbol.clone());
                average_price = 0.0;
            }
            let stop = h.stop.map(|s| d.convert(s, stored_currency(h.stop_currency), Currency::Usd)).filter(|s| s.is_finite() && *s > 0.0);
            Holding { id: h.id.clone(), symbol: h.symbol.clone(), kind: h.kind, name: h.name.clone(), quantity: h.quantity, average_price, stop }
        })
        .collect();
    let cash = d.convert(s.cash, stored_currency(s.cash_currency), Currency::Usd);
    UsdHoldings {
        holdings,
        cash: if cash.is_finite() { cash } else { 0.0 },
        updated_at: s.updated_at,
        unconverted,
        cash_unconverted: !cash.is_finite(),
    }
}

/// A saved amount shown in the display currency (NaN when no rate allows it).
pub fn shown(v: f64, c: Option<Currency>, d: &MoneyDisplay) -> f64 {
    d.convert(v, stored_currency(c), d.currency())
}

/// A second purchase merged into a line: weighted average cost in the currency of the new purchase (the previous
/// cost converted at the current rate when typed in another currency). None when no rate allows the conversion.
pub fn merge_line(x: &StoredHolding, quantity: f64, average_price: f64, cost_currency: Option<Currency>, d: &MoneyDisplay) -> Option<StoredHolding> {
    let cur = stored_currency(cost_currency);
    let prev = d.convert(x.average_price, stored_currency(x.cost_currency), cur);
    if !prev.is_finite() {
        return None;
    }
    let qty = x.quantity + quantity;
    Some(StoredHolding { quantity: qty, average_price: (x.quantity * prev + quantity * average_price) / qty, cost_currency: Some(cur), ..x.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::money::FxRate;

    fn eur() -> MoneyDisplay {
        MoneyDisplay::new(
            Currency::Eur,
            Some(FxRate { rate: 0.88, usd_per_eur: 1.0 / 0.88, time: 0.0, source: "t".into(), fetched_at: 0.0, stale: false }),
        )
    }

    #[test]
    fn state_round_trip_and_defaults() {
        let s = AppState::parse(None);
        assert_eq!(s, AppState::default());
        let raw = r#"{"version":1,"acceptedDisclaimer":true,"interval":"1d","horizon":"long","risk":{"feeRate":0.002},"watchlist":[{"symbol":"BTC","kind":"crypto","name":"Bitcoin"},{"symbol":"X"}],"currency":"USD","scoreWeights":{"tech":0,"mom":0,"fund":0,"sent":0,"news":0,"macro":0}}"#;
        let s = AppState::parse(Some(raw));
        assert!(s.accepted_disclaimer);
        assert_eq!((s.interval, s.horizon, s.currency), (Interval::D1, HorizonPref::Long, Currency::Usd));
        assert_eq!(s.watchlist.len(), 1);
        assert_eq!((s.risk.fee_rate, s.risk.max_position_percent), (0.002, 20.0));
        assert_eq!(s.score_weights, DEFAULT_SCORE_WEIGHTS);
        let out = s.to_json().to_string();
        assert!(out.starts_with(r#"{"version":1,"acceptedDisclaimer":true,"interval":"1d","watchlist":[{"symbol":"BTC","kind":"crypto","name":"Bitcoin"}],"risk":{"riskPerTradePercent":1,"#), "{out}");
        assert_eq!(AppState::parse(Some(&out)), s);
        // The state the tests of the house rules write (risk {} and no weights).
        let s = AppState::parse(Some(r#"{"version":1,"acceptedDisclaimer":true,"interval":"4h","horizon":"medium","risk":{},"watchlist":[]}"#));
        assert!(s.watchlist.is_empty() && s.currency == Currency::Eur && s.risk == DEFAULT_RISK);
        assert_eq!(AppState::parse(Some(r#"{"version":2}"#)), AppState::default());
    }

    #[test]
    fn weights() {
        let w = ScoreWeights::sanitize(Some(&serde_json::json!({ "tech": 140.4, "mom": -3, "fund": "x" })));
        assert_eq!((w.tech, w.mom, w.fund, w.sent), (100, 0, 20, 10));
        assert_eq!(DEFAULT_SCORE_WEIGHTS.param(), None);
        assert_eq!(w.param().unwrap(), "tech:100,mom:0,fund:20,sent:10,news:10,macro:10");
    }

    // money.test.ts "saved amounts and their currency"
    #[test]
    fn holdings_currency() {
        let raw = r#"{"version":1,"cash":1000,"updatedAt":0,"holdings":[
            {"id":"a","symbol":"BTC","kind":"crypto","name":"Bitcoin","quantity":1,"averagePrice":50000},
            {"id":"b","symbol":"AAPL","kind":"stock","name":"Apple","quantity":2,"averagePrice":176,"costCurrency":"EUR","stop":150,"stopCurrency":"EUR","note":"x"},
            {"id":"c","symbol":"BAD","kind":"stock","quantity":0,"averagePrice":1},
            {"id":"d","symbol":"ETH","kind":"crypto","quantity":1,"averagePrice":3000,"stop":-1,"stopCurrency":"EUR","costCurrency":"GBP"}]}"#;
        let s = HoldingsState::parse(Some(raw));
        assert_eq!(s.holdings.len(), 3);
        assert_eq!(s.holdings[1].extra.get("note"), Some(&serde_json::json!("x")));
        assert_eq!((s.holdings[2].stop, s.holdings[2].stop_currency, s.holdings[2].cost_currency), (None, None, None));
        let u = to_usd_holdings(&s, &eur());
        assert!((u.holdings[1].average_price - 200.0).abs() < 1e-9);
        assert!(u.unconverted.is_empty() && !u.cash_unconverted);
        let u = to_usd_holdings(&s, &MoneyDisplay::usd());
        assert_eq!(u.unconverted, vec!["AAPL".to_string()]);
        assert_eq!(u.holdings[1].stop, None);
        let back = HoldingsState::parse(Some(&s.to_json().to_string()));
        assert_eq!(back, s);
        let m = merge_line(&s.holdings[0], 1.0, 44_000.0, Some(Currency::Eur), &eur()).unwrap();
        assert!((m.average_price - 44_000.0).abs() < 1e-6 && m.quantity == 2.0 && m.cost_currency == Some(Currency::Eur));
        assert!(merge_line(&s.holdings[0], 1.0, 1.0, Some(Currency::Eur), &MoneyDisplay::usd()).is_none());
        assert_eq!(HoldingsState::import("{", 0.0), Err("Fichier illisible."));
        assert_eq!(HoldingsState::import("{}", 0.0), Err("Fichier invalide."));
        assert_eq!(HoldingsState::import(&s.export(), 5.0).unwrap().holdings.len(), 3);
    }
}

//! Real-time WebSocket of the apps (`/api/ws`, protocol v1, see `backend/src/app/ws.rs` and the README) as a client
//! reads it: message decoding, the subscription message, the reconnection pace and the honesty rule of the « en
//! direct » badge. Pure: the browser passes the clock and the socket.
use serde::Deserialize;
use serde_json::{Value, json};

use crate::types::Kind;

pub const PROTOCOL: u64 = 1;
/// Application ping from the client: the server answers `pong`, so a silent socket is a dead one.
pub const PING_MS: u32 = 20_000;
/// Without any message for this long the socket is not « en direct » any more: it is closed and reopened.
pub const SILENCE_MS: f64 = 45_000.0;
/// Failed openings in a row after which the client falls back to the SSE stream (`/api/live`).
pub const FALLBACK_AFTER: u32 = 2;
/// Same cap as the server.
pub const MAX_ASSETS: usize = 20;

/// Radar verdict of an asset, pushed when it changes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictPush {
    pub symbol: String,
    pub kind: Kind,
    /// "buy" | "buyZone" | "wait" | "trim" | "sell" | "noPosition" (the decision's verdict).
    pub verdict: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub rating: Option<String>,
    #[serde(default)]
    pub rating_label: Option<String>,
    #[serde(default)]
    pub chip_note: Option<String>,
    #[serde(default)]
    pub as_of: Option<f64>,
}

/// The `/api/alerts` items of the subscription, pushed when one changed.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertsPush {
    pub items: Vec<Value>,
    pub checked_at: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WsMsg {
    Hello {
        v: u64,
    },
    /// The tick object (same fields as an `/api/live` event).
    Tick(Value),
    Alerts(AlertsPush),
    Checked {
        checked_at: f64,
    },
    Verdict(VerdictPush),
    Fx {
        rate: f64,
        as_of: Option<f64>,
    },
    Pong,
    Error {
        code: String,
        message: String,
    },
    /// A message type this client does not know (a newer server): ignored.
    Other,
}

/// One server text frame; None when it is not a JSON object with a `type`.
pub fn parse_ws(text: &str) -> Option<WsMsg> {
    let v: Value = serde_json::from_str(text).ok()?;
    let ty = v.get("type")?.as_str()?.to_string();
    let num = |k: &str| v.get(k).and_then(Value::as_f64);
    Some(match ty.as_str() {
        "hello" => WsMsg::Hello { v: v.get("v").and_then(Value::as_u64).unwrap_or(0) },
        "tick" => WsMsg::Tick(v),
        "alerts" => WsMsg::Alerts(AlertsPush { items: v.get("items")?.as_array()?.clone(), checked_at: num("checkedAt").unwrap_or(0.0) }),
        "checked" => WsMsg::Checked { checked_at: num("checkedAt")? },
        "verdict" => WsMsg::Verdict(serde_json::from_value(v).ok()?),
        "fx" => WsMsg::Fx { rate: num("rate").filter(|r| r.is_finite() && *r > 0.0)?, as_of: num("asOf") },
        "pong" => WsMsg::Pong,
        "error" => WsMsg::Error {
            code: v.get("code").and_then(Value::as_str).unwrap_or("").into(),
            message: v.get("message").and_then(Value::as_str).unwrap_or("").into(),
        },
        _ => WsMsg::Other,
    })
}

/// `subscribe` message for asset keys ("crypto:BTC", "stock:AAPL"), at most [`MAX_ASSETS`] (the first ones).
pub fn subscribe_message(keys: &[String], interval: &str, usd: bool) -> String {
    let assets: Vec<Value> =
        keys.iter().filter_map(|k| k.split_once(':')).take(MAX_ASSETS).map(|(kind, symbol)| json!({ "kind": kind, "symbol": symbol })).collect();
    json!({ "type": "subscribe", "assets": assets, "interval": interval, "currency": if usd { "USD" } else { "EUR" } }).to_string()
}

/// Pause before reconnection attempt `attempt` (0-based): 1 s, 2 s, 4 s, 8 s, 16 s, then 30 s.
pub fn backoff_ms(attempt: u32) -> u32 {
    (1_000u32 << attempt.min(5)).min(30_000)
}

/// `wss://host/api/ws` for a page served over https, `ws://` otherwise.
pub fn ws_url(protocol: &str, host: &str) -> String {
    format!("{}://{host}/api/ws", if protocol == "https:" { "wss" } else { "ws" })
}

/// « En direct » only while messages arrive: an open socket that said nothing for [`SILENCE_MS`] is not.
pub fn is_live(open: bool, last_message: Option<f64>, now: f64) -> bool {
    open && last_message.is_some_and(|t| now - t < SILENCE_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode() {
        assert_eq!(parse_ws(r#"{"type":"hello","v":1,"maxAssets":20}"#), Some(WsMsg::Hello { v: 1 }));
        let t = parse_ws(r#"{"type":"tick","symbol":"BTC","kind":"crypto","price":60000}"#).unwrap();
        assert!(matches!(t, WsMsg::Tick(v) if v["price"] == 60000));
        let a = parse_ws(r#"{"type":"alerts","items":[{"symbol":"BTC","kind":"crypto","buy":true}],"checkedAt":5}"#).unwrap();
        assert!(matches!(a, WsMsg::Alerts(AlertsPush { ref items, checked_at }) if items.len() == 1 && checked_at == 5.0));
        let v = parse_ws(r#"{"type":"verdict","symbol":"AAPL","kind":"stock","verdict":"wait","label":"ATTENDRE","chipNote":"zone plus bas"}"#);
        let Some(WsMsg::Verdict(v)) = v else { panic!() };
        assert_eq!((v.kind, v.verdict.as_str(), v.chip_note.as_deref()), (Kind::Stock, "wait", Some("zone plus bas")));
        assert_eq!(parse_ws(r#"{"type":"fx","rate":0.91,"asOf":7}"#), Some(WsMsg::Fx { rate: 0.91, as_of: Some(7.0) }));
        assert_eq!(parse_ws(r#"{"type":"fx","rate":0}"#), None);
        assert_eq!(parse_ws(r#"{"type":"checked","checkedAt":9}"#), Some(WsMsg::Checked { checked_at: 9.0 }));
        assert_eq!(parse_ws(r#"{"type":"pong"}"#), Some(WsMsg::Pong));
        assert_eq!(
            parse_ws(r#"{"type":"error","code":"too_many_assets","message":"20 actifs au plus"}"#).map(|m| matches!(m, WsMsg::Error { .. })),
            Some(true)
        );
        assert_eq!(parse_ws(r#"{"type":"future"}"#), Some(WsMsg::Other));
        assert_eq!(parse_ws("[1]"), None);
        assert_eq!(parse_ws("nope"), None);
    }

    #[test]
    fn subscription_backoff_status() {
        let keys = vec!["crypto:BTC".to_string(), "stock:AAPL".to_string()];
        let m: Value = serde_json::from_str(&subscribe_message(&keys, "4h", false)).unwrap();
        assert_eq!(
            m,
            json!({ "type": "subscribe", "assets": [{ "kind": "crypto", "symbol": "BTC" }, { "kind": "stock", "symbol": "AAPL" }], "interval": "4h", "currency": "EUR" })
        );
        let many: Vec<String> = (0..30).map(|i| format!("crypto:C{i}")).collect();
        let m: Value = serde_json::from_str(&subscribe_message(&many, "1d", true)).unwrap();
        assert_eq!((m["assets"].as_array().unwrap().len(), m["currency"].as_str()), (20, Some("USD")));
        assert_eq!((0..8).map(backoff_ms).collect::<Vec<_>>(), [1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000, 30_000]);
        assert_eq!(ws_url("https:", "altim.example"), "wss://altim.example/api/ws");
        assert_eq!(ws_url("http:", "localhost:4700"), "ws://localhost:4700/api/ws");
        assert!(is_live(true, Some(1_000.0), 30_000.0));
        assert!(!is_live(true, Some(1_000.0), 50_000.0));
        assert!(!is_live(false, Some(1_000.0), 2_000.0));
        assert!(!is_live(true, None, 2_000.0));
    }
}

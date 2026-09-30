//! Last "positions devenues dangereuses" computed on Mes avoirs ("altim.dangers.v1", danger-store.ts), kept in the
//! browser so the Radar can show them with the time they were measured. `Danger` is the type of
//! engine/portfolio-risk.ts `dangerousPositions` (batch C ports the engine and returns this type).
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::Kind;

pub const DANGERS_KEY: &str = "altim.dangers.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerCode {
    StopBroken,
    NearStop,
    LossOverRisk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DangerReason {
    pub code: DangerCode,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Danger {
    pub id: String,
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    pub reasons: Vec<DangerReason>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DangerState {
    pub at: f64,
    pub items: Vec<Danger>,
}

/// A saved state, validated (localStorage can be edited by hand): a bad item is dropped, a bad state is None.
pub fn parse_dangers(raw: Option<&str>) -> Option<DangerState> {
    let p: Value = serde_json::from_str(raw?).ok()?;
    if p.get("version").and_then(Value::as_f64) != Some(1.0) {
        return None;
    }
    let at = p.get("at").and_then(Value::as_f64).filter(|a| a.is_finite())?;
    let items = p.get("items")?.as_array()?.iter().filter_map(|i| serde_json::from_value(i.clone()).ok()).collect();
    Some(DangerState { at, items })
}

/// The text saved by `saveDangers`.
pub fn dangers_json(items: &[Danger], now: f64) -> String {
    crate::js::to_value(&serde_json::json!({ "version": 1, "at": now, "items": items })).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_validation() {
        let d = Danger {
            id: "a".into(),
            symbol: "BTC".into(),
            kind: Kind::Crypto,
            name: "Bitcoin".into(),
            reasons: vec![DangerReason { code: DangerCode::NearStop, text: "proche du stop".into() }],
        };
        let s = dangers_json(std::slice::from_ref(&d), 5.0);
        assert_eq!(parse_dangers(Some(&s)), Some(DangerState { at: 5.0, items: vec![d] }));
        let bad = r#"{"version":1,"at":1,"items":[{"id":"x","symbol":"X","kind":"crypto","name":"X","reasons":[{"code":"other","text":""}]}]}"#;
        assert_eq!(parse_dangers(Some(bad)).unwrap().items.len(), 0);
        assert_eq!(parse_dangers(Some("{")), None);
        assert_eq!(parse_dangers(None), None);
    }
}

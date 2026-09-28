//! Helpers for the parity tests: golden files written by `parity/golden.ts` from the TypeScript engines.
#![allow(dead_code)]
use std::sync::LazyLock;

use altim::types::{Candle, Kind};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct Input {
    pub symbol: String,
    pub kind: Kind,
    pub interval: String,
    pub candles: Vec<Candle>,
}

pub static INPUTS: LazyLock<Vec<Input>> = LazyLock::new(|| {
    serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/inputs.json")).unwrap()).unwrap()
});

pub fn find(symbol: &str, interval: &str) -> Option<&'static Input> {
    INPUTS.iter().find(|i| i.symbol == symbol && i.interval == interval)
}

/// Same "now" as golden.ts.
pub fn now() -> i64 {
    INPUTS.iter().map(|i| i.candles.last().unwrap().time).max().unwrap() + 3_600_000
}

#[derive(Deserialize)]
pub struct Case {
    pub args: Value,
    pub output: Value,
}

pub fn golden(name: &str) -> Vec<Case> {
    let path = format!("{}/tests/golden/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} absent : bun parity/golden.ts"))).unwrap()
}

/// Deep comparison: same keys, same strings, numbers within 1e-9 (relative). `undefined` fields (absent in
/// the TypeScript JSON) must be absent or null on the Rust side.
pub fn assert_same(rust: &Value, ts: &Value, path: &str) {
    match (rust, ts) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            let tol = 1e-9 * a.abs().max(b.abs()).max(1.0);
            assert!((a - b).abs() <= tol, "{path}: {a} (Rust) ≠ {b} (TS)");
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: longueur {} (Rust) ≠ {} (TS)", a.len(), b.len());
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                assert_same(x, y, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for (k, y) in b {
                let x = a.get(k).unwrap_or(&Value::Null);
                assert_same(x, y, &format!("{path}.{k}"));
            }
            for (k, x) in a {
                if !b.contains_key(k) {
                    assert!(x.is_null(), "{path}.{k}: présent en Rust ({x}), absent en TS");
                }
            }
        }
        _ => assert_eq!(rust, ts, "{path}"),
    }
}

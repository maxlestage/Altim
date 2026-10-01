//! Same 17 reference scenarios as `web/test/signal.test.ts` (fixture shared with Swift and Kotlin).
use altim::engine::signal::{AnalyzeOptions, Candle, analyze};
use serde_json::Value;

fn candles(rows: &Value) -> Vec<Candle> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let r: Vec<f64> = r.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            Candle { time: r[0] as i64, open: r[1], high: r[2], low: r[3], close: r[4], volume: r[5] }
        })
        .collect()
}

fn close(a: f64, b: f64, digits: i32) {
    assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≠ {b}");
}

#[test]
fn seventeen_reference_cases() {
    let fixture: Value = serde_json::from_str(include_str!("samples/swift-fixture.json")).unwrap();
    let cases = fixture.as_array().unwrap();
    assert_eq!(cases.len(), 17);
    for c in cases {
        let higher = c.get("higher").map(candles);
        let s = analyze(&candles(&c["candles"]), &AnalyzeOptions { higher: higher.as_deref(), ..Default::default() }).unwrap();
        close(s.score, c["score"].as_f64().unwrap(), 9);
        close(s.confidence, c["confidence"].as_f64().unwrap(), 9);
        assert_eq!(s.action.as_str(), c["action"].as_str().unwrap());
        let warnings: Vec<String> = serde_json::from_value(c["warnings"].clone()).unwrap();
        assert_eq!(s.warnings, warnings);
        let factors = c["factors"].as_object().unwrap();
        assert_eq!(s.factors.len(), factors.len());
        for f in &s.factors {
            close(f.score, factors[&f.name].as_f64().unwrap(), 9);
        }
        if let Some(sl) = c.get("stopLoss").and_then(Value::as_f64) {
            close(s.stop_loss, sl, 6);
            close(s.take_profit, c["takeProfit"].as_f64().unwrap(), 6);
        }
    }
}

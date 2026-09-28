//! Rust engines vs TypeScript engines on real market data (golden files from `bun parity/golden.ts`).
mod common;
use altim::engine::signal::{AnalyzeOptions, analyze};
use altim::js::to_value;
use common::*;

#[test]
fn signal() {
    for c in golden("signal") {
        let i = find(c.args["symbol"].as_str().unwrap(), c.args["interval"].as_str().unwrap()).unwrap();
        let cut = c.args["cut"].as_u64().unwrap() as usize;
        let candles = &i.candles[..i.candles.len() - cut];
        let higher_iv = match i.interval.as_str() { "1h" => Some("4h"), "4h" => Some("1d"), _ => None };
        let higher: Option<Vec<_>> = higher_iv
            .and_then(|h| find(&i.symbol, h))
            .map(|h| h.candles.iter().filter(|x| x.time <= candles.last().unwrap().time).copied().collect());
        let out = analyze(candles, &AnalyzeOptions { higher: higher.as_deref(), interval_ms: Some(3_600_000), now: Some(now()) });
        assert_same(&to_value(&out), &c.output, &format!("signal {}", c.args));
    }
}

#[test]
fn reliability() {
    use altim::engine::reliability::{assess_quality, gate, reliability};
    for c in golden("reliability") {
        let a = &c.args;
        let i = find(a["symbol"].as_str().unwrap(), a["interval"].as_str().unwrap()).unwrap();
        let candles = &i.candles[(a["cut"].as_u64().unwrap() as usize).min(i.candles.len())..];
        let step = match i.interval.as_str() { "1h" => 3_600_000, "4h" => 14_400_000, _ => 86_400_000 };
        let q = assess_quality(candles, step, i.kind, now());
        let rel = reliability(q.score, a["n"].as_u64().unwrap() as usize, a["conflict"].as_bool().unwrap());
        let gated = analyze(candles, &AnalyzeOptions::default()).map(|s| gate(&s, &rel, &q.issues));
        let out = serde_json::json!({ "q": q, "rel": rel, "gated": gated });
        assert_same(&to_value(&out), &c.output, &format!("reliability {a}"));
    }
}

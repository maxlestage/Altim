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

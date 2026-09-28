//! fibonacci, alerts (+ formatPrice / formatPercent) and screener: Rust vs TypeScript on real market data (golden files
//! from `bun parity/golden.ts`, section `parity/golden-engines-b1.ts`).
mod common;
use altim::engine::alerts::{AlertInput, buy_alert};
use altim::engine::fibonacci::{
    EXTENSIONS, HORIZONS, Horizon as ZH, RATIOS, Swing, fib_zone, fib_zones, level, swing_at, weekly, zone_evidence, zone_state,
};
use altim::engine::format::{format_percent, format_price};
use altim::engine::screener::{
    self, CRITERIA, CandleInterval, HORIZON_LIST, RankRule, Spec, align_series, explain, factors_at, is_pegged, pick, ranks, roles, score_universe,
    span, to_horizon, validate,
};
use altim::engine::signal::atr;
use altim::js::to_value;
use altim::types::{Candle, Kind};
use common::*;
use serde_json::{Map, Value, json};
use std::sync::LazyLock;

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

/// serde_json (without its `float_roundtrip` feature) may parse a decimal 1 ULP away from `JSON.parse`, which changes
/// a `toLocaleString` rounding (58123.604999999996 → "58 123,61" instead of "58 123,60"). The inputs are re-read here
/// with Rust's correctly rounded `str::parse::<f64>`, in file order (time, open, high, low, close, volume).
static EXACT: LazyLock<Vec<Vec<Candle>>> = LazyLock::new(|| {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/inputs.json")).unwrap();
    let bytes = text.as_bytes();
    let mut nums = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            // Skip the string (symbols, intervals, keys).
            i += 1;
            while bytes[i] != b'"' {
                i += if bytes[i] == 0x5c { 2 } else { 1 };
            }
            i += 1;
        } else if b == b'-' || b.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || matches!(bytes[i], b'.' | b'e' | b'E' | b'+' | b'-')) {
                i += 1;
            }
            nums.push(text[start..i].parse::<f64>().unwrap());
        } else {
            i += 1;
        }
    }
    let mut it = nums.into_iter();
    let out: Vec<Vec<Candle>> = INPUTS
        .iter()
        .map(|inp| {
            inp.candles
                .iter()
                .map(|c| {
                    let mut n = || it.next().unwrap();
                    let x = Candle { time: n() as i64, open: n(), high: n(), low: n(), close: n(), volume: n() };
                    assert_eq!(x.time, c.time);
                    assert!((x.close - c.close).abs() <= 1e-9 * c.close.abs());
                    x
                })
                .collect()
        })
        .collect();
    assert!(it.next().is_none());
    out
});

fn exact(symbol: &str, interval: &str) -> &'static [Candle] {
    let k = INPUTS.iter().position(|i| i.symbol == symbol && i.interval == interval).unwrap();
    &EXACT[k]
}

fn input(a: &Value) -> &'static [Candle] {
    exact(s(&a["symbol"]), s(&a["interval"]))
}

const SYMBOLS: [&str; 7] = ["BTC", "ETH", "SOL", "DOGE", "AAPL", "NVDA", "SPY"];
const MARKETS: [Kind; 2] = [Kind::Stock, Kind::Crypto];

// ---------- fibonacci ----------

#[test]
fn fib_consts() {
    let out = json!({
        "RATIOS": RATIOS,
        "EXTENSIONS": EXTENSIONS,
        "HORIZONS": { "short": HORIZONS[0], "medium": HORIZONS[1], "long": HORIZONS[2] },
    });
    assert_same(&to_value(&out), &golden("fib-consts")[0].output, "fib-consts");
}

#[test]
fn fib_weekly() {
    for c in golden("fib-weekly") {
        let out = if c.args["symbol"].is_null() { weekly(&[]) } else { weekly(input(&c.args)) };
        assert_same(&to_value(&out), &c.output, &format!("weekly {}", c.args));
    }
}

#[test]
fn fib_swing() {
    for c in golden("fib-swing") {
        let candles = input(&c.args);
        let cfg = ZH::parse(s(&c.args["horizon"])).unwrap().cfg();
        let a = atr(candles, 14);
        let out: Vec<Option<Swing>> = (0..candles.len()).step_by(7).map(|u| swing_at(candles, u, cfg.window, cfg.min_atr, Some(&a))).collect();
        assert_same(&to_value(&out), &c.output, &format!("swingAt {}", c.args));
    }
}

#[test]
fn fib_evidence() {
    for c in golden("fib-evidence") {
        let out = zone_evidence(input(&c.args), ZH::parse(s(&c.args["horizon"])).unwrap());
        assert_same(&to_value(&out), &c.output, &format!("zoneEvidence {}", c.args));
    }
}

#[test]
fn fib_state() {
    for c in golden("fib-state") {
        let sw: Swing = serde_json::from_value(c.args["swing"].clone()).unwrap();
        let p = c.args["price"].as_f64().unwrap();
        let out = json!({ "state": zone_state(&sw, p), "levels": RATIOS.iter().map(|r| level(&sw, *r)).collect::<Vec<_>>() });
        assert_same(&to_value(&out), &c.output, &format!("zoneState {}", c.args));
    }
}

#[test]
fn fib_zone_cases() {
    for c in golden("fib-zone") {
        let a = &c.args;
        let h = ZH::parse(s(&a["horizon"])).unwrap();
        let out = if a["symbol"].is_null() {
            fib_zone(&[], h, None, true)
        } else {
            let candles = input(a);
            let cut = a["cut"].as_u64().unwrap() as usize;
            fib_zone(&candles[..candles.len().saturating_sub(cut)], h, a["price"].as_f64(), a["evidence"].as_bool().unwrap())
        };
        assert_same(&to_value(&out), &c.output, &format!("fibZone {a}"));
    }
}

#[test]
fn fib_zones_cases() {
    for c in golden("fib-zones") {
        let a = &c.args;
        let sym = s(&a["symbol"]);
        let (h4, d, l) = (exact(sym, "4h"), exact(sym, "1d"), exact(sym, "long"));
        let long: &[Candle] = if a["long"].as_bool().unwrap() { l } else { &[] };
        let out = fib_zones(h4, d, long, a["price"].as_f64(), a["evidence"].as_bool().unwrap());
        assert_same(&to_value(&out), &c.output, &format!("fibZones {a}"));
    }
}

// ---------- alerts ----------

#[test]
fn format() {
    for c in golden("format") {
        let v = c.args["v"].as_f64().unwrap();
        let out = json!({ "price": format_price(v), "percent": format_percent(v) });
        assert_same(&out, &c.output, &format!("format {}", c.args));
    }
}

#[test]
fn alerts() {
    for c in golden("alerts") {
        let input: AlertInput = serde_json::from_value(c.args.clone()).unwrap();
        assert_same(&to_value(&buy_alert(&input)), &c.output, &format!("buyAlert {}", c.args));
    }
}

// ---------- screener ----------

fn spec_named(name: &str) -> Spec {
    let (base, variant) = name.split_once('-').unwrap_or((name, ""));
    let (m, h) = base.split_once('/').unwrap();
    let mut sp = *screener::spec(Kind::parse(m).unwrap(), screener::Horizon::parse(h).unwrap());
    match variant {
        "reversal" => sp.rank = RankRule::Reversal,
        "cost" => sp.cost = 0.5,
        "momLen1000" => sp.mom_len = 1000,
        _ => {}
    }
    sp
}

#[test]
fn screener_consts() {
    let intervals = [CandleInterval::M5, CandleInterval::M15, CandleInterval::M30, CandleInterval::D1];
    let mut labels = Map::new();
    for h in HORIZON_LIST {
        labels.insert(h.as_str().into(), h.label().into());
    }
    let mut ranked = Map::new();
    for r in [RankRule::Signal, RankRule::Momentum, RankRule::Reversal, RankRule::LowRisk] {
        ranked.insert(r.as_str().into(), to_value(&r.criterion()));
    }
    let mut specs = Map::new();
    for m in MARKETS {
        let mut per = Map::new();
        for h in HORIZON_LIST {
            per.insert(h.as_str().into(), to_value(screener::spec(m, h)));
        }
        specs.insert(m.as_str().into(), Value::Object(per));
    }
    let nan = f64::NAN;
    let out = json!({
        "HORIZON_LIST": HORIZON_LIST,
        "HORIZON_LABEL": labels,
        "CRITERIA": CRITERIA,
        "RANKED_CRITERION": ranked,
        "SPECS": specs,
        "roles": MARKETS.map(|m| HORIZON_LIST.map(|h| roles(screener::spec(m, h), m))),
        "toHorizon": MARKETS.map(|m| ["30m", "1h", "5h", "7d", "14d", "1m", "3m", "6m", "short", "medium", "long", "2y", ""].map(|h| to_horizon(h, m))),
        "span": MARKETS.map(|m| intervals.map(|iv| [1, 4, 6, 7, 10, 11, 12, 20, 21, 29, 30, 45, 63, 78, 90, 126, 180].map(|n| span(n, iv, m)))),
        "isPegged": intervals.map(|iv| [None, Some(0.0), Some(0.01), Some(0.05), Some(0.1), Some(0.3), Some(0.49), Some(0.5), Some(2.5)].map(|a| is_pegged(a, iv))),
        "ranks": [
            ranks(&[Some(10.0), Some(30.0), Some(20.0), None], true),
            ranks(&[Some(10.0), Some(30.0), Some(20.0)], false),
            ranks(&[Some(5.0), Some(5.0), Some(5.0)], true),
            ranks(&[None, Some(3.0)], true),
            ranks(&[], true),
            ranks(&[Some(1.0), Some(2.0), Some(2.0), Some(3.0), Some(nan), Some(f64::INFINITY), None, Some(-4.0)], true),
            ranks(&[Some(nan), Some(1.0)], false),
        ],
    });
    assert_same(&to_value(&out), &golden("screener-consts")[0].output, "screener-consts");
}

#[test]
fn screener_factors() {
    for c in golden("screener-factors") {
        let a = &c.args;
        let out = factors_at(input(a), a["i"].as_u64().unwrap() as usize, &spec_named(s(&a["spec"])));
        assert_same(&to_value(&out), &c.output, &format!("factorsAt {a}"));
    }
}

#[test]
fn screener_universe() {
    for c in golden("screener-universe") {
        let a = &c.args;
        let interval = s(&a["interval"]);
        let m = Kind::parse(s(&a["market"])).unwrap();
        let h = s(&a["horizon"]);
        let sp = if h == "momLen1000" { spec_named("stock/3m-momLen1000") } else { *screener::spec(m, screener::Horizon::parse(h).unwrap()) };
        let list: Vec<_> = SYMBOLS
            .iter()
            .map(|sym| {
                let c = exact(sym, interval);
                if *sym == "DOGE" && interval == "1d" { None } else { factors_at(c, c.len() - 1, &sp) }
            })
            .collect();
        let sc = score_universe(&list, &sp);
        let why: Value = if h == "momLen1000" {
            Value::Null
        } else {
            to_value(
                &list
                    .iter()
                    .zip(&sc)
                    .map(|(f, x)| match (f, x) {
                        (Some(f), Some(x)) => Some(explain(f, x, &sp, m)),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let out = json!({ "list": to_value(&list), "sc": to_value(&sc), "why": why });
        assert_same(&out, &c.output, &format!("scoreUniverse {a}"));
    }
}

#[test]
fn screener_pick() {
    let items: Vec<(&str, Option<f64>, &str)> = vec![
        ("A", Some(99.0), "Tech"),
        ("B", Some(98.0), "Tech"),
        ("C", Some(97.0), "Tech"),
        ("D", Some(96.0), "Tech"),
        ("E", Some(95.0), "Santé"),
        ("F", None, "Santé"),
        ("G", Some(10.0), "Énergie"),
        ("H", Some(95.0), ""),
        ("I", Some(97.0), "Tech"),
    ];
    for c in golden("screener-pick") {
        let n = c.args["n"].as_u64().unwrap() as usize;
        let out: Vec<&str> = if c.args["sectors"].as_bool().unwrap() {
            pick(&items, |x| x.1, |x| Some(x.2.to_string()), n)
        } else {
            pick(&items, |x| x.1, |_| None, n)
        }
        .into_iter()
        .map(|x| x.0)
        .collect();
        assert_same(&to_value(&out), &c.output, &format!("pick {}", c.args));
    }
}

fn longs() -> Vec<Vec<Candle>> {
    align_series(&SYMBOLS.map(|s| exact(s, "long").to_vec()), false)
}
fn crypto_hourly() -> Vec<Vec<Candle>> {
    align_series(&["BTC", "ETH", "SOL", "DOGE"].map(|s| exact(s, "1h").to_vec()), true)
}
fn times(v: &[Vec<Candle>]) -> Value {
    to_value(&v.iter().map(|s| s.iter().map(|c| c.time).collect::<Vec<_>>()).collect::<Vec<_>>())
}

#[test]
fn screener_align() {
    let cases = golden("screener-align");
    assert_same(&times(&longs()), &cases[0].output, "align long");
    assert_same(&times(&crypto_hourly()), &cases[1].output, "align 1h");
    let mixed = align_series(&[exact("BTC", "1h").to_vec(), exact("AAPL", "1h").to_vec()], false);
    assert_same(&times(&mixed), &cases[2].output, "align 1h+1d");
}

#[test]
fn screener_validate() {
    let (long, hourly) = (longs(), crypto_hourly());
    let sectors: Vec<String> = ["A", "A", "B", "B", "B", "C", "C"].map(String::from).to_vec();
    for c in golden("screener-validate") {
        let a = &c.args;
        let name = s(&a["name"]);
        let sp = spec_named(name);
        let series = if s(&a["series"]) == "1h" { &hourly } else { &long };
        let out = validate(
            series,
            &sp,
            screener::Horizon::parse(s(&a["h"])).unwrap(),
            a["topN"].as_u64().unwrap() as usize,
            if a["sectors"].as_bool().unwrap() { Some(sectors.as_slice()) } else { None },
            a["bench"].as_u64().map(|b| b as usize),
        );
        assert_same(&to_value(&out), &c.output, &format!("validate {name}"));
    }
}

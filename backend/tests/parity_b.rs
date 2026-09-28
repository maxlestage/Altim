//! Macro and guard engines, and their server glue, vs the TypeScript (golden files from `bun parity/golden.ts`,
//! section `parity/golden-engines-b.ts`).
mod common;
use std::collections::HashMap;
use std::sync::LazyLock;

use altim::engine::guard::{
    Direction, GuardInput, MacroContext, NewsItem, Positioning, SentimentInput, Trend, divergence, guard, hourly_shock_factors, news_tone,
    percentile_rank, pivots, regime, reversal, reversal_evidence, shock, shock_evidence, technical_reversal, weigh,
};
use altim::engine::macro_ctx::{
    MacroKey, MacroLevel, MacroSeries, align, headline_themes, macro_advice, macro_evidence, macro_report, market_stress,
};
use altim::engine::signal::{atr, ema, rsi};
use altim::guard::{
    FEAR_GREED_URL, VIX_URL, assemble_positioning, assemble_sentiment, build_report, news_url, okx_urls, parse_guard, stocktwits_url,
};
use altim::js::to_value;
use altim::macro_data::{MACRO_SYMBOLS, assemble_series, dedupe_news, parse_series};
use altim::types::{Candle, Interval, Kind};
use common::*;
use serde::Deserialize;
use serde_json::{Value, json};

struct Sets {
    series: HashMap<String, MacroSeries>,
    headlines: Vec<NewsItem>,
    macros: Vec<Option<MacroContext>>,
}

static SETS: LazyLock<Sets> = LazyLock::new(|| {
    let mut series = HashMap::new();
    let (mut headlines, mut macros) = (vec![], vec![]);
    for c in golden("macro-sets") {
        let name = c.args["name"].as_str().unwrap().to_string();
        match name.as_str() {
            "headlines" => headlines = serde_json::from_value(c.output).unwrap(),
            "macros" => macros = serde_json::from_value(c.output).unwrap(),
            _ => {
                series.insert(name, serde_json::from_value(c.output).unwrap());
            }
        }
    }
    Sets { series, headlines, macros }
});

fn set(a: &Value) -> &'static MacroSeries {
    &SETS.series[a["set"].as_str().unwrap()]
}
fn from<T: for<'de> Deserialize<'de>>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap()
}
fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}
fn n(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn cut(c: &[Candle], k: usize) -> &[Candle] {
    &c[..c.len().saturating_sub(k)]
}
fn dir(v: &Value) -> Direction {
    if s(v) == "down" { Direction::Down } else { Direction::Up }
}
fn tf(v: &Value) -> Interval {
    Interval::parse(s(v)).unwrap()
}

#[test]
fn macro_engine() {
    let mut count = 0;
    for c in golden("macro") {
        let a = &c.args;
        let out = match s(&a["fn"]) {
            "align" => {
                let al = align(set(a));
                let values: serde_json::Map<String, Value> =
                    MacroKey::ALL.iter().map(|k| (k.as_str().to_string(), json!(al.values[k.index()]))).collect();
                json!({ "dates": al.dates, "times": al.times, "values": values })
            }
            "marketStress" => to_value(&market_stress(&align(set(a)).values, n(&a["i"]))),
            "macroReport" => {
                let news: Vec<NewsItem> = match s(&a["news"]) {
                    "all" => SETS.headlines.clone(),
                    "few" => SETS.headlines[..5].to_vec(),
                    _ => vec![],
                };
                to_value(&macro_report(set(a), &news, a["now"].as_i64().unwrap()))
            }
            "macroEvidence" => to_value(&macro_evidence(&find(s(&a["symbol"]), s(&a["interval"])).unwrap().candles, set(a))),
            "macroEvidenceCandles" => to_value(&macro_evidence(&from::<Vec<Candle>>(&a["candles"]), set(a))),
            "headlineThemes" => to_value(&headline_themes(&from::<Vec<NewsItem>>(&a["items"]), a["now"].as_i64().unwrap())),
            "macroAdvice" => {
                let level: MacroLevel = from(&a["level"]);
                to_value(&macro_advice(level, s(&a["horizon"])))
            }
            f => panic!("cas inconnu {f}"),
        };
        assert_same(&out, &c.output, &format!("macro {a}"));
        count += 1;
    }
    assert!(count > 900);
}

/// A guard input as JSON (the TypeScript `GuardInput`), owned.
#[derive(Deserialize)]
struct OwnedInput {
    kind: Kind,
    daily: Vec<Candle>,
    h4: Vec<Candle>,
    h1: Vec<Candle>,
    positioning: Option<Positioning>,
    sentiment: Option<SentimentInput>,
    news: Option<Vec<NewsItem>>,
    vix: Option<Vec<f64>>,
    #[serde(rename = "macro")]
    macro_ctx: Option<MacroContext>,
    now: i64,
}

impl OwnedInput {
    fn input(&self) -> GuardInput<'_> {
        GuardInput {
            kind: self.kind,
            daily: &self.daily,
            h4: &self.h4,
            h1: &self.h1,
            positioning: self.positioning.as_ref(),
            sentiment: self.sentiment.as_ref(),
            news: self.news.as_deref(),
            vix: self.vix.as_deref(),
            macro_ctx: self.macro_ctx.as_ref(),
            now: self.now,
        }
    }
}

/// `realInput` of golden-engines-b.ts: candles referenced by symbol, the other inputs inline.
fn real_input(a: &Value) -> OwnedInput {
    let sym = s(&a["symbol"]);
    let i = find(sym, s(&a["daily"])).unwrap();
    OwnedInput {
        kind: a.get("kind").map(from).unwrap_or(i.kind),
        daily: cut(&i.candles, n(&a["cutD"])).to_vec(),
        h4: cut(&find(sym, "4h").unwrap().candles, n(&a["cutH4"])).to_vec(),
        h1: cut(&find(sym, "1h").unwrap().candles, n(&a["cutH1"])).to_vec(),
        positioning: a.get("positioning").map(from).unwrap_or(None),
        sentiment: a.get("sentiment").map(from).unwrap_or(None),
        news: a.get("news").map(from).unwrap_or(None),
        vix: a.get("vix").map(from).unwrap_or(None),
        macro_ctx: a.get("macro").map(from).unwrap_or(None),
        now: a["now"].as_i64().unwrap(),
    }
}

#[test]
fn guard_real() {
    let cases = golden("guard-real");
    assert!(cases.len() > 100);
    for c in cases {
        let input = real_input(&c.args);
        assert_same(&to_value(&guard(&input.input())), &c.output, &format!("guard {}", c.args));
    }
}

#[test]
fn guard_scenarios() {
    for c in golden("guard-scenarios") {
        let input: OwnedInput = from(&c.args["input"]);
        let out = match c.args.get("trend") {
            Some(t) => to_value(&reversal(&input.input(), from::<Trend>(t))),
            None => to_value(&guard(&input.input())),
        };
        assert_same(&out, &c.output, &format!("scénario {}", c.args["name"]));
    }
}

#[test]
fn guard_parts() {
    for c in golden("guard-parts") {
        let a = &c.args;
        let sym = a.get("symbol").and_then(|x| x.as_str()).unwrap_or("");
        let out = match s(&a["fn"]) {
            "regime" => to_value(&regime(&find(sym, s(&a["daily"])).unwrap().candles, cut(&find(sym, "4h").unwrap().candles, n(&a["h4cut"])))),
            "regimeNoH4" => to_value(&regime(&find(sym, "1d").unwrap().candles, &[])),
            "shockEvidence" => {
                let k = n(&a["cut"]);
                to_value(&shock_evidence(cut(&find(sym, "1h").unwrap().candles, k), cut(&find(sym, "4h").unwrap().candles, k)))
            }
            "reversalEvidence" => to_value(&reversal_evidence(&find(sym, s(&a["interval"])).unwrap().candles, tf(&a["tf"]))),
            "technicalReversal" => {
                let c = &find(sym, s(&a["interval"])).unwrap().candles;
                let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
                let (r, e20, at) = (rsi(&closes, 14), ema(&closes, 20), atr(c, 14));
                let mut out = vec![];
                for i in (0..c.len()).step_by(3) {
                    for d in ["down", "up"] {
                        let d = dir(&json!(d));
                        out.push(json!({
                            "i": i, "dir": d,
                            "f": technical_reversal(c, &r, &e20, &at, i, d, tf(&a["tf"])),
                            "div": divergence(c, &r, d, i as i64, 60),
                        }));
                    }
                }
                to_value(&out)
            }
            "pivots" => {
                let c = &find(sym, s(&a["interval"])).unwrap().candles;
                let highs: Vec<f64> = c.iter().map(|x| x.high).collect();
                let lows: Vec<f64> = c.iter().map(|x| x.low).collect();
                json!({ "high": pivots(&highs, true, 3, highs.len() as i64 - 1), "low": pivots(&lows, false, 5, c.len() as i64 - 10) })
            }
            "hourlyShockFactors" => {
                let h1 = &find(sym, "1h").unwrap().candles;
                let r: Vec<f64> = h1.windows(2).map(|w| (w[1].close / w[0].close).ln()).collect();
                let out: Vec<Value> = (90..=r.len()).step_by(2).map(|i| json!({ "i": i, "f": hourly_shock_factors(h1, &r, i) })).collect();
                to_value(&out)
            }
            "percentileRank" => json!(percentile_rank(&from::<Vec<f64>>(&a["values"]), a["x"].as_f64().unwrap())),
            "weigh" => {
                let e: Option<altim::engine::Evidence> = from(&a["e"]);
                let (weight, status) = weigh(e.as_ref(), a["historical"].as_bool().unwrap());
                to_value(&json!({ "weight": weight, "status": status }))
            }
            "newsTone" => to_value(&news_tone(&from::<Vec<NewsItem>>(&a["items"]))),
            "divergenceRef" => {
                let c: Vec<Candle> = from(&a["candles"]);
                let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
                let r = rsi(&closes, 14);
                let until = c.len() as i64 - 1;
                json!({ "down": divergence(&c, &r, Direction::Down, until, 60), "up": divergence(&c, &r, Direction::Up, until, 60) })
            }
            "pivotsRef" => {
                let v = [1.0, 2.0, 3.0, 9.0, 3.0, 2.0, 1.0, 2.0, 3.0, 4.0, 10.0, 4.0, 3.0, 2.0];
                json!(pivots(&v, true, 3, v.len() as i64 - 1))
            }
            "shock" => to_value(&shock(&real_input(&a["input"]).input())),
            f => panic!("cas inconnu {f}"),
        };
        assert_same(&out, &c.output, &format!("guard-parts {}", a["fn"]));
    }
}

#[test]
fn guard_parsers() {
    for c in golden("guard-parse") {
        let (f, d) = (s(&c.args["fn"]), &c.args["d"]);
        let out = match f {
            "funding" => json!(parse_guard::funding(d)),
            "rubik" => json!(parse_guard::rubik(d, 1)),
            "fearGreed" => json!(parse_guard::fear_greed(d)),
            "stocktwits" => to_value(&parse_guard::stocktwits(d)),
            "vix" => json!(parse_guard::vix(d)),
            "rss" => to_value(&parse_guard::rss(s(d))),
            f => panic!("cas inconnu {f}"),
        };
        assert_same(&to_value(&out), &c.output, &format!("parseGuard.{f} {d}"));
    }
}

/// guardReport with the same upstream answers as the TypeScript (mocked there): parsers, assembly and report.
#[test]
fn guard_report() {
    let cases = golden("guard-report");
    assert_eq!(cases.len(), 7);
    for c in cases {
        let a = &c.args;
        let (sym, name) = (s(&a["symbol"]), s(&a["name"]));
        let kind: Kind = from(&a["kind"]);
        let body = |k: &str| -> Option<&Value> { a["bodies"].get(k).filter(|b| b["status"] == 200).map(|b| &b["body"]) };
        let crypto = kind == Kind::Crypto;
        let mut urls = vec![stocktwits_url(sym, kind), news_url(sym, kind, name)];
        if crypto {
            urls.extend(okx_urls(sym));
            urls.push(FEAR_GREED_URL.into());
        } else {
            urls.push(VIX_URL.into());
        }
        urls.sort();
        assert_eq!(to_value(&urls), a["urls"], "URL {sym}");
        let missing: Vec<String> = from(&a["missing"]);
        let candles = |iv: &str| -> Result<Vec<Candle>, String> {
            match find(sym, iv) {
                Some(i) if !missing.iter().any(|m| m == iv) => Ok(i.candles.clone()),
                _ => Err(format!("bougies {iv} indisponibles")),
            }
        };
        let out = match (candles("1d"), candles("4h")) {
            (Err(e), _) | (_, Err(e)) => json!({ "error": e }),
            (Ok(daily), Ok(h4)) => {
                let h1 = candles("1h").unwrap_or_default();
                let pos = if crypto { assemble_positioning(body("funding"), body("ls"), body("oi")) } else { None };
                let sent =
                    assemble_sentiment(if crypto { body("fng").map(parse_guard::fear_greed) } else { None }, body("st").map(parse_guard::stocktwits));
                let items = body("news").map(|b| parse_guard::rss(s(b))).unwrap_or_default();
                let vix = if crypto { vec![] } else { body("vix").map(parse_guard::vix).unwrap_or_default() };
                let mac = a["macro"].as_u64().and_then(|i| SETS.macros[i as usize].clone());
                to_value(&build_report(sym, kind, now(), &daily, &h4, &h1, pos.as_ref(), &sent, &items, &vix, mac.as_ref()))
            }
        };
        assert_same(&out, &c.output, &format!("guardReport {sym}"));
    }
}

/// macro() with the same Yahoo and Google News answers as the TypeScript.
#[test]
fn macro_server() {
    for c in golden("macro-server") {
        let a = &c.args;
        let entries = MACRO_SYMBOLS
            .iter()
            .map(|(k, sym)| {
                let b = a["yahoo"].get(*sym).filter(|b| b["status"] == 200).map(|b| &b["body"]);
                (*k, b.and_then(|b| parse_series(b).ok()).unwrap_or_default())
            })
            .collect();
        let out = match assemble_series(entries) {
            Err(e) => json!({ "error": e.0 }),
            Ok(series) => {
                let lists =
                    a["news"].as_array().unwrap().iter().map(|b| if b["status"] == 200 { parse_guard::rss(s(&b["body"])) } else { vec![] }).collect();
                to_value(&macro_report(&series, &dedupe_news(lists), now()))
            }
        };
        assert_same(&out, &c.output, &format!("macro() {}", a["name"]));
    }
}

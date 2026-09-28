//! Rust market data layer vs the TypeScript one (golden files from `bun parity/golden.ts`, section golden-server.ts).
mod common;
use altim::js::to_value;
use altim::live::{ChanIds, FEEDS, LiveQuote, live_consensus, us_market_open};
use altim::market::{
    NAN_TIME, Source, aggregate, aggregate_session, blend, closed_only, consensus, cross_check, deviations, ny_open, ny_open_checked, parse,
    parse_stock,
};
use altim::quotes::{self, QuoteResult, QuoteSourceStatus, SourceQuote, combine};
use altim::stocks_extra;
use altim::types::{Asset, Candle, Interval, Kind};
use common::*;
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{Value, json};

const H: i64 = 3_600_000;

fn interval(s: &str) -> Interval {
    Interval::parse(s).unwrap()
}

/// `perturb` of golden-server.ts (plain arithmetic: same doubles as the TypeScript).
fn perturb(c: &[Candle], k: f64, drop: usize) -> Vec<Candle> {
    c.iter()
        .enumerate()
        .filter(|(i, _)| drop == 0 || i % drop != 0)
        .map(|(_, x)| *x)
        .enumerate()
        .map(|(i, x)| {
            let f = 1.0 + (k * ((((i * 7) % 11) as f64) - 5.0)) / 5.0;
            Candle {
                time: x.time,
                open: x.open * f,
                high: x.high * (f + k.abs()),
                low: x.low * (f - k.abs()),
                close: x.close * (1.0 + (k * ((((i * 3) % 7) as f64) - 3.0)) / 3.0),
                volume: x.volume * (1.0 + (i % 3) as f64),
            }
        })
        .collect()
}

fn last(c: &[Candle], n: usize) -> Vec<Candle> {
    c[c.len().saturating_sub(n)..].to_vec()
}

/// Candles as the TypeScript JSON shows them (a NaN time is null).
fn candles_json(c: &[Candle]) -> Value {
    let mut v = to_value(c);
    for (x, k) in v.as_array_mut().unwrap().iter_mut().zip(c) {
        if k.time == NAN_TIME {
            x["time"] = Value::Null;
        }
    }
    v
}

fn attempt<T: Serialize>(r: altim::http::Result<T>) -> Value {
    match r {
        Ok(v) => json!({ "ok": to_value(&v) }),
        Err(e) => json!({ "error": e.0 }),
    }
}
fn attempt_candles(r: altim::http::Result<Vec<Candle>>) -> Value {
    match r {
        Ok(v) => json!({ "ok": candles_json(&v) }),
        Err(e) => json!({ "error": e.0 }),
    }
}
fn attempt_map<T: Serialize>(r: altim::http::Result<IndexMap<String, T>>) -> Value {
    match r {
        Ok(m) => json!({ "map": m.iter().map(|(k, v)| json!([k, to_value(v)])).collect::<Vec<_>>() }),
        Err(e) => json!({ "error": e.0 }),
    }
}

#[test]
fn aggregate_parity() {
    for c in golden("market-aggregate") {
        let a = &c.args;
        let i = find(a["symbol"].as_str().unwrap(), a["interval"].as_str().unwrap()).unwrap();
        let out = aggregate(&i.candles, a["from"].as_i64().unwrap(), a["to"].as_i64().unwrap());
        assert_same(&to_value(&out), &c.output, &format!("aggregate {a}"));
    }
}

#[test]
fn session_parity() {
    for c in golden("market-session") {
        let a = &c.args;
        let per = a["per"].as_u64().unwrap() as usize;
        let candles = if a["symbol"] == "synth" {
            let mut synth = Vec::new();
            for day in [
                "2025-03-06",
                "2025-03-07",
                "2025-03-10",
                "2025-03-11",
                "2025-10-31",
                "2025-11-03",
                "2025-11-04",
                "2026-03-06",
                "2026-03-09",
                "2026-10-30",
                "2026-11-02",
            ] {
                let p: Vec<u32> = day.split('-').map(|x| x.parse().unwrap()).collect();
                let open = ny_open(p[0] as i32, p[1], p[2]);
                for k in 0..7 {
                    let kf = k as f64;
                    synth.push(Candle {
                        time: open + k * H,
                        open: 100.0 + kf,
                        high: 101.0 + kf,
                        low: 99.0 + kf,
                        close: 100.5 + kf,
                        volume: 10.0 + kf,
                    });
                }
            }
            for t in ["2026-11-03T03:00:00Z", "2026-11-03T05:00:00Z"] {
                synth.push(Candle { time: altim::js::parse_date(t).unwrap(), open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: 1.0 });
            }
            synth
        } else {
            find(a["symbol"].as_str().unwrap(), "1h").unwrap().candles.clone()
        };
        assert_same(&to_value(&aggregate_session(&candles, per)), &c.output, &format!("session {a}"));
    }
}

fn sets(symbol: &str, iv: &str) -> [Vec<Candle>; 5] {
    let a = last(&find(symbol, iv).unwrap().candles, 150);
    let b = perturb(&a, 0.0005, 0);
    let c = perturb(&a, -0.0003, 7);
    let d = perturb(&a, 0.04, 0)[50..].to_vec();
    let e = last(&a, 10).into_iter().map(|x| Candle { time: x.time + 1, ..x }).collect();
    [a, b, c, d, e]
}

#[test]
fn deviations_parity() {
    for c in golden("market-deviations") {
        let a = &c.args;
        let [sa, sb, sc, sd, se] = sets(a["symbol"].as_str().unwrap(), a["interval"].as_str().unwrap());
        let series = match a["set"].as_str().unwrap() {
            "abcde" => vec![sa, sb, sc, sd, se],
            "ab" => vec![sa, sb],
            _ => vec![sa],
        };
        // Infinity is null in the TypeScript JSON.
        let out: Vec<Value> = deviations(&series).into_iter().map(|d| if d.is_finite() { json!(d) } else { Value::Null }).collect();
        assert_same(&Value::from(out), &c.output, &format!("deviations {a}"));
    }
}

#[test]
fn blend_parity() {
    for c in golden("market-blend") {
        let a = &c.args;
        let [sa, sb, sc, sd, _] = sets(a["symbol"].as_str().unwrap(), a["interval"].as_str().unwrap());
        let out = match a["set"].as_str().unwrap() {
            "a|bc" => blend(&sa, &[sb, sc]),
            _ => blend(&sc, &[sa, sb, sd]),
        };
        assert_same(&to_value(&out), &c.output, &format!("blend {a}"));
    }
}

#[tokio::test]
async fn consensus_parity() {
    for c in golden("market-consensus") {
        let a = &c.args;
        let iv = interval(a["interval"].as_str().unwrap());
        let base = last(&find(a["symbol"].as_str().unwrap(), a["interval"].as_str().unwrap()).unwrap().candles, 120);
        let series = |name: &str| -> Result<Vec<Candle>, String> {
            Ok(match name {
                "A" => base.clone(),
                "B" => perturb(&base, 0.0005, 0),
                "C" => perturb(&base, -0.0003, 7),
                "Faux" => perturb(&base, 0.04, 0),
                "Panne" => return Err("HTTP 451".into()),
                "Court" => last(&base, 30),
                "Lente" => base[..base.len() - 5].to_vec(),
                "Vide" => vec![],
                "Futur" => {
                    let mut v = base.clone();
                    v.push(Candle { time: altim::js::now_ms() + 10 * H, ..*base.last().unwrap() });
                    v
                }
                _ => unreachable!(),
            })
        };
        let sources: Vec<Source> = a["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                let n = n.as_str().unwrap();
                let s = series(n);
                Source::new(n, move |_, _| {
                    let s = s.clone();
                    async move { s.map_err(altim::http::Error) }
                })
            })
            .collect();
        let target = a["target"].as_u64().map(|t| t as usize);
        let out = match consensus("X", iv, &sources, target, a["tol"].as_f64().unwrap(), closed_only).await {
            Ok(r) => to_value(&r),
            Err(e) => json!({ "error": e.0 }),
        };
        assert_same(&out, &c.output, &format!("consensus {a}"));
    }
}

#[test]
fn cross_check_parity() {
    for c in golden("market-crosscheck") {
        let a = &c.args;
        let quotes: Vec<QuoteSourceStatus> = serde_json::from_value(a["quotes"].clone()).unwrap();
        let known: Vec<String> = serde_json::from_value(a["known"].clone()).unwrap();
        let out = cross_check(a["last"].as_f64().unwrap(), &quotes, &known, a["tol"].as_f64().unwrap());
        assert_same(&to_value(&out), &c.output, &format!("crossCheck {a}"));
    }
}

#[test]
fn ny_open_parity() {
    for c in golden("market-nyopen") {
        let n = |i: usize| c.args[i].as_f64().unwrap_or(f64::NAN);
        let out = match ny_open_checked(n(0), n(1), n(2)) {
            Ok(t) => json!(t),
            Err(e) => json!({ "error": e.0 }),
        };
        assert_same(&out, &c.output, &format!("nyOpen {}", c.args));
    }
}

#[test]
fn parsers_parity() {
    let mut n = 0;
    for c in golden("market-parsers") {
        let a = &c.args;
        let input = &a["input"];
        let f: fn(&Value) -> altim::http::Result<Vec<Candle>> = match (a["module"].as_str().unwrap(), a["parser"].as_str().unwrap()) {
            ("market", "binance") => parse::binance,
            ("market", "okx") => parse::okx,
            ("market", "coinbase") => parse::coinbase,
            ("market", "kraken") => parse::kraken,
            ("market", "kucoin") => parse::kucoin,
            ("market", "gate") => parse::gate,
            ("market", "bitstamp") => parse::bitstamp,
            ("market", "gemini") => parse::gemini,
            ("market", "bitfinex") => parse::bitfinex,
            ("market", "cryptocom") => parse::cryptocom,
            ("market", "bitget") => parse::bitget,
            ("market", "htx") => parse::htx,
            ("market", "poloniex") => parse::poloniex,
            ("market", "hitbtc") => parse::hitbtc,
            ("market", "whitebit") => parse::whitebit,
            ("market", "coinex") => parse::coinex,
            ("market", "xt") => parse::xt,
            ("market", "woox") => parse::woox,
            ("market", "bingx") => parse::bingx,
            ("market", "lbank") => parse::lbank,
            ("stock", "yahoo") => parse_stock::yahoo,
            ("stock", "nasdaq") => parse_stock::nasdaq,
            ("stock", "robinhood") => parse_stock::robinhood,
            ("stock", "stockanalysis") => parse_stock::stockanalysis,
            ("stock", "webull") => parse_stock::webull,
            ("stock", "cboe") => parse_stock::cboe,
            other => panic!("parseur inconnu {other:?}"),
        };
        assert_same(&attempt_candles(f(input)), &c.output, &format!("{}.{} {input}", a["module"], a["parser"]));
        n += 1;
    }
    assert!(n > 300);
}

fn asset(symbol: &str, kind: Kind, gecko: Option<&str>) -> Asset {
    Asset { symbol: symbol.into(), name: symbol.into(), kind, gecko: gecko.map(String::from) }
}

#[test]
fn quote_parsers_parity() {
    let bases: Vec<String> = ["BTC", "SOL", "BNB", "DOGE"].map(String::from).to_vec();
    let geckos = [asset("BTC", Kind::Crypto, Some("bitcoin")), asset("ETH", Kind::Crypto, Some("ethereum"))];
    for c in golden("quotes-parsers") {
        let a = &c.args;
        let d = &a["input"];
        let out = match a["parser"].as_str().unwrap() {
            "binance" => attempt_map(quotes::parse::binance(d)),
            "okx" => attempt_map(quotes::parse::okx(d, &bases)),
            "kraken" => attempt_map(quotes::parse::kraken(d, &bases)),
            "bitfinex" => attempt_map(quotes::parse::bitfinex(d)),
            "coingecko" => attempt_map(quotes::parse::coingecko(d, &geckos)),
            "nasdaq" => attempt(quotes::parse::nasdaq(d)),
            "cboe" => attempt(quotes::parse::cboe(d)),
            "robinhood" => attempt_map(quotes::parse::robinhood(d)),
            "tradingview" => attempt_map(quotes::parse::tradingview(d)),
            "webull" => attempt(quotes::parse::webull(d)),
            "zacks" => attempt_map(quotes::parse::zacks(d)),
            "webullTicker" => attempt(quotes::parse::webull_ticker(d, "BRK-B")),
            "yahoo" => attempt(quotes::parse::yahoo(d)),
            other => panic!("parseur inconnu {other}"),
        };
        assert_same(&out, &c.output, &format!("quotes.{} {d}", a["parser"]));
    }
}

#[test]
fn extra_parsers_parity() {
    use stocks_extra::parse as x;
    for c in golden("extra-parsers") {
        let a = &c.args;
        let d = &a["input"];
        let sym = a["symbol"].as_str().unwrap_or("");
        let text = d.as_str().unwrap_or("");
        let out = match a["parser"].as_str().unwrap() {
            "wsj" => attempt_candles(x::wsj(d)),
            "alphaquery" => attempt_candles(x::alphaquery(d)),
            "finviz" => attempt_candles(x::finviz(d)),
            "ft" => attempt_candles(x::ft(d)),
            "ftXid" => attempt(x::ft_xid(d, sym)),
            "etoro" => attempt_candles(x::etoro(d)),
            "etoroIds" => attempt_map(x::etoro_ids(d)),
            "fidelity" => {
                let r = attempt_map(x::fidelity(text));
                // JavaScriptCore's JSON.parse messages are not reproduced ("JSON invalide", as crate::http).
                if c.output.get("error").is_some() {
                    assert!(r.get("error").is_some(), "fidelity {d}: {r}");
                    continue;
                }
                r
            }
            "stockcharts" => attempt(x::stockcharts(d)),
            "tipranks" => attempt(x::tipranks(d)),
            "publicCom" => attempt(x::public_com(text, sym)),
            other => panic!("parseur inconnu {other}"),
        };
        assert_same(&out, &c.output, &format!("extra.{} {d} {sym}", a["parser"]));
    }
}

#[test]
fn combine_parity() {
    for c in golden("quotes-combine") {
        let assets: Vec<Asset> = serde_json::from_value(c.args["assets"].clone()).unwrap();
        let results: Vec<QuoteResult> = c.args["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| QuoteResult {
                name: r["name"].as_str().unwrap().into(),
                kind: Kind::parse(r["kind"].as_str().unwrap()).unwrap(),
                quotes: r["quotes"].as_array().map(|q| {
                    q.iter()
                        .map(|e| {
                            let price = e[1]["price"].as_f64().unwrap_or(f64::NAN);
                            (
                                e[0].as_str().unwrap().to_string(),
                                SourceQuote { price, change: e[1].get("change").map(|c| c.as_f64().unwrap_or(f64::NAN)) },
                            )
                        })
                        .collect()
                }),
                error: r["error"].as_str().map(String::from),
            })
            .collect();
        assert_same(&to_value(&combine(&assets, &results)), &c.output, "combine");
    }
}

/// `JSON.stringify` replacer of golden-server.ts: "time" and "id" (clock-dependent) set to 0.
fn zero_clock(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if k == "time" || k == "id" {
                    *x = json!(0);
                } else {
                    zero_clock(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(zero_clock),
        _ => {}
    }
}

#[test]
fn feeds_parity() {
    let live: Value =
        serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/live-samples.json")).unwrap()).unwrap();
    for c in golden("live-feeds") {
        let name = c.args["feed"].as_str().unwrap();
        let f = FEEDS.iter().find(|f| f.name == name).unwrap();
        if c.args.get("subscribe").is_some() {
            let b: Vec<String> = serde_json::from_value(c.args["subscribe"].clone()).unwrap();
            let mut out = json!({
                "subscribe": (f.subscribe)(&b),
                "unsubscribe": (f.unsubscribe)(&b[..2]),
                "ping": f.ping.map(|p| json!({ "every": p.every.as_millis() as u64, "message": [(p.message)()] })),
                "url": f.url,
            });
            zero_clock(&mut out);
            assert_same(&out, &c.output, &format!("feed {name} subscribe"));
            continue;
        }
        let mut msgs: Vec<Value> = live[name].as_array().cloned().unwrap_or_default();
        msgs.extend(
            serde_json::from_value::<Vec<Value>>(json!([
                { "event": "subscribe" }, [1, "hb"], null, "pong", { "arg": { "channel": "tickers" }, "data": [{ "instId": "ETH-USDT", "last": "0", "open24h": "1" }, { "instId": "SOL-USDT", "last": "150.5", "open24h": "0" }] },
                { "type": "ticker", "product_id": "PEPE-USD", "price": "0.0000123", "open_24h": "" }, { "channel": "ticker", "data": [{ "symbol": "DOGE/USD", "last": 0.2, "change_pct": "x" }] },
                { "event": "subscribed", "channel": "ticker", "chanId": 7, "symbol": "tDOGE:USD" }, [7, [1, 2, 3, 4, 5, -0.01, 0.21, 8]], [7, [1, 2, 3, 4, 5, null, 0.21, 8]],
                { "arg": { "channel": "ticker" }, "data": [{ "instId": "PEPEUSDT", "lastPr": "0.00001", "open24h": "0.000011" }] },
                { "channel": "spot.tickers", "event": "update", "result": { "currency_pair": "ETH_USDT", "last": "3000.1", "change_percentage": "-1.2" } }, { "channel": "spot.tickers", "event": "subscribe", "result": { "status": "success" } },
                { "method": "subscribe", "result": { "channel": "ticker", "instrument_name": "ETH_USDT", "data": [{ "a": "3000", "c": "-0.012" }, { "a": "0", "c": "1" }] } }
            ]))
            .unwrap(),
        );
        let mut ids = ChanIds::default();
        let parsed: Vec<Value> = msgs.iter().map(|m| to_value(&(f.parse)(m, &mut ids))).collect();
        let replies: Vec<Value> = msgs.iter().map(|m| f.reply.and_then(|r| r(m)).unwrap_or(Value::Null)).collect();
        let out = json!({ "parsed": parsed, "chanIds": ids.0.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(), "reply": replies });
        assert_same(&out, &c.output, &format!("feed {name}"));
    }
}

#[test]
fn live_consensus_parity() {
    for c in golden("live-consensus") {
        let a = &c.args;
        let q: IndexMap<String, LiveQuote> = a["quotes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e[0].as_str().unwrap().to_string(),
                    LiveQuote { price: e[1]["price"].as_f64().unwrap(), change: e[1]["change"].as_f64(), time: e[1]["time"].as_i64().unwrap() },
                )
            })
            .collect();
        let out = live_consensus(&q, a["now"].as_i64().unwrap(), a["maxAge"].as_i64().unwrap(), a["tol"].as_f64().unwrap());
        assert_same(&to_value(&out), &c.output, &format!("liveConsensus {a}"));
    }
}

#[test]
fn us_market_parity() {
    for c in golden("live-market") {
        assert_eq!(json!(us_market_open(c.args.as_i64().unwrap())), c.output, "usMarketOpen {}", c.args);
    }
}

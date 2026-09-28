//! Port of web/test/market.test.ts and web/test/sources.test.ts (offline).
use altim::market::{
    NAN_TIME, SOURCES, STOCK_SOURCES, Source, aggregate, blend, closed_only, consensus, cross_check, deviations, ny_open, parse, parse_stock,
};
use altim::quotes::{self, QuoteSourceStatus};
use altim::types::{Candle, Interval};
use serde_json::{Value, json};

fn samples() -> Value {
    serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/test/samples.json")).unwrap()).unwrap()
}

fn at(c: &[Candle], t: i64) -> Candle {
    *c.iter().find(|x| x.time == t).unwrap()
}

// Real responses captured on 27/09/2026.
#[test]
fn parseurs_memes_bougies() {
    let s = samples();
    let t = 1_790_496_000_000; // closed candle 07:00 UTC
    let okx = at(&parse::okx(&s["okx"]).unwrap(), t);
    let kucoin = at(&parse::kucoin(&s["kucoin"]).unwrap(), t);
    let gate = at(&parse::gate(&s["gate"]).unwrap(), t);
    let coinbase = at(&parse::coinbase(&s["coinbase"]).unwrap(), t);
    let kraken = at(&parse::kraken(&s["kraken"]).unwrap(), t);
    assert_eq!(okx.close, 84791.7);
    assert_eq!(kucoin.open, 84781.9);
    assert_eq!(gate.close, 84788.3);
    assert_eq!(coinbase.close, 84775.68);
    assert_eq!(kraken.close, 84774.7);
    let all = [okx, kucoin, gate, coinbase, kraken];
    for c in all {
        assert!(c.high >= c.open.max(c.close));
        assert!(c.low <= c.open.min(c.close));
    }
    // Every exchange agrees within 0.05 %.
    let closes: Vec<f64> = all.iter().map(|c| c.close).collect();
    let (max, min) = (closes.iter().cloned().fold(f64::MIN, f64::max), closes.iter().cloned().fold(f64::MAX, f64::min));
    assert!((max / min - 1.0) * 100.0 < 0.05);
}

fn series(factor: f64) -> Vec<Candle> {
    (0..100)
        .map(|i| Candle {
            time: i * 3_600_000,
            open: 100.0 * factor,
            high: 102.0 * factor,
            low: 98.0 * factor,
            close: (100.0 + (i % 5) as f64) * factor,
            volume: 1.0,
        })
        .collect()
}

fn src(name: &str, c: Result<Vec<Candle>, &str>) -> Source {
    let c = c.map_err(|e| altim::http::Error(e.into()));
    Source::new(name, move |_, _| {
        let c = c.clone();
        async move { c }
    })
}

#[test]
fn ecart_source_fausse() {
    let d = deviations(&[series(1.0), series(1.0002), series(1.05)]);
    assert!(d[0] < 0.1);
    assert!(d[2] > 4.0);
}

#[tokio::test]
async fn consensus_bascule_et_ecarte() {
    let sources = [src("Panne", Err("451")), src("Faux", Ok(series(1.05))), src("A", Ok(series(1.0))), src("B", Ok(series(1.0001))), src("C", Ok(series(0.9999)))];
    let r = consensus("BTC", Interval::H1, &sources, Some(3), 0.5, closed_only).await.unwrap();
    assert_eq!(r.source, "A");
    assert!(!r.sources.iter().find(|s| s.name == "Faux").unwrap().ok);
    assert_eq!(r.sources.iter().find(|s| s.name == "Panne").unwrap().error.as_deref(), Some("451"));
}

#[test]
fn agregation_1h_4h() {
    let h: Vec<Candle> = (0..8)
        .map(|i| Candle { time: i * 3_600_000, open: i as f64, high: i as f64 + 1.0, low: i as f64 - 1.0, close: i as f64 + 0.5, volume: 1.0 })
        .collect();
    let a = aggregate(&h, 3_600_000, 14_400_000);
    assert_eq!(a.len(), 2);
    assert_eq!(a[0], Candle { time: 0, open: 0.0, high: 4.0, low: -1.0, close: 3.5, volume: 4.0 });
}

#[test]
fn bougies_de_consensus_mediane() {
    let (a, b, c) = (series(1.0), series(1.0002), series(0.9998));
    // Bad tick on one exchange: absurd wick on candle 50.
    let spiked: Vec<Candle> = b.iter().enumerate().map(|(i, x)| if i == 50 { Candle { high: x.high * 1.3, close: x.close * 1.02, ..*x } } else { *x }).collect();
    let out = blend(&a, &[spiked, c]);
    assert_eq!(out.len(), a.len());
    assert!((out[50].high - 102.0).abs() < 1e-6); // median of 102, 132.6, 101.98
    assert!((out[50].close - a[50].close).abs() < 1e-6);
    assert!((out[10].close - a[10].close).abs() < 1e-6);
    assert!(out.iter().all(|k| k.high >= k.open.max(k.close) && k.low <= k.open.min(k.close)));
    assert_eq!(out[0].volume, a[0].volume);
    // A candle only the primary has keeps its values; no other source: unchanged.
    assert_eq!(blend(&a, &[b[10..].to_vec()])[5], a[5]);
    assert_eq!(blend(&a, &[]), a);
}

#[tokio::test]
async fn consensus_bougies_mediane_des_concordantes() {
    let sources = [src("A", Ok(series(1.0004))), src("B", Ok(series(1.0))), src("C", Ok(series(0.9999))), src("Faux", Ok(series(1.05)))];
    let r = consensus("BTC", Interval::H1, &sources, None, 0.5, closed_only).await.unwrap();
    assert_eq!(r.source, "A");
    let last = r.candles[r.candles.len() - 1];
    assert!((last.close - series(1.0)[99].close).abs() < 1e-6);
}

#[test]
fn nan_times_are_never_closed() {
    let c = parse::binance(&json!([["x", "1", "2", "0.5", "1.5", "3"], [3_600_000, "1", "2", "0.5", "1.5", "3"]])).unwrap();
    assert_eq!(c[0].time, NAN_TIME);
    assert_eq!(closed_only(c, Interval::H1).len(), 1);
}

// ---------- sources.test.ts ----------

const T0: i64 = 1_790_503_200_000;
const T1: i64 = 1_790_506_800_000;

fn sample(name: &str) -> Value {
    let s: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/parity/server-samples.json")).unwrap()).unwrap();
    let (group, key) = name.split_once('.').unwrap();
    s[group][key][0].clone()
}
fn times(c: &[Candle]) -> Vec<i64> {
    c.iter().map(|x| x.time).collect()
}

#[test]
fn plateformes_crypto_supplementaires() {
    let bitstamp = parse::bitstamp(&sample("parse.bitstamp")).unwrap();
    assert_eq!(times(&bitstamp), [T0, T1]);
    assert_eq!(bitstamp[0].close, 84865.17);
    let gemini = parse::gemini(&sample("parse.gemini")).unwrap();
    assert_eq!(times(&gemini), [1_790_499_600_000, T0]);
    assert_eq!(gemini[1].close, 84871.45);
    let cdc = parse::cryptocom(&sample("parse.cryptocom")).unwrap();
    assert_eq!(times(&cdc), [T0, T1]);
    assert_eq!(cdc[0].close, 84884.22);
    let bitget = parse::bitget(&sample("parse.bitget")).unwrap();
    assert_eq!(times(&bitget), [T0, T1]);
    assert_eq!((bitget[0].open, bitget[0].close), (84955.72, 84880.21));
    let mexc = parse::binance(&sample("parse.binance")).unwrap();
    assert_eq!(times(&mexc), [T0, T1]);
    assert_eq!(mexc[1].close, 84849.99);
    let htx = parse::htx(&sample("parse.htx")).unwrap();
    assert_eq!(times(&htx), [T0, T1]);
    assert_eq!((htx[0].close, htx[0].high), (84860.22, 85065.26));
    assert!(parse::bitget(&json!({ "code": "40034", "data": null })).is_err());
    assert!(parse::htx(&json!({ "status": "error" })).is_err());
}

#[test]
fn chaque_exchange_interroge_htx_intraday() {
    let mut names: Vec<&str> = SOURCES.iter().map(|s| s.name.as_str()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), SOURCES.len());
    assert!(!SOURCES.iter().find(|s| s.name == "HTX").unwrap().supports(Interval::D1));
}

#[test]
fn actions_robinhood_cboe_seance_new_york() {
    let rh = parse_stock::robinhood(&sample("stock.robinhood")).unwrap();
    assert_eq!(times(&rh), [ny_open(2026, 9, 24), ny_open(2026, 9, 25)]);
    assert_eq!(rh[1].close, 341.07);
    let cb = parse_stock::cboe(&sample("stock.cboe")).unwrap();
    let last = cb[cb.len() - 1];
    assert_eq!((last.time, last.close), (ny_open(2026, 9, 24), 335.92));
    assert_eq!(rh[0].close, last.close);
}

#[test]
fn cours_robinhood_tradingview() {
    let q = quotes::parse::robinhood(&sample("quotes.robinhood")).unwrap();
    assert_eq!(q["AAPL"].price, 341.04);
    assert_eq!(q["BRK-B"].price, 505.43);
    assert_eq!(q.len(), 2);
    let tv = quotes::parse::tradingview(&json!({ "data": [{ "s": "NYSE:BRK.B", "d": ["BRK.B", "NYSE", 505.48, 0.0594] }, { "s": "CBOE:BRK.B", "d": ["BRK.B", "CBOE", 505.5, 0.06] }] })).unwrap();
    assert_eq!(tv["BRK-B"], quotes::SourceQuote { price: 505.48, change: Some(0.0594) });
}

#[test]
fn verification_croisee_intraday() {
    let q = |name: &str, ok: bool, price: Option<f64>, error: Option<&str>| QuoteSourceStatus { name: name.into(), ok, price, error: error.map(String::from) };
    let checks = cross_check(
        100.0,
        &[q("Yahoo Finance", true, Some(100.1), None), q("Nasdaq", true, Some(101.0), None), q("Cboe", true, Some(105.0), None), q("TradingView", false, None, Some("non coté"))],
        &["Yahoo Finance".to_string()],
        2.0,
    );
    let got: Vec<(&str, bool)> = checks.iter().map(|c| (c.name.as_str(), c.ok)).collect();
    assert_eq!(got, [("Nasdaq (cours)", true), ("Cboe (cours)", false), ("TradingView (cours)", false)]);
}

#[test]
fn huit_plateformes_crypto_supplementaires() {
    const A: i64 = 1_790_514_000_000;
    const B: i64 = 1_790_517_600_000;
    let close = |c: &[Candle], t: i64| at(c, t).close;
    let polo = parse::poloniex(&sample("parse.poloniex")).unwrap();
    assert_eq!(times(&polo), [A, B]);
    assert_eq!((polo[0].open, polo[0].high, polo[0].low, polo[0].close), (84792.69, 85142.85, 84613.15, 85022.91));
    let hit = parse::hitbtc(&sample("parse.hitbtc")).unwrap();
    assert_eq!(times(&hit), [A, B]);
    assert_eq!((hit[1].high, hit[1].low, hit[1].close), (85037.17, 84675.11, 84784.83));
    let wb = parse::whitebit(&sample("parse.whitebit")).unwrap();
    assert_eq!(close(&wb, A), 85026.13);
    let cx = parse::coinex(&sample("parse.coinex")).unwrap();
    assert_eq!(cx[0], Candle { time: A, open: 84770.0, high: 85120.0, low: 84630.0, close: 84899.0, volume: cx[0].volume });
    let xt = parse::xt(&sample("parse.xt")).unwrap();
    assert_eq!(times(&xt), [A, B]);
    assert_eq!((xt[0].open, xt[0].high, xt[0].low, xt[0].close), (84802.01, 85159.02, 84637.73, 85030.41));
    let woo = parse::woox(&sample("parse.woox")).unwrap();
    assert_eq!(times(&woo), [A, B]);
    assert_eq!(close(&woo, A), 85030.42);
    let bingx = parse::bingx(&sample("parse.bingx")).unwrap();
    assert_eq!(times(&bingx), [A, B]);
    assert_eq!((bingx[1].low, bingx[1].close), (84682.42, 84765.97));
    let lbank = parse::lbank(&sample("parse.lbank")).unwrap();
    assert_eq!(lbank.iter().map(|c| c.close).collect::<Vec<_>>(), [84891.72, 84796.62]);
    // Same 13:00 candle, all within 0.02 % of each other.
    let closes: Vec<f64> = [polo, cx, xt, woo, bingx, wb].iter().map(|c| close(c, A)).collect();
    let (max, min) = (closes.iter().cloned().fold(f64::MIN, f64::max), closes.iter().cloned().fold(f64::MAX, f64::min));
    assert!(max / min - 1.0 < 0.002);
    assert!(parse::coinex(&json!({ "code": 3008, "data": [] })).is_err());
    assert!(parse::xt(&json!({ "rc": 1, "result": [] })).is_err());
    assert_eq!(SOURCES.len(), 22);
    let intraday: Vec<&str> = SOURCES.iter().filter(|s| s.supports.is_some() && !s.supports(Interval::D1)).map(|s| s.name.as_str()).collect();
    assert_eq!(intraday, ["HTX", "BingX", "LBank"]);
}

#[test]
fn actions_stockanalysis_webull_zacks() {
    let sa = parse_stock::stockanalysis(&sample("stock.stockanalysis")).unwrap();
    let wb = parse_stock::webull(&sample("stock.webull")).unwrap();
    assert_eq!(times(&sa), [ny_open(2026, 9, 24), ny_open(2026, 9, 25)]);
    assert_eq!(times(&wb), [ny_open(2026, 9, 24), ny_open(2026, 9, 25)]);
    for c in [sa[1], wb[1]] {
        assert_eq!((c.open, c.high, c.low, c.close), (336.04, 341.67, 334.53, 341.07));
    }
    assert_eq!(quotes::parse::webull(&sample("quotes.webull")).unwrap(), quotes::SourceQuote { price: 341.07, change: Some(1.53) });
    assert_eq!(quotes::parse::webull_ticker(&sample("quotes.webullTicker"), "BRK-B").unwrap(), Some(916040668.0));
    assert_eq!(quotes::parse::webull_ticker(&sample("quotes.webullTicker"), "BRK-A").unwrap(), None);
    assert_eq!(quotes::parse::zacks(&sample("quotes.zacks")).unwrap()["AAPL"].price, 341.07);
    let names: Vec<&str> = STOCK_SOURCES.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Yahoo Finance", "Nasdaq", "Robinhood", "StockAnalysis", "Webull", "Cboe", "WSJ / MarketWatch", "AlphaQuery", "Finviz", "Financial Times", "eToro"]);
}

#[tokio::test]
async fn source_en_retard_ecartee() {
    const H: i64 = 3_600_000;
    let serie = |n: i64, f: f64| -> Vec<Candle> {
        (0..n).map(|i| Candle { time: i * H, open: 100.0 * f, high: 101.0 * f, low: 99.0 * f, close: (100.0 + (i % 7) as f64) * f, volume: 1.0 }).collect()
    };
    let sources = [src("A", Ok(serie(200, 1.0))), src("B", Ok(serie(200, 1.0001))), src("Lente", Ok(serie(150, 1.0)))];
    let r = consensus("PEPE", Interval::H1, &sources, None, 0.5, closed_only).await.unwrap();
    let late = r.sources.iter().find(|s| s.name == "Lente").unwrap();
    assert!(!late.ok);
    assert!(late.error.as_deref().unwrap().contains("en retard"));
    assert_eq!(r.agreeing, 2);
}

#[test]
fn snapshot_is_send() {
    fn is_send<T: Send>(_: &T) {}
    is_send(&altim::market::snapshot("BTC", altim::types::Kind::Crypto, Interval::H1));
    is_send(&altim::market::long_daily("BTC", altim::types::Kind::Crypto));
    is_send(&altim::quotes::consensus_quotes(&[], &[]));
}

//! Port of web/test/guard.test.ts, guard-scenarios.test.ts, guard-data.test.ts and the macro part of zones.test.ts.
use altim::engine::Evidence;
use altim::engine::guard::{
    Direction, FactorStatus, GUARD, GuardInput, GuardResult, MacroContext, NewsItem, Positioning, Scalping, SentimentInput, ShockLevel, Trend,
    divergence, guard, news_tone, pivots, regime, reversal, reversal_evidence, shock, shock_evidence, technical_reversal, weigh,
};
use altim::engine::macro_ctx::{
    MACRO, MacroLevel, MacroPoint, MacroSeries, align, headline_themes, macro_advice, macro_evidence, macro_report, market_stress,
};
use altim::engine::signal::{atr, ema, rsi};
use altim::guard::{news_query, parse_guard};
use altim::types::{Candle, Interval, Kind};
use serde_json::json;

// Deterministic generator (same seed → same candles), shared scenarios with GuardTests.swift.
fn rng(seed: u32) -> impl FnMut() -> f64 {
    let mut s = seed;
    move || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        s as f64 / 4_294_967_296.0
    }
}
fn series_from(n: usize, step: i64, drift: f64, vol: f64, seed: u32, start: f64, t0: i64) -> Vec<Candle> {
    let mut r = rng(seed);
    let mut p = start;
    (0..n)
        .map(|i| {
            let o = p;
            p *= 1.0 + drift + (r() - 0.5) * 2.0 * vol;
            let hi = o.max(p) * (1.0 + r() * vol * 0.5);
            let lo = o.min(p) * (1.0 - r() * vol * 0.5);
            Candle { time: t0 + i as i64 * step, open: o, high: hi, low: lo, close: p, volume: 1000.0 + r() * 200.0 }
        })
        .collect()
}
fn series(n: usize, step: i64, drift: f64, vol: f64, seed: u32) -> Vec<Candle> {
    series_from(n, step, drift, vol, seed, 100.0, 1_700_000_000_000)
}
const D: i64 = 86_400_000;
const H4: i64 = 14_400_000;
const H1: i64 = 3_600_000;
const NOW: i64 = 1_800_000_000_000;

struct Owned {
    kind: Kind,
    daily: Vec<Candle>,
    h4: Vec<Candle>,
    h1: Vec<Candle>,
    positioning: Option<Positioning>,
    sentiment: Option<SentimentInput>,
    news: Option<Vec<NewsItem>>,
    vix: Option<Vec<f64>>,
    macro_ctx: Option<MacroContext>,
    now: i64,
}
impl Owned {
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
    fn guard(&self) -> GuardResult {
        guard(&self.input())
    }
}
fn calm_up() -> Owned {
    Owned {
        kind: Kind::Crypto,
        daily: series(400, D, 0.004, 0.01, 1),
        h4: series(300, H4, 0.0007, 0.004, 2),
        h1: series(400, H1, 0.0002, 0.002, 3),
        positioning: None,
        sentiment: None,
        news: None,
        vix: None,
        macro_ctx: None,
        now: NOW,
    }
}
fn jumped() -> Owned {
    let mut input = calm_up();
    let h1 = &mut input.h1;
    let n = h1.len();
    let l = h1[n - 1];
    h1[n - 3] = Candle { close: l.open * 0.95, low: l.open * 0.94, volume: 20_000.0, ..h1[n - 3] };
    h1[n - 2] = Candle { open: l.open * 0.95, close: l.open * 0.97, high: l.open * 0.975, low: l.open * 0.945, volume: 18_000.0, ..h1[n - 2] };
    h1[n - 1] = Candle { open: l.open * 0.97, close: l.open * 0.955, high: l.open * 0.975, low: l.open * 0.95, volume: 15_000.0, ..l };
    input
}
fn news(title: &str, time: i64) -> NewsItem {
    NewsItem { title: title.into(), time, source: None }
}
fn crowd_positioning() -> Positioning {
    let mut ls: Vec<f64> = (0..48).map(|i| 1.2 + (i % 5) as f64 * 0.02).collect();
    ls.push(1.9);
    Positioning { funding_rate: Some(0.0008), long_short_ratio: ls, open_interest: vec![] }
}
fn codes(fs: &[altim::engine::guard::GuardFactor]) -> Vec<&str> {
    fs.iter().map(|f| f.code.as_str()).collect()
}

#[test]
fn scenarios_de_reference() {
    let down = Owned {
        kind: Kind::Crypto,
        daily: series(400, D, -0.004, 0.01, 4),
        h4: series(300, H4, -0.0007, 0.004, 5),
        h1: series(400, H1, -0.0002, 0.002, 12),
        positioning: Some(Positioning { funding_rate: Some(-0.0005), ..Default::default() }),
        sentiment: Some(SentimentInput { fear_greed: vec![30.0, 18.0], ..Default::default() }),
        ..calm_up()
    };
    let mut broken_d = series(355, D, 0.003, 0.005, 6);
    broken_d.extend(series_from(25, D, -0.012, 0.005, 7, 289.0, 1_700_000_000_000 + 355 * D));
    let broken = Owned { daily: broken_d, h4: series(300, H4, 0.0, 0.004, 7), h1: series(400, H1, 0.0, 0.003, 13), ..calm_up() };
    let crowd = Owned {
        positioning: Some(crowd_positioning()),
        sentiment: Some(SentimentInput { fear_greed: vec![60.0, 70.0, 86.0], social_bullish: Some(91.0), social_sample: 30.0 }),
        news: Some(vec![
            news("Exchange hacked, $200M stolen", NOW - H1),
            news("SEC opens investigation into token issuer", NOW - 2 * H1),
            news("Bitcoin rally continues", NOW - 3 * H1),
        ]),
        ..calm_up()
    };
    let mut items: Vec<NewsItem> = (0..10).map(|i| news(&format!("Headline {i}"), NOW - (i + 1) * 20 * H1)).collect();
    items.extend((0..6).map(|i| news(&format!("Breaking {i}"), NOW - (i + 1) * H1 / 2)));
    let stock = Owned { kind: Kind::Stock, news: Some(items), vix: Some(vec![18.0, 24.0, 31.0]), ..calm_up() };

    assert_eq!(calm_up().guard().regime.trend, Trend::Up);
    assert_eq!(down.guard().regime.trend, Trend::Down);
    assert_eq!(broken.guard().regime.trend, Trend::Range);
    assert_ne!(jumped().guard().shock.level, ShockLevel::Calm);
    assert!(crowd.guard().reversal.score >= 50.0);
    let s = stock.guard();
    for c in ["newsBurst", "vixHigh", "vixJump"] {
        assert!(codes(&s.shock.factors).contains(&c), "{c}");
    }
}

#[test]
fn regime_tendance_de_fond() {
    assert_eq!(regime(&series(400, D, 0.004, 0.01, 1), &series(300, H4, 0.0007, 0.004, 2)).trend, Trend::Up);
    assert_eq!(regime(&series(400, D, -0.004, 0.01, 4), &series(300, H4, -0.0007, 0.004, 5)).trend, Trend::Down);
    let mut broken = series(355, D, 0.003, 0.005, 6);
    broken.extend(series_from(25, D, -0.012, 0.005, 7, 100.0 * 1.003f64.powi(355), 1_700_000_000_000 + 355 * D));
    assert_eq!(regime(&broken, &series(300, H4, 0.0, 0.004, 7)).trend, Trend::Range);
    assert!(regime(&broken, &[]).text.contains("en dessous de la moyenne 200 jours"));
    assert!(regime(&series(30, D, 0.004, 0.01, 1), &[]).text.contains("insuffisant"));
    let up = regime(&series(400, D, 0.004, 0.01, 1), &series(300, H4, 0.0007, 0.004, 2));
    assert!(up.strength > 0.0 && up.strength <= 100.0);
}

#[test]
fn marche_calme() {
    let g = calm_up().guard();
    assert_eq!(g.regime.trend, Trend::Up);
    assert_eq!(g.shock.level, ShockLevel::Calm);
    assert_eq!(g.policy.scalping, Scalping::Ok);
    assert_eq!(g.policy.size_multiplier, 1.0);
}

#[test]
fn choc_saut_de_prix() {
    let input = jumped();
    let s = shock(&input.input());
    let c = codes(&s.factors);
    assert!(c.contains(&"jump4") && c.contains(&"volume"));
    for f in s.factors.iter().filter(|x| x.code == "jump4" || x.code == "volume") {
        assert_eq!(f.status, FactorStatus::Unproven);
        assert_eq!(f.points, altim::js::round(f.base_points * 0.5));
    }
    assert_ne!(s.level, ShockLevel::Calm);
    assert_ne!(input.guard().policy.scalping, Scalping::Ok);
}

#[test]
fn auto_validation() {
    let ev = |samples, rate, base, lift| Evidence { samples, rate, base, lift };
    assert_eq!(weigh(None, false), (GUARD.unverifiable_weight, FactorStatus::Unverifiable));
    assert_eq!(weigh(Some(&ev(5.0, 80.0, 20.0, 4.0)), true), (0.5, FactorStatus::Unproven));
    assert_eq!(weigh(Some(&ev(100.0, 20.0, 20.0, 1.0)), true), (0.0, FactorStatus::Rejected));
    assert_eq!(weigh(Some(&ev(100.0, 30.0, 20.0, 1.5)), true), (1.0, FactorStatus::Verified));
    assert!((weigh(Some(&ev(100.0, 25.0, 20.0, 1.25)), true).0 - 0.5).abs() < 1e-9);
    assert!((weigh(Some(&ev(100.0, 22.4, 20.0, 1.12)), true).0 - 0.24).abs() < 1e-9);
}

#[test]
fn choc_rafale_et_vix() {
    let mut items: Vec<NewsItem> = (0..10).map(|i| news(&format!("Headline {i}"), NOW - (i + 1) * 20 * H1)).collect();
    items.extend((0..6).map(|i| news(&format!("Breaking {i}"), NOW - (i + 1) * H1 / 2)));
    let stock = Owned { kind: Kind::Stock, news: Some(items), vix: Some(vec![18.0, 24.0, 31.0]), ..calm_up() };
    let s = shock(&stock.input());
    for c in ["newsBurst", "vixHigh", "vixJump"] {
        assert!(codes(&s.factors).contains(&c));
    }
    // VIX is ignored for a crypto.
    let crypto = Owned { vix: Some(vec![18.0, 24.0, 31.0]), ..calm_up() };
    assert!(!codes(&shock(&crypto.input()).factors).contains(&"vixHigh"));
}

#[test]
fn pivots_et_divergence() {
    let v = [1.0, 2.0, 3.0, 9.0, 3.0, 2.0, 1.0, 2.0, 3.0, 4.0, 10.0, 4.0, 3.0, 2.0];
    assert_eq!(pivots(&v, true, 3, v.len() as i64 - 1), vec![3, 10]);
    let mut closes: Vec<f64> = (0..40).map(|i| 100.0 + i as f64 * 0.1).collect();
    closes.extend((0..12).map(|i| 104.0 + i as f64 * 1.5));
    closes.extend((0..10).map(|i| 119.3 - i as f64 * 1.2));
    closes.extend((0..20).map(|i| 108.5 + i as f64 * 0.65));
    closes.extend((0..5).map(|i| 121.0 - i as f64 * 0.8));
    let c: Vec<Candle> = closes
        .iter()
        .enumerate()
        .map(|(i, &x)| Candle { time: i as i64 * H4, open: x, high: x * 1.001, low: x * 0.999, close: x, volume: 1000.0 })
        .collect();
    let r = rsi(&closes, 14);
    let until = c.len() as i64 - 1;
    assert!(divergence(&c, &r, Direction::Down, until, 60));
    assert!(!divergence(&c, &r, Direction::Up, until, 60));
}

#[test]
fn retournement_a_la_baisse() {
    let input = Owned {
        positioning: Some(crowd_positioning()),
        sentiment: Some(SentimentInput { fear_greed: vec![60.0, 70.0, 86.0], social_bullish: Some(91.0), social_sample: 30.0 }),
        news: Some(vec![
            news("Exchange hacked, $200M stolen", NOW - H1),
            news("SEC opens investigation into token issuer", NOW - 2 * H1),
            news("Bitcoin rally continues", NOW - 3 * H1),
        ]),
        ..calm_up()
    };
    let r = reversal(&input.input(), Trend::Up);
    assert_eq!(r.direction, Some(Direction::Down));
    for c in ["funding", "longShort", "fearGreed", "social", "newsTone"] {
        assert!(codes(&r.factors).contains(&c), "{c}");
    }
    assert_eq!(r.factors.iter().find(|f| f.code == "funding").unwrap().base_points, 25.0);
    assert!(r.score >= GUARD.reversal_high);
    let g = input.guard();
    assert!(g.policy.notes.join(" ").contains("retournement à la baisse"));
    assert_ne!(g.policy.scalping, Scalping::Ok);
}

#[test]
fn retournement_a_la_hausse() {
    let input = Owned {
        daily: series(400, D, -0.004, 0.01, 4),
        h4: series(300, H4, -0.0007, 0.004, 5),
        h1: vec![],
        positioning: Some(Positioning { funding_rate: Some(-0.0005), ..Default::default() }),
        sentiment: Some(SentimentInput { fear_greed: vec![30.0, 18.0], ..Default::default() }),
        ..calm_up()
    };
    let r = reversal(&input.input(), Trend::Down);
    assert_eq!(r.direction, Some(Direction::Up));
    assert!(codes(&r.factors).contains(&"funding") && codes(&r.factors).contains(&"fearGreed"));
}

#[test]
fn ton_des_actualites() {
    let items = [
        news("Company hit by lawsuit", 0),
        news("Shares surge after upgrade", 0),
        news("Stock plunges then surges", 0),
        news("Quarterly report published", 0),
        news("Bankruptcy fears", 0),
    ];
    let t = news_tone(&items);
    assert_eq!((t.negative, t.positive), (2, 1));
    // A word inside another word does not count ("banner" is not "ban").
    let t = news_tone(&[news("New banner campaign", 0)]);
    assert_eq!((t.negative, t.positive), (0, 0));
}

#[test]
fn preuves_sans_regard_vers_le_futur() {
    let h4 = series(400, H4, 0.001, 0.01, 9);
    for e in reversal_evidence(&h4, Interval::H4).values() {
        assert!(e.samples > 0.0 && e.rate >= 0.0 && e.rate <= 100.0 && e.base > 0.0);
        assert!((e.lift - e.rate / e.base).abs() < 1e-9);
    }
    assert!(reversal_evidence(&h4[..80], Interval::H4).is_empty());
    for e in shock_evidence(&series(500, H1, 0.0, 0.003, 10), &series(400, H4, 0.0, 0.006, 11)).values() {
        assert!((e.lift - e.rate / e.base).abs() < 1e-9);
    }
    let i = 250;
    let closes: Vec<f64> = h4.iter().map(|c| c.close).collect();
    let before: Vec<String> = technical_reversal(&h4, &rsi(&closes, 14), &ema(&closes, 20), &atr(&h4, 14), i, Direction::Down, Interval::H4)
        .into_iter()
        .map(|f| f.code)
        .collect();
    let altered: Vec<Candle> = h4
        .iter()
        .enumerate()
        .map(|(j, c)| if j > i { Candle { close: c.close * 1.5, high: c.high * 1.5, low: c.low * 1.5, open: c.open * 1.5, ..*c } } else { *c })
        .collect();
    let ac: Vec<f64> = altered.iter().map(|c| c.close).collect();
    let after: Vec<String> = technical_reversal(&altered, &rsi(&ac, 14), &ema(&ac, 20), &atr(&altered, 14), i, Direction::Down, Interval::H4)
        .into_iter()
        .map(|f| f.code)
        .collect();
    assert_eq!(after, before);
}

#[test]
fn donnees_sans_historique_non_verifiees() {
    let input = Owned { positioning: Some(Positioning { funding_rate: Some(0.0008), ..Default::default() }), ..calm_up() };
    let r = reversal(&input.input(), Trend::Up);
    let f = r.factors.iter().find(|x| x.code == "funding").unwrap();
    assert_eq!(f.status, FactorStatus::Unverifiable);
    assert_eq!(f.points, altim::js::round(25.0 * GUARD.unverifiable_weight));
}

// ---------- guard-data.test.ts: real responses (27/09/2026) ----------

#[test]
fn positionnement_okx() {
    let funding = json!({"code": "0", "data": [{"instId": "BTC-USDT-SWAP", "fundingRate": "-0.0000093022018830", "fundingTime": "1790524800000"}]});
    let ls = json!({"code": "0", "data": [["1790517600000", "1.2"], ["1790514000000", "1.27"], ["1790510400000", "1.29"]], "msg": ""});
    let oi = json!({"code": "0", "data": [["1790521200000", "3096187639.1516", "373921486.7371"], ["1790517600000", "3151244635.3541", "379272299.6517"], ["1790514000000", "3131575585.0841", "119727471.7871"]], "msg": ""});
    assert!((parse_guard::funding(&funding).unwrap() - -0.000009302201883).abs() < 1e-15);
    assert_eq!(parse_guard::funding(&json!({"code": "51001", "data": []})), None);
    assert_eq!(parse_guard::rubik(&ls, 1), vec![1.29, 1.27, 1.2]);
    assert_eq!(parse_guard::rubik(&oi, 1), vec![3131575585.0841, 3151244635.3541, 3096187639.1516]);
    assert_eq!(parse_guard::rubik(&json!({"code": "50011", "data": null}), 1), Vec::<f64>::new());
}

#[test]
fn fear_greed_vix_et_rss() {
    let fng = json!({"data": [{"value": "70", "value_classification": "Greed", "timestamp": "1790467200"}, {"value": "74", "value_classification": "Greed", "timestamp": "1790380800"}, {"value": "71", "value_classification": "Greed", "timestamp": "1790294400"}]});
    let vix = json!({"data": [{"date": "2026-09-23", "close": "15.42"}, {"date": "2026-09-24", "close": "15.61"}, {"date": "2026-09-25", "close": "14.87"}]});
    let rss = "<rss><channel><title>Google News</title><item><title>Is It Too Late to Buy Bitcoin After a 32% Rally in Two Months? - 24/7 Wall St.</title><pubDate>Sun, 27 Sep 2026 14:25:00 GMT</pubDate><source url=\"https://247wallst.com\">24/7 Wall St.</source></item><item><title>Bitcoin Holders Are Selling, But This Time It’s Different: What You Need to Know - Yahoo Finance</title><pubDate>Sun, 27 Sep 2026 11:00:58 GMT</pubDate><source url=\"https://finance.yahoo.com\">Yahoo Finance</source></item><item><title><![CDATA[S&amp;P 500 &amp; Bitcoin: “risk-on” returns]]></title><pubDate>Sat, 26 Sep 2026 22:15:03 GMT</pubDate></item><item><title>No date here</title></item></channel></rss>";
    assert_eq!(parse_guard::fear_greed(&fng), vec![71.0, 74.0, 70.0]);
    assert_eq!(parse_guard::vix(&vix), vec![15.42, 15.61, 14.87]);
    let items = parse_guard::rss(rss);
    assert_eq!(items.len(), 3);
    assert!(items[0].title.contains("Is It Too Late to Buy Bitcoin"));
    assert_eq!(items[0].time, 1_790_519_100_000); // Date.UTC(2026, 8, 27, 14, 25, 0)
    assert_eq!(items[0].source.as_deref(), Some("24/7 Wall St."));
    assert_eq!(items[2].title, "S&P 500 & Bitcoin: “risk-on” returns");
}

#[test]
fn requete_actualites() {
    assert_eq!(news_query("BTC", Kind::Crypto, "Bitcoin"), "\"Bitcoin\" crypto");
    assert_eq!(news_query("BRK-B", Kind::Stock, "Berkshire Hathaway, Inc."), "BRK.B stock \"Berkshire Hathaway\"");
    assert_eq!(news_query("NVDA", Kind::Stock, "NVIDIA Corporation"), "NVDA stock \"NVIDIA\"");
}

// ---------- Macro (zones.test.ts) ----------

fn macro_series(stress_at_end: bool, stress_from: usize) -> MacroSeries {
    let t0 = 1_735_689_600_000; // Date.UTC(2025, 0, 1)
    let wave = |i: usize, base: f64, amp: f64| base * (1.0 + amp * (i as f64 / 7.0).sin());
    let end = |i: usize| stress_at_end && i >= stress_from && i < stress_from + 5;
    let mk = |f: &dyn Fn(usize) -> f64| Some((0..400).map(|i| MacroPoint { time: t0 + i as i64 * D, close: f(i) }).collect());
    MacroSeries {
        vix: mk(&|i| if end(i) { 34.0 } else { wave(i, 15.0, 0.05) }),
        spx: mk(&|i| if end(i) { 5000.0 * 0.9 } else { wave(i, 5000.0, 0.005) }),
        oil: mk(&|i| if end(i) { 80.0 * 1.25 } else { wave(i, 80.0, 0.01) }),
        gold: mk(&|i| wave(i, 2000.0, 0.005)),
        dollar: mk(&|i| wave(i, 100.0, 0.002)),
        rates: mk(&|i| wave(i, 4.0, 0.01)),
    }
}
const JAN_2026: i64 = 1_767_225_600_000; // Date.UTC(2026, 0, 1)

fn path(closes: &[f64], t0: i64) -> Vec<Candle> {
    closes
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let prev = if i > 0 { closes[i - 1] } else { c };
            Candle { time: t0 + i as i64 * D, open: prev, high: c.max(prev) * 1.002, low: c.min(prev) * 0.998, close: c, volume: 1000.0 }
        })
        .collect()
}

#[test]
fn stress_de_marche() {
    let calm = macro_report(&macro_series(false, 395), &[], JAN_2026);
    assert_eq!(calm.level, MacroLevel::Calm);
    assert!(calm.factors.is_empty());
    let tense = macro_report(&macro_series(true, 395), &[], JAN_2026);
    let c: Vec<&str> = tense.factors.iter().map(|f| f.code.as_str()).collect();
    for k in ["vixHigh", "vixJump", "spxDrawdown", "oil"] {
        assert!(c.contains(&k), "{k}");
    }
    assert_eq!(tense.level, MacroLevel::High);
    assert_eq!(tense.values.vix.unwrap().value, 34.0);
    let a = align(&macro_series(true, 395));
    assert_eq!(market_stress(&a.values, 300).score, market_stress(&align(&macro_series(false, 395)).values, 300).score);
}

#[test]
fn actualites_themes_et_escalade() {
    let now = 1_790_510_400_000; // Date.UTC(2026, 8, 27, 12)
    let items = [
        news("Country X declares war on country Y - Reuters", now - 3_600_000),
        news("Fed signals more rate hikes as inflation persists - WSJ", now - 7_200_000),
        news("Is an Agentic Bank Run Coming? - Apollo", now - 3_600_000),
        news("New tariffs on steel imports - CNN", now - 20 * 3_600_000),
        news("Old war story", now - 3 * D),
    ];
    let h = headline_themes(&items, now);
    let themes = serde_json::to_value(h.themes.iter().map(|t| t.theme).collect::<Vec<_>>()).unwrap();
    for t in ["geopolitics", "monetary", "trade"] {
        assert!(themes.as_array().unwrap().contains(&json!(t)), "{t}");
    }
    assert_eq!(h.factors.len(), 1);
    assert!(h.factors[0].text.contains("déclaration de guerre"));
    assert!(!h.factors[0].text.contains("panique bancaire"));
}

#[test]
fn conseil_macro_et_garde_fou() {
    assert_eq!(macro_advice(MacroLevel::Calm, "short"), None);
    assert!(macro_advice(MacroLevel::High, "short").unwrap().contains("éviter"));
    assert!(macro_advice(MacroLevel::High, "long").unwrap().contains("échelonnés"));
    let report = macro_report(&macro_series(true, 395), &[], JAN_2026);
    let closes: Vec<f64> = (0..400).map(|i| 100.0 + 40.0 * (i + 1) as f64 / 400.0).collect();
    let daily = path(&closes, 1_704_067_200_000);
    let verified = Evidence { samples: 150.0, rate: 48.0, base: 24.0, lift: 2.0 };
    let with = |report: altim::engine::macro_ctx::MacroReport, evidence: Evidence| Owned {
        kind: Kind::Stock,
        daily: daily.clone(),
        h4: vec![],
        h1: vec![],
        macro_ctx: Some(MacroContext { report, evidence: Some(evidence) }),
        ..calm_up()
    };
    let g = with(report.clone(), verified).guard();
    let f = g.shock.factors.iter().find(|x| x.code == "macro").unwrap();
    assert_eq!(f.status, FactorStatus::Verified);
    assert_eq!(f.points, 35.0);
    assert_ne!(g.shock.level, ShockLevel::Calm);
    let rejected = with(report, Evidence { samples: 50.0, rate: 16.0, base: 16.0, lift: 1.0 }).guard();
    assert_eq!(rejected.shock.factors.iter().find(|x| x.code == "macro").unwrap().points, 0.0);
    assert_eq!(weigh(None, true).1, FactorStatus::Unproven);
    let calm = with(macro_report(&macro_series(false, 395), &[], JAN_2026), verified).guard();
    assert!(!codes(&calm.shock.factors).contains(&"macro"));
}

#[test]
fn preuve_macro_sur_un_actif() {
    let s = macro_series(true, 300);
    let closes: Vec<f64> = (0..400).map(|i| if (302..310).contains(&i) { 60.0 } else { 100.0 + (i as f64 / 5.0).sin() }).collect();
    let e = macro_evidence(&path(&closes, 1_735_689_600_000), &s).unwrap();
    assert!(e.rate > e.base);
    assert!((e.lift - if e.base > 0.0 { e.rate / e.base } else { 1.0 }).abs() < 1e-9);
    const { assert!(MACRO.tense < MACRO.high) };
}

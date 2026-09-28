//! Anomalies of an asset, OKX derivatives and the opportunities scan: parsers on real saved responses
//! (`tests/samples/derivatives/`, trimmed, 28/09/2026) and every threshold on hand-made series.
use altim::derivatives::{self, Liquidation, funding_summary, open_interest_summary, parse, summarize};
use altim::engine::anomalies::{self, Severity, candle_anomalies, price_volume_divergence, volume_spike, z_score};
use altim::engine::opportunities::{Category, metrics, technical_hits};
use altim::opportunities::{stock_fundamental_hit, tvl_hit};
use altim::types::{Candle, DAY_MS};
use serde_json::Value;

fn sample(name: &str) -> Value {
    let path = format!("{}/tests/samples/derivatives/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn candle(i: usize, close: f64, volume: f64) -> Candle {
    Candle { time: i as i64 * DAY_MS, open: close, high: close * 1.01, low: close * 0.99, close, volume }
}

/// `n` flat sessions (price 100, volume 1 000) with a small alternating wiggle so the deviation is never zero.
fn flat(n: usize) -> Vec<Candle> {
    (0..n).map(|i| candle(i, if i % 2 == 0 { 100.0 } else { 101.0 }, 1000.0)).collect()
}

// ---------- OKX (real responses) ----------

#[test]
fn okx_contrat_et_liquidations() {
    let cv = parse::ct_val(&sample("okx-instrument-btc.json")).unwrap();
    assert_eq!(cv, 0.01, "BTC-USDT-SWAP : 0,01 BTC par contrat");
    let page = parse::liquidations(&sample("okx-liquidations-btc.json"), cv).unwrap();
    assert_eq!(page.len(), 40);
    // First order: posSide long, sz 6,31 contracts at 83 448,5 → 0,0631 BTC, 5 265,60 $.
    let o = page[0];
    assert!(o.long);
    assert!(close(o.coins, 0.0631, 1e-12));
    assert!(close(o.usd, 5_265.600_35, 1e-6));
    assert_eq!(o.time, 1_790_626_715_330);
    // Whole page read, last order at 1 790 624 336 866: 38 longs for 745 354,47 $, 2 shorts for 192,15 $.
    let now = 1_790_626_800_000;
    let s = summarize(&page, now, false, "BTC-USDT-SWAP");
    assert_eq!((s.long_count, s.short_count), (38, 2));
    assert!(close(s.long_usd, 745_354.470_07, 1e-4));
    assert!(close(s.short_usd, 192.154_53, 1e-4));
    let big = s.largest.unwrap();
    assert!(big.long && close(big.usd, 312_931.419_8, 1e-4) && big.price == 83_111.5);
    // Partial window: from the oldest order read to now, said in hours.
    assert!(!s.complete);
    assert_eq!(s.from, 1_790_624_336_866);
    assert_eq!(s.hours, 0.7);
    assert!(s.scope.starts_with("OKX seulement"));
    // Second page (after=…): older orders only, same format.
    let p2 = parse::liquidations(&sample("okx-liquidations-btc-page2.json"), cv).unwrap();
    assert_eq!(p2.len(), 20);
    assert!(p2.iter().all(|x| x.time < 1_790_624_336_866));
    // Complete day: the window is the full 24 h and older orders are left out.
    let old = Liquidation { time: now - DAY_MS - 1, price: 1.0, coins: 1e9, usd: 1e9, long: false };
    let mut all = page.clone();
    all.push(old);
    let s = summarize(&all, now, true, "BTC-USDT-SWAP");
    assert_eq!((s.hours, s.short_count), (24.0, 2));
    assert!(s.largest.unwrap().usd < 1e6);
}

#[test]
fn okx_mode_net_et_refus() {
    // Net mode: a forced sale closes a long, a forced buy a short.
    let d = serde_json::json!({"code":"0","data":[{"details":[
        {"bkPx":"100","posSide":"net","side":"sell","sz":"2","ts":"1000"},
        {"bkPx":"100","posSide":"net","side":"buy","sz":"1","ts":"999"}]}]});
    let l = parse::liquidations(&d, 0.1).unwrap();
    assert!(l[0].long && !l[1].long);
    assert!(close(l[0].usd, 20.0, 1e-9));
    // A refusal (region, rate limit): an error that names OKX's code, never an empty "no liquidation" answer.
    let refused = serde_json::json!({"code":"51155","msg":"Local compliance restrictions","data":[]});
    let e = parse::liquidations(&refused, 0.01).unwrap_err();
    assert!(e.0.contains("51155") && e.0.contains("compliance"));
    assert_eq!(parse::ct_val(&refused), None);
    assert!(derivatives::explain_error("Liquidations", &e).contains("OKX peut refuser les adresses américaines"));
}

#[test]
fn okx_open_interest_funding_ratio() {
    let oi = parse::oi_series(&sample("okx-oi-1h-btc.json"));
    assert_eq!(oi.len(), 200);
    assert!(oi.windows(2).all(|w| w[0].0 < w[1].0), "du plus ancien au plus récent");
    let s = open_interest_summary(&oi).unwrap();
    // 3 060 890 723,72 now, 3 108 544 747,93 24 h before, 3 350 195 851,53 7 days before.
    assert_eq!(s.usd, 3_060_890_723.721_8);
    assert!(close(s.change24h.unwrap(), (3_060_890_723.721_8 / 3_108_544_747.928_3 - 1.0) * 100.0, 1e-9));
    assert!(close(s.change7d.unwrap(), -8.6355, 1e-3));
    let a = anomalies::open_interest(&oi.iter().map(|x| x.1).collect::<Vec<_>>(), "OKX").unwrap();
    assert!(!a.triggered && a.severity == Severity::Normal, "−1,5 % en 24 h : normal");
    // A hole in the hourly series: no change computed across it.
    let mut holed = oi.clone();
    holed.remove(oi.len() - 10);
    assert_eq!(open_interest_summary(&holed).unwrap().change24h, None);

    let fh = parse::funding_history(&sample("okx-funding-history-btc.json"));
    assert_eq!(fh.len(), 100);
    assert_eq!(fh.last().unwrap(), &(1_790_611_200_000, 0.0000709507874477));
    let f = funding_summary(&fh).unwrap();
    assert_eq!((f.samples, f.period_hours), (99, Some(8.0)));
    assert!(close(f.rate, 0.00709507874477, 1e-12));
    let rates: Vec<f64> = fh.iter().map(|x| x.1).collect();
    let (&last, prior) = rates.split_last().unwrap();
    assert!(!anomalies::funding(last, prior, "OKX").unwrap().triggered);

    let ls = altim::guard::parse_guard::rubik(&sample("okx-long-short-btc.json"), 1);
    assert_eq!((ls.len(), *ls.last().unwrap()), (200, 1.31));
    assert!(!anomalies::long_short(&ls, "OKX").unwrap().triggered);
}

// ---------- Thresholds ----------

#[test]
fn seuils_volume() {
    let mut c = flat(30);
    c[29].volume = 2_900.0;
    let a = volume_spike(&c, "t").unwrap();
    assert!(!a.triggered && close(a.value, 2.9, 1e-12));
    c[29].volume = 3_000.0;
    let a = volume_spike(&c, "t").unwrap();
    assert!(a.triggered && a.severity == Severity::Warning && a.title == "Volume ×3 par rapport à la moyenne 20 j");
    c[29].volume = 5_000.0;
    assert_eq!(volume_spike(&c, "t").unwrap().severity, Severity::High);
    // Too short or a missing volume: nothing measured.
    assert!(volume_spike(&c[..20], "t").is_none());
    c[15].volume = f64::NAN;
    assert!(volume_spike(&c, "t").is_none());
}

#[test]
fn seuils_divergence_prix_volume() {
    // New 20-session closing high on a volume at 60 % of the 20 sessions before: divergence (warning).
    let mut c = flat(30);
    for (k, x) in c.iter_mut().enumerate().skip(25) {
        x.volume = 600.0;
        x.close = 102.0 + k as f64 * 0.1;
    }
    let a = price_volume_divergence(&c, "t").unwrap();
    assert!(a.triggered && a.severity == Severity::Warning, "{a:?}");
    assert!(close(a.value, 60.0, 1e-9) && a.title.contains("plus haut de 20 séances"));
    // Volume at 45 %: high.
    for x in c.iter_mut().skip(25) {
        x.volume = 450.0;
    }
    assert_eq!(price_volume_divergence(&c, "t").unwrap().severity, Severity::High);
    // Same volume, but the price is not at an extreme: no divergence.
    c[29].close = 100.5;
    let a = price_volume_divergence(&c, "t").unwrap();
    assert!(!a.triggered && a.meaning.starts_with("Rien d'inhabituel"));
    // New low on a normal volume (75 %): no divergence.
    let mut c = flat(30);
    for x in c.iter_mut().skip(25) {
        x.volume = 750.0;
    }
    c[29].close = 95.0;
    let a = price_volume_divergence(&c, "t").unwrap();
    assert!(!a.triggered && a.title.starts_with("Pas de divergence"), "{}", a.title);
}

#[test]
fn seuils_z_score() {
    let mut c = flat(40);
    c[39].close = 101.6;
    let a = z_score(&c, "t").unwrap();
    assert!(!a.triggered);
    c[39].close = 104.0;
    let a = z_score(&c, "t").unwrap();
    // Mean 100,675, population deviation of the 20 closes: z ≈ +3,76 → high.
    assert!(a.triggered && a.severity == Severity::High && a.value > 3.5, "{a:?}");
    c[39].close = 98.0;
    let a = z_score(&c, "t").unwrap();
    assert!(a.triggered && a.value < -2.5 && a.threshold == -2.5 && a.meaning.contains("sans certitude"));
}

#[test]
fn seuils_derives() {
    let series = |d: f64| (0..30).map(|i| if i < 5 { 100.0 } else { 100.0 * (1.0 + d / 100.0) }).collect::<Vec<f64>>();
    // 24 steps back from the last value is index 5: the change is fully inside the 24 h.
    let mut s = series(10.0);
    s[5] = 100.0;
    assert!(!anomalies::open_interest(&s, "t").unwrap().triggered);
    let mut s = series(20.0);
    s[5] = 100.0;
    assert_eq!(anomalies::open_interest(&s, "t").unwrap().severity, Severity::Warning);
    let mut s = series(35.0);
    s[5] = 100.0;
    let a = anomalies::open_interest(&s, "t").unwrap();
    assert!(a.severity == Severity::High && a.title == "Open interest +35 % en 24 h", "{}", a.title);
    let mut s = series(-40.0);
    s[5] = 100.0;
    assert!(anomalies::open_interest(&s, "t").unwrap().meaning.contains("se ferment"));

    // Funding: 100 settlements at 0,01 % (0,0001).
    let hist = vec![0.0001; 100];
    // Above all of them but under 0,02 %: not extreme.
    assert!(!anomalies::funding(0.00015, &hist, "t").unwrap().triggered);
    let a = anomalies::funding(0.0005, &hist, "t").unwrap();
    assert!(a.triggered && a.severity == Severity::Warning && a.title.starts_with("Funding extrême"));
    assert_eq!(anomalies::funding(0.0015, &hist, "t").unwrap().severity, Severity::High);
    let a = anomalies::funding(-0.0003, &hist, "t").unwrap();
    assert!(a.triggered && a.meaning.contains("vendeurs à découvert"));
    // Too few settlements to judge.
    assert!(anomalies::funding(0.01, &hist[..20], "t").is_none());

    // Long/short: 100 hourly values between 1,2 and 1,4, then 1,8.
    let mut ls: Vec<f64> = (0..100).map(|i| 1.2 + (i % 21) as f64 * 0.01).collect();
    ls.push(1.8);
    let a = anomalies::long_short(&ls, "t").unwrap();
    assert!(a.triggered && a.meaning.contains("à la hausse"));
    ls.push(1.3);
    ls.remove(100);
    assert!(!anomalies::long_short(&ls, "t").unwrap().triggered);
}

// ---------- Real candles ----------

/// Gate daily rows [t, quote volume, close, high, low, open, base volume, closed]: closed sessions only.
fn gate(v: &Value) -> Vec<Candle> {
    v.as_array()
        .unwrap()
        .iter()
        .filter(|r| r[7] == "true")
        .map(|r| {
            let n = |i: usize| r[i].as_str().unwrap().parse::<f64>().unwrap();
            Candle { time: n(0) as i64 * 1000, open: n(5), high: n(3), low: n(4), close: n(2), volume: n(6) }
        })
        .collect()
}

#[test]
fn qnt_cassure_sur_volume_reel() {
    let c = gate(&sample("gate-qnt-1d.json"));
    assert_eq!(c.last().unwrap().time, 1_790_467_200_000, "séance du 27/09/2026, la dernière close");
    let hits = technical_hits(&c);
    let cats: Vec<Category> = hits.iter().map(|h| h.category).collect();
    assert!(cats.contains(&Category::Breakout) && cats.contains(&Category::Volume), "{hits:?}");
    let b = hits.iter().find(|h| h.category == Category::Breakout).unwrap();
    assert!(b.reason.contains("plus haut 55 j"), "{}", b.reason);
    let m = metrics(&c).unwrap();
    assert!(m.volume_ratio.unwrap() > 20.0 && m.change1d.unwrap() > 80.0);
    let a = candle_anomalies(&c, "Gate");
    let v = a.iter().find(|x| x.code == "volume").unwrap();
    assert!(v.triggered && v.severity == Severity::High && v.meaning.contains("séance haussière"));
    assert!(b.reason.starts_with("Clôture 286 $"), "{}", b.reason);
    // No look-ahead: the day before reads its own close (151 $, already a breakout of a rally that started on the
    // 23rd), never the 27th's.
    let prev = technical_hits(&c[..c.len() - 1]);
    let pb = prev.iter().find(|h| h.category == Category::Breakout).unwrap();
    assert!(pb.reason.starts_with("Clôture 151 $") && !pb.reason.contains("286"), "{}", pb.reason);
}

#[test]
fn nee_survendu_reel() {
    let d = sample("altim-candles-nee-1d.json");
    let c: Vec<Candle> = serde_json::from_value(d["candles"].clone()).unwrap();
    let m = metrics(&c).unwrap();
    assert!(m.rsi14.unwrap() < 25.0, "{:?}", m.rsi14);
    let hits = technical_hits(&c);
    let o = hits.iter().find(|h| h.category == Category::Oversold).expect("survendu");
    assert!(o.reason.starts_with("RSI 14 à 2"), "{}", o.reason);
    let z = z_score(&c, "t").unwrap();
    assert!(z.value < 0.0);
}

#[test]
fn retournement_rsi_et_moyenne() {
    // Steady fall (RSI deep under 30), then three strong up days: RSI back over 30 and the close back over the
    // 20-day average.
    let mut c: Vec<Candle> = (0..80).map(|i| candle(i, 200.0 - i as f64 * 1.2, 1000.0)).collect();
    for (k, p) in [(80usize, 110.0), (81, 116.0), (82, 124.0)] {
        c.push(candle(k, p, 1500.0));
    }
    let hits = technical_hits(&c);
    let r = hits.iter().find(|h| h.category == Category::Reversal);
    assert!(r.is_some_and(|r| r.reason.contains("RSI 14 sorti de la survente") && r.reason.contains("moyenne 20 j")), "{hits:?}");
    // Only the fall: still oversold, no reversal.
    let hits = technical_hits(&c[..80]);
    assert!(hits.iter().any(|h| h.category == Category::Oversold) && !hits.iter().any(|h| h.category == Category::Reversal));
}

// ---------- Fundamentals ----------

#[test]
fn tvl_defillama_a_perimetre_constant() {
    let t = altim::tokenomics::parse::llama_tvl_month(&sample("defillama-lite-protocols2-tvl.json"));
    let near = t.iter().find(|x| x.symbol == "NEAR").unwrap();
    // NEAR Intents 255,13 M$ + NEAR Bridge 131,31 M$, a month ago 114,09 + 55,14.
    assert!(close(near.tvl, 386_438_605.527, 1e-2) && close(near.month_ago, 169_229_163.336, 1e-2));
    assert_eq!(near.protocols, vec!["NEAR Intents", "NEAR Bridge"]);
    let hit = tvl_hit(near).unwrap();
    assert!(hit.reason.starts_with("TVL +128 % sur 30 j"), "{}", hit.reason);
    // SUNSwap V2 had no TVL a month ago (tracked since): left out, so no fake +86 %.
    let sun = t.iter().find(|x| x.symbol == "SUN").unwrap();
    assert!(!sun.protocols.contains(&"SUNSwap V2".to_string()));
    assert!(tvl_hit(sun).is_none());
    // No token ("-"): not counted.
    assert!(!t.iter().any(|x| x.symbol == "-"));
    // Under 10 M$: ignored whatever the change.
    let small = altim::tokenomics::TvlMonth { symbol: "X".into(), tvl: 9e6, month_ago: 1e6, protocols: vec![] };
    assert!(tvl_hit(&small).is_none());
}

#[test]
fn depot_sec_et_revisions() {
    let path = format!("{}/tests/samples/decision-data/sec-aapl-companyfacts.json", env!("CARGO_MANIFEST_DIR"));
    let facts = altim::fundamentals::parse::company_facts(&serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap());
    let t = altim::fundamentals::filing_trend(&facts).unwrap();
    assert_eq!(t.form, "10-Q");
    // Same figures as the decision's (12 months to 27/06/2026).
    assert_eq!((t.revenue_growth, t.eps_growth), (Some(14.24), Some(32.17)));
    let filed = t.filed;
    // Within 45 days of the filing, a growth change of 5 points (revenue) or 10 points (EPS) is a hit.
    let hit = stock_fundamental_hit(Some(&t), None, filed + 10);
    let moved = matches!((t.revenue_growth, t.revenue_growth_before), (Some(a), Some(b)) if (a - b).abs() >= 5.0)
        || matches!((t.eps_growth, t.eps_growth_before), (Some(a), Some(b)) if (a - b).abs() >= 10.0);
    assert_eq!(hit.is_some(), moved, "{t:?}");
    // 46 days later: no longer recent.
    assert!(stock_fundamental_hit(Some(&t), None, filed + 46).is_none());
    // Revisions: −0,23 % is not a hit, +4 % is.
    let small = altim::engine::decision_types::Revisions { month_ago: 8.76, now: 8.74, change_pct: -0.23 };
    assert!(stock_fundamental_hit(None, Some(&small), filed).is_none());
    let big = altim::engine::decision_types::Revisions { month_ago: 2.0, now: 2.08, change_pct: 4.0 };
    let h = stock_fundamental_hit(None, Some(&big), filed).unwrap();
    assert_eq!(h.reason, "consensus BPA de l'exercice révisé de +4 % en un mois (2,00 → 2,08)");
    // A synthetic trend: revenue growth from +10 % to +20 % filed 3 days ago.
    let s = altim::fundamentals::FilingTrend {
        form: "10-K".into(),
        filed: 20_000,
        end: 19_970,
        revenue_growth: Some(20.0),
        revenue_growth_before: Some(10.0),
        eps_growth: Some(-75.0),
        eps_growth_before: Some(-70.0),
    };
    let h = stock_fundamental_hit(Some(&s), None, 20_003).unwrap();
    assert!(h.reason.starts_with("10-K déposé le ") && h.reason.contains("croissance du CA +20 % sur 12 mois (contre +10 %"), "{}", h.reason);
    assert!(!h.reason.contains("BPA"), "−75 contre −70 : moins de 10 points");
}

/// Live: OKX derivatives of Bitcoin (network; may be refused from a US server).
#[tokio::test]
#[ignore]
async fn derives_okx_live() {
    let (d, found) = derivatives::derivatives("BTC").await;
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    let l = d.liquidations.unwrap();
    assert!(l.long_count + l.short_count > 0 && l.hours > 0.0);
    assert!(d.open_interest.is_some() && d.funding.is_some() && d.long_short.is_some());
    assert_eq!(found.len(), 3);
}

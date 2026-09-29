//! Decision engine (`engine::decision::decide`): real candles of BTC, ETH, SOL, DOGE, AAPL, NVDA and SPY, then
//! hand-made scenarios for every verdict, every veto, the setup states, the risk/reward refusal, the progressive
//! exits, the exposure warning and the two modes.
mod common;

use altim::engine::decision::{DecisionInput, ExposureInput, HoldingSeries, MarketInputs, decide};
use altim::engine::decision_types::*;
use altim::engine::guard::{GuardInput, GuardResult, NewsItem, guard};
use altim::engine::macro_ctx::{MacroFactor, MacroLevel, MacroReport, MacroValue, MacroValues};
use altim::engine::model_evidence::{EVIDENCE_CAP, NOT_COMPUTED};
use altim::engine::reliability::{Reliability, assess_quality, reliability};
use altim::engine::structure::Benchmark;
use altim::engine::synthesis::{DEGRADED_DATA_HEADLINE, DEGRADED_HEADLINE, DEGRADED_RECORD_HEADLINE, Rating, RegimeKind, ScoreWeights};
use altim::engine::validation::{AssetClass, ValidationReport, Verdict as Proof};
use altim::types::{Candle, DAY_MS, Interval, Kind};
use common::{INPUTS, find, now};

// ---------- Helpers ----------

struct Series {
    kind: Kind,
    h1: Vec<Candle>,
    h4: Vec<Candle>,
    d: Vec<Candle>,
    long: Vec<Candle>,
}

fn real(symbol: &str) -> Series {
    let get = |i: &str| find(symbol, i).unwrap_or_else(|| panic!("{symbol} {i}")).candles.clone();
    Series { kind: find(symbol, "1d").unwrap().kind, h1: get("1h"), h4: get("4h"), d: get("1d"), long: get("long") }
}

fn rel_of(d: &[Candle], kind: Kind, t: i64) -> (Reliability, Vec<String>) {
    let q = assess_quality(d, Interval::D1.step(), kind, t);
    (reliability(q.score, 5, false), q.issues)
}

fn input<'a>(symbol: &'a str, s: &'a Series, g: Option<&'a GuardResult>, t: i64) -> DecisionInput<'a> {
    let (reliability, quality_issues) = rel_of(&s.d, s.kind, t);
    DecisionInput {
        symbol,
        kind: s.kind,
        name: symbol,
        now: t,
        price: None,
        h1: &s.h1,
        h4: &s.h4,
        daily: &s.d,
        long: &s.long,
        reliability,
        quality_issues,
        agreeing: 5,
        sources: 6,
        zone_evidence: vec![],
        guard: g,
        macro_report: None,
        macro_evidence: None,
        market: MarketInputs::default(),
        fundamentals: None,
        liquidity: None,
        track: None,
        cost: None,
        exposure: None,
        benchmarks: vec![],
        score_weights: None,
        events: None,
        validation: None,
        bot: None,
    }
}

/// The benchmark of the real candles: Bitcoin for a crypto, the S&P 500 (SPY) for a stock (QQQ is not in the
/// captured inputs).
fn benchmark_of(kind: Kind) -> Benchmark {
    let (symbol, name) = if kind == Kind::Crypto { ("BTC", "Bitcoin (BTC)") } else { ("SPY", "S&P 500 (SPY)") };
    Benchmark { name: name.into(), symbol: symbol.into(), kind, daily: real(symbol).long }
}

fn guard_of(s: &Series, t: i64) -> GuardResult {
    guard(&GuardInput {
        kind: s.kind,
        daily: &s.d,
        h4: &s.h4,
        h1: &s.h1,
        positioning: None,
        sentiment: None,
        news: None,
        vix: None,
        macro_ctx: None,
        now: t,
    })
}

fn veto<'a>(d: &'a Decision, code: &str) -> &'a Veto {
    d.vetoes.iter().find(|v| v.code == code).unwrap_or_else(|| panic!("veto {code} absent"))
}

/// Every verdict path explains itself: a headline, why not buy (unless Buy), pros and cons, at least one
/// invalidation condition, three scenarios, every veto listed, a disclaimer matching the mode.
fn assert_coherent(d: &Decision, what: &str) {
    assert!(!d.headline.is_empty(), "{what}: headline");
    assert_eq!(d.why_wait.is_empty(), d.verdict == Verdict::Buy, "{what}: why_wait {:?}", d.why_wait);
    assert!(!d.pros.is_empty() && !d.cons.is_empty(), "{what}: pros/cons");
    assert!(!d.why_not.invalidation.is_empty() && !d.why_not.risks.is_empty(), "{what}: why_not");
    assert_eq!(d.scenarios.len(), 3, "{what}: scenarios");
    assert!(!d.to_buy.is_empty() && !d.to_sell.is_empty(), "{what}: conditions");
    assert_eq!(d.vetoes.len(), 16, "{what}: vetoes");
    assert_eq!(d.blocked, d.vetoes.iter().any(|v| v.active), "{what}: blocked");
    assert!(d.vetoes.iter().all(|v| v.verifiable || !v.active), "{what}: an unverifiable veto cannot be active");
    assert_eq!(d.label, d.verdict.label());
    assert_eq!(d.level_label, d.level.label());
    assert!((0.0..=100.0).contains(&d.confidence), "{what}: confidence");
    assert!(!d.confidence_text.is_empty());
    assert_eq!(d.setup.steps.len(), 9);
    assert_eq!(d.setup.met, d.setup.steps.iter().filter(|s| s.state == StepState::Ok).count());
    for f in &d.families {
        assert_eq!(f.score.is_none(), f.status == Status::Unavailable, "{what}: family {}", f.key);
        assert!(!f.summary.is_empty() && !f.source.is_empty(), "{what}: family {}", f.key);
        if let Some(s) = f.score {
            assert!((-100.0..=100.0).contains(&s));
        }
    }
    if d.verdict == Verdict::Buy {
        assert!(!d.blocked, "{what}: Buy with a veto");
        let p = d.plan.as_ref().expect("Buy without a plan");
        assert!(p.stop < p.entry && p.entry < p.target1);
    }
    if let Some(p) = &d.plan {
        assert!(p.zone_from <= p.zone_to && p.stop < p.target1, "{what}: plan {p:?}");
        assert!((p.risk_reward - 2.0).abs() < 0.01 || p.acceptable == (p.risk_reward >= 2.0), "{what}: {p:?}");
        assert_eq!(p.min_risk_reward, 2.0);
    }
    assert_eq!(d.mode == "personal", d.position.is_some() || d.exposure.is_some(), "{what}: mode");
    assert!(d.disclaimer.starts_with(if d.mode == "personal" { "Mode personnel" } else { "Mode informationnel" }));
    assert!(matches!(d.verdict, Verdict::Trim | Verdict::Sell) <= d.position.is_some(), "{what}: sell side without a position");
    // Summaries: rating, composite score, degraded signal, horizon, structure.
    assert_eq!(d.rating_label, d.rating.label(), "{what}");
    if d.degraded.active {
        assert!(!matches!(d.rating, Rating::StrongBuy | Rating::Buy), "{what}: degraded but {:?}", d.rating);
        assert!([DEGRADED_HEADLINE, DEGRADED_DATA_HEADLINE, DEGRADED_RECORD_HEADLINE].contains(&d.degraded.headline.as_str()), "{what}");
        assert!(!d.degraded.reasons.is_empty());
    } else {
        assert!(d.degraded.headline.is_empty() && d.degraded.reasons.is_empty(), "{what}");
    }
    match d.verdict {
        Verdict::Buy | Verdict::BuyZone => assert!(matches!(d.rating, Rating::StrongBuy | Rating::Buy | Rating::Hold), "{what}"),
        Verdict::Wait => assert_eq!(d.rating, Rating::Hold, "{what}"),
        Verdict::Trim => assert_eq!(d.rating, Rating::Reduce, "{what}"),
        Verdict::Sell => assert!(matches!(d.rating, Rating::Sell | Rating::StrongSell), "{what}"),
        Verdict::NoPosition => assert!(matches!(d.rating, Rating::Sell | Rating::Hold), "{what}"),
    }
    assert_eq!(d.score.factors.len(), 6, "{what}");
    if let Some(v) = d.score.value {
        assert!((-100.0..=100.0).contains(&v), "{what}: score {v}");
        let applied: f64 = d.score.factors.iter().map(|f| f.applied).sum();
        assert!((applied - 100.0).abs() < 0.5, "{what}: weights {applied}");
        let sum: f64 = d.score.factors.iter().filter_map(|f| f.contribution).sum();
        assert!((sum - v).abs() <= 1.0, "{what}: contributions {sum} vs {v}");
    }
    assert_eq!(d.score.missing.len(), d.score.factors.iter().filter(|f| f.value.is_none()).count());
    assert_eq!(d.horizon.is_some(), d.plan.as_ref().is_some_and(|p| p.target1 > p.entry), "{what}: horizon");
    if let Some(p) = &d.plan {
        assert_eq!(p.target3.is_some(), p.target2.is_some_and(|t| t > p.target1), "{what}: target 3");
        if let (Some(t2), Some(t3)) = (p.target2, p.target3) {
            assert!(t3 > t2 && t3 <= t2 + (t2 - p.target1) + 1e-9, "{what}: {p:?}");
            assert!(p.target3_source.is_some() && p.reward3_pct.is_some());
        }
    }
    let st = d.structure.as_ref().expect("structure");
    assert!(st.levels.iter().all(|l| l.touches >= 2), "{what}");
    assert_guidance(d, what);
    assert_evidence(d, what);
    // Round trip through the contract.
    let json = serde_json::to_string(d).unwrap();
    let back: Decision = serde_json::from_str(&json).unwrap();
    assert_eq!(back.verdict, d.verdict);
}

/// "Quand ne pas trader", action zones, watched scenarios, counter-argument and snapshot follow what the decision
/// measured, and never change the verdict.
fn assert_guidance(d: &Decision, what: &str) {
    // Not the moment to trade: every reason is sourced.
    let nt = &d.no_trade;
    assert_eq!(nt.active, !nt.reasons.is_empty(), "{what}");
    assert_eq!(nt.headline.is_empty(), !nt.active, "{what}");
    if nt.active {
        assert!(nt.headline.starts_with("🕰️ Pas le moment de trader : "), "{what}: {}", nt.headline);
    }
    for r in &nt.reasons {
        assert!(!r.label.is_empty() && !r.detail.is_empty(), "{what}: {r:?}");
        match r.code.as_str() {
            c @ ("volatility" | "liquidity" | "spread" | "announcement" | "event") => assert!(veto(d, c).active, "{what}: {c}"),
            "degraded" => assert!(d.degraded.active, "{what}"),
            "earnings" | "marketClosed" => assert_eq!(d.kind, Kind::Stock, "{what}"),
            "trendless" | "weakSignal" => {}
            c => panic!("{what}: unknown code {c}"),
        }
    }
    assert_eq!(nt.reasons.iter().any(|r| r.code == "degraded"), d.degraded.active, "{what}");
    // Action zones: the plan as an ascending ladder.
    assert_eq!(d.action_zones.is_some(), d.plan.is_some(), "{what}");
    if let (Some(z), Some(p)) = (&d.action_zones, &d.plan) {
        assert_eq!(z.zones.len(), 5, "{what}");
        assert!(z.zones.windows(2).all(|w| w[0].from <= w[1].from), "{what}: {:?}", z.zones);
        assert!(z.zones.iter().all(|x| x.from <= x.to && !x.label.is_empty() && !x.note.is_empty()), "{what}: {:?}", z.zones);
        let get = |k: &str| z.zones.iter().find(|x| x.kind == k).unwrap_or_else(|| panic!("{what}: zone {k}"));
        assert_eq!((get("invalidation").from, get("invalidation").to), (p.stop, p.stop), "{what}");
        assert_eq!((get("buy").from, get("buy").to), (p.zone_from, p.zone_to), "{what}");
        assert_eq!((get("wait").from, get("profit").from), (p.zone_to, p.target1), "{what}");
        if let Some(h) = &z.here {
            let x = get(h);
            assert!(z.price >= x.from && z.price <= x.to, "{what}: {h} {z:?}");
        }
        assert!(z.here_text.starts_with("Vous êtes ici"), "{what}");
    }
    // Scenarios: 3 / 2 / 3 conditions, exactly one unfolding, the same as `unfolding`.
    for s in &d.scenarios {
        let n = if s.kind == ScenarioKind::Neutral { 2 } else { 3 };
        assert_eq!(s.conditions.len(), n, "{what}: {:?}", s.kind);
        assert_eq!(s.met, s.conditions.iter().filter(|c| c.state == CheckState::Met).count(), "{what}");
    }
    let u = d.unfolding.as_ref().expect("unfolding");
    let on: Vec<&Scenario> = d.scenarios.iter().filter(|s| s.unfolding).collect();
    assert_eq!(on.len(), 1, "{what}");
    assert_eq!((on[0].kind, on[0].met, on[0].conditions.len()), (u.kind, u.met, u.total), "{what}");
    let share = |s: &Scenario| s.met as f64 / s.conditions.len() as f64;
    assert!(d.scenarios.iter().all(|s| share(s) <= share(on[0]) + 1e-9 || u.kind == ScenarioKind::Neutral), "{what}");
    // Counter-argument: counts from pros / cons, every invalidation condition listed.
    let c = &d.counter_argument;
    assert!(c.favourable <= d.pros.len() && c.unfavourable <= d.cons.len(), "{what}");
    assert_eq!(c.text, format!("🟢 Raisons favorables : {} / 🔴 Raisons défavorables : {}", c.favourable, c.unfavourable));
    for line in &d.why_not.invalidation {
        assert!(c.invalidators.iter().any(|i| &i.text == line), "{what}: {line}");
    }
    assert!(c.invalidators.iter().filter(|i| i.kind == "level" || i.kind == "volume").all(|i| i.value.is_some_and(|v| v > 0.0)), "{what}");
    // Snapshot: the same numbers as the decision.
    let sn = &d.snapshot;
    assert_eq!(sn.composite, d.score.value, "{what}");
    assert_eq!(sn.families.len(), d.families.len(), "{what}");
    assert_eq!(sn.momentum, d.families.iter().find(|f| f.key == "momentum").and_then(|f| f.score), "{what}");
    assert_eq!(sn.nearest_resistance.as_ref().map(|l| l.price), st_price(d, true), "{what}");
    assert_eq!(sn.nearest_support.as_ref().map(|l| l.price), st_price(d, false), "{what}");
}

/// « Preuve du modèle »: consistent figures, a con whenever the validation shows no edge, the cap when weak, no
/// strong rating without an edge, nothing at all without a cached report.
fn assert_evidence(d: &Decision, what: &str) {
    let e = &d.model_evidence;
    assert_eq!(e.link, "/app/validation", "{what}");
    assert!(!e.text.is_empty(), "{what}");
    let con = d.cons.iter().any(|c| c.starts_with("Le signal n'a pas démontré d'avantage sur cette classe d'actifs (validation sur "));
    if !e.available {
        assert!(e.class_verdict.is_none() && e.beat_hold.is_none() && !e.weak, "{what}: {e:?}");
        assert!(!con && d.rating_reason.is_none() && !d.confidence_text.contains("plafonnée"), "{what}");
        return;
    }
    let v = e.class_verdict.expect("class verdict");
    assert!(e.as_of.is_some() && e.assets > 0, "{what}");
    assert_eq!(e.beat_hold.as_deref(), Some(format!("{}/{}", e.beat_hold_count, e.assets).as_str()), "{what}");
    assert_eq!(e.weak, v == Proof::Negative || e.beat_hold_count * 3 < e.assets, "{what}");
    let edge = v == Proof::Edge && !e.weak;
    assert_eq!(con, !edge, "{what}: {:?}", d.cons);
    if e.weak {
        assert!(d.confidence <= EVIDENCE_CAP, "{what}: {}", d.confidence);
    }
    if !edge {
        assert!(!matches!(d.rating, Rating::StrongBuy | Rating::StrongSell), "{what}: {:?}", d.rating);
    }
    if let Some(r) = &d.rating_reason {
        assert!(matches!(d.rating, Rating::Buy | Rating::Sell) && r.contains("validation du modèle"), "{what}: {r}");
    }
}

fn st_price(d: &Decision, resistance: bool) -> Option<f64> {
    let st = d.structure.as_ref()?;
    if resistance { st.nearest_resistance.as_ref().map(|l| l.price) } else { st.nearest_support.as_ref().map(|l| l.price) }
}

// ---------- Real candles ----------

#[test]
fn real_assets_give_coherent_decisions() {
    let t = now();
    for sym in ["BTC", "ETH", "SOL", "DOGE", "AAPL", "NVDA", "SPY"] {
        let s = real(sym);
        let g = guard_of(&s, t);
        let mut i = input(sym, &s, Some(&g), t);
        i.benchmarks = vec![benchmark_of(s.kind)];
        let d = decide(&i);
        assert_coherent(&d, sym);
        assert_eq!(d.mode, "informational");
        // Structure on the daily candles: every indicator is measured; relative strength except for the benchmark.
        let st = d.structure.as_ref().unwrap();
        assert!(st.ichimoku.is_some() && st.supertrend.is_some() && st.donchian.is_some() && st.vwap.is_some(), "{sym}");
        assert!(st.volume_profile.is_some() && st.pivots.is_some() && st.breakout.is_some() && st.score.is_some(), "{sym}");
        assert_eq!(st.relative.is_empty(), matches!(sym, "BTC" | "SPY"), "{sym}: {:?}", st.relative_note);
        if let Some(r) = st.relative.first() {
            assert_eq!(r.periods.len(), 3, "{sym}");
            assert!(r.correlation.is_some_and(|c| (-1.0..=1.0).contains(&c)), "{sym}");
        }
        // No macro report: the regime comes from the benchmark's trend alone (never risk-on).
        assert!(d.market_regime.as_ref().is_some_and(|r| r.kind != RegimeKind::RiskOn), "{sym}: {:?}", d.market_regime);
        assert!(d.position.is_none() && d.exposure.is_none());
        // Without fundamentals, liquidity or track: shown as unavailable, never estimated.
        let fam = |k: &str| d.families.iter().find(|f| f.key == k).unwrap();
        assert_eq!(fam("fundamentals").status, Status::Unavailable, "{sym}");
        assert_eq!(fam("valuation").status, Status::Unavailable, "{sym}");
        assert!(!veto(&d, "spread").verifiable, "{sym}");
        assert!(!veto(&d, "regulation").verifiable, "{sym}");
        assert_eq!(veto(&d, "unlock").verifiable, s.kind == Kind::Stock, "{sym}");
        assert_eq!(veto(&d, "earnings").verifiable, s.kind == Kind::Crypto, "{sym}");
        assert!(d.track.is_none() && d.fundamentals.is_none() && d.liquidity.is_none());
        // Deterministic.
        assert_eq!(decide(&i), d, "{sym}");
    }
}

#[test]
fn real_assets_with_a_position_and_weights() {
    let t = now();
    let btc = real("BTC");
    let spy = real("SPY");
    for sym in ["BTC", "ETH", "AAPL", "NVDA"] {
        let s = real(sym);
        let g = guard_of(&s, t);
        let price = s.h1.last().unwrap().close;
        for cost in [price * 0.5, price, price * 1.5] {
            let mut i = input(sym, &s, Some(&g), t);
            i.cost = Some(cost);
            let d = decide(&i);
            assert_coherent(&d, &format!("{sym} cost {cost}"));
            assert!(matches!(d.verdict, Verdict::Trim | Verdict::Sell | Verdict::Wait), "{sym}: {:?}", d.verdict);
            let p = d.position.as_ref().unwrap();
            assert_eq!(p.cost, cost);
            assert!(p.exits.len() >= 4 && p.exits.iter().any(|e| e.kind == ExitKind::Defensive) && p.exits.iter().any(|e| e.kind == ExitKind::Macro));
            assert_eq!(d.mode, "personal");
        }
        let factor = if s.kind == Kind::Crypto { &btc } else { &spy };
        let mut i = input(sym, &s, Some(&g), t);
        i.exposure = Some(ExposureInput {
            factor: if s.kind == Kind::Crypto { "Bitcoin" } else { "S&P 500" }.into(),
            factor_symbol: if s.kind == Kind::Crypto { "BTC" } else { "SPY" }.into(),
            factor_daily: factor.long.clone(),
            holdings: vec![
                HoldingSeries { symbol: "BTC".into(), kind: Kind::Crypto, weight: 30.0, daily: btc.long.clone() },
                HoldingSeries { symbol: "ETH".into(), kind: Kind::Crypto, weight: 20.0, daily: real("ETH").long },
                HoldingSeries { symbol: "SPY".into(), kind: Kind::Stock, weight: 30.0, daily: spy.long.clone() },
                HoldingSeries { symbol: "NVDA".into(), kind: Kind::Stock, weight: 10.0, daily: real("NVDA").long },
            ],
        });
        let d = decide(&i);
        assert_coherent(&d, &format!("{sym} weights"));
        let e = d.exposure.as_ref().unwrap();
        assert!(e.correlation.is_some_and(|c| (-1.0..=1.0).contains(&c)), "{sym}: {e:?}");
        assert!(e.weight <= 100.0);
        assert_eq!(d.mode, "personal");
    }
}

// ---------- Hand-made scenarios ----------

/// Deterministic noise in [−1, 1].
fn noise(i: usize) -> f64 {
    let x = ((i as f64 + 1.0) * 12.9898).sin() * 43_758.545_3;
    (x - x.floor()) * 2.0 - 1.0
}

/// Daily candles following `path` (closes), with ±`wiggle` noise and volumes from `vol`.
fn candles(path: &[f64], wiggle: f64, vol: impl Fn(usize) -> f64, start: i64, step: i64) -> Vec<Candle> {
    let mut out = Vec::with_capacity(path.len());
    let mut prev = path[0];
    for (i, &p) in path.iter().enumerate() {
        let close = p * (1.0 + wiggle * noise(i));
        let open = prev;
        let span = (close - open).abs().max(close * 0.01);
        out.push(Candle {
            time: start + i as i64 * step,
            open,
            high: open.max(close) + span * 0.3,
            low: open.min(close) - span * 0.3,
            close,
            volume: vol(i),
        });
        prev = close;
    }
    out
}

const T0: i64 = 1_700_000_000_000 - 1_700_000_000_000 % DAY_MS;

/// Scenario made of daily candles: 4 h and 1 h candles are derived by splitting each day (enough for the guard and the
/// 4 h signal; their exact shape does not matter here).
fn scenario(path: &[f64], vol: impl Fn(usize) -> f64) -> Series {
    let d = candles(path, 0.004, vol, T0, DAY_MS);
    let split = |parts: i64| -> Vec<Candle> {
        let mut out = vec![];
        for c in &d {
            let step = DAY_MS / parts;
            for k in 0..parts {
                let f0 = k as f64 / parts as f64;
                let f1 = (k + 1) as f64 / parts as f64;
                let o = c.open + (c.close - c.open) * f0;
                let cl = c.open + (c.close - c.open) * f1;
                out.push(Candle {
                    time: c.time + k * step,
                    open: o,
                    high: o.max(cl) + (c.high - c.open.max(c.close)) / parts as f64,
                    low: o.min(cl) - (c.open.min(c.close) - c.low) / parts as f64,
                    close: cl,
                    volume: c.volume / parts as f64,
                });
            }
        }
        out
    };
    let h4 = split(6);
    let h1 = split(24);
    Series { kind: Kind::Crypto, h1: h1[h1.len() - 300..].to_vec(), h4: h4[h4.len() - 300..].to_vec(), d: d[d.len() - 300..].to_vec(), long: d }
}

fn scenario_now(s: &Series) -> i64 {
    s.d.last().unwrap().time + DAY_MS + 3_600_000
}

fn ramp(from: f64, to: f64, n: usize) -> Vec<f64> {
    (0..n).map(|i| from + (to - from) * (i as f64 + 1.0) / n as f64).collect()
}

/// Long uptrend (≈ 400 days), a sharp rise, then a pull-back to `depth` of that rise; `tail` closes are appended.
fn uptrend_pullback(depth: f64, tail: &[f64]) -> Series {
    let mut p = vec![50.0];
    p.extend(ramp(50.0, 100.0, 400));
    p.extend(ramp(100.0, 160.0, 60));
    let high = 160.0;
    let low = 100.0;
    let bottom = high - depth * (high - low);
    p.extend(ramp(high, bottom, 20));
    p.extend_from_slice(tail);
    let n_rise = 461;
    scenario(&p, move |i| {
        if i >= n_rise && i < n_rise + 20 {
            600.0
        } else if i >= n_rise + 20 {
            1_500.0
        } else {
            1_000.0
        }
    })
}

fn bottom_of(depth: f64) -> f64 {
    160.0 - depth * 60.0
}

fn decide_on<'a>(s: &'a Series, f: impl FnOnce(&mut DecisionInput<'a>)) -> Decision {
    let t = scenario_now(s);
    let mut i = input("TEST", s, None, t);
    f(&mut i);
    let d = decide(&i);
    assert_coherent(&d, "scenario");
    d
}

fn buy_setup() -> Series {
    // Pull-back to 62 % of the rise, a hammer, then a strong close above the previous high on volume.
    let b = bottom_of(0.62);
    uptrend_pullback(0.62, &[b * 0.995, b * 1.005, b * 1.02])
}

#[test]
fn buy_when_the_setup_is_complete() {
    let s = buy_setup();
    let d = decide_on(&s, |_| {});
    let steps: Vec<(String, StepState)> = d.setup.steps.iter().map(|s| (s.label.clone(), s.state)).collect();
    assert_eq!(d.verdict, Verdict::Buy, "{steps:?}\n{:?}\n{}", d.vetoes.iter().filter(|v| v.active).collect::<Vec<_>>(), d.headline);
    assert!(d.why_wait.is_empty());
    assert!(matches!(d.level, Level::Strong | Level::Moderate));
    let p = d.plan.unwrap();
    assert!(p.acceptable && p.risk_reward >= 2.0);
    assert!(d.setup.steps[6].state == StepState::Ok, "confirmation");
}

#[test]
fn a_scheduled_announcement_turns_a_buy_into_a_wait() {
    let s = buy_setup();
    let t = scenario_now(&s);
    let fed: altim::calendar::CalendarEvent = serde_json::from_value(serde_json::json!({
        "date": t + DAY_MS, "day": "2026-10-01", "time": "20:00", "kind": "centralBank", "category": "tauxDirecteurs",
        "importance": "high", "title": "Décision de la Fed sur les taux", "country": "États-Unis",
        "source": "Réserve fédérale", "url": "https://www.federalreserve.gov/"
    }))
    .unwrap();
    let d = decide_on(&s, |i| i.events = Some(vec![fed.clone()]));
    let v = d.vetoes.iter().find(|v| v.code == "announcement").unwrap();
    assert!(v.active && v.detail.contains("Décision de la Fed sur les taux, États-Unis, le 01/10 à 20:00"), "{}", v.detail);
    assert_eq!(d.verdict, Verdict::Wait, "a matter of timing, not a reason to stay out");
    assert_eq!(d.events.as_ref().map(|e| e.len()), Some(1));
    // Without a calendar: said, never taken as "nothing announced".
    let d = decide_on(&s, |_| {});
    let v = d.vetoes.iter().find(|v| v.code == "announcement").unwrap();
    assert!(!v.active && !v.verifiable);
    assert_eq!(d.verdict, Verdict::Buy);
    // An announcement a week away does not block.
    let later = altim::calendar::CalendarEvent { date: t + 7 * DAY_MS, ..fed };
    let d = decide_on(&s, |i| i.events = Some(vec![later]));
    assert!(!d.vetoes.iter().find(|v| v.code == "announcement").unwrap().active);
    assert_eq!(d.verdict, Verdict::Buy);
}

#[test]
fn buy_zone_without_confirmation() {
    let b = bottom_of(0.62);
    let s = uptrend_pullback(0.62, &[b * 0.998]);
    let d = decide_on(&s, |_| {});
    assert_eq!(d.verdict, Verdict::BuyZone, "{}\n{:?}", d.headline, d.setup.steps);
    assert_eq!(d.setup.steps[2].state, StepState::Ok);
    assert_eq!(d.setup.steps[6].state, StepState::No);
    assert!(!d.why_wait.is_empty());
    assert!(d.to_buy.iter().any(|c| c.text.contains("clôture")));
}

#[test]
fn wait_above_the_zone_with_a_poor_risk_reward() {
    // Pull-back of only 10 %: price above the zone.
    let s = uptrend_pullback(0.1, &[]);
    let d = decide_on(&s, |_| {});
    assert_eq!(d.verdict, Verdict::Wait, "{}", d.headline);
    let rr = veto(&d, "riskReward");
    assert!(rr.active && rr.verifiable, "{rr:?}");
    let p = d.plan.as_ref().unwrap();
    // Above the zone, the plan's entry is the highest price of the zone that still gives a risk/reward of 2.
    assert!(p.entry >= p.zone_from && p.entry <= p.zone_to, "{p:?}");
    assert!(p.acceptable && (p.risk_reward - 2.0).abs() < 0.01, "{p:?}");
    assert!(d.to_buy.iter().any(|c| c.level.is_some_and(|l| (l - p.entry).abs() < 1e-6)), "{:?}", d.to_buy);
    assert!(d.why_wait.iter().any(|w| w.contains("au-dessus de la zone")));
    assert!(d.to_buy.iter().any(|c| c.text.starts_with("Repli dans la zone")));
    assert_eq!(d.setup.steps[1].state, StepState::No, "no correction yet");
    assert!(d.why_not.invalidation.iter().any(|x| x.contains("Cassure")));
}

#[test]
fn buy_refused_when_the_risk_reward_is_below_2() {
    // Shallow pull-back into the top of the zone (40 %) with confirmation: complete setup but R/R < 2.
    let b = bottom_of(0.40);
    let s = uptrend_pullback(0.40, &[b * 0.997, b * 1.004, b * 1.02]);
    let d = decide_on(&s, |_| {});
    assert_ne!(d.verdict, Verdict::Buy);
    assert!(veto(&d, "riskReward").active, "{:?}", veto(&d, "riskReward"));
    assert!(d.to_buy.iter().any(|c| c.text.contains("rapport gain/risque")));
}

fn downtrend() -> Series {
    let mut p = vec![200.0];
    p.extend(ramp(200.0, 100.0, 480));
    scenario(&p, |_| 1_000.0)
}

#[test]
fn no_position_in_a_downtrend() {
    let s = downtrend();
    let d = decide_on(&s, |_| {});
    assert_eq!(d.verdict, Verdict::NoPosition, "{}", d.headline);
    assert_eq!(d.level, Level::Exit);
    assert!(veto(&d, "downtrend").active);
    assert!(d.to_buy.iter().any(|c| c.text.contains("200 jours")));
    assert!(d.why_not.invalidation.iter().any(|x| x.contains("200 jours")));
}

#[test]
fn sell_when_held_in_a_downtrend_and_trim_at_the_target() {
    let s = downtrend();
    let d = decide_on(&s, |i| i.cost = Some(150.0));
    assert_eq!(d.verdict, Verdict::Sell, "{}", d.headline);
    assert_eq!(d.level, Level::Exit);
    let pos = d.position.as_ref().unwrap();
    assert!(pos.pnl_pct.unwrap() < 0.0);
    assert!(pos.exits.iter().any(|e| e.kind == ExitKind::Defensive && e.now));

    // Strong rally to a new high (RSI above 70): the first profit exit is met now.
    let mut p = vec![50.0];
    p.extend(ramp(50.0, 100.0, 400));
    p.extend(ramp(100.0, 150.0, 50));
    let s = scenario(&p, |_| 1_000.0);
    let d = decide_on(&s, |i| i.cost = Some(80.0));
    assert_eq!(d.verdict, Verdict::Trim, "{}\n{:?}", d.headline, d.position);
    let pos = d.position.as_ref().unwrap();
    let first = &pos.exits[0];
    assert!(first.now && first.kind == ExitKind::Profit && first.share == 20.0, "{first:?}");
    assert!(pos.exits.iter().map(|e| e.share).any(|s| s == 30.0));
    assert!(d.headline.contains("alléger 20 %"), "{}", d.headline);
    // Same rally without a position: nothing to sell, informational.
    let d = decide_on(&s, |_| {});
    assert!(matches!(d.verdict, Verdict::Wait | Verdict::NoPosition), "{:?}", d.verdict);
    assert_eq!(d.mode, "informational");
}

#[test]
fn held_without_exit_condition_is_wait() {
    let s = uptrend_pullback(0.1, &[]);
    let d = decide_on(&s, |i| i.cost = Some(150.0));
    assert_eq!(d.verdict, Verdict::Wait, "{}", d.headline);
    assert!(d.position.as_ref().unwrap().exits.iter().all(|e| !e.now));
    assert!(d.headline.starts_with("Position à conserver"));
    assert_eq!(d.mode, "personal");
    assert!(d.disclaimer.contains("jamais conservés"));
    // A close under the 20-session low (above the plan's stop): half the position, the rest under the stop.
    let b = bottom_of(0.45);
    let s = uptrend_pullback(0.45, &[b * 0.95]);
    let d = decide_on(&s, |i| i.cost = Some(90.0));
    assert_eq!(d.verdict, Verdict::Trim, "{}", d.headline);
    assert_eq!(d.level, Level::HighRisk);
    let exits = &d.position.as_ref().unwrap().exits;
    assert!(exits.iter().any(|e| e.kind == ExitKind::Defensive && e.share == 50.0 && e.now), "{exits:?}");
    assert!(exits.iter().any(|e| e.kind == ExitKind::Defensive && e.share == 100.0 && !e.now), "{exits:?}");
    assert!(d.headline.contains("réduire la position de moitié"), "{}", d.headline);
}

fn macro_report(score: f64, vix: f64, escalation: bool) -> MacroReport {
    let mut factors = vec![MacroFactor { code: "vixHigh".into(), points: 25.0, text: format!("Peur généralisée : VIX à {vix}.") }];
    if escalation {
        factors.push(MacroFactor { code: "escalation".into(), points: 30.0, text: "Actualité : invasion (dernières 12 h).".into() });
    }
    MacroReport {
        score,
        level: if score >= 50.0 {
            MacroLevel::High
        } else if score >= 25.0 {
            MacroLevel::Tense
        } else {
            MacroLevel::Calm
        },
        market_score: score,
        factors,
        themes: vec![],
        values: MacroValues { vix: Some(MacroValue { value: vix, change5d: 20.0 }), ..Default::default() },
        as_of: None,
    }
}

#[test]
fn every_veto_can_fire() {
    let base = buy_setup();
    let t = scenario_now(&base);
    let expect = |d: &Decision, code: &str| {
        let v = veto(d, code);
        assert!(v.active && v.verifiable, "{code}: {v:?}");
        assert_ne!(d.verdict, Verdict::Buy, "{code}");
        assert!(d.blocked);
    };
    // Spread and liquidity.
    let d = decide_on(&base, |i| {
        i.liquidity = Some(Liquidity { spread_pct: Some(1.2), daily_value: Some(4e5), relative_volume: None, source: "test".into() })
    });
    expect(&d, "spread");
    expect(&d, "liquidity");
    assert_eq!(d.verdict, Verdict::NoPosition);
    // Extreme volatility.
    let mut p = vec![100.0];
    for i in 0..480 {
        p.push(100.0 + i as f64 * 0.1 + if i % 2 == 0 { 15.0 } else { -15.0 });
    }
    let wild = scenario(&p, |_| 1_000.0);
    expect(&decide_on(&wild, |_| {}), "volatility");
    // Major event: escalation headlines.
    let g = guard_of(&base, t);
    // Escalation headlines alone (markets calm) are shown, never blocking.
    let calm = macro_report(20.0, 16.0, true);
    let d = decide_on(&base, |i| {
        i.guard = Some(&g);
        i.macro_report = Some(&calm);
    });
    assert!(!veto(&d, "event").active && veto(&d, "event").detail.contains("Non confirmé"), "{:?}", veto(&d, "event"));
    assert!(d.why_not.risks.iter().any(|r| r.contains("invasion")), "{:?}", d.why_not.risks);
    let m = macro_report(55.0, 22.0, true);
    let d = decide_on(&base, |i| {
        i.guard = Some(&g);
        i.macro_report = Some(&m);
    });
    expect(&d, "event");
    // Earnings within the next days (stock).
    let stock = Series { kind: Kind::Stock, ..buy_setup() };
    let d = decide_on(&stock, |i| {
        i.fundamentals = Some(Fundamentals::Stock(stock_fundamentals(Some(i.now + 2 * DAY_MS))));
    });
    expect(&d, "earnings");
    assert_eq!(d.verdict, Verdict::Wait, "earnings is a matter of timing");
    // Crash without stabilisation.
    let b = bottom_of(0.3);
    let crash = uptrend_pullback(0.3, &[b * 0.85, b * 0.72, b * 0.6]);
    let d = decide_on(&crash, |_| {});
    expect(&d, "crash");
    // Abnormal pump.
    let mut p = vec![50.0];
    p.extend(ramp(50.0, 100.0, 470));
    p.extend([125.0, 160.0, 210.0]);
    let pump = scenario(&p, |_| 1_000.0);
    expect(&decide_on(&pump, |_| {}), "pump");
    // Artificial volume: 30 times the usual volume, no move.
    let mut p = vec![50.0];
    p.extend(ramp(50.0, 100.0, 470));
    let n = p.len();
    let wash = scenario(&p, move |i| if i == n - 1 { 30_000.0 } else { 1_000.0 });
    expect(&decide_on(&wash, |_| {}), "volumeAnomaly");
    // Regulation: two headlines in 24 h.
    let d = decide_on(&base, |i| {
        i.market.news = Some(vec![
            NewsItem { title: "SEC sues TEST Foundation over token sale".into(), time: t - 3_600_000, source: None },
            NewsItem { title: "Exchange delists TEST after probe".into(), time: t - 7_200_000, source: None },
        ])
    });
    expect(&d, "regulation");
    // Token unlock: never verifiable for a crypto without a source, never active.
    let u = veto(&d, "unlock");
    assert!(!u.verifiable && !u.active);
    // Bearish divergence: new high, weaker RSI.
    let mut p = vec![50.0];
    p.extend(ramp(50.0, 100.0, 420));
    p.extend(ramp(100.0, 130.0, 12));
    p.extend(ramp(130.0, 118.0, 8));
    p.extend(ramp(118.0, 131.5, 20));
    p.extend(ramp(131.5, 127.0, 4));
    let div = scenario(&p, |_| 1_000.0);
    expect(&decide_on(&div, |_| {}), "divergence");
    // Downtrend.
    expect(&decide_on(&downtrend(), |_| {}), "downtrend");
    // Risk / reward.
    expect(&decide_on(&uptrend_pullback(0.1, &[]), |_| {}), "riskReward");
    // Macro shock.
    let m = macro_report(60.0, 34.0, false);
    let d = decide_on(&base, |i| i.macro_report = Some(&m));
    expect(&d, "macro");
    assert_eq!(d.verdict, Verdict::NoPosition);
    // Unreliable data.
    let d = decide_on(&base, |i| i.reliability = reliability(20.0, 1, true));
    expect(&d, "reliability");
    assert_eq!(d.verdict, Verdict::NoPosition);
    assert_eq!(d.level, Level::HighRisk);
}

fn stock_fundamentals(next: Option<i64>) -> StockFundamentals {
    StockFundamentals {
        period: "12 mois au 28/06/2026 (10-Q)".into(),
        revenue: Some(4.2e11),
        revenue_growth: Some(6.0),
        net_income: Some(1.1e11),
        eps: Some(7.3),
        eps_growth: Some(9.0),
        gross_margin: Some(46.0),
        operating_margin: Some(32.0),
        net_margin: Some(26.0),
        free_cash_flow: Some(1.0e11),
        fcf_margin: Some(24.0),
        debt: Some(9.6e10),
        cash: Some(6.5e10),
        net_debt: Some(3.1e10),
        roe: Some(150.0),
        per: Some(46.0),
        peg: Some(4.9),
        ev_ebitda: Some(33.0),
        dividend_yield: Some(0.3),
        share_change: Some(-2.6),
        next_earnings: next.map(|date| EarningsDate { date, estimated: true }),
        surprises: vec![EarningsSurprise { quarter: "T2 2026".into(), eps: 1.62, consensus: 1.53, surprise_pct: 5.9 }],
        revisions: Some(Revisions { month_ago: 8.76, now: 8.74, change_pct: -0.2 }),
        sector_note: "Comparaison au secteur non disponible.".into(),
        source: "SEC EDGAR (10-K, 10-Q), Nasdaq (Zacks)".into(),
        ..Default::default()
    }
}

#[test]
fn extras_fill_the_families_and_vetoes() {
    let stock = Series { kind: Kind::Stock, ..buy_setup() };
    let d = decide_on(&stock, |i| {
        i.fundamentals = Some(Fundamentals::Stock(stock_fundamentals(Some(i.now + 40 * DAY_MS))));
        i.liquidity = Some(Liquidity { spread_pct: Some(0.01), daily_value: Some(9e9), relative_volume: Some(1.1), source: "Nasdaq".into() });
        i.track = Some(Track {
            period: "sept. 2023 – sept. 2026".into(),
            trades: 14,
            win_rate: 57.0,
            avg_win: Some(9.8),
            avg_loss: Some(-4.2),
            profit_factor: Some(2.1),
            sharpe: Some(1.0),
            sortino: Some(1.6),
            max_drawdown: -18.0,
            total_return: 61.0,
            buy_and_hold: 142.0,
            fees_pct: 0.1,
            slippage_pct: 0.05,
            losing_streak: 3,
            note: "test".into(),
            details: Default::default(),
        });
    });
    let fam = |k: &str| d.families.iter().find(|f| f.key == k).unwrap();
    assert_eq!(fam("valuation").status, Status::Negative, "PER 46, PEG 4,9: {:?}", fam("valuation"));
    assert_eq!(fam("fundamentals").status, Status::Positive, "{:?}", fam("fundamentals"));
    assert_eq!(fam("liquidity").status, Status::Positive);
    assert!(veto(&d, "spread").verifiable && !veto(&d, "spread").active);
    assert!(veto(&d, "earnings").verifiable && !veto(&d, "earnings").active);
    assert!(d.confidence_text.contains("sur 14,"), "{}", d.confidence_text);
    assert!(d.fundamentals.is_some() && d.track.is_some() && d.liquidity.is_some());
    assert_eq!(d.verdict, Verdict::Buy, "{}", d.headline);
}

#[test]
fn onchain_family_from_stablecoins_and_developer_activity() {
    let s = buy_setup();
    assert_eq!(s.kind, Kind::Crypto);
    // Without network data: unavailable.
    let d = decide_on(&s, |_| {});
    let fam = d.families.iter().find(|f| f.key == "onchain").unwrap();
    assert_eq!(fam.status, Status::Unavailable);
    let flows = |p30: f64| StablecoinFlows {
        scope: "Tous réseaux".into(),
        date: 0,
        total: 3.1e11,
        change7d: None,
        change7d_pct: Some(0.9),
        change30d: None,
        change30d_pct: Some(p30),
        source: "DefiLlama (stablecoins)".into(),
    };
    let crypto = |p30: f64, commits: f64| {
        let mut f = altim::tokenomics::assemble("TEST", None, None, Some(1e9), None, None, None, (None, None));
        f.stablecoins = Some(flows(p30));
        f.dev_activity =
            Some(DevActivity {
                commits4w: Some(commits), smart_contract_platform: true, source: "GitHub (dépôt a/b)".into(), ..Default::default()
            });
        Fundamentals::Crypto(f)
    };
    // Stablecoins +3 % over 30 days, +0.9 % over 7, 120 commits: positive backdrop.
    let d = decide_on(&s, |i| i.fundamentals = Some(crypto(3.0, 120.0)));
    let fam = d.families.iter().find(|f| f.key == "onchain").unwrap();
    assert_eq!(fam.status, Status::Positive, "{fam:?}");
    assert!(fam.summary.starts_with("Stablecoins en circulation : "), "{}", fam.summary);
    assert!(fam.points.iter().any(|p| p.starts_with("Non couverts")), "{:?}", fam.points);
    // Stablecoins leaving (−3 %) and a platform nobody develops any more: negative.
    let d = decide_on(&s, |i| i.fundamentals = Some(crypto(-3.0, 1.0)));
    let fam = d.families.iter().find(|f| f.key == "onchain").unwrap();
    assert_eq!(fam.status, Status::Negative, "{fam:?}");
    assert!(fam.points.iter().any(|p| p.starts_with("Activité de développement quasi nulle")));
}

#[test]
fn exposure_warning_when_the_portfolio_already_follows_the_factor() {
    let s = buy_setup();
    let factor = uptrend_pullback(0.62, &[bottom_of(0.62) * 0.995, bottom_of(0.62) * 1.005, bottom_of(0.62) * 1.02]);
    let d = decide_on(&s, |i| {
        i.exposure = Some(ExposureInput {
            factor: "Bitcoin".into(),
            factor_symbol: "BTC".into(),
            factor_daily: factor.long.clone(),
            holdings: vec![
                HoldingSeries { symbol: "BTC".into(), kind: Kind::Crypto, weight: 40.0, daily: factor.long.clone() },
                HoldingSeries { symbol: "ALT".into(), kind: Kind::Crypto, weight: 25.0, daily: factor.long.clone() },
                HoldingSeries { symbol: "CASHLIKE".into(), kind: Kind::Crypto, weight: 10.0, daily: downtrend().long },
            ],
        })
    });
    let e = d.exposure.as_ref().unwrap();
    assert_eq!(e.weight, 65.0, "{e:?}");
    assert_eq!(e.assets, vec!["BTC", "ALT"]);
    assert_eq!(e.factor, "Bitcoin");
    let w = e.warning.as_ref().unwrap();
    assert!(w.starts_with("Signal favorable mais exposition déjà élevée au même facteur de risque"), "{w}");
    assert!(d.cons[0] == *w && d.why_not.risks[0] == *w);
    assert_eq!(d.verdict, Verdict::Buy);
    assert_eq!(d.level, Level::Moderate, "never a strong signal with a concentrated portfolio");
    assert_eq!(d.mode, "personal");
    // Below the threshold: no warning.
    let d = decide_on(&s, |i| {
        i.exposure = Some(ExposureInput {
            factor: "Bitcoin".into(),
            factor_symbol: "BTC".into(),
            factor_daily: factor.long.clone(),
            holdings: vec![HoldingSeries { symbol: "BTC".into(), kind: Kind::Crypto, weight: 20.0, daily: factor.long.clone() }],
        })
    });
    assert!(d.exposure.unwrap().warning.is_none());
}

#[test]
fn informational_by_default() {
    let d = decide_on(&buy_setup(), |_| {});
    assert_eq!(d.mode, "informational");
    assert!(d.disclaimer.contains("pas une recommandation personnalisée"));
    assert!(!d.disclaimer.contains("conseil en investissement réglementé") || d.disclaimer.contains("Pas un"));
}

#[test]
fn every_verdict_is_reachable() {
    let mut seen = std::collections::HashSet::new();
    let b = bottom_of(0.62);
    let cases: Vec<(Series, Option<f64>)> = vec![
        (buy_setup(), None),
        (uptrend_pullback(0.62, &[b * 0.998]), None),
        (uptrend_pullback(0.1, &[]), None),
        (downtrend(), None),
        (downtrend(), Some(150.0)),
        (
            {
                let mut p = vec![50.0];
                p.extend(ramp(50.0, 100.0, 400));
                p.extend(ramp(100.0, 150.0, 50));
                scenario(&p, |_| 1_000.0)
            },
            Some(80.0),
        ),
    ];
    for (s, cost) in &cases {
        let d = decide_on(s, |i| i.cost = *cost);
        seen.insert(format!("{:?}", d.verdict));
    }
    for v in ["Buy", "BuyZone", "Wait", "NoPosition", "Trim", "Sell"] {
        assert!(seen.contains(v), "{v} never produced: {seen:?}");
    }
}

fn track(total: f64, hold: f64, win: f64, trades: usize) -> Track {
    Track {
        period: "sept. 2023 – sept. 2026 (bougies journalières)".into(),
        trades,
        win_rate: win,
        avg_win: Some(6.0),
        avg_loss: Some(-4.0),
        profit_factor: Some(if total < 0.0 { 0.7 } else { 1.4 }),
        sharpe: None,
        sortino: None,
        max_drawdown: -30.0,
        total_return: total,
        buy_and_hold: hold,
        fees_pct: 0.1,
        slippage_pct: 0.05,
        losing_streak: 5,
        note: "test".into(),
        details: Default::default(),
    }
}

/// The signal's own history on the asset (measured by the backtest, fees and slippage included): a losing one, or
/// one that did worse than holding, lowers the confidence, forbids "Signal fort" and is said plainly.
#[test]
fn the_signal_track_record_is_told_honestly() {
    let s = buy_setup();
    let base = decide_on(&s, |_| {});
    // BTC: −25.5 % against +36.2 % held, 31.6 % winning trades.
    let d = decide_on(&s, |i| i.track = Some(track(-25.5, 36.2, 31.6, 19)));
    assert_eq!(d.verdict, Verdict::Buy, "the buy rests on the setup, zone and trend, not on the signal");
    assert_ne!(d.level, Level::Strong);
    assert!(d.confidence <= 55.0 && d.confidence < base.confidence, "{} vs {}", d.confidence, base.confidence);
    assert!(d.confidence_text.contains("a perdu de l'argent") && d.confidence_text.contains("simple détention"), "{}", d.confidence_text);
    assert!(d.why_not.risks.iter().any(|r| r.contains("−25,5 %") && r.contains("+36,2 %")), "{:?}", d.why_not.risks);
    assert!(d.headline.contains("n'a pas battu la simple détention"), "{}", d.headline);
    // A signal that lost money on the asset: "signal dégradé", and no buy rating even though the verdict is ACHETER.
    assert!(d.degraded.active && d.degraded.reasons.iter().any(|r| r.contains("a perdu de l'argent")), "{:?}", d.degraded);
    assert_eq!(d.rating, Rating::Hold);
    assert!(!base.degraded.active && matches!(base.rating, Rating::Buy | Rating::StrongBuy), "{:?} {:?}", base.degraded, base.rating);
    // AAPL: +10.6 % against +97.1 % held: positive but far behind holding.
    let d = decide_on(&s, |i| i.track = Some(track(10.6, 97.1, 48.0, 12)));
    assert!(d.confidence <= 55.0 && d.level != Level::Strong);
    assert!(d.confidence_text.contains("pas d'avance prouvée"), "{}", d.confidence_text);
    // SOL: +16.5 % against −6.2 % held: the only case where it beats holding, no penalty.
    let d = decide_on(&s, |i| i.track = Some(track(16.5, -6.2, 55.0, 12)));
    assert!(d.confidence >= base.confidence, "{} vs {}", d.confidence, base.confidence);
    assert!(!d.headline.contains("n'a pas battu"));
    assert!(d.confidence_text.contains("+16,5 % contre −6,2 %"), "{}", d.confidence_text);
    // Too few trades: neutral.
    let d = decide_on(&s, |i| i.track = Some(track(-40.0, 10.0, 0.0, 2)));
    assert_eq!(d.confidence, base.confidence);
    assert!(d.confidence_text.contains("non concluant"));
    // Held: selling on the signal did worse than holding, said in "Pourquoi pas ?".
    let mut p = vec![50.0];
    p.extend(ramp(50.0, 100.0, 400));
    p.extend(ramp(100.0, 150.0, 50));
    let rally = scenario(&p, |_| 1_000.0);
    let d = decide_on(&rally, |i| {
        i.cost = Some(80.0);
        i.track = Some(track(-25.5, 36.2, 31.6, 19));
    });
    assert_eq!(d.verdict, Verdict::Trim);
    assert!(d.why_not.risks.iter().any(|r| r.starts_with("Suivre les signaux a fait moins bien que conserver")), "{:?}", d.why_not.risks);
}

#[test]
fn correlations_on_real_candles() {
    use altim::engine::decision::{aligned_returns, correlation};
    let c = |a: &str, b: &str| {
        let (x, y) = (real(a).long, real(b).long);
        let r = aligned_returns(&[&x, &y], 90);
        assert_eq!(r[0].len(), 90, "{a}/{b}");
        correlation(&r[0], &r[1]).unwrap()
    };
    let (aapl, nvda, eth, btc_spy) = (c("AAPL", "SPY"), c("NVDA", "SPY"), c("ETH", "BTC"), c("BTC", "SPY"));
    // AAPL moved on its own news over these 90 days (−6 %, +5 %, −7 % days with a flat S&P 500): value checked
    // independently (Python, statistics.correlation on the shared dates).
    assert!((aapl - 0.113_426_704_391_562_34).abs() < 1e-9, "AAPL/SPY {aapl}");
    assert!(nvda > 0.4 && eth > 0.7, "NVDA/SPY {nvda}, ETH/BTC {eth}");
    assert!(btc_spy < eth, "BTC/SPY {btc_spy}");
}

/// `cargo test --test decision -- --ignored --nocapture print_real` prints the decisions of the real candles.
#[test]
#[ignore]
fn print_real() {
    let t = now();
    for sym in ["BTC", "ETH", "SOL", "DOGE", "AAPL", "NVDA", "SPY"] {
        let s = real(sym);
        let g = guard_of(&s, t);
        let d = decide(&input(sym, &s, Some(&g), t));
        println!("{}", serde_json::to_string_pretty(&d).unwrap());
    }
    let _ = INPUTS.len();
}

/// Composite score with the user's weights (`w=`): renormalised over the measured factors, contributions adding up
/// to the score, the missing factors named.
#[test]
fn composite_score_follows_the_weights() {
    let s = buy_setup();
    let d = decide_on(&s, |_| {});
    assert!(!d.score.custom);
    assert_eq!(d.score.missing, ["Fondamentaux", "Sentiment", "Actualités", "Macro"], "{:?}", d.score);
    let tech = &d.score.factors[0];
    assert!(tech.sources.contains(&"structure".to_string()), "{tech:?}");
    // Momentum only.
    let only = decide_on(&s, |i| i.score_weights = Some(ScoreWeights([0.0, 100.0, 0.0, 0.0, 0.0, 0.0])));
    let mom = only.score.factors.iter().find(|f| f.key == "mom").unwrap();
    assert!(only.score.custom && only.score.value == mom.value && mom.applied == 100.0, "{:?}", only.score);
    assert_eq!(only.verdict, d.verdict, "the weights never change the verdict");
    assert!(only.score.text.contains("poids personnalisés"));
}

/// Target 3 and the horizon of the plan on a hand-made uptrend.
#[test]
fn third_target_and_horizon() {
    let d = decide_on(&buy_setup(), |_| {});
    let p = d.plan.as_ref().unwrap();
    let (t1, t2, t3) = (p.target1, p.target2.unwrap(), p.target3.unwrap());
    assert!(t3 > t2 && t3 <= 2.0 * t2 - t1 + 1e-9, "{p:?}");
    let source = p.target3_source.as_deref().unwrap();
    assert!(source.starts_with("Projection") || source.starts_with("Niveau"), "{source}");
    let h = d.horizon.as_ref().unwrap();
    assert!(h.detail.starts_with("Plan sur bougies journalières"), "{}", h.detail);
    assert!(h.atr_distance > 0.0);
}

/// Macro stress and the benchmark's trend give the market regime.
#[test]
fn market_regime_in_the_decision() {
    let s = buy_setup();
    let up = Benchmark { name: "Bitcoin (BTC)".into(), symbol: "BTC".into(), kind: Kind::Crypto, daily: s.long.clone() };
    let calm = macro_report(10.0, 14.0, false);
    let d = decide_on(&s, |i| {
        i.macro_report = Some(&calm);
        i.benchmarks = vec![up.clone()];
    });
    let r = d.market_regime.as_ref().unwrap();
    assert_eq!((r.kind, r.label.as_str()), (RegimeKind::RiskOn, "Risk-on"), "{r:?}");
    let shock = macro_report(60.0, 34.0, false);
    let d = decide_on(&s, |i| {
        i.macro_report = Some(&shock);
        i.benchmarks = vec![up.clone()];
    });
    assert_eq!(d.market_regime.unwrap().kind, RegimeKind::RiskOff);
    assert_eq!(d.rating, Rating::Hold, "no position on a macro shock without a downtrend: ATTENDRE");
}

/// "Quand ne pas trader": an announcement within 48 h and the sessions right after earnings, without changing the
/// verdict when no veto is involved.
#[test]
fn not_the_moment_to_trade() {
    let s = buy_setup();
    let t = scenario_now(&s);
    let fed: altim::calendar::CalendarEvent = serde_json::from_value(serde_json::json!({
        "date": t + DAY_MS, "day": "2026-10-01", "time": "20:00", "kind": "centralBank", "category": "tauxDirecteurs",
        "importance": "high", "title": "Décision de la Fed sur les taux", "country": "États-Unis",
        "source": "Réserve fédérale", "url": "https://www.federalreserve.gov/"
    }))
    .unwrap();
    let d = decide_on(&s, |i| i.events = Some(vec![fed.clone()]));
    let r = d.no_trade.reasons.iter().find(|r| r.code == "announcement").expect("announcement");
    assert!(r.detail.contains("Décision de la Fed sur les taux"), "{r:?}");
    assert!(d.no_trade.headline.contains("annonce économique imminente"), "{}", d.no_trade.headline);
    assert!(d.counter_argument.invalidators.iter().any(|i| i.kind == "event" && i.text.contains("Décision de la Fed")), "{:?}", d.counter_argument);
    // Without a calendar: not a reason, said as not checked.
    let d = decide_on(&s, |_| {});
    assert!(d.no_trade.unchecked.iter().any(|u| u.starts_with("Annonce économique dans les 48 h")), "{:?}", d.no_trade.unchecked);
    // A stock the session after its earnings: digesting, the verdict is untouched (the earnings veto only looks ahead).
    let stock = Series { kind: Kind::Stock, ..buy_setup() };
    let base = decide_on(&stock, |i| i.fundamentals = Some(Fundamentals::Stock(stock_fundamentals(Some(i.now + 40 * DAY_MS)))));
    let after = decide_on(&stock, |i| {
        // Midnight of the previous weekday (bare date, as the earnings sources give it).
        let mut day = i.now.div_euclid(DAY_MS) - 1;
        while (day + 3).rem_euclid(7) >= 5 {
            day -= 1;
        }
        i.fundamentals = Some(Fundamentals::Stock(stock_fundamentals(Some(day * DAY_MS))));
    });
    let r = after.no_trade.reasons.iter().find(|r| r.code == "earnings").expect("earnings");
    assert_eq!(r.label, "Lendemain de résultats", "{r:?}");
    assert!(!base.no_trade.reasons.iter().any(|r| r.code == "earnings"));
    assert_eq!(after.verdict, base.verdict, "the guidance never changes the verdict");
    let soon = decide_on(&stock, |i| i.fundamentals = Some(Fundamentals::Stock(stock_fundamentals(Some(i.now + 2 * DAY_MS)))));
    assert_eq!(soon.no_trade.reasons.iter().find(|r| r.code == "earnings").map(|r| r.label.as_str()), Some("Résultats imminents"));
    assert!(veto(&soon, "earnings").active, "same window as the veto");
}

/// Action zones and watched scenarios on hand-made paths: above the zone the price is in the wait zone; in a
/// downtrend the bearish scenario's conditions are the ones met.
#[test]
fn action_zones_and_watched_scenarios() {
    let d = decide_on(&uptrend_pullback(0.1, &[]), |_| {});
    let z = d.action_zones.as_ref().unwrap();
    assert_eq!(z.here.as_deref(), Some("wait"), "{z:?}");
    assert!(z.here_text.starts_with("Vous êtes ici : zone d'attente"), "{}", z.here_text);
    let d = decide_on(&buy_setup(), |_| {});
    let z = d.action_zones.as_ref().unwrap();
    assert!(matches!(z.here.as_deref(), Some("buy") | Some("wait")), "{z:?}");
    let d = decide_on(&downtrend(), |_| {});
    assert!(d.action_zones.is_none() && d.plan.is_none());
    let bear = d.scenarios.iter().find(|s| s.kind == ScenarioKind::Bear).unwrap();
    assert!(bear.conditions.iter().any(|c| c.text == "RSI sous 50" && c.state == CheckState::Met), "{:?}", bear.conditions);
    let u = d.unfolding.as_ref().unwrap();
    assert!(u.text.contains("pas une prévision"), "{}", u.text);
    assert_ne!(u.kind, ScenarioKind::Bull, "{u:?}");
    // Counter-argument: in a downtrend, the resistance and a volume surge would prove the wait wrong.
    let c = &d.counter_argument;
    assert!(c.invalidators.iter().any(|i| i.kind == "volume" && i.text.contains("1,5 × la moyenne 20 j")), "{c:?}");
    assert!(c.unfavourable > 0);
}

// ---------- « Preuve du modèle » ----------

fn validation_sample() -> ValidationReport {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/validation.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Without a cached report nothing changes (checked against the previous engine's output when this was added); with
/// the saved real run, each class is told honestly and the confidence is only ever lowered.
#[test]
fn model_evidence_from_the_saved_validation() {
    let t = now();
    let report = validation_sample();
    for (sym, class, weak) in [
        ("BTC", AssetClass::Btc, true),
        ("ETH", AssetClass::Eth, true),
        ("SOL", AssetClass::Altcoin, false),
        ("DOGE", AssetClass::Altcoin, false),
        ("AAPL", AssetClass::Stock, true),
        ("NVDA", AssetClass::Stock, true),
        ("SPY", AssetClass::Stock, true),
    ] {
        let s = real(sym);
        let g = guard_of(&s, t);
        let without = decide(&input(sym, &s, Some(&g), t));
        let e = &without.model_evidence;
        assert!(!e.available && e.text == NOT_COMPUTED && e.as_of.is_none() && e.asset_class == class, "{sym}: {e:?}");
        assert!(without.rating_reason.is_none());
        let mut i = input(sym, &s, Some(&g), t);
        i.validation = Some(&report);
        let d = decide(&i);
        assert_coherent(&d, sym);
        let e = &d.model_evidence;
        let group = report.classes.iter().find(|c| c.id == class.id()).unwrap();
        assert!(e.available && e.asset_class == class && e.as_of == Some(report.as_of), "{sym}: {e:?}");
        assert_eq!((e.class_verdict, e.t_stat, e.trades, e.assets), (Some(group.verdict), group.pooled.t_stat, group.pooled.trades, group.assets));
        assert_eq!(e.weak, weak, "{sym}");
        // The regime is the one the validation's rule gives on the last closed daily candle, same for both.
        assert_eq!(e.regime, without.model_evidence.regime, "{sym}");
        let known = e.regime != altim::engine::backtest::Regime::Unknown;
        assert_eq!(e.regime_verdict, group.regimes.iter().find(|r| known && r.regime == e.regime).map(|r| r.verdict), "{sym}");
        // Same verdict; confidence capped at 60 when weak, else unchanged; never raised.
        assert_eq!(d.verdict, without.verdict, "{sym}");
        let expected = if weak { without.confidence.min(EVIDENCE_CAP) } else { without.confidence };
        assert_eq!(d.confidence, expected, "{sym}");
        assert_eq!(d.confidence_text.contains("plafonnée à 60"), without.confidence > EVIDENCE_CAP && weak, "{sym}: {}", d.confidence_text);
        assert!(d.cons.last().unwrap().contains(&format!("(validation sur {} actif", group.assets)), "{sym}: {:?}", d.cons);
        assert!(e.text.ends_with("Résultats passés, sans garantie pour la suite."), "{sym}: {}", e.text);
        if class == AssetClass::Stock {
            // Stocks: positive mean per trade (t ≈ 2,2) but holding did better on 20 of the 22.
            assert!(
                e.text.starts_with(
                    "Sur les actions et ETF testés (22), gain moyen positif par trade (t = 2,2) mais la simple détention a fait mieux dans 20 cas sur 22 ; en "
                ),
                "{}",
                e.text
            );
            assert_eq!(e.beat_hold.as_deref(), Some("2/22"));
        }
        if class == AssetClass::Altcoin {
            assert!(
                e.text.starts_with("Sur les altcoins testés (10), avantage par trade non démontré (t = −1,6) ; la simple détention a fait mieux dans 6 cas sur 10 ; en "),
                "{}",
                e.text
            );
        }
        println!("{sym}: {}", e.text);
        let back: Decision = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back.model_evidence, d.model_evidence);
    }
}

/// Hand-made reports: a weak class caps the confidence and removes "ACHAT FORT" (with the reason), an unproven one
/// Hand-made class verdicts on real candles (NVDA, confidence above 60 without validation): an edge changes nothing,
/// an unproven class adds the con without a cap, a weak one (holding better on more than two thirds, or a negative
/// verdict) caps the confidence at 60 and says so. (The strong-rating downgrade is unit-tested in `model_evidence`.)
#[test]
fn model_evidence_caps_only_weak_classes() {
    let t = now();
    let s = real("NVDA");
    let g = guard_of(&s, t);
    let base = decide(&input("NVDA", &s, Some(&g), t));
    assert!(base.confidence > EVIDENCE_CAP, "{}", base.confidence);
    let with = |verdict: Proof, beat: usize, assets: usize| {
        let mut r = validation_sample();
        let c = r.classes.iter_mut().find(|c| c.id == "stock").unwrap();
        (c.verdict, c.beat_hold, c.assets) = (verdict, beat, assets);
        r
    };
    let run = |r: &ValidationReport| {
        let mut i = input("NVDA", &s, Some(&g), t);
        i.validation = Some(r);
        let d = decide(&i);
        assert_coherent(&d, "NVDA");
        d
    };
    // Edge, beating holding on half the assets: nothing changes but the evidence itself.
    let d = run(&with(Proof::Edge, 5, 10));
    assert!(d.model_evidence.available && !d.model_evidence.weak);
    let mut same = d.clone();
    same.model_evidence = base.model_evidence.clone();
    assert_eq!(same, base, "an edge never changes the decision");
    // Unproven, not weak: the con only.
    let d = run(&with(Proof::Unproven, 5, 10));
    assert_eq!((d.verdict, d.confidence, d.level, d.rating), (base.verdict, base.confidence, base.level, base.rating));
    assert!(d.cons.iter().any(|c| c == "Le signal n'a pas démontré d'avantage sur cette classe d'actifs (validation sur 10 actifs)"), "{:?}", d.cons);
    assert_eq!(d.cons.len(), base.cons.len() + 1);
    // Insufficient: the con only.
    let d = run(&with(Proof::Insufficient, 4, 10));
    assert_eq!(d.confidence, base.confidence);
    assert!(d.model_evidence.text.contains("trop peu de trades"), "{}", d.model_evidence.text);
    // Edge per trade but holding did better on 8 of 10 (beat < a third): weak, capped at 60.
    let d = run(&with(Proof::Edge, 2, 10));
    assert!(d.model_evidence.weak);
    assert_eq!((d.verdict, d.confidence), (base.verdict, EVIDENCE_CAP));
    assert!(d.confidence_text.contains(&format!("plafonnée à 60 (au lieu de {})", base.confidence)), "{}", d.confidence_text);
    assert!(d.cons.iter().any(|c| c.ends_with("(validation sur 10 actifs : la simple détention a fait mieux dans 8 cas)")), "{:?}", d.cons);
    // Exactly a third is not weak.
    assert!(!run(&with(Proof::Unproven, 3, 9)).model_evidence.weak);
    // Negative: weak even when it beat holding everywhere.
    let d = run(&with(Proof::Negative, 10, 10));
    assert!(d.model_evidence.weak && d.confidence == EVIDENCE_CAP);
    assert!(d.model_evidence.text.contains("perte moyenne par trade"), "{}", d.model_evidence.text);
}

fn bot_sample() -> altim::engine::bot::BotReport {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/samples/bot.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// « Bot Altim » in the decision. Saved real report (no edge): shown only, the decision is otherwise the same. A side
/// with an edge and today's action on it: one pro (ACHETER) or con (VENDRE) and ± 3 confidence points on a buy-side
/// verdict; the verdict and the vetoes never change.
#[test]
fn bot_counts_only_on_a_side_with_an_edge() {
    use altim::engine::bot::{BotAction, BotGroup, Candidate, LiveSide, ModelOut, NO_EDGE, NOT_COMPUTED as BOT_NOT_COMPUTED, NUDGE, Weight};
    let report = bot_sample();
    let t = now();
    for sym in ["BTC", "SOL", "AAPL", "NVDA"] {
        let s = real(sym);
        let mut b = input(sym, &s, None, t);
        b.benchmarks = vec![benchmark_of(s.kind)];
        let base = decide(&b);
        assert!(!base.bot.available && base.bot.text == BOT_NOT_COMPUTED, "{sym}");
        let mut i = input(sym, &s, None, t);
        i.benchmarks = vec![benchmark_of(s.kind)];
        i.bot = Some(&report);
        let d = decide(&i);
        assert!(d.bot.available && d.bot.action.is_some() && !d.bot.counts, "{sym}: {}", d.bot.text);
        assert_eq!(d.bot.note, NO_EDGE, "{sym}");
        let mut same = d.clone();
        same.bot = base.bot.clone();
        assert_eq!(same, base, "{sym}: sans avantage, le bot ne change rien");
        let back: Decision = serde_json::from_value(serde_json::to_value(&d).unwrap()).unwrap();
        assert_eq!(back.bot, d.bot);
    }
    // A buy scenario (crypto; its own candles stand for bitcoin's) and a crypto group with an edge on both sides.
    let s = buy_setup();
    let btc = Benchmark { name: "Bitcoin (BTC)".into(), symbol: "BTC".into(), kind: Kind::Crypto, daily: s.long.clone() };
    let base = decide_on(&s, |i| i.benchmarks = vec![btc.clone()]);
    assert!(matches!(base.verdict, Verdict::Buy | Verdict::BuyZone));
    let side = |intercept: f64| LiveSide {
        base_rate: 50.0,
        threshold: 55.0,
        logit: Some(ModelOut {
            base_rate: 50.0,
            threshold: 55.0,
            intercept,
            weights: vec![Weight { id: "ret5".into(), coef: 0.0, mean: 0.0, sd: 1.0 }],
        }),
        ..LiveSide::default()
    };
    let forced = |up: f64| {
        let mut r = report.clone();
        let g = r.groups.iter_mut().find(|g| g.id == BotGroup::Crypto).unwrap();
        g.stats.buy.verdict = Some(Proof::Edge);
        g.stats.sell.verdict = Some(Proof::Edge);
        let m = g.model.as_mut().unwrap();
        m.candidate = Candidate::Logit;
        (m.up_model, m.down_model) = (side(up), side(-up));
        r
    };
    let buy = forced(20.0);
    let d = decide_on(&s, |i| {
        i.benchmarks = vec![btc.clone()];
        i.bot = Some(&buy);
    });
    assert_eq!((d.bot.action, d.bot.counts), (Some(BotAction::Buy), true));
    assert_eq!((d.verdict, d.vetoes.clone()), (base.verdict, base.vetoes.clone()));
    assert_eq!(d.confidence, (base.confidence + NUDGE).min(100.0));
    assert!(d.pros.iter().any(|p| p.starts_with("Le bot appris est favorable")), "{:?}", d.pros);
    assert!(d.confidence_text.contains("Bot Altim : +3 points"), "{}", d.confidence_text);
    let sell = forced(-20.0);
    let d = decide_on(&s, |i| {
        i.benchmarks = vec![btc.clone()];
        i.bot = Some(&sell);
    });
    assert_eq!((d.bot.action, d.bot.counts), (Some(BotAction::Sell), true));
    assert_eq!(d.verdict, base.verdict);
    assert_eq!(d.confidence, (base.confidence - NUDGE).max(0.0));
    assert!(d.cons.iter().any(|c| c.starts_with("Le bot appris est défavorable")), "{:?}", d.cons);
    // Held: « conseille de sortir », as a con; the verdict stays the position's own.
    let held_base = decide_on(&s, |i| {
        i.benchmarks = vec![btc.clone()];
        i.cost = Some(100.0);
    });
    let held = decide_on(&s, |i| {
        i.benchmarks = vec![btc.clone()];
        i.cost = Some(100.0);
        i.bot = Some(&sell);
    });
    assert_eq!(held.verdict, held_base.verdict);
    assert!(held.cons.iter().any(|c| c.starts_with("Le bot appris conseille de sortir")), "{:?}", held.cons);
}

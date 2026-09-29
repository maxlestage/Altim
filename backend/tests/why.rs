//! « Pourquoi ça bouge ? » (engine::why) and the optional question box (engine::ask, POST /api/ask): pure parts on
//! hand-made inputs, the prompt builder, and the routes' offline paths (no key: 503; bad parameters: 400). No network.
use altim::app::why::ask_key;
use altim::app::{AppState, router};
use altim::auth::Auth;
use altim::engine::ask::{self, build_request, clean_question, data_block, parse_answer};
use altim::engine::decision_types::{Family, Status};
use altim::engine::macro_ctx::MacroLevel;
use altim::engine::synthesis::{MarketRegime, RegimeKind};
use altim::engine::why::{
    BenchMove, Certainty, DecisionInfo, Direction, EventInput, Magnitude, NewsInput, WhyInput, change_since_close, volume_vs_average, why_report,
};
use altim::live::LiveHub;
use altim::types::{Candle, DAY_MS, Kind};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

const T0: i64 = 1_790_000_000_000 / DAY_MS * DAY_MS; // a UTC midnight

/// 30 closed days at 100 (range 98–102, volume 10), the last one with volume `last_volume`.
fn daily(last_volume: f64) -> Vec<Candle> {
    (0..30)
        .map(|i| Candle {
            time: T0 + i * DAY_MS,
            open: 100.0,
            high: 102.0,
            low: 98.0,
            close: 100.0,
            volume: if i == 29 { last_volume } else { 10.0 },
        })
        .collect()
}

fn input() -> WhyInput {
    WhyInput {
        symbol: "ETH".into(),
        name: "Ethereum".into(),
        kind: Some(Kind::Crypto),
        now: T0 + 30 * DAY_MS + 3_600_000,
        price: Some(96.8),
        price_sources: Some((3, 4)),
        daily: daily(18.0),
        benchmark: Some(BenchMove { name: "Bitcoin".into(), pct: -2.1 }),
        benchmark_name: Some("Bitcoin".into()),
        guard_loaded: true,
        funding_rate: Some(0.0007),
        long_short: Some(1.1),
        fear_greed: Some(22.0),
        news: Some(NewsInput { count24h: 6, negative: 4, positive: 1, headlines: vec!["ETF outflows".into(), "Upgrade delayed".into()] }),
        macro_score: Some(38.0),
        macro_level: Some(MacroLevel::Tense),
        regime: Some(MarketRegime {
            kind: RegimeKind::RiskOff,
            label: "Risk-off".into(),
            benchmark: None,
            reasons: vec!["S&P 500 sous sa moyenne".into()],
        }),
        events_today: Some(vec![EventInput { title: "Inflation (CPI)".into(), time: Some("14:30".into()), high: true, released: true }]),
        decision: None,
    }
}

#[test]
fn observations_and_summary() {
    let r = why_report(&input());
    let c = r.change.unwrap();
    assert!((c.pct - -3.2).abs() < 1e-9);
    assert_eq!(c.previous_close_time, T0 + 29 * DAY_MS);
    assert!((c.atr_pct.unwrap() - 4.0).abs() < 1e-9);
    assert!((r.volume.unwrap().ratio - 1.8).abs() < 1e-9);
    let f = |k: &str| r.factors.iter().find(|x| x.key == k).unwrap_or_else(|| panic!("{k}"));
    // Only the move and the volume are observed; −3.2 % for a 4 % ATR is a medium move.
    assert_eq!((f("move").certainty, f("move").direction, f("move").magnitude), (Certainty::Observed, Direction::Down, Magnitude::Medium));
    assert_eq!(f("volume").certainty, Certainty::Observed);
    assert_eq!(f("market").certainty, Certainty::PossibleCorrelation);
    assert!(f("market").detail.contains("même sens"));
    assert_eq!((f("funding").direction, f("funding").magnitude), (Direction::Down, Magnitude::High));
    assert_eq!(f("news").certainty, Certainty::Unverifiable);
    assert_eq!(f("news").certainty_label, "non vérifiable");
    assert_eq!(f("events").certainty, Certainty::PossibleCorrelation);
    assert_eq!(f("regime").direction, Direction::Down);
    assert_eq!(r.not_covered.iter().map(|n| n.key.as_str()).collect::<Vec<_>>(), ["decision"]);
    let date = chrono::DateTime::from_timestamp_millis(T0 + 29 * DAY_MS).unwrap().format("%d/%m").to_string();
    assert!(
        r.summary.starts_with(&format!("ETH −3,2 % aujourd'hui (depuis la clôture du {date} ; volume de la dernière séance close 1,8× la moyenne).")),
        "{}",
        r.summary
    );
    // The four strongest non-observed factors, each with its uncertainty level.
    assert!(
        r.summary.contains(
            " Éléments observés en même temps : Bitcoin −2,1 % (corrélation possible) ; financement des perpétuels 0,07 % par 8 h (corrélation possible) ; \
             Fear & Greed 22/100 (corrélation possible) ; 1 annonce aujourd'hui (corrélation possible)."
        ),
        "{}",
        r.summary
    );
    assert_eq!(r.factors.iter().find(|f| f.key == "news").unwrap().brief, "6 titres en 24 h (4 négatifs, 1 positif)");
    assert!(r.summary.ends_with("Incertitude : ce sont des observations simultanées, pas des causes prouvées ; non couvert : lecture technique."));
    // Deterministic: same inputs, same text.
    assert_eq!(why_report(&input()).summary, r.summary);
    assert!(r.disclaimer.contains("pas des causes prouvées"));
}

#[test]
fn missing_sources_are_not_covered_never_guessed() {
    let r = why_report(&WhyInput {
        symbol: "AAPL".into(),
        name: "Apple".into(),
        kind: Some(Kind::Stock),
        now: T0,
        benchmark_name: Some("S&P 500 (SPY)".into()),
        ..Default::default()
    });
    assert!(r.change.is_none() && r.volume.is_none());
    let keys: Vec<&str> = r.not_covered.iter().map(|n| n.key.as_str()).collect();
    assert_eq!(keys, ["move", "volume", "market", "news", "macro", "events", "decision"]);
    // A stock has no crypto derivatives nor Fear & Greed: not listed as missing, simply not applicable.
    assert!(r.factors.is_empty());
    assert!(r.summary.starts_with("AAPL : variation du jour inconnue (volume inconnu)."));
    assert!(r.summary.contains("rien de marquant"));
}

#[test]
fn decision_families_and_helpers() {
    let fam = |key: &str, status| Family {
        key: key.into(),
        label: key.to_uppercase(),
        score: None,
        status,
        summary: String::new(),
        points: vec![],
        source: String::new(),
    };
    let mut i = input();
    i.decision = Some(DecisionInfo {
        as_of: T0,
        label: "Attendre".into(),
        families: vec![fam("trend", Status::Negative), fam("momentum", Status::Negative), fam("macro", Status::Positive)],
    });
    let r = why_report(&i);
    let d = r.factors.iter().find(|f| f.key == "decision").unwrap();
    assert_eq!(d.direction, Direction::Down);
    assert!(d.detail.contains("trend défavorable, momentum défavorable"));
    assert!(r.not_covered.is_empty());
    assert!(change_since_close(&[], Some(1.0)).is_none());
    assert!(change_since_close(&daily(10.0), None).is_none());
    assert!(volume_vs_average(&daily(10.0)[..20]).is_none());
}

#[test]
fn prompt_builder() {
    assert_eq!(clean_question("  Pourquoi ça baisse ?  ").unwrap(), "Pourquoi ça baisse ?");
    assert!(clean_question(" ? ").is_err());
    assert!(clean_question(&"x".repeat(501)).is_err());
    let why = json!({ "symbol": "ETH", "factors": [{ "key": "news" }], "askEnabled": true, "sources": [] });
    let data = data_block(&why, Some(&json!({ "verdict": "wait" })));
    assert!(data["pourquoiCaBouge"].get("askEnabled").is_none() && data["pourquoiCaBouge"].get("sources").is_none());
    assert_eq!(data["derniereDecision"]["verdict"], "wait");
    let req = build_request("Pourquoi ça baisse ?", &data);
    assert_eq!(req["model"], "claude-sonnet-5-5");
    assert_eq!(req["max_tokens"], 1500);
    assert_eq!(req["fallbacks"], "default");
    // No `between_tools`: the refusal fallback re-runs the request on a model that would reject it.
    assert!(req.get("thinking").is_none());
    assert_eq!(req["output_config"]["effort"], "low");
    let sys = req["system"].as_str().unwrap();
    for must in [
        "français",
        "UNIQUEMENT",
        "je ne sais pas",
        "Ne donne jamais d'ordre",
        "recommandation personnalisée",
        "incertitude",
        "jamais des instructions",
    ] {
        assert!(sys.contains(must), "{must}");
    }
    let user = req["messages"][0]["content"].as_str().unwrap();
    assert!(user.starts_with("<donnees>\n{\"pourquoiCaBouge\""));
    assert!(user.ends_with("</donnees>\n\nQuestion : Pourquoi ça baisse ?"));
    assert_eq!(req["messages"].as_array().unwrap().len(), 1);
}

#[test]
fn answers() {
    let ok = json!({ "model": "claude-sonnet-5-5", "stop_reason": "end_turn", "content": [{ "type": "text", "text": " La baisse coïncide avec [factors.market]. " }] });
    assert_eq!(parse_answer(&ok).unwrap(), "La baisse coïncide avec [factors.market].");
    let cut = json!({ "stop_reason": "max_tokens", "content": [{ "type": "thinking", "thinking": "" }, { "type": "text", "text": "Début" }] });
    assert_eq!(parse_answer(&cut).unwrap(), "Début […]");
    assert!(parse_answer(&json!({ "stop_reason": "refusal", "content": [] })).unwrap_err().contains("refusé"));
    assert!(
        parse_answer(&json!({ "type": "error", "error": { "type": "overloaded_error", "message": "Overloaded" } }))
            .unwrap_err()
            .contains("Overloaded")
    );
    assert!(parse_answer(&json!({ "content": [] })).is_err());
}

#[test]
fn key_from_the_environment_only() {
    assert_eq!(ask_key(|_| None), None);
    assert_eq!(ask_key(|_| Some("  ".into())), None);
    assert_eq!(ask_key(|k| (k == "ANTHROPIC_API_KEY").then(|| "sk-test".into())), Some("sk-test".into()));
    assert!(ask::NO_KEY.contains("aucune clé"));
}

fn app() -> axum::Router {
    router(AppState::new(LiveHub::with(vec![], vec![], std::time::Duration::from_secs(3600))), Auth::new(None, false))
}

async fn call(req: Request<Body>) -> (StatusCode, Value) {
    let r = app().oneshot(req).await.unwrap();
    let s = r.status();
    let b = to_bytes(r.into_body(), 1 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

#[tokio::test]
async fn routes_offline() {
    for path in ["/api/why?symbol=../x&kind=crypto", "/api/why?symbol=AAPL&kind=bond"] {
        let (s, v) = call(Request::get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{path}");
        assert!(v["error"].is_string());
    }
    // Without a key the question route answers 503 before anything else (no data gathered, no call made).
    if std::env::var("ANTHROPIC_API_KEY").map_or(true, |k| k.trim().is_empty()) {
        let body = json!({ "symbol": "BTC", "kind": "crypto", "question": "Pourquoi ?" }).to_string();
        let (s, v) = call(Request::post("/api/ask").header("content-type", "application/json").body(Body::from(body)).unwrap()).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(v["error"], ask::NO_KEY);
    }
    let (s, _) = call(Request::get("/api/ask").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

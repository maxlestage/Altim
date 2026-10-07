//! Real-time WebSocket (`/api/ws`): handshake (session, Origin, limits), subscription → snapshot ticks, alerts and
//! verdicts, and the change-only push logic. No upstream call: alerts come from a fake computation.
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use altim::app::ws::{self, AlertHub, AssetState, ClientMsg, FxNow, Pushed, merge_alerts, origin_allowed, parse_client};
use altim::app::{AppState, router};
use altim::auth::{Auth, AuthConfig, SESSION_COOKIE, make_session};
use altim::js::now_ms;
use altim::live::LiveHub;
use altim::types::{Interval, Kind};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{self, Message};

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn cfg() -> AuthConfig {
    AuthConfig {
        user: "max".into(),
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
        totp_secret: None,
        session_secret: "s".repeat(64),
        api_token: Some("bot-token-123".into()),
    }
}

fn fake_alerts(calls: Arc<AtomicUsize>) -> AlertHub {
    AlertHub::with(
        move |a, _usd| {
            calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let verdict = json!({ "type": "verdict", "symbol": a.symbol, "kind": a.kind, "verdict": "wait", "label": "ATTENDRE",
                    "rating": "hold", "ratingLabel": "ATTENDRE", "chipNote": "zone d'achat plus bas", "asOf": 1 });
                AssetState {
                    alert: json!({ "symbol": a.symbol, "kind": a.kind, "name": a.name, "price": 100.0, "asOf": now_ms(), "buy": false,
                        "strong": false, "title": format!("{} : pas pour l'instant", a.symbol) }),
                    verdict: Some(verdict),
                    checked_at: now_ms(),
                }
            })
        },
        |_| Duration::from_secs(3600),
    )
}

struct Server {
    addr: SocketAddr,
    live: LiveHub,
    alerts: AlertHub,
    calls: Arc<AtomicUsize>,
}

async fn server() -> Server {
    let live = LiveHub::with(vec![], vec![], Duration::from_secs(3600));
    let calls = Arc::new(AtomicUsize::new(0));
    let alerts = fake_alerts(calls.clone());
    let app = router(AppState::with_alerts(live.clone(), alerts.clone()), Auth::new(Some(cfg()), true));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap() });
    Server { addr, live, alerts, calls }
}

fn cookie() -> String {
    format!("{SESSION_COOKIE}={}", make_session(&cfg(), now_ms()))
}

async fn connect(addr: SocketAddr, headers: &[(&'static str, String)]) -> Result<Socket, tungstenite::Error> {
    let mut req = format!("ws://{addr}/api/ws").into_client_request().unwrap();
    for (k, v) in headers {
        req.headers_mut().insert(*k, v.parse().unwrap());
    }
    tokio_tungstenite::connect_async(req).await.map(|(s, _)| s)
}

fn status(e: tungstenite::Error) -> u16 {
    match e {
        tungstenite::Error::Http(r) => r.status().as_u16(),
        other => panic!("HTTP error expected, got {other:?}"),
    }
}

/// Next JSON text message (control frames skipped), 5 s at most.
async fn next_json(s: &mut Socket) -> Value {
    loop {
        let m = tokio::time::timeout(Duration::from_secs(5), s.next()).await.expect("message attendu").unwrap().unwrap();
        if let Message::Text(t) = m {
            return serde_json::from_str(t.as_str()).unwrap();
        }
    }
}

async fn send(s: &mut Socket, v: Value) {
    s.send(Message::Text(v.to_string().into())).await.unwrap();
}

#[tokio::test]
async fn handshake_refuse_sans_session_et_origine_etrangere() {
    let srv = server().await;
    assert_eq!(status(connect(srv.addr, &[]).await.unwrap_err()), 401);
    assert_eq!(status(connect(srv.addr, &[("cookie", format!("{SESSION_COOKIE}=forged.sig"))]).await.unwrap_err()), 401);
    // Cross-site WebSocket hijacking: a page of another site carries the cookie but its own Origin.
    let evil = [("cookie", cookie()), ("origin", "https://evil.example".to_string())];
    assert_eq!(status(connect(srv.addr, &evil).await.unwrap_err()), 403);
    // Same origin (browser), no origin (native apps), bot token: accepted.
    let same = [("cookie", cookie()), ("origin", format!("http://{}", srv.addr))];
    let mut s = connect(srv.addr, &same).await.unwrap();
    assert_eq!(next_json(&mut s).await, json!({ "type": "hello", "v": 1, "maxAssets": 20, "pingEvery": 20 }));
    connect(srv.addr, &[("cookie", cookie())]).await.unwrap();
    connect(srv.addr, &[("authorization", "Bearer bot-token-123".to_string())]).await.unwrap();
    assert_eq!(status(connect(srv.addr, &[("authorization", "Bearer wrong".to_string())]).await.unwrap_err()), 401);
}

#[tokio::test]
async fn abonnement_ticks_alertes_verdicts() {
    let srv = server().await;
    srv.live.on_quote("crypto:BTC", "OKX", 60_000.0, Some(1.5));
    tokio::time::sleep(Duration::from_millis(400)).await;
    let mut s = connect(srv.addr, &[("cookie", cookie())]).await.unwrap();
    assert_eq!(next_json(&mut s).await["type"], "hello");
    let sub = json!({ "type": "subscribe", "assets": [{ "kind": "crypto", "symbol": "btc" }, { "kind": "stock", "symbol": "AAPL" }],
        "interval": "4h", "currency": "USD" });
    send(&mut s, sub.clone()).await;
    // 1. The last known price right away (same fields as the SSE tick).
    let t = next_json(&mut s).await;
    assert_eq!((t["type"].as_str(), t["symbol"].as_str(), t["price"].as_f64()), (Some("tick"), Some("BTC"), Some(60_000.0)));
    assert_eq!(t["sources"], json!(["OKX"]));
    // 2. Alerts once both assets were checked, in the subscription's order, then the verdicts.
    let a = next_json(&mut s).await;
    assert_eq!(a["type"], "alerts");
    let symbols: Vec<&str> = a["items"].as_array().unwrap().iter().map(|i| i["symbol"].as_str().unwrap()).collect();
    assert_eq!(symbols, ["BTC", "AAPL"]);
    assert!(a["checkedAt"].as_i64().unwrap() > 0);
    let v1 = next_json(&mut s).await;
    let v2 = next_json(&mut s).await;
    assert_eq!((v1["type"].as_str(), v1["symbol"].as_str(), v2["symbol"].as_str()), (Some("verdict"), Some("BTC"), Some("AAPL")));
    // 3. A new price is pushed.
    srv.live.on_quote("crypto:BTC", "OKX", 60_100.0, Some(1.6));
    let t = next_json(&mut s).await;
    assert_eq!((t["type"].as_str(), t["price"].as_f64()), (Some("tick"), Some(60_100.0)));
    // 4. Application ping.
    send(&mut s, json!({ "type": "ping" })).await;
    assert_eq!(next_json(&mut s).await["type"], "pong");
    // A second socket on the same assets shares the workers (one computation per asset).
    let mut s2 = connect(srv.addr, &[("cookie", cookie())]).await.unwrap();
    next_json(&mut s2).await;
    send(&mut s2, sub).await;
    loop {
        if next_json(&mut s2).await["type"] == "alerts" {
            break;
        }
    }
    assert_eq!(srv.calls.load(Ordering::SeqCst), 2);
    assert_eq!(srv.alerts.workers(), 2);
    // Both gone: the workers stop, the prices are no longer watched.
    drop(s);
    drop(s2);
    for _ in 0..50 {
        if srv.alerts.workers() == 0 && srv.live.watched() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!((srv.alerts.workers(), srv.live.watched()), (0, 0));
}

#[tokio::test]
async fn limites() {
    let srv = server().await;
    // Too many assets, bad message, binary frame: an error message, the socket stays open.
    let mut s = connect(srv.addr, &[("cookie", cookie())]).await.unwrap();
    next_json(&mut s).await;
    let many: Vec<Value> = (0..21).map(|i| json!({ "kind": "crypto", "symbol": format!("C{i}") })).collect();
    send(&mut s, json!({ "type": "subscribe", "assets": many })).await;
    assert_eq!(next_json(&mut s).await["code"], "too_many_assets");
    send(&mut s, json!({ "type": "nope" })).await;
    assert_eq!(next_json(&mut s).await["code"], "bad_message");
    s.send(Message::Binary(vec![1, 2, 3].into())).await.unwrap();
    assert_eq!(next_json(&mut s).await["code"], "bad_message");
    // A message over 16 KB closes the socket.
    let _ = s.send(Message::Text("x".repeat(ws::MAX_MESSAGE + 10).into())).await;
    let end = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match s.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                _ => {}
            }
        }
    })
    .await;
    assert!(end.is_ok());
    // Eight sockets per address, the ninth is refused until one closes.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut open = Vec::new();
    for _ in 0..ws::MAX_SOCKETS {
        open.push(connect(srv.addr, &[("cookie", cookie())]).await.unwrap());
    }
    assert_eq!(status(connect(srv.addr, &[("cookie", cookie())]).await.unwrap_err()), 429);
    drop(open.pop());
    tokio::time::sleep(Duration::from_millis(300)).await;
    connect(srv.addr, &[("cookie", cookie())]).await.unwrap();
}

#[test]
fn messages_client() {
    let m = parse_client(r#"{"type":"subscribe","assets":[{"kind":"crypto","symbol":"btc"},{"kind":"crypto","symbol":"BTC"},{"kind":"stock","symbol":"AAPL"}],"interval":"1d","currency":"EUR"}"#).unwrap();
    let ClientMsg::Subscribe { assets, interval, usd } = m else { panic!() };
    assert_eq!(assets.iter().map(|a| (a.symbol.as_str(), a.kind)).collect::<Vec<_>>(), [("BTC", Kind::Crypto), ("AAPL", Kind::Stock)]);
    assert_eq!((interval, usd), (Interval::D1, false));
    assert_eq!(parse_client(r#"{"type":"ping"}"#), Ok(ClientMsg::Ping));
    assert!(matches!(parse_client(r#"{"type":"subscribe","assets":[]}"#), Ok(ClientMsg::Subscribe { usd: false, interval: Interval::H4, .. })));
    for bad in [
        r#"{"type":"subscribe","assets":[{"kind":"forex","symbol":"EUR"}]}"#,
        r#"{"type":"subscribe","currency":"GBP"}"#,
        "[]",
        "{",
        r#"{"type":"subscribe","interval":"2h"}"#,
    ] {
        assert_eq!(parse_client(bad).unwrap_err().code, "bad_message", "{bad}");
    }
}

#[test]
fn origine() {
    assert!(origin_allowed(None, Some("altim.example")));
    assert!(origin_allowed(Some("https://altim.example"), Some("altim.example")));
    assert!(origin_allowed(Some("http://localhost:4700"), Some("localhost:4700")));
    assert!(!origin_allowed(Some("https://evil.example"), Some("altim.example")));
    assert!(!origin_allowed(Some("https://altim.example.evil.example"), Some("altim.example")));
    assert!(!origin_allowed(Some("null"), Some("altim.example")));
    assert!(!origin_allowed(Some("https://altim.example"), None));
    assert!(!origin_allowed(Some("chrome-extension://altim.example"), Some("altim.example")));
}

#[test]
fn csp_websocket_du_meme_hote() {
    let csp = altim::app::web::csp_for(Some("altim.example"));
    assert!(csp.contains("connect-src 'self' wss://altim.example https://api.binance.com"));
    assert_eq!(altim::app::web::csp_for(Some("evil.example; script-src *")), altim::app::web::CSP);
    assert_eq!(altim::app::web::csp_for(None), altim::app::web::CSP);
}

fn state(symbol: &str, buy: bool, price: f64, checked: i64, verdict: &str) -> Arc<AssetState> {
    Arc::new(AssetState {
        alert: json!({ "symbol": symbol, "kind": "crypto", "price": price, "asOf": checked, "buy": buy, "fx": { "rate": 0.9 } }),
        verdict: Some(json!({ "type": "verdict", "symbol": symbol, "kind": "crypto", "verdict": verdict, "label": verdict, "chipNote": null })),
        checked_at: checked,
    })
}

#[test]
fn envoi_seulement_quand_ca_change() {
    let order = vec!["crypto:BTC".to_string(), "crypto:ETH".to_string()];
    let mut states = std::collections::HashMap::new();
    assert_eq!(merge_alerts(&order, &states), None);
    states.insert("crypto:ETH".to_string(), state("ETH", false, 3000.0, 2_000, "wait"));
    let (items, at) = merge_alerts(&order, &states).unwrap();
    assert_eq!((items.len(), at), (1, 2_000));
    states.insert("crypto:BTC".to_string(), state("BTC", false, 60_000.0, 1_000, "wait"));
    let (items, at) = merge_alerts(&order, &states).unwrap();
    assert_eq!((items[0]["symbol"].as_str(), at), (Some("BTC"), 1_000));

    let mut p = Pushed::default();
    assert_eq!(p.alerts(&items, at).unwrap()["type"], "alerts");
    assert_eq!(p.alerts(&items, at + 1_000), None);
    // Only the price and time stamps moved: no alerts message, a "checked" one after 30 s.
    states.insert("crypto:BTC".to_string(), state("BTC", false, 60_500.0, 40_000, "wait"));
    let (items, _) = merge_alerts(&order, &states).unwrap();
    assert_eq!(p.alerts(&items, 20_000), None);
    assert_eq!(p.alerts(&items, 31_000), Some(json!({ "type": "checked", "checkedAt": 31_000 })));
    // Buyable now: pushed.
    states.insert("crypto:ETH".to_string(), state("ETH", true, 3000.0, 41_000, "buy"));
    let (items, at) = merge_alerts(&order, &states).unwrap();
    assert_eq!(p.alerts(&items, at).unwrap()["items"][1]["buy"], true);

    let v = |s: &str| states.get(s).unwrap().verdict.clone().unwrap();
    let all = [v("crypto:BTC"), v("crypto:ETH")];
    assert_eq!(p.verdicts(all.iter()).len(), 2);
    assert!(p.verdicts(all.iter()).is_empty());
    let changed = [v("crypto:BTC"), state("ETH", true, 1.0, 1, "wait").verdict.clone().unwrap()];
    let out = p.verdicts(changed.iter());
    assert_eq!((out.len(), out[0]["symbol"].as_str(), out[0]["verdict"].as_str()), (1, Some("ETH"), Some("wait")));
    // A new subscription sends everything once more.
    p.reset();
    assert_eq!(p.verdicts(changed.iter()).len(), 2);

    let fx = FxNow { rate: 0.92, usd_per_eur: 1.0 / 0.92, time: 5, source: "ECB".into() };
    assert_eq!(p.fx(None), None);
    assert_eq!(p.fx(Some(&fx)).unwrap()["rate"], 0.92);
    assert_eq!(p.fx(Some(&fx)), None);
    assert!(p.fx(Some(&FxNow { rate: 0.93, ..fx })).is_some());
}

#[test]
fn budget_et_cadence() {
    let mut b = ws::Budget::default();
    let t0 = 1_000_000;
    assert!((0..ws::MAX_CLIENT_MESSAGES).all(|_| b.take(t0)));
    assert!(!b.take(t0 + 10));
    assert!(b.take(t0 + 60_000));
    // Stocks: every minute while the US market is open, every 10 minutes when it is closed (a Sunday).
    let sunday = 1_759_658_400_000; // 2025-10-05 10:00 UTC
    assert_eq!(ws::check_every(Kind::Stock, sunday), ws::CHECK_CLOSED);
    assert_eq!(ws::check_every(Kind::Crypto, sunday), ws::CHECK_EVERY);
    let tuesday_open = 1_759_852_800_000; // 2025-10-07 16:00 UTC = 12:00 New York
    assert_eq!(ws::check_every(Kind::Stock, tuesday_open), ws::CHECK_EVERY);
}

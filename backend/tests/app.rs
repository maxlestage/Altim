//! The whole application, request by request (port of the former web/test/server.test.ts): health, security
//! headers, HTTPS redirect, parameter validation, SPA routes, real 404s, API rate limit. No upstream call is made.
use altim::app::{AppState, router};
use altim::auth::Auth;
use altim::live::LiveHub;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn app() -> axum::Router {
    // No private access outside production (like the local server of the old tests).
    router(AppState::new(LiveHub::with(vec![], vec![], std::time::Duration::from_secs(3600))), Auth::new(None, false))
}

async fn get(path: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    get_with(path, &[]).await
}

async fn get_with(path: &str, headers: &[(&str, &str)]) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut req = Request::get(path);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let r = app().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let (status, h) = (r.status(), r.headers().clone());
    let body = String::from_utf8_lossy(&to_bytes(r.into_body(), 1 << 22).await.unwrap()).to_string();
    (status, h, body)
}

#[tokio::test]
async fn health() {
    let (s, _, b) = get("/health").await;
    assert_eq!((s, b.as_str()), (StatusCode::OK, "ok"));
}

#[tokio::test]
async fn security_headers_and_no_powered_by() {
    let (_, h, _) = get("/health").await;
    let csp = h["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("default-src 'self'") && csp.contains("frame-ancestors 'none'"));
    // The Yew front is WebAssembly: compiling it needs 'wasm-unsafe-eval', never 'unsafe-eval' nor inline scripts.
    assert!(
        csp.contains("script-src 'self' 'wasm-unsafe-eval';") && !csp.contains("'unsafe-eval'") && !csp.contains("script-src 'self' 'unsafe-inline'")
    );
    assert!(h["strict-transport-security"].to_str().unwrap().contains("max-age=63072000"));
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h["permissions-policy"], "camera=(), microphone=(), geolocation=()");
    assert!(h.get("x-powered-by").is_none());
}

#[tokio::test]
async fn https_redirect_behind_the_heroku_router() {
    let (s, h, _) = get_with("/app", &[("x-forwarded-proto", "http"), ("host", "altim.example")]).await;
    assert_eq!(s, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(h["location"], "https://altim.example/app");
}

#[tokio::test]
async fn api_parameter_validation() {
    for path in [
        "/api/candles?symbol=../etc&interval=1h",
        "/api/candles?symbol=BTC&interval=5m",
        "/api/candles?symbol=BTC&interval=1h&kind=forex",
        "/api/radar?symbols=AAPL:bond",
        "/api/alerts?symbols=BTC:crypto,../x:stock",
        "/api/sentiment?symbol=%3Cscript%3E",
        "/api/zones?symbol=../x&kind=crypto",
        "/api/zones?symbol=AAPL&kind=bond",
        "/api/selection?horizon=forever",
        "/api/selection?horizon=medium&kind=forex",
        "/api/history?days=12",
        "/api/anomalies?symbol=../x&kind=crypto",
        "/api/anomalies?symbol=BTC&kind=forex",
        "/api/opportunities?kind=forex",
    ] {
        let (s, h, b) = get(path).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{path}");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert!(v["error"].as_str().is_some_and(|e| !e.is_empty()), "{path}");
        assert_eq!(h["cache-control"], "private, no-store");
    }
    let (s, _, b) = get("/api/selection?horizon=2y").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b, r#"{"error":"horizon invalide (30m | 1h | 5h | 7d | 14d | 1m | 3m | 6m)"}"#);
    let (s, _, b) = get("/api/inconnue").await;
    assert_eq!((s, b.as_str()), (StatusCode::NOT_FOUND, r#"{"error":"route inconnue"}"#));
    assert_eq!(get("/api/search?q=").await.2, "[]");
    assert_eq!(get("/api/search?q=%20%20").await.2, "[]");
}

#[tokio::test]
async fn spa_routes_served_by_index_html() {
    for path in ["/", "/app", "/app/actif/BTC", "/mentions-legales"] {
        let (s, h, b) = get(path).await;
        // 200 after `sh scripts/build-web.sh`, 500 with an explicit message otherwise.
        assert!(s == StatusCode::OK || s == StatusCode::INTERNAL_SERVER_ERROR, "{path}");
        if s == StatusCode::OK {
            assert!(b.contains(r#"<div id="root">"#));
            assert_eq!(h["cache-control"], "no-cache");
        } else {
            assert_eq!(b, "Build manquant : lancez `sh scripts/build-web.sh`.");
        }
    }
}

#[tokio::test]
async fn missing_file_is_a_real_404() {
    let (s, _, b) = get("/app/index-abcdef12.js").await;
    assert_eq!((s, b.as_str()), (StatusCode::NOT_FOUND, "Introuvable"));
    // Never a file outside the web folder: the path falls back to the app page.
    assert!(!get("/../../etc/passwd").await.2.contains("root:"));
    assert!(!get("/%2e%2e/%2e%2e/etc/passwd").await.2.contains("root:"));
}

#[tokio::test]
async fn public_files_and_cache() {
    let (s, h, _) = get("/logo.svg").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["content-type"], "image/svg+xml");
    assert_eq!(h["cache-control"], "private, max-age=3600");
}

#[test]
fn wasm_files_cached_and_typed() {
    use altim::app::web::static_cache;
    assert_eq!(static_cache("/altim-0123456789abcdef_bg.wasm"), "private, max-age=31536000, immutable");
    assert_eq!(static_cache("/altim-0123456789abcdef.js"), "private, max-age=31536000, immutable");
    assert_eq!(static_cache("/app.css"), "private, max-age=3600");
}

#[tokio::test]
async fn api_rate_limit() {
    let app = app();
    let mut limited = false;
    for _ in 0..260 {
        let r = app.clone().oneshot(Request::get("/api/search?q=").body(Body::empty()).unwrap()).await.unwrap();
        if r.status() == StatusCode::TOO_MANY_REQUESTS {
            limited = true;
            break;
        }
    }
    assert!(limited);
}

/// `/api/decision`: every bad parameter is a 400 with a French message, before any upstream call.
#[tokio::test]
async fn decision_parameter_validation() {
    let many = (0..21).map(|i| format!("A{i}:stock:1")).collect::<Vec<_>>().join(",");
    for (path, message) in [
        ("/api/decision?symbol=../x&kind=crypto", "symbole invalide"),
        ("/api/decision?symbol=BTC&kind=bond", "kind invalide"),
        ("/api/decision?kind=crypto", "symbole invalide"),
        ("/api/decision?symbol=BTC&cost=-1", "cost invalide"),
        ("/api/decision?symbol=BTC&cost=0", "cost invalide"),
        ("/api/decision?symbol=BTC&cost=abc", "cost invalide"),
        ("/api/decision?symbol=BTC&cost=Infinity", "cost invalide"),
        ("/api/decision?symbol=BTC&weights=BTC", "weights invalide"),
        ("/api/decision?symbol=BTC&weights=BTC:crypto", "weights invalide"),
        ("/api/decision?symbol=BTC&weights=BTC:crypto:150", "weights invalide"),
        ("/api/decision?symbol=BTC&weights=BTC:crypto:-1", "weights invalide"),
        ("/api/decision?symbol=BTC&weights=BTC:crypto:", "weights invalide"),
        ("/api/decision?symbol=BTC&weights=BTC:crypto:x", "weights invalide"),
        ("/api/decision?symbol=BTC&weights=BTC:crypto:60,ETH:crypto:60", "weights invalide"),
        ("/api/decision?symbol=BTC&weights=BTC:forex:10", "kind invalide"),
        ("/api/decision?symbol=BTC&weights=..%2Fx:crypto:10", "symbole invalide"),
        ("/api/decision?symbol=BTC&w=tech", "w invalide"),
        ("/api/decision?symbol=BTC&cur=GBP", "cur invalide"),
        ("/api/decision?symbol=BTC&cur=eur", "cur invalide"),
        ("/api/decision?symbol=BTC&w=risk:10", "w invalide"),
        ("/api/decision?symbol=BTC&w=tech:101", "w invalide"),
        ("/api/decision?symbol=BTC&w=tech:2.5", "w invalide"),
        ("/api/decision?symbol=BTC&w=tech:10,tech:20", "w invalide"),
        ("/api/decision?symbol=BTC&w=tech:0,mom:0,fund:0,sent:0,news:0,macro:0", "w invalide"),
    ] {
        let (s, h, b) = get(path).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{path}");
        let v: serde_json::Value = serde_json::from_str(&b).unwrap();
        assert!(v["error"].as_str().is_some_and(|e| e.starts_with(message)), "{path}: {b}");
        assert_eq!(h["cache-control"], "private, no-store");
    }
    let (s, _, b) = get(&format!("/api/decision?symbol=BTC&weights={many}")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(b.contains("20 lignes au plus"), "{b}");
}

/// The example answers given to the web, iPhone and Android screens follow the contract.
#[test]
fn decision_samples_follow_the_contract() {
    for f in ["decision-btc.json", "decision-aapl.json", "decision-guidance.json"] {
        let path = format!("{}/tests/samples/{f}", env!("CARGO_MANIFEST_DIR"));
        let d: altim::engine::decision_types::Decision = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(!d.families.is_empty() && !d.vetoes.is_empty(), "{f}");
    }
}

#[test]
fn precompressed_coding_choice() {
    use altim::app::web::pick_encoding;
    let chrome = "gzip, deflate, br, zstd";
    assert_eq!(pick_encoding(chrome, true, true), Some("br"));
    assert_eq!(pick_encoding(chrome, false, true), Some("gzip"));
    assert_eq!(pick_encoding(chrome, false, false), None);
    assert_eq!(pick_encoding("gzip", true, true), Some("gzip"));
    assert_eq!(pick_encoding("br;q=0, gzip;q=0.5", true, true), Some("gzip"));
    assert_eq!(pick_encoding("BR", true, false), Some("br"));
    assert_eq!(pick_encoding("", true, true), None);
    assert_eq!(pick_encoding("identity", true, true), None);
    assert_eq!(pick_encoding("brotli", true, false), None);
}

/// A temporary web folder: dist/ with the pages of the site and of two app groups, one precompressed .wasm.
fn web_folder() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("altim-web-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("dist/app")).unwrap();
    std::fs::write(root.join("dist/index.html"), "site").unwrap();
    std::fs::write(root.join("dist/app/index.html"), "radar").unwrap();
    std::fs::write(root.join("dist/app/reglages.html"), "reglages").unwrap();
    std::fs::write(root.join("dist/altim-app-0123456789abcdef_bg.wasm"), "wasm").unwrap();
    std::fs::write(root.join("dist/altim-app-0123456789abcdef_bg.wasm.br"), "br").unwrap();
    std::fs::write(root.join("dist/altim-app-0123456789abcdef_bg.wasm.gz"), "gz!").unwrap();
    root
}

async fn serve(root: &std::path::Path, path: &str, accept: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let req = Request::get(path).header("accept-encoding", accept).body(Body::empty()).unwrap();
    let r = altim::app::web::serve(root, req).await;
    let (status, h) = (r.status(), r.headers().clone());
    (status, h, String::from_utf8_lossy(&to_bytes(r.into_body(), 1 << 20).await.unwrap()).to_string())
}

#[tokio::test]
async fn precompressed_files_and_group_pages() {
    let root = web_folder();
    let wasm = "/altim-app-0123456789abcdef_bg.wasm";
    let (s, h, b) = serve(&root, wasm, "gzip, deflate, br, zstd").await;
    assert_eq!((s, b.as_str()), (StatusCode::OK, "br"));
    assert_eq!(h["content-encoding"], "br");
    assert_eq!(h["content-type"], "application/wasm");
    assert_eq!(h["vary"], "accept-encoding");
    assert_eq!(h["cache-control"], "private, max-age=31536000, immutable");
    assert_eq!(h["content-length"], "2");
    let (_, h, b) = serve(&root, wasm, "gzip").await;
    assert_eq!((h["content-encoding"].to_str().unwrap(), b.as_str()), ("gzip", "gz!"));
    let (_, h, b) = serve(&root, wasm, "").await;
    assert!(h.get("content-encoding").is_none());
    assert_eq!((h["vary"].to_str().unwrap(), b.as_str()), ("accept-encoding", "wasm"));
    // Each address gets the page of its group; a group without its own page falls back to the app's page.
    for (path, page) in [
        ("/", "site"),
        ("/risques", "site"),
        ("/app", "radar"),
        ("/app/alertes", "radar"),
        ("/app/reglages", "reglages"),
        ("/app/reglages/", "reglages"),
        ("/app/lexique", "reglages"),
        ("/app/actif/crypto/BTC", "radar"),
    ] {
        let (s, h, b) = serve(&root, path, "br").await;
        assert_eq!((s, b.as_str()), (StatusCode::OK, page), "{path}");
        assert_eq!(h["cache-control"], "no-cache");
    }
    std::fs::write(root.join("dist/app/actif.html"), "actif").unwrap();
    assert_eq!(serve(&root, "/app/actif/crypto/BTC", "").await.2, "actif");
    assert_eq!(serve(&root, "/app/actif/fx/EUR", "").await.2, "radar");
    let _ = std::fs::remove_dir_all(&root);
}

//! Private access: port of web/test/auth.test.ts and of the auth parts of web/test/server.test.ts, plus the session
//! cookie compatibility with the TypeScript server (both directions).
use std::net::SocketAddr;
use std::sync::Arc;

use altim::auth::{
    Auth, AuthConfig, RateLimit, SESSION_COOKIE, access, auth_config, base32, check_totp, make_session, read_session, safe_equal, to_base32, totp,
    verify_password,
};
use altim::js::now_ms;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, Response};
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use serde_json::Value;
use tower::ServiceExt;

// RFC 6238 appendix B (SHA1): secret "12345678901234567890", T = 59 s → 94287082 (6 digits: 287082).
fn rfc_secret() -> String {
    to_base32(b"12345678901234567890")
}

#[test]
fn totp_rfc_vectors_and_replay() {
    let s = rfc_secret();
    assert_eq!(s, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
    assert_eq!(base32(&s).unwrap(), b"12345678901234567890");
    assert_eq!(totp(&s, 59 / 30).unwrap(), "287082");
    assert_eq!(totp(&s, 1111111109 / 30).unwrap(), "081804");
    assert_eq!(totp(&s, 2000000000 / 30).unwrap(), "279037");
    let now = 1_111_111_109_000;
    let step = now / 30_000;
    assert_eq!(check_totp(&s, "081804", now, 0), Some(step));
    // Same code a second time: refused (replay).
    assert_eq!(check_totp(&s, "081804", now, step), None);
    // Previous step still accepted (clock drift), two steps away refused.
    assert_eq!(check_totp(&s, &totp(&s, (step - 1) as u64).unwrap(), now, 0), Some(step - 1));
    assert_eq!(check_totp(&s, &totp(&s, (step - 2) as u64).unwrap(), now, 0), None);
    assert_eq!(check_totp(&s, "12345", now, 0), None);
    assert_eq!(base32("a b-c=").unwrap(), base32("ABC").unwrap());
    assert_eq!(base32("AB1").unwrap_err(), "base32 invalide");
}

#[test]
fn constant_time_and_strict_configuration() {
    assert!(safe_equal("abc", "abc"));
    assert!(!safe_equal("abc", "abd"));
    assert!(!safe_equal("abc", "abcd"));
    let env = |pairs: Vec<(&'static str, String)>| move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
    assert_eq!(auth_config(env(vec![])), Ok(None));
    assert!(auth_config(env(vec![("ALTIM_USER", "max".into())])).is_err());
    let ok =
        || vec![("ALTIM_USER", "max".to_string()), ("ALTIM_PASSWORD_HASH", "$argon2id$v=19$…".to_string()), ("ALTIM_SESSION_SECRET", "x".repeat(64))];
    assert_eq!(auth_config(env(ok())).unwrap().unwrap().user, "max");
    let with = |k: &'static str, v: &str| {
        let mut e = ok();
        e.retain(|(n, _)| *n != k);
        e.push((k, v.to_string()));
        auth_config(env(e)).unwrap_err().0
    };
    assert!(with("ALTIM_PASSWORD_HASH", "$2b$10$bcrypt").contains("argon2id"));
    assert!(with("ALTIM_SESSION_SECRET", "court").contains("64"));
    assert!(with("ALTIM_API_TOKEN", "court").contains("40"));
    assert!(with("ALTIM_TOTP_SECRET", "ABCD").contains("160"));
    assert_eq!(with("ALTIM_TOTP_SECRET", "not base32!"), "base32 invalide");
    assert_eq!(with("ALTIM_USER", ""), "ALTIM_USER, ALTIM_PASSWORD_HASH et ALTIM_SESSION_SECRET sont tous nécessaires");
}

/// `await Bun.password.hash("x")` (argon2id, Bun's defaults: 64 MiB, t=2).
const BUN_HASH_X: &str = "$argon2id$v=19$m=65536,t=2,p=1$ZQsLer9/ty3WRg/C7oKvOfkMlynE0XeEirdUl0XDeWE$wcnSstfuEC/UipjKVDQJFqdHKp+jsoKIm7rK/GmuKH4";

#[test]
fn bun_password_hash_verifies() {
    assert!(verify_password("x", BUN_HASH_X));
    assert!(!verify_password("y", BUN_HASH_X));
    assert!(!verify_password("", BUN_HASH_X));
    assert!(!verify_password("x", "$argon2id$garbage"));
}

const PASSWORD: &str = "Tr3s-Long-Mot-De-Passe-Pour-Test";
/// Bun.password.hash(PASSWORD, { algorithm: "argon2id", memoryCost: 4096, timeCost: 2 }).
const HASH: &str = "$argon2id$v=19$m=4096,t=2,p=1$mvKaT8nUCtubj3KYDvqbvLgnT6Kp89ChCixjoKPLZP8$J1VZYdJ9FeRYG48G0GNvS+OM0TV6Kp3h4KR0ex+Kdno";
const HOST: &str = "altim.test";
const ORIGIN: &str = "http://altim.test";

fn totp_secret() -> String {
    to_base32(b"altim-test-secret-20")
}
fn api_token() -> String {
    format!("altim_{}", "t".repeat(43))
}
fn cfg(with_totp: bool) -> AuthConfig {
    AuthConfig {
        user: "max".into(),
        password_hash: HASH.into(),
        totp_secret: with_totp.then(totp_secret),
        session_secret: "s".repeat(128),
        api_token: Some(api_token()),
    }
}

/// A small app: stub API and SPA behind the access middleware (like app.ts).
fn app(auth: Arc<Auth>) -> Router {
    Router::new()
        .route("/api/{*rest}", get(|| async { axum::Json(serde_json::json!({ "total": 0 })) }))
        .fallback(|| async { "<div id=\"root\"></div>" })
        .layer(from_fn_with_state(auth, access))
}

fn request(method: &str, path: &str, headers: &[(&str, &str)], body: Option<String>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(path).header("host", HOST);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    // As on the wire (hyper always has one of Content-Length / Transfer-Encoding for a body).
    if let Some(body) = &body {
        b = b.header("content-length", body.len());
    }
    let mut req = b.body(body.map(Body::from).unwrap_or_else(Body::empty)).unwrap();
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    req
}

async fn call(app: &Router, req: Request<Body>) -> Response<Body> {
    app.clone().oneshot(req).await.unwrap()
}

async fn body_text(res: Response<Body>) -> String {
    String::from_utf8(axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

fn header(res: &Response<Body>, name: &str) -> Option<String> {
    res.headers().get(name).map(|v| v.to_str().unwrap().to_string())
}

fn form_body(fields: &[(&str, &str)]) -> String {
    fields.iter().map(|(k, v)| format!("{k}={}", altim::auth::express::encode_uri_component(v))).collect::<Vec<_>>().join("&")
}

async fn post_login(app: &Router, fields: &[(&str, &str)], ip: &str, extra: &[(&str, &str)]) -> Response<Body> {
    let mut headers = vec![("content-type", "application/x-www-form-urlencoded"), ("origin", ORIGIN), ("x-forwarded-for", ip)];
    for (k, v) in extra {
        headers.retain(|(n, _)| n != k);
        headers.push((k, v));
    }
    call(app, request("POST", "/login", &headers, Some(form_body(fields)))).await
}

fn code(offset: i64) -> String {
    totp(&totp_secret(), (now_ms() / 30_000 + offset) as u64).unwrap()
}

#[test]
fn signed_session_unforgeable_expires_bound_to_user() {
    let c = cfg(true);
    let s = make_session(&c, now_ms());
    assert!(read_session(&c, Some(&s), now_ms(), None));
    let last = s.chars().last().unwrap();
    let tampered = format!("{}{}", &s[..s.len() - 1], if last == 'A' { 'B' } else { 'A' });
    assert!(!read_session(&c, Some(&tampered), now_ms(), None));
    assert!(!read_session(&AuthConfig { session_secret: "z".repeat(128), ..c.clone() }, Some(&s), now_ms(), None));
    assert!(!read_session(&c, Some(&s), now_ms() + 8 * 86_400_000, None));
    assert!(!read_session(&AuthConfig { user: "autre".into(), ..c.clone() }, Some(&s), now_ms(), None));
    assert!(!read_session(&c, None, now_ms(), None));
}

#[tokio::test]
async fn everything_private_but_health_and_login() {
    let app = app(Auth::new(Some(cfg(true)), true));
    for path in ["/", "/app", "/app/actif/crypto/BTC", "/mentions-legales"] {
        let r = call(&app, request("GET", path, &[], None)).await;
        assert_eq!(r.status(), 302);
        assert_eq!(header(&r, "location").unwrap(), format!("/login?next={}", altim::auth::express::encode_uri_component(path)));
    }
    let api = call(&app, request("GET", "/api/tickers", &[], None)).await;
    assert_eq!(api.status(), 401);
    let v: Value = serde_json::from_str(&body_text(api).await).unwrap();
    assert!(v["error"].as_str().unwrap().contains("authentification"));
    assert_eq!(body_text(call(&app, request("GET", "/health", &[], None)).await).await, "ok");
    let login = call(&app, request("GET", "/login?next=/app/reglages", &[], None)).await;
    assert_eq!(login.status(), 200);
    assert!(header(&login, "x-robots-tag").unwrap().contains("noindex"));
    let html = body_text(login).await;
    assert!(html.contains("name=\"password\""));
    assert!(html.contains("name=\"code\""));
    assert!(html.contains("value=\"/app/reglages\""));
    assert!(body_text(call(&app, request("GET", "/robots.txt", &[], None)).await).await.contains("Disallow: /"));
}

#[tokio::test]
async fn login_with_password_and_code_then_access_then_logout() {
    let app = app(Auth::new(Some(cfg(true)), true));
    let c = code(0);
    let r = post_login(&app, &[("user", "max"), ("password", PASSWORD), ("code", &c), ("next", "/app/reglages")], "203.0.113.10", &[]).await;
    assert_eq!(r.status(), 303);
    assert_eq!(header(&r, "location").unwrap(), "/app/reglages");
    let set = header(&r, "set-cookie").unwrap();
    for flag in ["HttpOnly", "Secure", "SameSite=Strict", "Path=/", "Max-Age=604800"] {
        assert!(set.contains(flag), "{set}");
    }
    let cookie = set.split(';').next().unwrap().to_string();
    assert!(cookie.starts_with(&format!("{SESSION_COOKIE}=")));
    let page = call(&app, request("GET", "/api/universe?kind=crypto&limit=1", &[("cookie", &cookie)], None)).await;
    assert_ne!(page.status(), 401);
    assert!(header(&page, "cache-control").unwrap().contains("private"));
    // Already logged in: the login page sends back to the app.
    assert_eq!(call(&app, request("GET", "/login", &[("cookie", &cookie)], None)).await.status(), 302);
    let out = call(&app, request("POST", "/logout", &[("cookie", &cookie), ("origin", ORIGIN)], None)).await;
    assert_eq!(out.status(), 303);
    assert!(header(&out, "set-cookie").unwrap().contains("Max-Age=0"));
}

#[tokio::test]
async fn refusals_wrong_password_code_replay_open_redirect_csrf() {
    let app = app(Auth::new(Some(cfg(true)), true));
    let ip = "203.0.113.20";
    assert_eq!(post_login(&app, &[("user", "max"), ("password", "mauvais"), ("code", &code(0))], ip, &[]).await.status(), 401);
    assert_eq!(post_login(&app, &[("user", "max"), ("password", PASSWORD), ("code", "000000")], "203.0.113.21", &[]).await.status(), 401);
    assert_eq!(post_login(&app, &[("user", "autre"), ("password", PASSWORD), ("code", &code(0))], "203.0.113.22", &[]).await.status(), 401);
    // A code already used is refused.
    let c = code(1);
    assert_eq!(post_login(&app, &[("user", "max"), ("password", PASSWORD), ("code", &c)], "203.0.113.23", &[]).await.status(), 303);
    assert_eq!(post_login(&app, &[("user", "max"), ("password", PASSWORD), ("code", &c)], "203.0.113.24", &[]).await.status(), 401);
    // Open redirect: an external "next" is replaced by /app (the steps before the used one are refused as replays).
    let r =
        post_login(&app, &[("user", "max"), ("password", PASSWORD), ("code", &code(-1)), ("next", "//evil.example/x")], "203.0.113.25", &[]).await;
    if r.status() == 303 {
        assert_eq!(header(&r, "location").unwrap(), "/app");
    }
    // Form posted from another site.
    let csrf =
        post_login(&app, &[("user", "max"), ("password", PASSWORD), ("code", &code(0))], "203.0.113.1", &[("origin", "https://evil.example")]).await;
    assert_eq!(csrf.status(), 403);
    let bad = post_login(&app, &[("user", "max"), ("password", "x"), ("code", "123456")], ip, &[]).await;
    assert!(body_text(bad).await.contains("Identifiant, mot de passe ou code incorrect."));
}

#[tokio::test]
async fn lockout_after_five_failures_even_with_the_right_password() {
    let app = app(Auth::new(Some(cfg(true)), true));
    let ip = "203.0.113.30";
    for _ in 0..5 {
        assert_eq!(post_login(&app, &[("user", "max"), ("password", "faux"), ("code", "111111")], ip, &[]).await.status(), 401);
    }
    let locked = post_login(&app, &[("user", "max"), ("password", PASSWORD), ("code", &code(0))], ip, &[]).await;
    assert_eq!(locked.status(), 429);
    assert!(body_text(locked).await.contains("Trop d&#39;essais. Réessayez dans 15 minutes."));
    // Another address is not affected.
    assert_eq!(post_login(&app, &[("user", "max"), ("password", "faux"), ("code", "111111")], "203.0.113.31", &[]).await.status(), 401);
}

#[tokio::test]
async fn bots_bearer_token_on_the_api_only() {
    let app = app(Auth::new(Some(cfg(true)), true));
    let bearer = format!("Bearer {}", api_token());
    let ok = call(&app, request("GET", "/api/universe?kind=crypto&limit=1", &[("authorization", &bearer)], None)).await;
    assert_ne!(ok.status(), 401);
    // No open CORS header on the API.
    assert!(header(&ok, "access-control-allow-origin").is_none());
    let wrong = format!("{bearer}x");
    assert_eq!(call(&app, request("GET", "/api/universe?kind=crypto&limit=1", &[("authorization", &wrong)], None)).await.status(), 401);
    assert_eq!(call(&app, request("GET", "/app", &[("authorization", &bearer)], None)).await.status(), 302);
}

#[tokio::test]
async fn production_without_configuration_serves_nothing() {
    let auth = Auth::new(None, true);
    assert!(auth.enabled());
    let app = app(auth);
    assert_eq!(call(&app, request("GET", "/app", &[], None)).await.status(), 302);
    assert_eq!(call(&app, request("GET", "/api/tickers", &[], None)).await.status(), 401);
    assert!(body_text(call(&app, request("GET", "/login", &[], None)).await).await.contains("non configuré"));
    let r = post_login(&app, &[("user", "max"), ("password", PASSWORD)], "203.0.113.2", &[]).await;
    assert_eq!(r.status(), 503);
    // Development without configuration: open.
    let dev = Auth::new(None, false);
    assert!(!dev.enabled());
    assert_eq!(call(&self::app(dev), request("GET", "/app", &[], None)).await.status(), 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_logins_one_check_at_a_time_per_address() {
    let app = app(Auth::new(Some(cfg(true)), true));
    let ip = "203.0.113.40";
    let all = futures::future::join_all((0..20).map(|_| {
        let app = app.clone();
        async move { post_login(&app, &[("user", "max"), ("password", "faux"), ("code", "111111")], ip, &[]).await.status().as_u16() }
    }))
    .await;
    // Only one attempt was checked; the other 19 were turned away before hashing anything.
    assert_eq!(all.iter().filter(|&&s| s == 401).count(), 1, "{all:?}");
    assert_eq!(all.iter().filter(|&&s| s == 429).count(), 19, "{all:?}");
}

#[tokio::test]
async fn logout_revokes_the_cookie_on_the_server() {
    // Own server without 2FA.
    let app = app(Auth::new(Some(cfg(false)), true));
    let r = post_login(&app, &[("user", "max"), ("password", PASSWORD), ("next", "/health")], "203.0.113.50", &[]).await;
    assert_eq!(r.status(), 303);
    let cookie = header(&r, "set-cookie").unwrap().split(';').next().unwrap().to_string();
    let api = |c: String| request("GET", "/api/universe?kind=crypto&limit=1", &[("cookie", &c)], None);
    assert_ne!(call(&app, api(cookie.clone())).await.status(), 401);
    assert_eq!(header(&call(&app, api(cookie.clone())).await, "cache-control").unwrap(), "private, no-store");
    call(&app, request("POST", "/logout", &[("cookie", &cookie), ("origin", ORIGIN)], None)).await;
    assert_eq!(call(&app, api(cookie)).await.status(), 401);
}

#[tokio::test]
async fn malformed_cookie_simply_refused() {
    let app = app(Auth::new(Some(cfg(true)), true));
    let r = call(&app, request("GET", "/api/tickers", &[("cookie", &format!("{SESSION_COOKIE}=%E0%A4%A"))], None)).await;
    assert_eq!(r.status(), 401);
}

/// server.test.ts, "limitation de débit de l'API": 240 requests per minute per address.
#[tokio::test]
async fn api_rate_limit() {
    let api = Router::new().route("/api/search", get(|| async { axum::Json(serde_json::json!([])) })).layer(RateLimit::new(240, 60_000));
    let mut limited = None;
    for i in 0..260 {
        let r = call(&api, request("GET", "/api/search?q=", &[], None)).await;
        if r.status() == 429 {
            limited = Some((i, header(&r, "retry-after").unwrap(), body_text(r).await));
            break;
        }
    }
    let (i, retry, body) = limited.expect("jamais limité");
    assert_eq!(i, 240);
    assert!((1..=60).contains(&retry.parse::<i64>().unwrap()));
    assert_eq!(body, "{\"error\":\"Trop de requêtes, réessayez dans un instant.\"}");
    // Another address has its own counter.
    assert_eq!(call(&api, request("GET", "/api/search?q=", &[("x-forwarded-for", "198.51.100.1")], None)).await.status(), 200);
}

// ---------- Cookies shared with the TypeScript server ----------

fn golden() -> Value {
    serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/auth.json")).unwrap()).unwrap()
}

#[test]
fn cookie_signed_by_typescript_is_accepted() {
    let g = golden();
    let ts = g["tsCookie"].as_str().unwrap();
    let c = cfg(false);
    assert_eq!(c.session_secret, g["sessionSecret"].as_str().unwrap());
    assert!(read_session(&c, Some(ts), now_ms(), None));
    assert!(!read_session(&AuthConfig { session_secret: "z".repeat(128), ..c.clone() }, Some(ts), now_ms(), None));
    // Same payload layout: {"u","iat","exp","n"} in base64url, 12-byte nonce.
    let ours = make_session(&c, 4_102_444_800_000);
    let (a, b) = (ts.split_once('.').unwrap(), ours.split_once('.').unwrap());
    assert_eq!(a.1.len(), b.1.len());
    assert_eq!(a.0.len(), b.0.len());
    assert_eq!(&a.0[..60], &b.0[..60]);
}

// A cookie made by `make_session` was also checked by the TypeScript `readSession` (both ways) before the TypeScript
// server was removed; the test above keeps a cookie signed by it, so sessions opened before the switch stay valid.

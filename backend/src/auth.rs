//! Private access (port of `web/server/auth.ts` and of the access block of `web/server/app.ts`): the whole site,
//! the web app and the API are reserved to the owner.
//!
//! Secrets live only in the environment (Heroku config vars), never in the code:
//! - `ALTIM_USER`            login name
//! - `ALTIM_PASSWORD_HASH`   argon2id hash of the password (PHC string of `Bun.password.hash`)
//! - `ALTIM_TOTP_SECRET`     base32 secret of the 6-digit code from an authenticator app (optional second factor)
//! - `ALTIM_SESSION_SECRET`  key that signs the session cookie (changing it logs every device out)
//! - `ALTIM_API_TOKEN`       token for trading bots: `Authorization: Bearer <token>` on /api/*
//!
//! Same protections as the TypeScript: argon2id, TOTP (RFC 6238) with anti-replay, HMAC-SHA256 signed cookie
//! (`altim_session`, HttpOnly, Secure in production, SameSite=Strict, 7 days, revoked at logout; the format is
//! unchanged so the cookies issued by the TypeScript server stay valid), constant-time comparisons, lock-out after
//! 5 failures per IP (IPv6: per /64) plus a global cap, one attempt at a time per IP, at most 2 password checks at
//! once, Origin check on forms (CSRF), no indexing, fails closed in production without configuration.
//!
//! # Mounting in Axum
//!
//! ```ignore
//! let auth = altim::auth::Auth::from_env()?;          // Arc<Auth>; Err on a weak configuration
//! let app = Router::new()
//!     .nest("/api", api.layer(altim::auth::RateLimit::new(240, 60_000)))
//!     .fallback_service(static_files_and_spa)
//!     // /health, /robots.txt, GET|POST /login, POST /logout, then the guard: the whole access block of app.ts.
//!     .layer(axum::middleware::from_fn_with_state(auth.clone(), altim::auth::access))
//!     /* helmet headers, compression, HTTPS redirect: outer layers */;
//! axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;
//! ```
//!
//! `access` answers the routes app.ts registered before `app.use(access.guard)` itself, with Express matching
//! (case-insensitive, optional trailing slash, any other method falls through to the guard), so nothing else changes.
//! The pieces are also available separately: [`router`] (the /login and /logout routes), [`guard`] (the middleware
//! alone) and [`RateLimit`] (the per-IP limiter of app.ts, a tower layer).
pub mod express;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use argon2::{Argon2, PasswordVerifier};
use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine;
use futures::future::BoxFuture;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha1::Sha1;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::js::now_ms;
use express::{Field, Fields, Res, client_ip, encode_uri_component, escape_html, js_len, node_header, route_is};

/// Access configuration (`AuthConfig`). `Debug` shows the user only (never the secrets).
#[derive(Clone, PartialEq, Eq)]
pub struct AuthConfig {
    pub user: String,
    pub password_hash: String,
    pub totp_secret: Option<String>,
    pub session_secret: String,
    pub api_token: Option<String>,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field("user", &self.user)
            .field("totp", &self.totp_secret.is_some())
            .field("api_token", &self.api_token.is_some())
            .finish_non_exhaustive()
    }
}

/// A weak or incomplete configuration (the TypeScript threw at start-up with this message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ConfigError {}

pub const SESSION_COOKIE: &str = "altim_session";
const SESSION_DAYS: i64 = 7;
const MAX_FAILURES: u32 = 5;
const LOCK_MS: i64 = 15 * 60_000;
/// Failures from all addresses together within LOCK_MS before every new login is paused (distributed guessing).
const GLOBAL_FAILURES: usize = 30;
/// Password checks running at the same time (argon2id: 64 MiB each).
const MAX_VERIFYING: usize = 2;

/// Configuration from the environment (`authConfig`); `Ok(None)` when access control is not configured.
/// `env` reads a variable; an empty value counts as absent, like in JavaScript.
pub fn auth_config(env: impl Fn(&str) -> Option<String>) -> Result<Option<AuthConfig>, ConfigError> {
    let get = |k: &str| env(k).filter(|v| !v.is_empty());
    let (user, hash, totp, session, api) =
        (get("ALTIM_USER"), get("ALTIM_PASSWORD_HASH"), get("ALTIM_TOTP_SECRET"), get("ALTIM_SESSION_SECRET"), get("ALTIM_API_TOKEN"));
    if user.is_none() && hash.is_none() && session.is_none() {
        return Ok(None);
    }
    let (Some(user), Some(hash), Some(session)) = (user, hash, session) else {
        return Err(ConfigError("ALTIM_USER, ALTIM_PASSWORD_HASH et ALTIM_SESSION_SECRET sont tous nécessaires".into()));
    };
    if !hash.starts_with("$argon2id$") {
        return Err(ConfigError("ALTIM_PASSWORD_HASH doit être un hachage argon2id (bun run secrets)".into()));
    }
    if js_len(&session) < 64 {
        return Err(ConfigError("ALTIM_SESSION_SECRET trop court (64 caractères minimum)".into()));
    }
    if api.as_deref().is_some_and(|t| js_len(t) < 40) {
        return Err(ConfigError("ALTIM_API_TOKEN trop court (40 caractères minimum)".into()));
    }
    if let Some(t) = &totp {
        if base32(t).map_err(ConfigError)?.len() < 20 {
            return Err(ConfigError("ALTIM_TOTP_SECRET trop court (160 bits minimum)".into()));
        }
    }
    Ok(Some(AuthConfig { user, password_hash: hash, totp_secret: totp, session_secret: session, api_token: api }))
}

/// `authConfig()` on the process environment.
pub fn auth_config_from_env() -> Result<Option<AuthConfig>, ConfigError> {
    auth_config(|k| std::env::var(k).ok())
}

fn hmac256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC: toute longueur de clé");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// Equality in constant time (length difference included) against timing attacks (`safeEqual`).
pub fn safe_equal(a: &str, b: &str) -> bool {
    let ha = hmac256(b"altim-compare", a.as_bytes());
    let hb = hmac256(b"altim-compare", b.as_bytes());
    bool::from(ha.ct_eq(&hb)) && js_len(a) == js_len(b)
}

// ---------- TOTP (RFC 6238: HMAC-SHA1, 30 s, 6 digits) ----------

const B32: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Base32 decoding (spaces, `=` and `-` ignored, case-insensitive). Err("base32 invalide").
pub fn base32(s: &str) -> Result<Vec<u8>, String> {
    let mut bits = String::new();
    for ch in s.to_uppercase().chars().filter(|&c| !(express::is_js_space(c) || c == '=' || c == '-')) {
        let v = B32.find(ch).filter(|_| ch.is_ascii()).ok_or_else(|| "base32 invalide".to_string())?;
        bits.push_str(&format!("{v:05b}"));
    }
    Ok((0..bits.len() / 8).map(|i| u8::from_str_radix(&bits[i * 8..i * 8 + 8], 2).unwrap()).collect())
}

pub fn to_base32(buf: &[u8]) -> String {
    let bits: String = buf.iter().map(|b| format!("{b:08b}")).collect();
    let mut out = String::new();
    let mut i = 0;
    while i < bits.len() {
        let chunk = format!("{:0<5}", &bits[i..(i + 5).min(bits.len())]);
        out.push(B32.as_bytes()[usize::from_str_radix(&chunk, 2).unwrap()] as char);
        i += 5;
    }
    out
}

/// Code of the 30 s step `counter`.
pub fn totp(secret: &str, counter: u64) -> Result<String, String> {
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(&base32(secret)?).expect("HMAC: toute longueur de clé");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let o = (h[h.len() - 1] & 0xf) as usize;
    let n = u32::from_be_bytes([h[o], h[o + 1], h[o + 2], h[o + 3]]) & 0x7fff_ffff;
    Ok(format!("{:06}", n % 1_000_000))
}

/// Accepts the current 30 s step and the neighbouring ones (clock drift); a step already used is refused (replay).
pub fn check_totp(secret: &str, code: &str, now: i64, last_used: i64) -> Option<i64> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let step = now.div_euclid(30_000);
    [step, step - 1, step + 1].into_iter().find(|&c| c > last_used && c >= 0 && totp(secret, c as u64).is_ok_and(|t| safe_equal(&t, code)))
}

// ---------- Signed session cookie ----------

fn b64url(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// `Buffer.from(s, "base64url")`: lenient like Node (both alphabets, other characters skipped, stops at `=`).
fn b64url_decode(s: &str) -> Vec<u8> {
    let mut sextets = Vec::with_capacity(s.len());
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        sextets.push(v as u32);
    }
    let mut out = Vec::with_capacity(sextets.len() * 3 / 4);
    for chunk in sextets.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &v)| acc | v << (18 - 6 * i));
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len().saturating_sub(1)]);
    }
    out
}

fn sign(secret: &str, data: &str) -> String {
    b64url(&hmac256(secret.as_bytes(), data.as_bytes()))
}

/// New session cookie value (`makeSession`): base64url(JSON {u, iat, exp, n}) + "." + base64url(HMAC-SHA256).
pub fn make_session(cfg: &AuthConfig, now: i64) -> String {
    let nonce: [u8; 12] = rand::random();
    let payload = json!({ "u": cfg.user, "iat": now, "exp": now + SESSION_DAYS * 86_400_000, "n": b64url(&nonce) });
    let payload = b64url(payload.to_string().as_bytes());
    let mac = sign(&cfg.session_secret, &payload);
    format!("{payload}.{mac}")
}

/// Nonce and expiry of a valid session (for revocation).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionPayload {
    pub n: String,
    pub exp: f64,
}

/// Valid session → its payload; otherwise None (`sessionPayload`).
pub fn session_payload(cfg: &AuthConfig, cookie: Option<&str>, now: i64) -> Option<SessionPayload> {
    let cookie = cookie.filter(|c| !c.is_empty() && js_len(c) <= 512)?;
    let mut parts = cookie.split('.');
    let (payload, mac) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if payload.is_empty() || mac.is_empty() || !safe_equal(&sign(&cfg.session_secret, payload), mac) {
        return None;
    }
    let p: Value = serde_json::from_str(&String::from_utf8_lossy(&b64url_decode(payload))).ok()?;
    let exp = p.get("exp").and_then(Value::as_f64)?;
    let n = p.get("n").and_then(Value::as_str)?;
    (p.get("u").and_then(Value::as_str) == Some(cfg.user.as_str()) && exp > now as f64).then(|| SessionPayload { n: n.to_string(), exp })
}

/// Valid and not revoked (`readSession`).
pub fn read_session(cfg: &AuthConfig, cookie: Option<&str>, now: i64, revoked: Option<&HashMap<String, f64>>) -> bool {
    session_payload(cfg, cookie, now).is_some_and(|p| !revoked.is_some_and(|r| r.contains_key(&p.n)))
}

/// Cookies of the request (`cookies(req)`): pairs with exactly one `=`, values URI-decoded when possible.
pub fn cookies(headers: &HeaderMap) -> HashMap<String, String> {
    let raw = node_header(headers, "cookie").unwrap_or_default();
    let mut out = HashMap::new();
    for c in raw.split(';') {
        let kv: Vec<&str> = express::js_trim(c).split('=').collect();
        if let [k, v] = kv[..] {
            // Malformed escape (%E0…): kept raw, it will simply not match.
            out.insert(k.to_string(), express::decode_uri_component(v).unwrap_or_else(|| v.to_string()));
        }
    }
    out
}

// ---------- Login page ----------

/// What the login page shows (`loginPage` options).
#[derive(Debug, Clone, Default)]
pub struct LoginPage<'a> {
    pub error: Option<&'a str>,
    pub next: Option<&'a str>,
    pub totp: bool,
    pub configured: bool,
}

pub fn login_page(opts: &LoginPage) -> String {
    let body = if !opts.configured {
        "<p>Accès privé non configuré. Ajoutez les variables ALTIM_* dans Heroku → Settings → Config Vars (voir DEPLOIEMENT.md).</p>".to_string()
    } else {
        let totp = if opts.totp {
            "<label>Code à 6 chiffres (application d'authentification)<input name=\"code\" inputmode=\"numeric\" pattern=\"[0-9]{6}\" maxlength=\"6\" autocomplete=\"one-time-code\" required></label>"
        } else {
            ""
        };
        let error = opts.error.filter(|e| !e.is_empty()).map(|e| format!("<p class=\"err\" role=\"alert\">{}</p>", escape_html(e))).unwrap_or_default();
        format!(
            "<form method=\"post\" action=\"/login\" autocomplete=\"on\">
      <input type=\"hidden\" name=\"next\" value=\"{}\">
      <label>Identifiant<input name=\"user\" autocomplete=\"username\" required autocapitalize=\"none\" spellcheck=\"false\"></label>
      <label>Mot de passe<input name=\"password\" type=\"password\" autocomplete=\"current-password\" required></label>
      {totp}
      {error}
      <button type=\"submit\">Se connecter</button>
    </form>",
            escape_html(opts.next.unwrap_or("/app"))
        )
    };
    format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<meta name=\"robots\" content=\"noindex, nofollow\"><title>Altim — Accès privé</title>
<style>
:root{{color-scheme:dark}}*{{box-sizing:border-box}}body{{margin:0;min-height:100svh;display:grid;place-items:center;padding:16px;background:#05070f;color:#e8ecff;font:16px/1.5 system-ui,-apple-system,sans-serif}}
main{{width:100%;max-width:380px;padding:28px 22px;border:1px solid rgba(0,240,255,.25);border-radius:20px;background:#0b0f1f;box-shadow:0 0 40px rgba(0,240,255,.08)}}
h1{{margin:0 0 4px;font-size:22px;letter-spacing:.2em;color:#00f0ff}}p{{color:#9aa3c7;margin:0 0 18px;font-size:14px}}
form{{display:grid;gap:14px}}label{{display:grid;gap:6px;font-size:13px;color:#9aa3c7}}
input{{width:100%;min-height:48px;padding:10px 12px;border-radius:12px;border:1px solid #232a45;background:#060913;color:#e8ecff;font-size:16px}}
input:focus{{outline:2px solid #00f0ff;outline-offset:1px}}button{{min-height:48px;border:0;border-radius:12px;background:#00f0ff;color:#05070f;font-weight:700;font-size:16px;cursor:pointer}}
.err{{color:#ff3b5c;margin:0}}
</style></head><body><main><h1>ALTIM</h1><p>Accès privé.</p>{body}</main></body></html>"
    )
}

// ---------- Per-IP rate limit (`rateLimit` of app.ts) ----------

struct Hit {
    count: u32,
    reset: i64,
}

struct RateInner {
    max: u32,
    window_ms: i64,
    hits: Mutex<HashMap<String, Hit>>,
}

/// `rateLimit(max, windowMs)` of app.ts: at most `max` requests per `req.ip` per window, then 429 JSON
/// `{"error":"Trop de requêtes, réessayez dans un instant."}` with `Retry-After`. A tower layer
/// (`router.layer(RateLimit::new(240, 60_000))`); each instance has its own counters, like each `rateLimit()` call.
#[derive(Clone)]
pub struct RateLimit(Arc<RateInner>);

impl RateLimit {
    pub fn new(max: u32, window_ms: i64) -> Self {
        RateLimit(Arc::new(RateInner { max, window_ms, hits: Mutex::new(HashMap::new()) }))
    }

    /// Counts one request of `key` (`req.ip ?? "?"`): None when it may go on, the 429 response otherwise.
    pub fn check(&self, key: &str, method: &Method, headers: &HeaderMap) -> Option<Response> {
        let now = now_ms();
        let mut hits = self.0.hits.lock().unwrap();
        match hits.get_mut(key) {
            Some(h) if h.reset >= now => {
                h.count += 1;
                if h.count > self.0.max {
                    let retry = ((h.reset - now) as f64 / 1000.0).ceil() as i64;
                    return Some(Res::new().status(429).set("retry-after", &retry.to_string()).json(
                        method,
                        headers,
                        &json!({ "error": "Trop de requêtes, réessayez dans un instant." }),
                    ));
                }
            }
            _ => {
                hits.insert(key.to_string(), Hit { count: 1, reset: now + self.0.window_ms });
            }
        }
        if hits.len() > 10_000 {
            hits.retain(|_, v| v.reset >= now);
        }
        None
    }
}

impl<S> tower::Layer<S> for RateLimit {
    type Service = RateLimitService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService { inner, limit: self.clone() }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    limit: RateLimit,
}

impl<S> tower::Service<Request> for RateLimitService<S>
where
    S: tower::Service<Request, Response = Response> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Response, S::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let key = client_ip(&req).unwrap_or_else(|| "?".into());
        if let Some(res) = self.limit.check(&key, req.method(), req.headers()) {
            return Box::pin(async move { Ok(res) });
        }
        Box::pin(self.inner.call(req))
    }
}

// ---------- Access control ----------

/// Paths reachable without a session: health check (Heroku) and the login page itself.
const PUBLIC_PATHS: [&str; 3] = ["/health", "/login", "/robots.txt"];

#[derive(Clone, Copy)]
struct Failure {
    n: u32,
    first: i64,
    until: i64,
    busy: bool,
}

#[derive(Default)]
struct AuthState {
    failures: HashMap<String, Failure>,
    /// Times of the recent failures, all addresses together.
    global_failures: Vec<i64>,
    global_until: i64,
    verifying: usize,
    /// Sessions closed by "Se déconnecter" (nonce → expiry), until they would have expired anyway.
    revoked: HashMap<String, f64>,
    last_totp: i64,
}

impl AuthState {
    fn locked(&self, key: &str, now: i64) -> bool {
        self.failures.get(key).map_or(0, |f| f.until) > now || self.global_until > now
    }

    fn fail(&mut self, key: &str, now: i64) {
        let mut f = self.failures.get(key).copied().unwrap_or(Failure { n: 0, first: now, until: 0, busy: false });
        // Failures are counted over a 15-minute window, not forever.
        if now - f.first > LOCK_MS {
            f.n = 0;
            f.first = now;
        }
        f.n += 1;
        if f.n >= MAX_FAILURES {
            f.until = now + LOCK_MS;
            f.n = 0;
            f.first = now;
        }
        self.failures.insert(key.to_string(), f);
        self.global_failures.retain(|&t| now - t < LOCK_MS);
        self.global_failures.push(now);
        if self.global_failures.len() >= GLOBAL_FAILURES {
            self.global_until = now + LOCK_MS;
            self.global_failures.clear();
        }
    }

    /// Forgets expired entries (called on each attempt, bounded work).
    fn sweep(&mut self, now: i64) {
        if self.failures.len() > 1_000 {
            self.failures.retain(|_, v| v.busy || v.until.max(v.first + LOCK_MS) >= now);
        }
        if self.revoked.len() > 1_000 {
            self.revoked.retain(|_, &mut exp| exp >= now as f64);
        }
    }
}

/// State of the private access (`createAuth`): configuration, lock-outs, revoked sessions, TOTP anti-replay.
pub struct Auth {
    cfg: Option<AuthConfig>,
    production: bool,
    state: Mutex<AuthState>,
    /// `rateLimit(10, 60_000)` in front of POST /login.
    login_limit: RateLimit,
}

impl Auth {
    /// `createAuth(cfg, production)`.
    pub fn new(cfg: Option<AuthConfig>, production: bool) -> Arc<Auth> {
        Arc::new(Auth { cfg, production, state: Mutex::new(AuthState::default()), login_limit: RateLimit::new(10, 60_000) })
    }

    /// Configuration from the environment, production when `NODE_ENV=production` or on a Heroku dyno (`DYNO`).
    pub fn from_env() -> Result<Arc<Auth>, ConfigError> {
        let production = std::env::var("NODE_ENV").is_ok_and(|v| v == "production") || std::env::var("DYNO").is_ok_and(|v| !v.is_empty());
        Ok(Auth::new(auth_config_from_env()?, production))
    }

    pub fn config(&self) -> Option<&AuthConfig> {
        self.cfg.as_ref()
    }

    /// Access control active (`enabled`).
    pub fn enabled(&self) -> bool {
        self.cfg.is_some() || self.production
    }

    fn cookie_header(&self, value: &str, max_age: i64) -> String {
        format!("{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{}", if self.production { "; Secure" } else { "" })
    }

    /// Valid session cookie, or the bot token on /api/* (`isAuthenticated`).
    pub fn is_authenticated(&self, headers: &HeaderMap, path: &str) -> bool {
        let Some(cfg) = &self.cfg else { return !self.production };
        let jar = cookies(headers);
        {
            let st = self.state.lock().unwrap();
            if read_session(cfg, jar.get(SESSION_COOKIE).map(String::as_str), now_ms(), Some(&st.revoked)) {
                return true;
            }
        }
        // /^Bearer (.+)$/
        let bearer = node_header(headers, "authorization").and_then(|a| a.strip_prefix("Bearer ").map(str::to_string)).filter(|b| !b.is_empty());
        match (&cfg.api_token, bearer) {
            (Some(token), Some(b)) => path.starts_with("/api/") && safe_equal(&b, token),
            _ => false,
        }
    }
}

/// IPv4 as is; IPv6 by /64 (one subscriber owns a whole /64 and could rotate addresses in it).
pub fn client_key(ip: &str) -> String {
    let v6 = ip.strip_prefix("::ffff:").unwrap_or(ip);
    if !v6.contains(':') {
        return v6.to_string();
    }
    let head = v6.split("::").next().unwrap_or("");
    let parts: Vec<&str> = head.split(':').take(4).collect();
    format!("{}::/64", parts.join(":"))
}

/// Host the form must come from: the Host header (as Express), or the authority of the URI (HTTP/2).
fn request_host(req: &Request) -> Option<String> {
    node_header(req.headers(), "host").or_else(|| req.uri().authority().map(|a| a.as_str().to_string()))
}

/// Same-origin form only (CSRF): the Origin (or Referer) must be this host.
fn same_origin(headers: &HeaderMap, host: Option<&str>) -> bool {
    let Some(origin) = node_header(headers, "origin").or_else(|| node_header(headers, "referer")).filter(|o| !o.is_empty()) else {
        return false;
    };
    let Ok(url) = reqwest::Url::parse(&origin) else { return false };
    let url_host = match url.port() {
        Some(p) => format!("{}:{p}", url.host_str().unwrap_or("")),
        None => url.host_str().unwrap_or("").to_string(),
    };
    host == Some(url_host.as_str())
}

/// `safeNext`: a local path only (no `//host`, no loop back to /login), otherwise /app.
pub fn safe_next(v: Option<&Field>) -> String {
    let ok = |s: &str| {
        let b = s.as_bytes();
        b.first() == Some(&b'/')
            && b.get(1) != Some(&b'/')
            && b[1..].iter().all(|&c| c.is_ascii_alphanumeric() || b"_-./?=&%".contains(&c))
            && !s.starts_with("/login")
    };
    match v.and_then(Field::as_str) {
        Some(s) if ok(s) => s.to_string(),
        _ => "/app".into(),
    }
}

fn is_get(m: &Method) -> bool {
    *m == Method::GET || *m == Method::HEAD
}

/// Bun `String(password).slice(0, 256)`: 256 UTF-16 units; half a surrogate pair becomes U+FFFD in UTF-8.
fn slice_utf16(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut units = 0;
    for c in s.chars() {
        let n = c.len_utf16();
        if units + n > max {
            if units < max {
                out.push('\u{fffd}');
            }
            break;
        }
        out.push(c);
        units += n;
    }
    out
}

/// `Bun.password.verify` (argon2 PHC string; errors and an empty password give false).
pub fn verify_password(password: &str, hash: &str) -> bool {
    !password.is_empty() && <Argon2 as PasswordVerifier<str>>::verify_password(&Argon2::default(), password.as_bytes(), hash).is_ok()
}

fn text(method: &Method, headers: &HeaderMap, status: u16, body: &str) -> Response {
    Res::new().status(status).send(method, headers, "text/plain", body)
}

/// GET /login (`loginForm`).
fn login_form(auth: &Auth, req: &Request) -> Response {
    let (method, headers) = (req.method(), req.headers());
    let query = express::parse_query(req.uri().query());
    let next = safe_next(query.get("next"));
    let (path, _) = express::path_and_original(req);
    if auth.cfg.is_some() && auth.is_authenticated(headers, &path) {
        return Res::new().redirect(method, headers, 302, &next);
    }
    let page = login_page(&LoginPage { error: None, next: Some(&next), totp: auth.cfg.as_ref().is_some_and(|c| c.totp_secret.is_some()), configured: auth.cfg.is_some() });
    Res::new().set("cache-control", "no-store").set("x-robots-tag", "noindex, nofollow").send(method, headers, "text/html", page)
}

/// POST /login (`login`), after the rate limit.
async fn login(auth: Arc<Auth>, req: Request) -> Response {
    let method = req.method().clone();
    let headers = req.headers().clone();
    let host = request_host(&req);
    let ip = client_key(&client_ip(&req).unwrap_or_else(|| "?".into()));
    let body = match express::urlencoded(req, 2 * 1024).await {
        Ok(b) => b,
        Err(e) => return express::request_error(&method, &headers, e.0),
    };
    // Runs to the end even when the client goes away (as in Node): failures are counted and "busy" is released.
    let task = tokio::spawn(login_attempt(auth, method, headers, host, ip, body.unwrap_or_default()));
    task.await.unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn login_attempt(auth: Arc<Auth>, method: Method, headers: HeaderMap, host: Option<String>, ip: String, body: Fields) -> Response {
    let now = now_ms();
    let next = safe_next(body.get("next"));
    let field = |k: &str| body.get(k).map(Field::js_string).unwrap_or_default();
    let totp_on = auth.cfg.as_ref().is_some_and(|c| c.totp_secret.is_some());
    let page = |status: u16, error: &str| {
        Res::new().set("x-robots-tag", "noindex, nofollow").set("cache-control", "no-store").status(status).send(
            &method,
            &headers,
            "text/html",
            login_page(&LoginPage { error: Some(error), next: Some(&next), totp: totp_on, configured: auth.cfg.is_some() }),
        )
    };
    let (cfg, user_ok) = {
        let mut st = auth.state.lock().unwrap();
        st.sweep(now);
        let Some(cfg) = auth.cfg.clone() else { return page(503, "Accès privé non configuré.") };
        if !same_origin(&headers, host.as_deref()) {
            return page(403, "Requête refusée.");
        }
        if st.locked(&ip, now) {
            return page(429, "Trop d'essais. Réessayez dans 15 minutes.");
        }
        // One attempt at a time per address, reserved before any await: parallel requests cannot all slip past the
        // lock-out check before the first failure is counted.
        let mut entry = st.failures.get(&ip).copied().unwrap_or(Failure { n: 0, first: now, until: 0, busy: false });
        if entry.busy {
            return page(429, "Une connexion est déjà en cours. Réessayez dans un instant.");
        }
        if st.verifying >= MAX_VERIFYING {
            return page(503, "Serveur occupé. Réessayez dans un instant.");
        }
        entry.busy = true;
        st.failures.insert(ip.clone(), entry);
        // Every check runs whatever the result of the previous ones (no hint on which one failed).
        let user_ok = safe_equal(&field("user"), &cfg.user);
        st.verifying += 1;
        (cfg, user_ok)
    };
    let password = slice_utf16(&field("password"), 256);
    let hash = cfg.password_hash.clone();
    let pass_ok = tokio::task::spawn_blocking(move || verify_password(&password, &hash)).await.unwrap_or(false);
    let failed = {
        let mut st = auth.state.lock().unwrap();
        st.verifying -= 1;
        let step = match &cfg.totp_secret {
            Some(secret) => check_totp(secret, &field("code"), now_ms(), st.last_totp),
            None => Some(0),
        };
        match step {
            Some(step) if user_ok && pass_ok => {
                if step != 0 {
                    st.last_totp = step;
                }
                st.failures.remove(&ip);
                false
            }
            _ => {
                st.fail(&ip, now_ms());
                true
            }
        }
    };
    if failed {
        // Slows guessing down; the address stays "busy" meanwhile, but the password-check slot is already free.
        tokio::time::sleep(Duration::from_secs_f64((400.0 + rand::random::<f64>() * 400.0) / 1000.0)).await;
        if let Some(f) = auth.state.lock().unwrap().failures.get_mut(&ip) {
            f.busy = false;
        }
        return page(401, "Identifiant, mot de passe ou code incorrect.");
    }
    let cookie = auth.cookie_header(&make_session(&cfg, now_ms()), SESSION_DAYS * 86_400);
    let mut res = Res::new().set("x-robots-tag", "noindex, nofollow").set("cache-control", "no-store");
    res.headers.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).expect("cookie ASCII"));
    res.redirect(&method, &headers, 303, &next)
}

/// POST /logout (`logout`).
fn logout(auth: &Auth, req: &Request) -> Response {
    let (method, headers) = (req.method(), req.headers());
    if !same_origin(headers, request_host(req).as_deref()) {
        return text(method, headers, 403, "Requête refusée.");
    }
    // The cookie is revoked on the server too: a copy of it (stolen, or on another device) stops working.
    if let Some(cfg) = &auth.cfg {
        if let Some(p) = session_payload(cfg, cookies(headers).get(SESSION_COOKIE).map(String::as_str), now_ms()) {
            auth.state.lock().unwrap().revoked.insert(p.n, p.exp);
        }
    }
    let mut res = Res::new();
    res.headers.insert(header::SET_COOKIE, HeaderValue::from_str(&auth.cookie_header("", 0)).unwrap());
    res.redirect(method, headers, 303, "/login")
}

fn set_if_absent(res: &mut Response, name: header::HeaderName, value: &'static str) {
    if !res.headers().contains_key(&name) {
        res.headers_mut().insert(name, HeaderValue::from_static(value));
    }
}

/// The guard alone (`access.guard`): public paths pass, a session (or the bot token on /api/*) passes with
/// `Cache-Control: private, no-store`, otherwise 401 JSON on /api/*, 401 text for other methods, or a 302 to
/// `/login?next=<original URL>`. Every response carries `X-Robots-Tag: noindex, nofollow`.
pub async fn guard(State(auth): State<Arc<Auth>>, req: Request, next: Next) -> Response {
    let (path, original) = express::path_and_original(&req);
    if PUBLIC_PATHS.contains(&path.as_str()) {
        let mut res = next.run(req).await;
        set_if_absent(&mut res, header::HeaderName::from_static("x-robots-tag"), "noindex, nofollow");
        return res;
    }
    if auth.is_authenticated(req.headers(), &path) {
        let mut res = next.run(req).await;
        set_if_absent(&mut res, header::HeaderName::from_static("x-robots-tag"), "noindex, nofollow");
        set_if_absent(&mut res, header::CACHE_CONTROL, "private, no-store");
        return res;
    }
    let (method, headers) = (req.method(), req.headers());
    let res = Res::new().set("x-robots-tag", "noindex, nofollow");
    if path.starts_with("/api/") {
        return res.status(401).json(method, headers, &json!({ "error": "authentification requise" }));
    }
    if !is_get(method) {
        return res.status(401).send(method, headers, "text/plain", "authentification requise");
    }
    res.redirect(method, headers, 302, &format!("/login?next={}", encode_uri_component(&original)))
}

/// The whole access block of app.ts as one middleware, to layer on the full app: GET /health ("ok"),
/// GET /robots.txt, GET /login, POST /login (10 per minute per IP, form of 2 KB at most), POST /logout, then
/// [`guard`] for everything else.
pub async fn access(State(auth): State<Arc<Auth>>, req: Request, next: Next) -> Response {
    let (path, _) = express::path_and_original(&req);
    let method = req.method().clone();
    if is_get(&method) && route_is(&path, "/health") {
        return text(&method, req.headers(), 200, "ok");
    }
    if is_get(&method) && route_is(&path, "/robots.txt") {
        return text(&method, req.headers(), 200, "User-agent: *\nDisallow: /\n");
    }
    if route_is(&path, "/login") {
        if is_get(&method) {
            return login_form(&auth, &req);
        }
        if method == Method::POST {
            let key = client_ip(&req).unwrap_or_else(|| "?".into());
            if let Some(res) = auth.login_limit.check(&key, &method, req.headers()) {
                return res;
            }
            return login(auth, req).await;
        }
    }
    if method == Method::POST && route_is(&path, "/logout") {
        return logout(&auth, &req);
    }
    guard(State(auth), req, next).await
}

/// GET/POST /login and POST /logout as plain Axum routes (state applied). Prefer [`access`]: with routes, a
/// wrong method gets Axum's 405 instead of falling through to the guard, and /login/ or /LOGIN do not match.
pub fn router(auth: Arc<Auth>) -> Router {
    async fn form(State(auth): State<Arc<Auth>>, req: Request) -> Response {
        login_form(&auth, &req)
    }
    async fn submit(State(auth): State<Arc<Auth>>, req: Request) -> Response {
        let key = client_ip(&req).unwrap_or_else(|| "?".into());
        if let Some(res) = auth.login_limit.check(&key, req.method(), req.headers()) {
            return res;
        }
        login(auth, req).await
    }
    async fn out(State(auth): State<Arc<Auth>>, req: Request) -> Response {
        logout(&auth, &req)
    }
    Router::new().route("/login", get(form).post(submit)).route("/logout", post(out)).with_state(auth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_lenient() {
        assert_eq!(b64url_decode(&b64url(b"hello world!?")), b"hello world!?");
        assert_eq!(b64url_decode("aGk"), b"hi");
        assert_eq!(b64url_decode("aG k="), b"hi");
    }

    #[test]
    fn keys_and_next() {
        assert_eq!(client_key("::ffff:203.0.113.1"), "203.0.113.1");
        assert_eq!(client_key("2001:db8:1:2:3:4:5:6"), "2001:db8:1:2::/64");
        assert_eq!(client_key("2001:db8::1"), "2001:db8::/64");
        let one = |s: &str| Field::One(s.into());
        assert_eq!(safe_next(Some(&one("/app/reglages"))), "/app/reglages");
        assert_eq!(safe_next(Some(&one("//evil.example/x"))), "/app");
        assert_eq!(safe_next(Some(&one("/login?next=/x"))), "/app");
        assert_eq!(safe_next(Some(&one("/a b"))), "/app");
        assert_eq!(safe_next(Some(&Field::Many(vec!["/a".into(), "/b".into()]))), "/app");
        assert_eq!(slice_utf16(&"😀".repeat(200), 5), "😀😀\u{fffd}");
    }
}

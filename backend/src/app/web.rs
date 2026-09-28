//! What every response gets (security headers like helmet 8 with the same CSP, Permissions-Policy, HTTPS
//! redirect behind the Heroku router) and the static site: built files, public files, then the SPA page.
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use regex::Regex;

pub const CSP: &str = "default-src 'self';script-src 'self';style-src 'self' 'unsafe-inline' https://fonts.googleapis.com;font-src https://fonts.gstatic.com;img-src 'self' data:;connect-src 'self' https://api.binance.com https://api.coingecko.com;frame-ancestors 'none';base-uri 'self';form-action 'self';object-src 'none'";

/// Headers of helmet 8 (defaults, custom CSP, HSTS 2 years with subdomains, no COEP) + Permissions-Policy.
const SECURITY: [(&str, &str); 13] = [
    ("content-security-policy", CSP),
    ("cross-origin-opener-policy", "same-origin"),
    ("cross-origin-resource-policy", "same-origin"),
    ("referrer-policy", "strict-origin-when-cross-origin"),
    ("strict-transport-security", "max-age=63072000; includeSubDomains"),
    ("x-content-type-options", "nosniff"),
    ("x-dns-prefetch-control", "off"),
    ("x-frame-options", "SAMEORIGIN"),
    ("x-xss-protection", "0"),
    ("origin-agent-cluster", "?1"),
    ("x-download-options", "noopen"),
    ("x-permitted-cross-domain-policies", "none"),
    ("permissions-policy", "camera=(), microphone=(), geolocation=()"),
];

pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    for (k, v) in SECURITY {
        h.insert(k, HeaderValue::from_static(v));
    }
    res
}

/// HTTPS behind the Heroku router: a plain-HTTP request is sent to the same address in HTTPS.
pub async fn https_redirect(req: Request, next: Next) -> Response {
    if req.headers().get("x-forwarded-proto").is_some_and(|v| v == "http") {
        let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
        let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/");
        return Redirect::permanent(&format!("https://{host}{path}")).into_response();
    }
    next.run(req).await
}

/// Folder holding `dist/` (built app) and `public/`: $ALTIM_WEB_ROOT, else ../web next to the crate.
pub fn web_root() -> PathBuf {
    std::env::var("ALTIM_WEB_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../web"))
}

static HASHED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-[a-z0-9]{8,}\.(js|css|svg|png)$").unwrap());
static EXTENSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.[a-z0-9]{2,5}$").unwrap());

/// Cache of a static file: hashed build files forever, the others one hour.
pub fn static_cache(path: &str) -> &'static str {
    if HASHED.is_match(path) { "private, max-age=31536000, immutable" } else { "private, max-age=3600" }
}

/// A file of `dir` for this URL path, never outside it (no "..", no hidden segment tricks).
fn file_in(dir: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(url_path).decode_utf8().ok()?;
    let mut p = dir.to_path_buf();
    for seg in decoded.split('/').filter(|s| !s.is_empty()) {
        if seg == ".." || seg == "." || seg.contains('\\') || seg.contains('\0') {
            return None;
        }
        p.push(seg);
    }
    p.is_file().then_some(p)
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

fn http_date(t: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(t).format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

async fn send_file(path: &Path, cache: &str, req_headers: &HeaderMap, head: bool) -> Response {
    let Ok(meta) = tokio::fs::metadata(path).await else { return not_found() };
    let modified = meta.modified().ok();
    let etag = modified.map(|m| {
        let ms = m.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        format!("W/\"{:x}-{:x}\"", meta.len(), ms)
    });
    if let (Some(tag), Some(inm)) = (&etag, req_headers.get(header::IF_NONE_MATCH)) {
        if inm.to_str().is_ok_and(|v| v.split(',').any(|x| x.trim() == tag)) {
            let mut r = StatusCode::NOT_MODIFIED.into_response();
            r.headers_mut().insert(header::ETAG, HeaderValue::from_str(tag).unwrap());
            r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_str(cache).unwrap());
            return r;
        }
    }
    let body = if head { Body::empty() } else {
        match tokio::fs::read(path).await {
            Ok(b) => Body::from(b),
            Err(_) => return not_found(),
        }
    };
    let mut r = Response::new(body);
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime(path)));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(meta.len()));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_str(cache).unwrap());
    if let Some(tag) = etag {
        h.insert(header::ETAG, HeaderValue::from_str(&tag).unwrap());
    }
    if let Some(m) = modified {
        h.insert(header::LAST_MODIFIED, HeaderValue::from_str(&http_date(m)).unwrap());
    }
    r
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "Introuvable").into_response()
}

/// Built files, then public files, then the SPA: site, legal pages and /app/* are served by index.html. A missing
/// file (with an extension) is a real 404, never the HTML page.
pub async fn site(req: Request) -> Response {
    let head = req.method() == Method::HEAD;
    if req.method() != Method::GET && !head {
        return not_found();
    }
    let root = web_root();
    let path = req.uri().path();
    for dir in [root.join("dist"), root.join("public")] {
        if let Some(f) = file_in(&dir, path) {
            return send_file(&f, static_cache(path), req.headers(), head).await;
        }
    }
    if EXTENSION.is_match(path) {
        return not_found();
    }
    let index = root.join("dist/index.html");
    if !index.is_file() {
        return (StatusCode::INTERNAL_SERVER_ERROR, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "Build manquant : lancez `bun run build`.")
            .into_response();
    }
    send_file(&index, "no-cache", req.headers(), head).await
}

//! The exchanges recorded on the TypeScript server (`bun parity/auth.ts` → tests/golden/auth.json) replayed in the
//! same order against the Rust access middleware: same status, same body, same headers set by the auth code.
use std::net::SocketAddr;

use altim::auth::{Auth, AuthConfig, LoginPage, access, login_page, read_session};
use altim::js::now_ms;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use axum::middleware::from_fn_with_state;
use serde_json::Value;
use tower::ServiceExt;

const KEEP: [&str; 9] = ["content-type", "content-length", "location", "cache-control", "x-robots-tag", "vary", "etag", "retry-after", "set-cookie"];

fn golden() -> Value {
    serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/auth.json")).unwrap()).unwrap()
}

fn cfg(g: &Value) -> AuthConfig {
    AuthConfig {
        user: g["user"].as_str().unwrap().into(),
        password_hash: g["passwordHash"].as_str().unwrap().into(),
        totp_secret: None,
        session_secret: g["sessionSecret"].as_str().unwrap().into(),
        api_token: Some(g["apiToken"].as_str().unwrap().into()),
    }
}

#[test]
fn login_pages_byte_identical() {
    let g = golden();
    for p in g["pages"].as_array().unwrap() {
        let o = &p["opts"];
        let page = login_page(&LoginPage {
            error: o["error"].as_str(),
            next: o["next"].as_str(),
            totp: o["totp"].as_bool().unwrap(),
            configured: o["configured"].as_bool().unwrap(),
        });
        assert_eq!(page, p["html"].as_str().unwrap(), "{o}");
    }
}

/// Vary without the Accept-Encoding the compression layer of app.ts adds (not part of the auth code).
fn vary(v: &str) -> String {
    v.split(',').map(str::trim).filter(|t| !t.eq_ignore_ascii_case("accept-encoding")).collect::<Vec<_>>().join(", ")
}

#[tokio::test]
async fn http_exchanges_like_express() {
    let g = golden();
    let c = cfg(&g);
    let app = Router::new().fallback(|| async { "aval" }).layer(from_fn_with_state(Auth::new(Some(c.clone()), true), access));
    let mut etags: Vec<Option<String>> = vec![];
    for (i, ex) in g["exchanges"].as_array().unwrap().iter().enumerate() {
        let (rq, want) = (&ex["req"], &ex["res"]);
        let mut b = Request::builder().method(rq["method"].as_str().unwrap()).uri(rq["path"].as_str().unwrap()).header("host", "altim.test");
        for (k, v) in rq["headers"].as_object().unwrap() {
            b = b.header(k.as_str(), v.as_str().unwrap());
        }
        if let Some(of) = rq["etagOf"].as_u64() {
            b = b.header("if-none-match", etags[of as usize].clone().unwrap());
        }
        let body = rq["body"].as_str().map(|s| Body::from(s.to_string())).unwrap_or_else(Body::empty);
        let mut req = b.header("content-length", rq["body"].as_str().map_or(0, str::len)).body(body).unwrap();
        if rq["body"].is_null() {
            req.headers_mut().remove("content-length");
        }
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
        let res = app.clone().oneshot(req).await.unwrap();
        let ctx = format!("échange {i} : {} {}", rq["method"], rq["path"]);
        assert_eq!(res.status().as_u16() as u64, want["status"].as_u64().unwrap(), "{ctx}");
        let got = |k: &str| res.headers().get_all(k).iter().map(|v| v.to_str().unwrap().to_string()).collect::<Vec<_>>().join("\n");
        let wanted = want["headers"].as_object().unwrap();
        etags.push(wanted.get("etag").and_then(Value::as_str).map(str::to_string));
        for k in KEEP {
            let (w, r) = (wanted.get(k).and_then(Value::as_str).unwrap_or(""), got(k));
            match k {
                "vary" => assert_eq!(vary(&r), vary(w), "{ctx} : {k}"),
                // Axum's router adds "content-length: 0" in process; hyper does not send it on a 304 (checked on the wire).
                "content-length" if res.status() == 304 => assert!(w.is_empty() && (r.is_empty() || r == "0"), "{ctx} : {k}"),
                "retry-after" => {
                    assert_eq!(r.is_empty(), w.is_empty(), "{ctx} : {k}");
                    if !r.is_empty() {
                        assert!((0..=60).contains(&r.parse::<i64>().unwrap()), "{ctx} : {k} = {r}");
                    }
                }
                "set-cookie" if w.contains("Max-Age=604800") => {
                    // A new session: random nonce and time, same attributes; valid for the Rust and signed alike.
                    let (value, attrs) = r.split_once(';').unwrap();
                    assert_eq!(attrs, w.split_once(';').unwrap().1, "{ctx}");
                    let session = value.strip_prefix("altim_session=").unwrap();
                    assert!(read_session(&c, Some(session), now_ms(), None), "{ctx}");
                }
                _ => assert_eq!(r, w, "{ctx} : {k}"),
            }
        }
        let body = String::from_utf8(axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        assert_eq!(body, want["body"].as_str().unwrap(), "{ctx} : corps");
    }
}

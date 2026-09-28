//! Route errors, answered like the Express error handler: 400 for a bad parameter, 502 for an upstream failure
//! (its reason is useful to the user, never the addresses: they can carry source tokens).
use std::sync::LazyLock;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use regex::Regex;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Upstream(String),
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn bad<T>(msg: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::BadRequest(msg.into()))
}

impl From<crate::http::Error> for ApiError {
    fn from(e: crate::http::Error) -> Self {
        ApiError::Upstream(e.0)
    }
}

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://\S+").unwrap());

/// `message.replace(/https?:\/\/\S+/g, "[source]").slice(0, 200)` (200 UTF-16 units, as JavaScript counts).
pub fn scrub(message: &str) -> String {
    let s = URL.replace_all(message, "[source]");
    let mut out = String::new();
    let mut units = 0;
    for c in s.chars() {
        units += c.len_utf16();
        if units > 200 {
            break;
        }
        out.push(c);
    }
    out
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, Json(json!({ "error": m }))).into_response(),
            ApiError::Upstream(m) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": scrub(&m) }))).into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scrub_urls() {
        assert_eq!(super::scrub("échec https://x.io/a?token=1 fin"), "échec [source] fin");
        assert_eq!(super::scrub(&"é".repeat(300)).chars().count(), 200);
    }
}

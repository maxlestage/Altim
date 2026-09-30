//! Upstream HTTP: one shared client, the same headers and timeout as the TypeScript `getJSON`.
use std::sync::LazyLock;
use std::time::Duration;

use serde_json::Value;

pub const UA: &str = "Mozilla/5.0 Altim/1.0";

pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder().user_agent(UA).timeout(Duration::from_secs(8)).pool_idle_timeout(Duration::from_secs(60)).build().expect("client HTTP")
});

pub use altim_core::error::{Error, Result, err};

/// `reqwest` failure as the message the TypeScript showed.
pub fn from_reqwest(e: reqwest::Error) -> Error {
    if e.is_timeout() {
        Error("The operation timed out.".into())
    } else if let Some(s) = e.status() {
        Error(format!("HTTP {}", s.as_u16()))
    } else if e.is_decode() {
        Error("JSON invalide".into())
    } else {
        Error(e.to_string())
    }
}

/// GET → JSON, `Accept: application/json`, 8 s, error `HTTP <status>` when not 2xx.
pub async fn get_json(url: &str) -> Result<Value> {
    get_json_with(url, &[], Duration::from_secs(8)).await
}

pub async fn get_json_with(url: &str, headers: &[(&str, &str)], timeout: Duration) -> Result<Value> {
    let text = get_text_with(url, &[&[("Accept", "application/json")], headers].concat(), timeout).await?;
    serde_json::from_str(&text).map_err(|_| Error("JSON invalide".into()))
}

/// GET → text (feeds, HTML pages).
pub async fn get_text_with(url: &str, headers: &[(&str, &str)], timeout: Duration) -> Result<String> {
    let mut req = CLIENT.get(url).timeout(timeout);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = req.send().await.map_err(from_reqwest)?;
    if !res.status().is_success() {
        return err(format!("HTTP {}", res.status().as_u16()));
    }
    res.text().await.map_err(from_reqwest)
}

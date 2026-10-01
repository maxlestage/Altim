//! Client of the Altim server (api.ts): same routes, same-origin cookies (the private session), a 401 goes back to
//! the login page and then to the same screen, `{ pending: true }` (HTTP 202) for the heavy reports still being
//! computed. Response types live in altim-core (`engine::*` for the server's own contracts, `web::*` otherwise).
use altim_core::types::Kind;
use serde::de::DeserializeOwned;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// Error shown to the user (French message of the server, or "Erreur <status>").
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError(pub String);

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A heavy report: ready, or still being computed on the server (202 `{ pending: true }`).
#[derive(Debug, Clone, PartialEq)]
pub enum Pending<T> {
    Ready(T),
    Pending,
}

/// GET → (status, body text).
pub async fn fetch_text(url: &str) -> Result<(u16, String), ApiError> {
    let w = web_sys::window().ok_or_else(|| ApiError("fenêtre indisponible".into()))?;
    let res = JsFuture::from(w.fetch_with_str(url)).await.map_err(|_| ApiError("Serveur injoignable".into()))?;
    let res: web_sys::Response = res.dyn_into().map_err(|_| ApiError("Réponse illisible".into()))?;
    let text = match res.text() {
        Ok(p) => JsFuture::from(p).await.ok().and_then(|t| t.as_string()).unwrap_or_default(),
        Err(_) => String::new(),
    };
    Ok((res.status(), text))
}

/// Parsed JSON body (Value), handling the session and the errors like `get` of api.ts.
pub async fn get_value(url: &str) -> Result<(u16, serde_json::Value), ApiError> {
    let (status, text) = fetch_text(url).await?;
    // Session expired (private access): back to the login page, then to the same screen.
    if status == 401 {
        if let Some(w) = web_sys::window() {
            let l = w.location();
            let next = format!("{}{}", l.pathname().unwrap_or_default(), l.search().unwrap_or_default());
            let _ = l.assign(&format!("/login?next={}", altim_core::web::bot::encode_uri_component(&next)));
        }
        return Err(ApiError("Session expirée".into()));
    }
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({}));
    if !(200..300).contains(&status) {
        let msg = body.get("error").and_then(|e| e.as_str()).map(String::from).unwrap_or_else(|| format!("Erreur {status}"));
        return Err(ApiError(msg));
    }
    Ok((status, body))
}

/// GET → typed JSON.
pub async fn get<T: DeserializeOwned>(url: &str) -> Result<T, ApiError> {
    let (_, body) = get_value(url).await?;
    altim_core::web::json::from_value(&body).map_err(|e| ApiError(format!("Réponse inattendue du serveur ({e})")))
}

/// GET of a heavy report: `Pending::Pending` while the server computes it.
pub async fn get_pending<T: DeserializeOwned>(url: &str) -> Result<Pending<T>, ApiError> {
    let (status, body) = get_value(url).await?;
    if status == 202 || body.get("pending").and_then(|p| p.as_bool()) == Some(true) {
        return Ok(Pending::Pending);
    }
    altim_core::web::json::from_value(&body).map(Pending::Ready).map_err(|e| ApiError(format!("Réponse inattendue du serveur ({e})")))
}

/// Public API outside the server (the site's fallbacks: Binance, CoinGecko), no session handling.
pub async fn get_public<T: DeserializeOwned>(url: &str) -> Result<T, ApiError> {
    let (status, text) = fetch_text(url).await?;
    if !(200..300).contains(&status) {
        return Err(ApiError(format!("{status}")));
    }
    serde_json::from_str(&text).map_err(|e| ApiError(e.to_string()))
}

/// `encodeURIComponent("BTC:crypto,AAPL:stock")`: the `symbols=` list of the batch routes.
pub fn list(items: &[(String, Kind)]) -> String {
    let s: Vec<String> = items.iter().map(|(sym, k)| format!("{sym}:{}", k.as_str())).collect();
    altim_core::web::bot::encode_uri_component(&s.join(","))
}

pub fn enc(s: &str) -> String {
    altim_core::web::bot::encode_uri_component(s)
}

/// The server accepts 20 assets per request: larger lists are split into parallel batches (`batched`).
pub const BATCH: usize = 20;

pub async fn batched<T: DeserializeOwned>(items: &[(String, Kind)], url: impl Fn(&str) -> String) -> Result<Vec<T>, ApiError> {
    let urls: Vec<String> = items.chunks(BATCH).map(|c| url(&list(c))).collect();
    let calls = urls.iter().map(|u| get::<Vec<T>>(u)).collect::<Vec<_>>();
    let mut out = Vec::new();
    for r in futures::future::join_all(calls).await {
        out.extend(r?);
    }
    Ok(out)
}

// ---------- Routes of the site and the shell; screens add theirs in their own module ----------

/// Consolidated prices (`/api/tickers`, server consensus), falling back to Binance and CoinGecko directly.
pub async fn tickers() -> Vec<altim_core::web::market::Tick> {
    use altim_core::web::market::*;
    if let Ok(rows) = get::<Vec<Tick>>("/api/tickers").await {
        if !rows.is_empty() {
            return rows;
        }
    }
    let symbols: Vec<&str> = COINS.iter().map(|c| c.symbol).collect();
    let bin_url = format!("https://api.binance.com/api/v3/ticker/24hr?symbols={}", enc(&serde_json::to_string(&symbols).unwrap_or_default()));
    let bin = get_public::<Vec<BinanceTicker>>(&bin_url);
    let ids: Vec<&str> = COINS.iter().map(|c| c.gecko).collect();
    let gecko_url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_24hr_change=true", ids.join(","));
    let gecko = get_public::<std::collections::HashMap<String, GeckoPrice>>(&gecko_url);
    let (b, g) = futures::join!(bin, gecko);
    let (b, g) = (b.ok(), g.ok());
    fallback_ticks(b.as_deref(), g.as_ref())
}

/// Candles validated by multi-source consensus (`/api/candles?base=`), falling back to Binance directly.
pub async fn site_candles(coin: &altim_core::web::market::Coin, interval: &str) -> Result<altim_core::web::market::CandlesReply, ApiError> {
    use altim_core::web::market::*;
    if let Ok(d) = get::<CandlesReply>(&format!("/api/candles?base={}&interval={interval}", coin.base())).await {
        if !d.candles.is_empty() {
            return Ok(d);
        }
    }
    let rows: Vec<Vec<serde_json::Value>> =
        get_public(&format!("https://api.binance.com/api/v3/klines?symbol={}&interval={interval}&limit=500", coin.symbol)).await?;
    let candles = binance_klines(&rows, js_sys::Date::now());
    Ok(CandlesReply {
        candles,
        source: "Binance".into(),
        sources: vec![SourceStatus { name: "Binance".into(), ok: true, deviation: None, error: None }],
        agreeing: 1,
    })
}

/// « Bot Altim » report (heavy: pending until the first computation is ready).
pub async fn bot() -> Result<Pending<altim_core::web::bot::BotReport>, ApiError> {
    get_pending("/api/bot").await
}

/// Today's view of the bot for these assets (cached report only).
pub async fn bot_views(items: &[(String, Kind)]) -> Result<altim_core::web::bot::BotViews, ApiError> {
    get(&altim_core::web::bot::bot_views_url(items)).await
}

pub const INTERVAL_LABEL: [(&str, &str); 3] = [("1h", "1 h"), ("4h", "4 h"), ("1d", "1 j")];

//! Crypto derivatives and big moves (`/api/anomalies`, "Dérivés et gros mouvements"): liquidations, open interest,
//! funding and long/short ratio of the OKX USDT perpetual. Only OKX's own figures: its liquidations are not the
//! whole market's. Binance and Bybit refuse this server (HTTP 451 / 403), and whales or exchange flows have no free
//! verifiable source (Glassnode, CryptoQuant, Whale Alert are paid): shown as not covered, never estimated.
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::cache::cached;
use crate::engine::anomalies::{self, Anomaly, change, percentile};
use crate::guard::{UA, js_number};
use crate::http::{Error, Result, get_json_with};
use crate::js::{median, now_ms};

const TIMEOUT: Duration = Duration::from_secs(8);
pub const SOURCE: &str = "OKX (contrat perpétuel USDT)";
const DAY_MS: i64 = 86_400_000;
/// At most this many pages of 100 liquidations (≈ 1 h of BTC each on a busy day).
const MAX_PAGES: usize = 30;

async fn get(url: &str) -> Result<Value> {
    get_json_with(url, &[("User-Agent", UA)], TIMEOUT).await
}

/// OKX's answer, or why it refused ("code" other than "0", e.g. a region refusal).
fn okx_data(d: &Value) -> Result<&[Value]> {
    match d.get("code").and_then(|c| c.as_str()) {
        Some("0") => Ok(d.get("data").and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[])),
        code => Err(Error(format!(
            "OKX a refusé la requête (code {}{})",
            code.unwrap_or("?"),
            d.get("msg").and_then(|m| m.as_str()).filter(|m| !m.is_empty()).map(|m| format!(" : {m}")).unwrap_or_default()
        ))),
    }
}

/// One liquidation order (bankruptcy price, size in coins, USD value, which side was liquidated).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Liquidation {
    pub time: i64,
    pub price: f64,
    pub coins: f64,
    pub usd: f64,
    /// true = a long position was liquidated (forced sale).
    pub long: bool,
}

pub mod parse {
    use super::*;

    /// `/public/instruments?instType=SWAP&instId=BTC-USDT-SWAP`: coins per contract (`ctVal`, `ctValCcy` = the coin).
    pub fn ct_val(d: &Value) -> Option<f64> {
        let i = okx_data(d).ok()?.first()?;
        let v = js_number(i.get("ctVal"));
        (v.is_finite() && v > 0.0).then_some(v)
    }

    /// `/public/liquidation-orders`: filled liquidations, most recent first. `posSide` long/short says which
    /// position was closed; in net mode ("net"), a forced sale (`side` = sell) closes a long.
    pub fn liquidations(d: &Value, ct_val: f64) -> Result<Vec<Liquidation>> {
        let data = okx_data(d)?;
        Ok(data
            .iter()
            .flat_map(|x| x.get("details").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]))
            .filter_map(|o| {
                let time = js_number(o.get("ts")) as i64;
                let price = js_number(o.get("bkPx"));
                let sz = js_number(o.get("sz"));
                if !(price > 0.0 && sz > 0.0 && time > 0) {
                    return None;
                }
                let pos = o.get("posSide").and_then(|v| v.as_str()).unwrap_or("");
                let side = o.get("side").and_then(|v| v.as_str()).unwrap_or("");
                let long = match pos {
                    "long" => true,
                    "short" => false,
                    _ => side == "sell",
                };
                let coins = sz * ct_val;
                Some(Liquidation { time, price, coins, usd: coins * price, long })
            })
            .collect())
    }

    /// `/public/funding-rate-history`: settled rates, most recent first → (time, rate) oldest first.
    pub fn funding_history(d: &Value) -> Vec<(i64, f64)> {
        let mut v: Vec<(i64, f64)> = okx_data(d)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        let r = js_number(x.get("realizedRate").filter(|v| v.as_str().is_some_and(|s| !s.is_empty())).or(x.get("fundingRate")));
                        let t = js_number(x.get("fundingTime")) as i64;
                        (r.is_finite() && t > 0).then_some((t, r))
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by_key(|x| x.0);
        v
    }

    /// `/rubik/stat/contracts/open-interest-volume`: [ts, OI USD, volume USD], most recent first → oldest first.
    pub fn oi_series(d: &Value) -> Vec<(i64, f64)> {
        let mut v: Vec<(i64, f64)> = okx_data(d)
            .map(|a| {
                a.iter()
                    .filter_map(|r| {
                        let t = js_number(r.get(0)) as i64;
                        let oi = js_number(r.get(1));
                        (t > 0 && oi.is_finite() && oi > 0.0).then_some((t, oi))
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by_key(|x| x.0);
        v
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Largest {
    pub usd: f64,
    pub long: bool,
    pub price: f64,
    pub time: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiquidationSummary {
    /// USD liquidated on each side, and the number of orders.
    pub long_usd: f64,
    pub short_usd: f64,
    pub long_count: usize,
    pub short_count: usize,
    pub largest: Option<Largest>,
    /// Window actually read (ms) and its length in hours; `complete` = the full 24 h.
    pub from: i64,
    pub to: i64,
    pub hours: f64,
    pub complete: bool,
    /// "OKX seulement : …".
    pub scope: String,
}

/// Summary of the liquidations of the last 24 h (`now`), from the orders read (possibly fewer hours).
pub fn summarize(orders: &[Liquidation], now: i64, complete: bool, instrument: &str) -> LiquidationSummary {
    let from_limit = now - DAY_MS;
    let day: Vec<&Liquidation> = orders.iter().filter(|o| o.time >= from_limit && o.time <= now).collect();
    let (mut long_usd, mut short_usd, mut long_count, mut short_count) = (0.0, 0.0, 0, 0);
    for o in &day {
        if o.long {
            long_usd += o.usd;
            long_count += 1;
        } else {
            short_usd += o.usd;
            short_count += 1;
        }
    }
    let largest = day.iter().max_by(|a, b| a.usd.total_cmp(&b.usd)).map(|o| Largest { usd: o.usd, long: o.long, price: o.price, time: o.time });
    let from = if complete { from_limit } else { day.iter().map(|o| o.time).min().unwrap_or(now) };
    LiquidationSummary {
        long_usd,
        short_usd,
        long_count,
        short_count,
        largest,
        from,
        to: now,
        hours: ((now - from) as f64 / 3_600_000.0 * 10.0).round() / 10.0,
        complete,
        scope: format!(
            "OKX seulement ({instrument}) : les liquidations des autres plateformes (Binance, Bybit…) ne sont pas incluses, ce n'est pas le total du marché."
        ),
    }
}

async fn ct_val(base: &str) -> Result<f64> {
    let url = format!("https://www.okx.com/api/v5/public/instruments?instType=SWAP&instId={base}-USDT-SWAP");
    let v = cached(&format!("okx:ctval:{base}"), 24 * 3_600_000, move || async move {
        parse::ct_val(&get(&url).await?).ok_or_else(|| Error("contrat perpétuel introuvable sur OKX".into()))
    })
    .await?;
    Ok(*v)
}

/// Liquidations of the last 24 h, reading pages of 100 orders back in time (at most MAX_PAGES).
async fn liquidations_raw(base: String) -> Result<LiquidationSummary> {
    let cv = ct_val(&base).await?;
    let now = now_ms();
    let mut all: Vec<Liquidation> = Vec::new();
    let mut after: Option<i64> = None;
    let mut complete = false;
    for _ in 0..MAX_PAGES {
        let url = format!(
            "https://www.okx.com/api/v5/public/liquidation-orders?instType=SWAP&uly={base}-USDT&state=filled&limit=100{}",
            after.map(|t| format!("&after={t}")).unwrap_or_default()
        );
        let page = match get(&url).await.and_then(|d| parse::liquidations(&d, cv)) {
            Ok(p) => p,
            // A later page failing keeps what was read (the window says how many hours).
            Err(e) if all.is_empty() => return Err(e),
            Err(_) => break,
        };
        let Some(oldest) = page.iter().map(|o| o.time).min() else {
            // Nothing older: the whole day was read.
            complete = true;
            break;
        };
        all.extend(page);
        if oldest <= now - DAY_MS {
            complete = true;
            break;
        }
        after = Some(oldest);
    }
    Ok(summarize(&all, now, complete, &format!("{base}-USDT-SWAP")))
}

pub async fn liquidations(base: &str) -> Result<Arc<LiquidationSummary>> {
    let b = base.to_string();
    cached(&format!("okx:liq:{base}"), 300_000, move || liquidations_raw(b)).await
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInterest {
    pub usd: f64,
    pub time: i64,
    pub change24h: Option<f64>,
    pub change7d: Option<f64>,
}

/// Open interest now and its change over 24 h and 7 days, from the hourly series (oldest first).
pub fn open_interest_summary(hourly: &[(i64, f64)]) -> Option<OpenInterest> {
    let &(time, usd) = hourly.last()?;
    let v: Vec<f64> = hourly.iter().map(|x| x.1).collect();
    // Hourly points: 24 and 168 steps back, if the series has no hole at those places.
    let back = |k: usize| {
        let n = hourly.len();
        (n > k && (hourly[n - 1].0 - hourly[n - 1 - k].0 - k as i64 * 3_600_000).abs() <= 600_000).then(|| change(&v, k)).flatten()
    };
    Some(OpenInterest { usd, time, change24h: back(24), change7d: back(168) })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingSummary {
    /// Last settled rate and the 5–95 % range of the settlements read, % per period.
    pub rate: f64,
    pub p5: f64,
    pub p95: f64,
    pub samples: usize,
    /// Hours between two settlements (8 h for most contracts).
    pub period_hours: Option<f64>,
    pub time: i64,
}

/// Last settlement against the ones before it.
pub fn funding_summary(hist: &[(i64, f64)]) -> Option<FundingSummary> {
    let (&(time, rate), prior) = hist.split_last()?;
    let rates: Vec<f64> = prior.iter().map(|x| x.1).collect();
    let gaps: Vec<f64> = hist.windows(2).map(|w| (w[1].0 - w[0].0) as f64 / 3_600_000.0).collect();
    Some(FundingSummary {
        rate: rate * 100.0,
        p5: percentile(&rates, 5.0)? * 100.0,
        p95: percentile(&rates, 95.0)? * 100.0,
        samples: rates.len(),
        period_hours: (!gaps.is_empty()).then(|| median(&gaps)),
        time,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LongShort {
    pub ratio: f64,
    pub p5: f64,
    pub p95: f64,
    pub samples: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotCovered {
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Derivatives {
    pub source: String,
    pub liquidations: Option<LiquidationSummary>,
    pub open_interest: Option<OpenInterest>,
    pub funding: Option<FundingSummary>,
    pub long_short: Option<LongShort>,
    /// What failed, in words (e.g. OKX refusing this server's region).
    pub errors: Vec<String>,
    pub not_covered: Vec<NotCovered>,
}

pub fn not_covered() -> Vec<NotCovered> {
    vec![
        NotCovered {
            label: "Baleines (gros portefeuilles)".into(),
            reason: "aucune source gratuite et vérifiable (Whale Alert, Glassnode, Arkham : offres payantes)".into(),
        },
        NotCovered {
            label: "Entrées et sorties des plateformes".into(),
            reason: "aucune source gratuite et vérifiable (CryptoQuant, Glassnode : offres payantes)".into(),
        },
        NotCovered {
            label: "Liquidations des autres plateformes".into(),
            reason: "Binance et Bybit refusent ce serveur (HTTP 451 / 403) ; seules celles d'OKX sont lues".into(),
        },
    ]
}

/// Plain-language reason for an OKX failure (the production server is in the United States, which OKX may refuse).
pub fn explain_error(what: &str, e: &Error) -> String {
    format!(
        "{what} : OKX n'a pas répondu depuis ce serveur ({}) — OKX peut refuser les adresses américaines ; donnée indisponible, rien n'est estimé.",
        e.0
    )
}

async fn funding_history(base: &str) -> Result<Arc<Vec<(i64, f64)>>> {
    let url = format!("https://www.okx.com/api/v5/public/funding-rate-history?instId={base}-USDT-SWAP&limit=100");
    cached(&format!("okx:fh:{base}"), 1_800_000, move || async move { Ok(parse::funding_history(&get(&url).await?)) }).await
}

async fn oi_hourly(base: &str) -> Result<Arc<Vec<(i64, f64)>>> {
    let url = format!("https://www.okx.com/api/v5/rubik/stat/contracts/open-interest-volume?ccy={base}&period=1H");
    cached(&format!("okx:oi1h:{base}"), 300_000, move || async move {
        let d = get(&url).await?;
        okx_data(&d)?;
        Ok(parse::oi_series(&d))
    })
    .await
}

/// Derivatives block and its anomalies (open interest, funding, long/short). Each part fails on its own.
pub async fn derivatives(base: &str) -> (Derivatives, Vec<Anomaly>) {
    let (liq, oi, fh, pos) = tokio::join!(liquidations(base), oi_hourly(base), funding_history(base), crate::guard::positioning(base));
    let mut errors = Vec::new();
    let mut found = Vec::new();
    let liquidations = match liq {
        Ok(l) => Some((*l).clone()),
        Err(e) => {
            errors.push(explain_error("Liquidations", &e));
            None
        }
    };
    let open_interest = match oi {
        Ok(s) => {
            let v: Vec<f64> = s.iter().map(|x| x.1).collect();
            found.extend(anomalies::open_interest(&v, SOURCE));
            open_interest_summary(&s)
        }
        Err(e) => {
            errors.push(explain_error("Open interest", &e));
            None
        }
    };
    let funding = match fh {
        Ok(h) => {
            let rates: Vec<f64> = h.iter().map(|x| x.1).collect();
            if let Some((&last, prior)) = rates.split_last() {
                found.extend(anomalies::funding(last, prior, SOURCE));
            }
            funding_summary(&h)
        }
        Err(e) => {
            errors.push(explain_error("Funding", &e));
            None
        }
    };
    let ls_series = pos.as_ref().map(|p| p.long_short_ratio.clone()).unwrap_or_default();
    let long_short_sum = match ls_series.split_last() {
        Some((&ratio, hist)) if hist.len() >= 48 => {
            found.extend(anomalies::long_short(&ls_series, SOURCE));
            Some(LongShort { ratio, p5: percentile(hist, 5.0).unwrap_or(ratio), p95: percentile(hist, 95.0).unwrap_or(ratio), samples: hist.len() })
        }
        _ => {
            errors.push("Ratio acheteurs/vendeurs : OKX n'a pas répondu depuis ce serveur (ou historique trop court) ; donnée indisponible.".into());
            None
        }
    };
    (
        Derivatives {
            source: format!("{SOURCE} {base}-USDT-SWAP"),
            liquidations,
            open_interest,
            funding,
            long_short: long_short_sum,
            errors,
            not_covered: not_covered(),
        },
        found,
    )
}

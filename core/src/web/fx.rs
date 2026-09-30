//! EUR/USD rate of the display (`web/src/webapp/fx.ts`): /api/fx body, and the last valid rate kept in the browser
//! ("altim.fx.v1", the raw body) for 7 days at most so a server hiccup does not switch amounts back to dollars.
use serde_json::Value;

use super::money::FxRate;

pub const FX_KEY: &str = "altim.fx.v1";
pub const FX_REFRESH_MS: u32 = 10 * 60_000;
const KEEP_MS: f64 = 7.0 * 86_400_000.0;

/// A valid rate from the response, else None (never a default).
pub fn parse_fx(x: &Value, now: f64) -> Option<FxRate> {
    let rate = x.get("rate")?.as_f64()?;
    if !rate.is_finite() || rate <= 0.0 || rate > 5.0 {
        return None;
    }
    let time = x.get("time")?.as_f64()?;
    let source = x.get("source")?.as_str()?.to_string();
    Some(FxRate {
        rate,
        usd_per_eur: x.get("usdPerEur").and_then(Value::as_f64).unwrap_or(1.0 / rate),
        time,
        source,
        fetched_at: x.get("fetchedAt").and_then(Value::as_f64).unwrap_or(now),
        stale: x.get("stale").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// The saved rate when younger than 7 days (read by Altim, not quoted: a weekend's Friday close stays valid).
pub fn saved_fx(raw: Option<&str>, now: f64) -> Option<FxRate> {
    let v: Value = serde_json::from_str(raw?).ok()?;
    let r = parse_fx(&v, now)?;
    (now - r.fetched_at < KEEP_MS).then_some(FxRate { stale: true, ..r })
}

/// The error the body carries when it has no valid rate ("taux indisponible" by default).
pub fn fx_error(x: &Value) -> String {
    x.get("error").and_then(Value::as_str).unwrap_or("taux indisponible").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // money.test.ts "/api/fx"
    #[test]
    fn valid_rate_kept_missing_never_replaced() {
        let body =
            json!({ "base": "USD", "quote": "EUR", "rate": 0.88, "usdPerEur": 1.136, "time": 1, "source": "BCE", "fetchedAt": 2, "stale": false });
        assert_eq!(parse_fx(&body, 0.0).unwrap().rate, 0.88);
        let mut none = body.clone();
        none["rate"] = Value::Null;
        assert!(parse_fx(&none, 0.0).is_none());
        let mut big = body.clone();
        big["rate"] = json!(88);
        assert!(parse_fx(&big, 0.0).is_none());
        let s = body.to_string();
        assert!(saved_fx(Some(&s), 2.0 + 3_600_000.0).unwrap().stale);
        assert!(saved_fx(Some(&s), 2.0 + 8.0 * 86_400_000.0).is_none());
        assert!(saved_fx(Some("{"), 0.0).is_none());
    }
}

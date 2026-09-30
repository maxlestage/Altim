//! The rate line under amounts (`fxLine` of web/src/money.ts), without the browser: the caller gives the time as the
//! viewer's clock shows it ("14:05", or "29 sept. 14:05" when not today).
use crate::js::fr;
use crate::web::money::FxRate;

/// "1 $ = 0,881 € · Yahoo Finance, 14:05".
// TODO(phase 3): frontend/src/money.rs `fx_line` (phase 1, shared) can format with this once the batches merge.
pub fn fx_line(r: &FxRate, when: &str) -> String {
    format!("1 $ = {} € · {}, {when}", fr(r.rate, 3, 4), r.source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_line() {
        let r = FxRate { rate: 0.88, usd_per_eur: 1.0 / 0.88, time: 0.0, source: "Yahoo Finance".into(), fetched_at: 0.0, stale: false };
        assert_eq!(fx_line(&r, "14:05"), "1 $ = 0,880 € · Yahoo Finance, 14:05");
        assert_eq!(fx_line(&FxRate { rate: 0.88123, ..r }, "29 sept. 14:05"), "1 $ = 0,8812 € · Yahoo Finance, 29 sept. 14:05");
    }
}

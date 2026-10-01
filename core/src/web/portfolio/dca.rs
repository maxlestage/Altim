//! "What if I had invested X $ every week / month" (`web/src/engine/dca.ts`): regular purchases at the daily close,
//! over real past prices, compared with the same total invested at once on the first day. No fees, no taxes; the
//! past does not predict the future. Same computation on the iPhone and Android.
use serde::{Deserialize, Serialize};

use super::holdings::positive;
use crate::engine::history::Close;
use crate::types::DAY_MS;
use crate::web::sorting::Sorting;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LumpSum {
    pub value: f64,
    pub gain: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DcaPoint {
    pub t: i64,
    pub invested: f64,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DcaResult {
    /// Number of purchases made.
    pub buys: f64,
    pub invested: f64,
    pub units: f64,
    /// Value of the units at the last close.
    pub value: f64,
    /// Gain in % of the invested amount.
    pub gain: f64,
    /// Average price paid per unit.
    pub average_price: f64,
    pub last_price: f64,
    /// The same total invested at once at the first purchase.
    pub lump_sum: LumpSum,
    /// Value of the regular plan after each purchase day, for a small chart.
    pub path: Vec<DcaPoint>,
    pub first: i64,
    pub last: i64,
}

/// One purchase repeats nothing: its "next purchase" is far beyond any period (DcaCard's "Une fois").
pub const ONCE: i64 = 100_000;

pub fn simulate_dca(closes: &[Close], amount: f64, every_days: i64, days: i64, now: i64) -> Option<DcaResult> {
    if !positive(amount) || every_days <= 0 {
        return None;
    }
    let mut sorted: Vec<Close> = closes.iter().filter(|(t, c)| *c > 0.0 && *t <= now).copied().collect();
    sorted.sort_by_key_dyn(|c| c.0);
    if sorted.len() < 2 {
        return None;
    }
    let end = sorted[sorted.len() - 1].0;
    let start = end - days * DAY_MS;
    let in_range: Vec<Close> = sorted.iter().filter(|(t, _)| *t >= start).copied().collect();
    // The asset must have existed for the whole period, otherwise the comparison is meaningless.
    if in_range.len() < 2 || sorted[0].0 > start + 7 * DAY_MS {
        return None;
    }
    let (mut units, mut invested) = (0.0, 0.0);
    let mut next = in_range[0].0;
    let mut path = Vec::with_capacity(in_range.len());
    for &(t, c) in &in_range {
        if t >= next {
            units += amount / c;
            invested += amount;
            next += every_days * DAY_MS;
            // Weekends and holidays: the next purchase happens at the next close available.
            while next <= t {
                next += every_days * DAY_MS;
            }
        }
        path.push(DcaPoint { t, invested, value: units * c });
    }
    let last_price = in_range[in_range.len() - 1].1;
    let value = units * last_price;
    let first_price = in_range[0].1;
    let lump = invested / first_price * last_price;
    Some(DcaResult {
        buys: crate::js::round(invested / amount),
        invested,
        units,
        value,
        gain: (value / invested - 1.0) * 100.0,
        average_price: invested / units,
        last_price,
        lump_sum: LumpSum { value: lump, gain: (lump / invested - 1.0) * 100.0 },
        path,
        first: in_range[0].0,
        last: end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = DAY_MS;
    /// `Date.parse("2026-09-01T00:00:00Z")`.
    const SEP1: i64 = 1_788_220_800_000;

    fn close(a: f64, b: f64, digits: i32) {
        assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≠ {b}");
    }

    fn btc() -> Vec<Close> {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../../../backend/tests/samples/history-sample.json")).unwrap();
        let s = v["series"].as_array().unwrap().iter().find(|s| s["symbol"] == "BTC").unwrap();
        s["closes"].as_array().unwrap().iter().map(|c| (c[0].as_i64().unwrap(), c[1].as_f64().unwrap())).collect()
    }

    #[test]
    fn exact_on_three_purchases() {
        // 100 $ on days 0, 7 and 14 at 10, 20 and 5 $; last close 10 $.
        let closes: Vec<Close> = (0..21)
            .map(|i| {
                let v = if i < 7 {
                    10.0
                } else if i < 14 {
                    20.0
                } else if i < 20 {
                    5.0
                } else {
                    10.0
                };
                (SEP1 + i * DAY, v)
            })
            .collect();
        let r = simulate_dca(&closes, 100.0, 7, 20, SEP1 + 21 * DAY).unwrap();
        assert_eq!(r.buys, 3.0);
        assert_eq!(r.invested, 300.0);
        close(r.units, 10.0 + 5.0 + 20.0, 9);
        close(r.value, 350.0, 9);
        close(r.gain, 50.0 / 3.0, 9);
        close(r.average_price, 300.0 / 35.0, 9);
        // All at once on day 0 at 10 $: 30 units → 300 $, no gain.
        close(r.lump_sum.value, 300.0, 9);
        close(r.lump_sum.gain, 0.0, 9);
        close(r.path[r.path.len() - 1].value, 350.0, 9);
    }

    #[test]
    fn weekend_purchase_at_next_close() {
        // Trading days only (Mon–Fri), purchase every 7 days starting on Monday 31 Aug.: always the next Monday.
        let monday = SEP1 - DAY;
        let closes: Vec<Close> = (0..28).filter(|i| i % 7 < 5).map(|i| (monday + i * DAY, 100.0 + i as f64)).collect();
        let r = simulate_dca(&closes, 50.0, 7, 27, SEP1 + 27 * DAY).unwrap();
        assert_eq!(r.buys, 4.0);
        assert_eq!(r.invested, 200.0);
    }

    #[test]
    fn real_btc_closes_weekly() {
        let btc = btc();
        let r = simulate_dca(&btc, 100.0, 7, 90, i64::MAX).unwrap();
        assert_eq!(r.buys, 13.0);
        assert_eq!(r.invested, 1300.0);
        close(r.last_price, 84464.605, 3);
        close(r.value, r.units * 84464.605, 6);
        assert!(r.average_price > 60000.0 && r.average_price < 84464.605);
        // BTC rose over the period: buying everything on day one did better than spreading the purchases.
        assert!(r.lump_sum.gain > r.gain);
    }

    #[test]
    fn single_purchase() {
        let btc = btc();
        let r = simulate_dca(&btc, 1000.0, ONCE, 90, i64::MAX).unwrap();
        assert_eq!(r.buys, 1.0);
        assert_eq!(r.invested, 1000.0);
        close(r.value, r.lump_sum.value, 9);
        let bought = btc.iter().find(|(t, _)| *t == r.first).unwrap().1;
        close(r.average_price, bought, 6);
        close(r.value, 1000.0 / bought * r.last_price, 6);
    }

    #[test]
    fn longer_than_history_and_bad_amounts() {
        let btc = btc();
        assert_eq!(simulate_dca(&btc, 100.0, 30, 365, i64::MAX), None);
        assert_eq!(simulate_dca(&btc, 0.0, 7, 90, i64::MAX), None);
        assert_eq!(simulate_dca(&[], 100.0, 7, 90, i64::MAX), None);
    }
}

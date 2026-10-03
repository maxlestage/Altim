//! Multi-day candles (4 d, 1 w) built from daily ones, so that every provider gives the same buckets: 4 calendar days
//! counted from the Unix epoch (stable over time), weeks from Monday 00:00 UTC (Binance's weekly candles).
use crate::types::{Candle, DAY_MS, Interval};

/// Monday 5 January 1970 00:00 UTC (the epoch was a Thursday).
pub const WEEK_ANCHOR_MS: i64 = 4 * DAY_MS;

/// Start of the bucket holding `time` (ms).
pub fn bucket_start(time: i64, interval: Interval) -> i64 {
    let step = interval.step();
    let anchor = if interval == Interval::W1 { WEEK_ANCHOR_MS } else { 0 };
    anchor + (time - anchor).div_euclid(step) * step
}

/// `Math.max` / `Math.min` (NaN wins: a bad daily candle is not hidden).
fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}
fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// Daily candles grouped into `interval` buckets, each dated at its bucket start: open = first, close = last,
/// high = max, low = min, volume = sum. Missing days (weekends, holidays, a provider's gap) only make a bucket hold
/// fewer days; nothing is padded. The first bucket is dropped (the history window may cut it); the still-forming last
/// one is kept here and removed by the usual closed filter (bucket start + step ≤ now), like every other timeframe.
pub fn bucket(daily: &[Candle], interval: Interval) -> Vec<Candle> {
    let mut sorted = daily.to_vec();
    sorted.sort_by_key(|c| c.time);
    let mut out: Vec<Candle> = Vec::new();
    for c in sorted {
        let key = bucket_start(c.time, interval);
        match out.last_mut() {
            Some(b) if b.time == key => {
                b.high = jmax(b.high, c.high);
                b.low = jmin(b.low, c.low);
                b.close = c.close;
                b.volume += c.volume;
            }
            _ => out.push(Candle { time: key, ..c }),
        }
    }
    if !out.is_empty() {
        out.remove(0);
    }
    out
}

/// A provider's native multi-day candles dated at their bucket start (Yahoo dates its weeks Monday 04:00 UTC and adds
/// a live point inside the current week): one candle per bucket, the first one kept.
pub fn align(candles: &[Candle], interval: Interval) -> Vec<Candle> {
    let mut sorted = candles.to_vec();
    sorted.sort_by_key(|c| c.time);
    let mut out: Vec<Candle> = Vec::new();
    for c in sorted {
        let key = bucket_start(c.time, interval);
        if out.last().is_none_or(|b| b.time != key) {
            out.push(Candle { time: key, ..c });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(n: i64, close: f64) -> Candle {
        Candle { time: n * DAY_MS, open: close - 1.0, high: close + 2.0, low: close - 3.0, close, volume: 10.0 }
    }

    #[test]
    fn interval_round_trip() {
        for i in Interval::ALL {
            assert_eq!(Interval::parse(i.as_str()), Some(i));
            assert_eq!(serde_json::to_value(i).unwrap(), serde_json::json!(i.as_str()));
            assert_eq!(serde_json::from_value::<Interval>(serde_json::json!(i.as_str())).unwrap(), i);
        }
        assert_eq!(Interval::parse("1W"), None);
        assert_eq!(Interval::ALL.map(Interval::label), ["1 h", "4 h", "1 j", "4 j", "1 sem."]);
        assert_eq!((Interval::D4.step(), Interval::W1.step()), (4 * DAY_MS, 7 * DAY_MS));
        // Validated pairs unchanged; the new ones.
        assert_eq!(Interval::H1.higher(), Some(Interval::H4));
        assert_eq!(Interval::H4.higher(), Some(Interval::D1));
        assert_eq!(Interval::D1.higher(), None);
        assert_eq!(Interval::D4.higher(), Some(Interval::W1));
        assert_eq!(Interval::W1.higher(), None);
    }

    #[test]
    fn four_days_anchored_on_the_epoch() {
        // 2026-10-01 00:00 UTC = day 20 727 = 4 × 5 181 + 3: its bucket started on day 20 724 (2026-09-28).
        let t = 20_727 * DAY_MS + 13 * 3_600_000;
        assert_eq!(bucket_start(t, Interval::D4), 20_724 * DAY_MS);
        assert_eq!(bucket_start(20_724 * DAY_MS, Interval::D4), 20_724 * DAY_MS);
        assert_eq!(bucket_start(-1, Interval::D4), -4 * DAY_MS);
        // Days 3..=13: bucket 0..4 (cut, dropped), 4..8, 8..12, 12..16 (forming, kept for the closed filter).
        let daily: Vec<Candle> = (3..=13).map(|n| day(n, n as f64 * 10.0)).collect();
        let b = bucket(&daily, Interval::D4);
        assert_eq!(b.iter().map(|c| c.time / DAY_MS).collect::<Vec<_>>(), [4, 8, 12]);
        let c = b[0];
        assert_eq!((c.open, c.close, c.high, c.low, c.volume), (39.0, 70.0, 72.0, 37.0, 40.0));
        assert_eq!(b[2].volume, 20.0);
        // Same buckets whatever the first day fetched: stable over time.
        let later = bucket(&daily[2..], Interval::D4);
        assert_eq!(later[0], b[1]);
    }

    #[test]
    fn four_days_with_stock_gaps() {
        // Sessions at 13:30 UTC, Monday to Friday only (day 20 724 = Monday 2026-09-28), Friday 2 October a holiday.
        let session = |n: i64, close: f64| Candle { time: n * DAY_MS + 48_600_000, ..day(n, close) };
        let days = [20_717, 20_718, 20_719, 20_720, 20_721, 20_724, 20_725, 20_726, 20_727, 20_731];
        let daily: Vec<Candle> = days.iter().map(|&n| session(n, n as f64 - 20_000.0)).collect();
        let b = bucket(&daily, Interval::D4);
        // Buckets 20 716 (Sun–Wed, cut), 20 720 (Thu + Fri, then the weekend), 20 724 (Mon–Thu), 20 728 (Fri holiday,
        // weekend: only Monday 5).
        assert_eq!(b.iter().map(|c| c.time / DAY_MS).collect::<Vec<_>>(), [20_720, 20_724, 20_728]);
        assert_eq!((b[0].open, b[0].close, b[0].volume), (719.0, 721.0, 20.0));
        assert_eq!((b[1].open, b[1].close, b[1].volume), (723.0, 727.0, 40.0));
        assert_eq!((b[2].open, b[2].close, b[2].volume), (730.0, 731.0, 10.0));
        assert!(b.iter().all(|c| c.time % (4 * DAY_MS) == 0));
    }

    #[test]
    fn weeks_start_on_monday() {
        // Day 20 724 = Monday 2026-09-28; days 20 720 (Thu) to 20 738 (Mon).
        let daily: Vec<Candle> = (20_720..=20_738).map(|n| day(n, n as f64)).collect();
        let b = bucket(&daily, Interval::W1);
        assert_eq!(b.iter().map(|c| c.time / DAY_MS).collect::<Vec<_>>(), [20_724, 20_731, 20_738]);
        assert_eq!((b[0].open, b[0].close, b[0].high, b[0].low, b[0].volume), (20_723.0, 20_730.0, 20_732.0, 20_721.0, 70.0));
        // Native weeks dated Monday 04:00 UTC plus a live point on Friday: one candle per week, at Monday 00:00.
        let native = [day(20_724, 1.0), day(20_731, 2.0), day(20_735, 3.0)].map(|c| Candle { time: c.time + 4 * 3_600_000, ..c });
        let a = align(&native, Interval::W1);
        assert_eq!(a.iter().map(|c| (c.time / DAY_MS, c.close)).collect::<Vec<_>>(), [(20_724, 1.0), (20_731, 2.0)]);
    }
}

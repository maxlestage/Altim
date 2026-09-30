//! JavaScript `Date` semantics the engines rely on (`Date.UTC`, New York session times), shared by the server's
//! market module and the browser.
use chrono::{Datelike, Timelike, Weekday};
use chrono_tz::America::New_York;

use crate::error::{Error, Result, err};

/// Candle time of a JavaScript `NaN` (an unparsable date): never "closed", so `closed_only` drops it like
/// `Number.isFinite` / the NaN comparison do in the TypeScript.
pub const NAN_TIME: i64 = i64::MAX;

/// A JavaScript time value (ms) as a candle time.
pub fn ms(x: f64) -> i64 {
    if x.is_finite() { x as i64 } else { NAN_TIME }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `Date.UTC(y, m0, d, h, min)`, with its overflow rules (month 12 = next January, day 0 = last day of the
/// previous month, years 0–99 = 1900–1999). NaN when an argument is not finite or the date is out of range.
pub fn date_utc(y: f64, m0: f64, d: f64, h: f64, min: f64) -> f64 {
    if ![y, m0, d, h, min].iter().all(|x| x.is_finite()) {
        return f64::NAN;
    }
    let mut y = y.trunc();
    if (0.0..=99.0).contains(&y) {
        y += 1900.0;
    }
    let m0 = m0.trunc();
    let ym = y + (m0 / 12.0).floor();
    let mn = m0.rem_euclid(12.0);
    if ym.abs() > 400_000.0 || d.abs() > 1e9 || h.abs() > 1e12 || min.abs() > 1e14 {
        return f64::NAN;
    }
    let day = days_from_civil(ym as i64, mn as i64 + 1, 1) as f64 + d.trunc() - 1.0;
    let t = day * 86_400_000.0 + h.trunc() * 3_600_000.0 + min.trunc() * 60_000.0;
    if t.abs() > 8.64e15 { f64::NAN } else { t }
}

const NOT_FINITE: &str = "date value is not finite in DateTimeFormat format()";

fn ny(t: f64) -> Result<chrono::DateTime<chrono_tz::Tz>> {
    if !t.is_finite() {
        return err(NOT_FINITE);
    }
    chrono::DateTime::from_timestamp_millis(t as i64).map(|d| d.with_timezone(&New_York)).ok_or_else(|| Error(NOT_FINITE.into()))
}

/// `NY_DAY.format(t)` / `NY_DATE.format(t)` (en-CA: "2026-09-25"), New York calendar date of a time value.
pub fn ny_date_string(t: f64) -> Result<String> {
    let d = ny(t)?;
    Ok(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()))
}

/// `nyOpen` with JavaScript numbers: throws like `Intl.DateTimeFormat` on a non-finite date.
pub fn ny_open_checked(year: f64, month: f64, day: f64) -> Result<i64> {
    for utc_hour in [13.0, 14.0] {
        let t = date_utc(year, month - 1.0, day, utc_hour, 30.0);
        let local = ny(t)?;
        if local.hour() == 9 && local.minute() == 30 {
            return Ok(t as i64);
        }
    }
    Ok(ms(date_utc(year, month - 1.0, day, 13.0, 30.0)))
}

/// Opening of the New York session (9:30 local) for a date, US daylight saving time handled (`nyOpen`).
pub fn ny_open(year: i32, month: u32, day: u32) -> i64 {
    ny_open_checked(year as f64, month as f64, day as f64).unwrap_or(NAN_TIME)
}

/// US regular session (9:30 – 16:00 New York, weekdays). Holidays are not known: the price simply stops moving.
pub fn us_market_open(now: i64) -> bool {
    let Some(d) = chrono::DateTime::from_timestamp_millis(now) else { return false };
    let d = d.with_timezone(&New_York);
    if matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let open = ny_open(d.year(), d.month(), d.day());
    now >= open && now < open + 23_400_000
}

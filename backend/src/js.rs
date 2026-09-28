//! JavaScript semantics the TypeScript engines rely on, so the Rust port gives the same numbers and texts:
//! `Math.round`, `toFixed`, `toLocaleString("fr-FR")`, `JSON.stringify` of numbers, `Date.now()`.
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

/// `Date.now()`.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// `Math.round`: nearest integer, ties towards +∞.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// `Number.prototype.toFixed`: exact binary value, a tie rounds away from zero.
pub fn to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x.abs() >= 1e21 {
        return number_to_string(x);
    }
    let x = if x == 0.0 { 0.0 } else { x };
    // Rust rounds an exact tie to even; JavaScript picks the larger magnitude.
    let exact = format!("{:.1100}", x.abs());
    let dot = exact.find('.').unwrap_or(exact.len());
    let tail = &exact[dot + 1 + digits..];
    let tie = tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0');
    let v = if tie { if x > 0.0 { x.next_up() } else { x.next_down() } } else { x };
    format!("{:.*}", digits, v)
}

/// `String(x)` for a number (shortest round-trip, as `JSON.stringify`).
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x == 0.0 {
        return "0".into();
    }
    if x.fract() == 0.0 && x.abs() < 1e21 {
        return format!("{x:.0}");
    }
    // Rust never uses exponents in `{}`; JavaScript does below 1e-6 ("1.5e-7" in both).
    if x.abs() < 1e-6 {
        return format!("{x:e}");
    }
    format!("{x}")
}

/// Shortest round-trip decimal digits of |x| and the position of the decimal point (`digits` × 10^(point − len)).
fn shortest(x: f64) -> (Vec<u8>, i32) {
    let s = format!("{:e}", x.abs()); // "1.2345e3"
    let (mant, exp) = s.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: Vec<u8> = mant.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
    (digits, exp + 1)
}

/// Rounds a decimal (digits, point) at `keep` digits after the point, half away from zero (Intl's `halfExpand`).
fn round_decimal(mut digits: Vec<u8>, mut point: i32, keep_after_point: i32) -> (Vec<u8>, i32) {
    let keep = point + keep_after_point; // number of digits kept
    if keep < 0 {
        return (vec![0], 1);
    }
    let keep = keep as usize;
    if keep >= digits.len() {
        return (digits, point);
    }
    let up = digits[keep] >= 5;
    digits.truncate(keep);
    if up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                point += 1;
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    if digits.is_empty() {
        return (vec![0], 1);
    }
    (digits, point)
}

fn render_fr(negative: bool, digits: &[u8], point: i32, min_frac: usize) -> String {
    let mut int_part = String::new();
    let mut frac = String::new();
    for i in 0..point.max(1) {
        let d = if point <= 0 { 0 } else { *digits.get(i as usize).unwrap_or(&0) };
        int_part.push((b'0' + d) as char);
    }
    if point < 0 {
        frac.extend(std::iter::repeat_n('0', (-point) as usize));
    }
    for d in digits.iter().skip(point.max(0) as usize) {
        frac.push((b'0' + d) as char);
    }
    while frac.ends_with('0') && frac.len() > min_frac {
        frac.pop();
    }
    while frac.len() < min_frac {
        frac.push('0');
    }
    // Groups of three with a narrow no-break space (fr-FR), from four digits on.
    let mut grouped = String::new();
    let n = int_part.len();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) && n > 3 {
            grouped.push('\u{202f}');
        }
        grouped.push(c);
    }
    // Intl keeps the sign of a negative value rounded to zero ("-0").
    let sign = if negative { "-" } else { "" };
    if frac.is_empty() { format!("{sign}{grouped}") } else { format!("{sign}{grouped},{frac}") }
}

/// `x.toLocaleString("fr-FR", { minimumFractionDigits, maximumFractionDigits })`.
pub fn fr(x: f64, min_frac: usize, max_frac: usize) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "∞".into() } else { "-∞".into() };
    }
    if x == 0.0 {
        return render_fr(x.is_sign_negative(), &[0], 1, min_frac);
    }
    let (d, p) = shortest(x);
    let (d, p) = round_decimal(d, p, max_frac as i32);
    render_fr(x < 0.0, &d, p, min_frac)
}

/// `x.toLocaleString("fr-FR", { maximumSignificantDigits })`.
pub fn fr_sig(x: f64, max_sig: usize) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "∞".into() } else { "-∞".into() };
    }
    if x == 0.0 {
        return render_fr(x.is_sign_negative(), &[0], 1, 0);
    }
    let (d, p) = shortest(x);
    let (d, p) = round_decimal(d, p, max_sig as i32 - p);
    render_fr(x < 0.0, &d, p, 0)
}

/// Median as in the TypeScript (mean of the two middle values when even). Empty → NaN.
pub fn median(v: &[f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = s.len();
    if n % 2 == 1 { s[(n - 1) / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 }
}

/// `new Date(ms).toISOString()`.
pub fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_else(|| "Invalid Date".into())
}

/// `new Date(ms).toISOString().slice(0, 10)`.
pub fn iso_date(ms: i64) -> String {
    iso(ms)[..10].to_string()
}

/// `Date.parse` for the ISO / RFC 2822 dates the upstream APIs and feeds use. None = NaN.
pub fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.timestamp_millis());
    }
    if let Ok(d) = chrono::DateTime::parse_from_rfc2822(s) {
        return Some(d.timestamp_millis());
    }
    // "2026-09-25T14:00:00" (local = UTC on the server), "2026-09-25 14:00:00", "2026-09-25"
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(d) = chrono::NaiveDateTime::parse_from_str(s, f) {
            return Some(d.and_utc().timestamp_millis());
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis());
    }
    // RFC 822 variants with a named zone ("GMT", "EST", "EDT"...) chrono refuses.
    for (zone, offset) in [("GMT", "+0000"), ("UTC", "+0000"), ("UT", "+0000"), ("EST", "-0500"), ("EDT", "-0400"), ("CST", "-0600"), ("CDT", "-0500"), ("PST", "-0800"), ("PDT", "-0700")] {
        if let Some(head) = s.strip_suffix(zone) {
            if let Ok(d) = chrono::DateTime::parse_from_rfc2822(&format!("{}{}", head, offset)) {
                return Some(d.timestamp_millis());
            }
        }
    }
    None
}

/// `JSON.stringify` of numbers: an integral float is written without ".0" (the apps decode some fields as
/// integers), NaN and ±∞ become null.
pub fn normalize(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if n.is_f64() && f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
                    *v = Value::from(f as i64);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(normalize),
        Value::Object(o) => o.values_mut().for_each(normalize),
        _ => {}
    }
}

/// Serializes like `JSON.stringify` would (see `normalize`).
pub fn to_value<T: Serialize + ?Sized>(t: &T) -> Value {
    let mut v = serde_json::to_value(t).unwrap_or(Value::Null);
    normalize(&mut v);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_and_round() {
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(-0.0, 1), "0.0");
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(0.49999999999999994), 0.0);
    }

    #[test]
    fn french() {
        assert_eq!(fr(1234567.891, 0, 2), "1\u{202f}234\u{202f}567,89");
        assert_eq!(fr(1.005, 0, 2), "1,01");
        assert_eq!(fr(999.999, 2, 2), "1\u{202f}000,00");
        assert_eq!(fr(-3.24159, 0, 1), "-3,2");
        assert_eq!(fr(0.5, 0, 0), "1");
        assert_eq!(fr(1000.0, 0, 0), "1\u{202f}000");
        assert_eq!(fr(12.0, 2, 2), "12,00");
        assert_eq!(fr_sig(0.000123456, 4), "0,0001235");
        assert_eq!(fr_sig(123456.0, 4), "123\u{202f}500");
        assert_eq!(fr_sig(0.5, 4), "0,5");
        assert_eq!(fr_sig(1e-7, 4), "0,0000001");
        assert_eq!(fr(-0.001, 0, 1), "-0");
        assert_eq!(fr(1.45, 0, 1), "1,5");
    }
}

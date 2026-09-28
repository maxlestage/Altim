//! Reading upstream JSON the way the TypeScript parsers do: `Number(x)`, `String(x)`, truthiness, optional chaining,
//! and the TypeErrors JavaScriptCore throws on a malformed response (their messages end up in the "sources" lists).
//! `None` stands for `undefined`.
use serde_json::Value;

use crate::http::{Error, Result};
use crate::js::number_to_string;

pub type V<'a> = Option<&'a Value>;

/// `v.key` / `v?.key`: `undefined` on anything but an object that has the key.
pub fn get<'a>(v: V<'a>, key: &str) -> V<'a> {
    v.and_then(|v| v.as_object()).and_then(|o| o.get(key))
}

/// `v[i]` / `v?.[i]` on an array.
pub fn idx(v: V<'_>, i: usize) -> V<'_> {
    v.and_then(|v| v.as_array()).and_then(|a| a.get(i))
}

/// `v == null`.
pub fn nullish(v: V) -> bool {
    matches!(v, None | Some(Value::Null))
}

/// `a ?? b`.
pub fn or<'a>(a: V<'a>, b: V<'a>) -> V<'a> {
    if nullish(a) { b } else { a }
}

/// JavaScript truthiness.
pub fn truthy(v: V) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// A number is truthy (neither 0 nor NaN).
pub fn truthy_num(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

fn js_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

/// `Number(s)` for a string: trimmed, "" → 0, decimal / 0x / 0o / 0b literals, ±Infinity, anything else NaN.
pub fn string_to_number(s: &str) -> f64 {
    let s = s.trim_matches(js_whitespace);
    if s.is_empty() {
        return 0.0;
    }
    match s {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = s.strip_prefix(prefix) {
            if digits.is_empty() {
                return f64::NAN;
            }
            let mut x = 0.0f64;
            for c in digits.chars() {
                match c.to_digit(radix) {
                    Some(d) => x = x * radix as f64 + d as f64,
                    None => return f64::NAN,
                }
            }
            return x;
        }
    }
    // StrDecimalLiteral: [+-] (digits [. digits] | . digits) [e [+-] digits]
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut mantissa_digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        mantissa_digits += i - frac_start;
    }
    if mantissa_digits == 0 {
        return f64::NAN;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return f64::NAN;
        }
    }
    if i != b.len() {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// `Number(v)`.
pub fn number(v: V) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => string_to_number(s),
        Some(Value::Array(a)) => match a.len() {
            0 => 0.0,
            1 => string_to_number(&to_string(Some(&a[0]))),
            _ => f64::NAN,
        },
        Some(Value::Object(_)) => f64::NAN,
    }
}

/// `String(v)`.
pub fn to_string(v: V) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => number_to_string(n.as_f64().unwrap_or(f64::NAN)),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a.iter().map(|x| if x.is_null() { String::new() } else { to_string(Some(x)) }).collect::<Vec<_>>().join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}

/// Strict equality with a string literal (`v === "0"`).
pub fn is_str(v: V, s: &str) -> bool {
    v.and_then(|v| v.as_str()) == Some(s)
}

/// Strict equality with a number literal (`v === 0`).
pub fn is_num(v: V, x: f64) -> bool {
    v.and_then(|v| v.as_f64()) == Some(x)
}

/// A property read on `v` that throws on null / undefined, as `evaluating` says.
pub fn need(v: V, evaluating: &str) -> Result<()> {
    match v {
        None => Err(Error(format!("undefined is not an object (evaluating '{evaluating}')"))),
        Some(Value::Null) => Err(Error(format!("null is not an object (evaluating '{evaluating}')"))),
        Some(_) => Ok(()),
    }
}

/// Receiver of an array method call: `expr.method(...)` (`call` is the full call text JavaScriptCore quotes).
pub fn array<'a>(v: V<'a>, expr: &str, method: &str, call: &str) -> Result<&'a [Value]> {
    need(v, &format!("{expr}.{method}"))?;
    match v {
        Some(Value::Array(a)) => Ok(a),
        _ => Err(Error(format!("{expr}.{method} is not a function. (In '{call}', '{expr}.{method}' is undefined)"))),
    }
}

/// Array or `[]` (for `(x ?? []).map(...)` where x is known to be nullish or an array).
pub fn items(v: V<'_>) -> &[Value] {
    v.and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}

/// Characters `encodeURIComponent` leaves as they are: A–Z a–z 0–9 - _ . ! ~ * ' ( ).
const URI_COMPONENT: &percent_encoding::AsciiSet =
    &percent_encoding::NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'!').remove(b'~').remove(b'*').remove(b'\'').remove(b'(').remove(b')');

/// `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, URI_COMPONENT).to_string()
}

/// `s.slice(0, n)` (UTF-16 units, as JavaScript counts them).
pub fn slice_utf16(s: &str, n: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units + c.len_utf16() > n {
            return &s[..i];
        }
        units += c.len_utf16();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn number_like_js() {
        let n = |v: Value| number(Some(&v));
        assert_eq!(n(json!(" 12 ")), 12.0);
        assert_eq!(n(json!("")), 0.0);
        assert_eq!(n(json!("1.")), 1.0);
        assert_eq!(n(json!(".5e1")), 5.0);
        assert_eq!(n(json!("0x1F")), 31.0);
        assert!(n(json!("-0x1F")).is_nan());
        assert!(n(json!("1_0")).is_nan());
        assert!(n(json!("inf")).is_nan());
        assert!(n(json!("NaN")).is_nan());
        assert_eq!(n(json!("-Infinity")), f64::NEG_INFINITY);
        assert_eq!(n(json!(null)), 0.0);
        assert_eq!(n(json!(true)), 1.0);
        assert_eq!(n(json!([])), 0.0);
        assert_eq!(n(json!(["7"])), 7.0);
        assert!(n(json!({})).is_nan());
        assert!(number(None).is_nan());
        assert_eq!(to_string(Some(&json!(1.5))), "1.5");
        assert_eq!(to_string(None), "undefined");
    }
}

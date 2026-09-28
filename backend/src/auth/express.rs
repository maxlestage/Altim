//! What Express 5 (and Node's HTTP layer) did around the auth routes, so the Rust responses keep the same bytes:
//! `res.send` / `res.json` (Content-Type with charset, Content-Length, weak ETag, 304 when fresh, no body for HEAD),
//! `res.redirect` (Location through `encodeurl`, body negotiated on `Accept`, `Vary: Accept`), `req.ip` with
//! `trust proxy = 1`, header values as Node reads them (latin1, duplicates joined), `req.query` (Node
//! `querystring`), `express.urlencoded({ extended: false })` bodies (qs, depth 0) and the URI codecs of JavaScript.
//!
//! Reusable by the other routes of the port (`/api/*` responses, the HTTPS redirect, the per-IP counters).
use std::collections::HashMap;
use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, header};
use axum::response::Response;
use base64::Engine;
use indexmap::IndexMap;
use serde_json::Value;
use sha1::{Digest, Sha1};

// ---------- Strings as JavaScript sees them ----------

/// `String.prototype.length`: UTF-16 code units.
pub fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// JavaScript `WhiteSpace` and `LineTerminator` (what `trim()` and `\s` remove).
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `String.prototype.trim()`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// Bytes read as latin1, the way Node exposes header values.
pub fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Headers Node keeps only once (the first) when a request repeats them.
const SINGLE: [&str; 17] = [
    "age",
    "authorization",
    "content-length",
    "content-type",
    "etag",
    "expires",
    "from",
    "host",
    "if-modified-since",
    "if-unmodified-since",
    "last-modified",
    "location",
    "max-forwards",
    "proxy-authorization",
    "referer",
    "retry-after",
    "user-agent",
];

/// `req.headers[name]` in Node: latin1, first value for the single-valued headers, `; ` between cookies, `, ` otherwise.
pub fn node_header(headers: &HeaderMap, name: &str) -> Option<String> {
    let mut values = headers.get_all(name).iter().map(|v| latin1(v.as_bytes()));
    let first = values.next()?;
    if SINGLE.contains(&name) {
        return Some(first);
    }
    let sep = if name == "cookie" { "; " } else { ", " };
    Some(values.fold(first, |acc, v| acc + sep + &v))
}

// ---------- URI codecs ----------

/// `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    encode_with(s, |b| b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b))
}

/// `encodeURI`.
pub fn encode_uri(s: &str) -> String {
    encode_with(s, |b| b.is_ascii_alphanumeric() || b"-_.!~*'();/?:@&=+$,#".contains(&b))
}

fn encode_with(s: &str, keep: impl Fn(u8) -> bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if keep(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

/// `decodeURIComponent`: None where JavaScript throws `URIError` (bad escape, invalid UTF-8).
pub fn decode_uri_component(s: &str) -> Option<String> {
    if !s.contains('%') {
        return Some(s.to_string());
    }
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let byte = |at: usize| -> Option<u8> {
            if bytes.get(at) != Some(&b'%') {
                return None;
            }
            Some(hex_val(*bytes.get(at + 1)?)? << 4 | hex_val(*bytes.get(at + 2)?)?)
        };
        let b = byte(i)?;
        i += 3;
        if b < 0x80 {
            out.push(b);
            continue;
        }
        let n = match b {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => return None,
        };
        let mut seq = vec![b];
        for _ in 1..n {
            let c = byte(i)?;
            if c & 0xc0 != 0x80 {
                return None;
            }
            seq.push(c);
            i += 3;
        }
        std::str::from_utf8(&seq).ok()?;
        out.extend_from_slice(&seq);
    }
    String::from_utf8(out).ok()
}

/// Node `querystring.unescape` (after `+` → space): `decodeURIComponent`, or byte by byte when it throws.
fn qs_unescape(s: &str) -> String {
    if let Some(d) = decode_uri_component(s) {
        return d;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `encodeurl` (what `res.location` applies): encodes what cannot appear in a URL, keeps valid `%XX` escapes.
pub fn encode_url(url: &str) -> String {
    let allowed = |c: char| matches!(c, '\x21' | '\x23'..='\x3b' | '\x3d' | '\x3f'..='\x5f' | '\x61'..='\x7a' | '\x7c' | '\x7e');
    let chars: Vec<char> = url.chars().collect();
    let hex = |c: Option<&char>| c.is_some_and(|c| c.is_ascii_hexdigit());
    let mut out = String::with_capacity(url.len());
    let mut i = 0;
    let enc = |c: char| encode_uri(c.encode_utf8(&mut [0; 4]));
    while i < chars.len() {
        let c = chars[i];
        if c == '%' {
            let (n1, n2) = (chars.get(i + 1), chars.get(i + 2));
            match (n1, n2) {
                // "%" at the end.
                (None, _) => {
                    out.push_str("%25");
                    i += 1;
                }
                // "%" + non-hex.
                (Some(&a), _) if !a.is_ascii_hexdigit() => {
                    out.push_str("%25");
                    out.push_str(&enc(a));
                    i += 2;
                }
                // "%" + hex + non-hex.
                (Some(&a), Some(&b)) if !hex(Some(&b)) => {
                    out.push_str("%25");
                    out.push(a);
                    out.push_str(&enc(b));
                    i += 3;
                }
                // "%XX", or "%X" at the end: left as is.
                _ => {
                    out.push('%');
                    i += 1;
                }
            }
        } else if allowed(c) {
            out.push(c);
            i += 1;
        } else {
            out.push_str(&enc(c));
            i += 1;
        }
    }
    out
}

/// `escape-html`.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

// ---------- Parsed fields (req.query, req.body) ----------

/// A query or form field: one string, or an array when the key is repeated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    One(String),
    Many(Vec<String>),
}

impl Field {
    /// `typeof v === "string" ? v : undefined`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Field::One(s) => Some(s),
            Field::Many(_) => None,
        }
    }
    /// `String(v)` (an array joins with commas).
    pub fn js_string(&self) -> String {
        match self {
            Field::One(s) => s.clone(),
            Field::Many(v) => v.join(","),
        }
    }
    fn push(&mut self, v: String) {
        match self {
            Field::One(s) => *self = Field::Many(vec![std::mem::take(s), v]),
            Field::Many(list) => list.push(v),
        }
    }
}

pub type Fields = IndexMap<String, Field>;

/// `req.query` of Express 5 (query parser "simple" = Node `querystring.parse`, 1 000 keys at most).
pub fn parse_query(query: Option<&str>) -> Fields {
    let mut out = Fields::new();
    let Some(q) = query else { return out };
    for pair in q.split('&').filter(|p| !p.is_empty()).take(1000) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let k = qs_unescape(&k.replace('+', " "));
        let v = qs_unescape(&v.replace('+', " "));
        match out.get_mut(&k) {
            Some(f) => f.push(v),
            None => {
                out.insert(k, Field::One(v));
            }
        }
    }
    out
}

/// qs `utils.decode`: `+` → space, then `decodeURIComponent` (the raw text when it throws); `%XX` bytes as
/// characters for iso-8859-1.
fn qs_decode(s: &str, latin: bool) -> String {
    let s = s.replace('+', " ");
    if latin {
        let b = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' && i + 2 < b.len() {
                if let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                    out.push((h << 4 | l) as char);
                    i += 3;
                    continue;
                }
            }
            let c = s[i..].chars().next().unwrap_or('\0');
            out.push(c);
            i += c.len_utf8();
        }
        return out;
    }
    decode_uri_component(&s).unwrap_or(s)
}

/// Why a body was refused (`http-errors` status of body-parser).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyError(pub u16);

/// `express.urlencoded({ extended: false, limit })` on a request: None when the body is not a form (req.body stays
/// undefined), the fields otherwise, or the error status body-parser gives (413 too large / too many parameters,
/// 415 charset or content encoding, 400 unreadable).
pub async fn urlencoded(req: Request<Body>, limit: usize) -> Result<Option<Fields>, BodyError> {
    let headers = req.headers();
    let has_body = headers.contains_key(header::TRANSFER_ENCODING)
        || node_header(headers, "content-length").is_some_and(|v| {
            let v = js_trim(&v);
            v.is_empty() || v.parse::<f64>().is_ok()
        });
    if !has_body {
        return Ok(None);
    }
    let Some(ct) = node_header(headers, "content-type") else { return Ok(None) };
    let (media, params) = parse_media_type(&ct);
    if media != "application/x-www-form-urlencoded" {
        return Ok(None);
    }
    let charset = params.get("charset").map(|c| c.to_lowercase()).unwrap_or_else(|| "utf-8".into());
    if charset != "utf-8" && charset != "iso-8859-1" {
        return Err(BodyError(415));
    }
    let encoding = node_header(headers, "content-encoding").unwrap_or_else(|| "identity".into()).to_lowercase();
    if encoding != "identity" {
        // body-parser inflates gzip / deflate / br; browsers never compress a form, so they are refused here.
        return Err(BodyError(415));
    }
    if let Some(len) = node_header(headers, "content-length").and_then(|v| v.trim().parse::<usize>().ok()) {
        if len > limit {
            return Err(BodyError(413));
        }
    }
    let bytes = axum::body::to_bytes(req.into_body(), limit).await.map_err(|e| {
        // http-body-util's LengthLimitError ("length limit exceeded") somewhere in the chain: too large.
        let mut src: Option<&(dyn std::error::Error + 'static)> = Some(&e);
        let mut too_large = false;
        while let Some(s) = src {
            too_large |= s.to_string().contains("length limit exceeded");
            src = s.source();
        }
        BodyError(if too_large { 413 } else { 400 })
    })?;
    let latin = charset == "iso-8859-1";
    let text = if latin { latin1(&bytes) } else { String::from_utf8_lossy(&bytes).into_owned() };
    parse_form(&text, latin).map(Some)
}

/// qs.parse(body, { depth: 0, allowPrototypes: true, parameterLimit: 1000 }) as body-parser 2 calls it.
pub fn parse_form(body: &str, latin: bool) -> Result<Fields, BodyError> {
    let mut out = Fields::new();
    if body.is_empty() {
        return Ok(out);
    }
    if body.matches('&').count() + 1 > 1000 {
        return Err(BodyError(413));
    }
    let clean = replace_ci(&replace_ci(body, "%5B", "["), "%5D", "]");
    for part in clean.split('&') {
        let pos = match part.find("]=") {
            Some(p) => Some(p + 1),
            None => part.find('='),
        };
        let (key, val) = match pos {
            None => (qs_decode(part, latin), String::new()),
            Some(p) => (qs_decode(&part[..p], latin), qs_decode(&part[p + 1..], latin)),
        };
        // parseKeys, depth 0: the whole key; "[x]" is read as "x"; "" and "__proto__" are dropped.
        if key.is_empty() {
            continue;
        }
        let key = if key.len() >= 2 && key.starts_with('[') && key.ends_with(']') && key != "[]" { key[1..key.len() - 1].to_string() } else { key };
        if key == "__proto__" {
            continue;
        }
        match out.get_mut(&key) {
            Some(f) => f.push(val),
            None => {
                out.insert(key, Field::One(val));
            }
        }
    }
    Ok(out)
}

fn replace_ci(s: &str, pat: &str, with: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let pat = pat.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for (i, _) in lower.match_indices(&pat) {
        out.push_str(&s[last..i]);
        out.push_str(with);
        last = i + pat.len();
    }
    out.push_str(&s[last..]);
    out
}

// ---------- Media types and negotiation (content-type 2, negotiator 1) ----------

/// `content-type` parse: lowercased type and parameters (first occurrence wins), stopping at `,` when `comma`.
fn parse_content_type(h: &[char], start: usize, comma: bool) -> (String, HashMap<String, String>, usize) {
    let stop = |c: char| comma && c == ',';
    let len = h.len();
    let skip_ows = |mut i: usize| {
        while i < len && (h[i] == ' ' || h[i] == '\t') {
            i += 1;
        }
        i
    };
    let trailing = |start: usize, mut end: usize| {
        while end > start && (h[end - 1] == ' ' || h[end - 1] == '\t') {
            end -= 1;
        }
        end
    };
    let skip_value = |mut i: usize| {
        while i < len && h[i] != ';' && !stop(h[i]) {
            i += 1;
        }
        i
    };
    let mut index = skip_ows(start);
    let value_start = index;
    index = skip_value(index);
    let ty: String = h[value_start..trailing(value_start, index)].iter().collect::<String>().to_lowercase();
    let mut params = HashMap::new();
    'parameter: while index < len {
        if stop(h[index]) {
            break;
        }
        index = skip_ows(index + 1);
        let key_start = index;
        while index < len {
            let c = h[index];
            if stop(c) {
                break 'parameter;
            }
            if c == ';' {
                continue 'parameter;
            }
            if c == '=' {
                let key: String = h[key_start..trailing(key_start, index)].iter().collect::<String>().to_lowercase();
                index = skip_ows(index + 1);
                if index < len && h[index] == '"' {
                    index += 1;
                    let mut value = String::new();
                    while index < len {
                        let c = h[index];
                        index += 1;
                        if c == '"' {
                            index = skip_value(index);
                            params.entry(key.clone()).or_insert(value.clone());
                            break;
                        }
                        if c == '\\' && index < len {
                            value.push(h[index]);
                            index += 1;
                            continue;
                        }
                        value.push(c);
                    }
                    continue 'parameter;
                }
                let vs = index;
                index = skip_value(index);
                params.entry(key).or_insert_with(|| h[vs..trailing(vs, index)].iter().collect());
                continue 'parameter;
            }
            index += 1;
        }
    }
    (ty, params, index)
}

/// Media type (lowercase, without parameters) and parameters of a Content-Type.
pub fn parse_media_type(ct: &str) -> (String, HashMap<String, String>) {
    let chars: Vec<char> = ct.chars().collect();
    let (t, p, _) = parse_content_type(&chars, 0, false);
    (t, p)
}

/// JavaScript `parseFloat`.
pub fn js_parse_float(s: &str) -> f64 {
    let s = s.trim_start_matches(is_js_space);
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if s[i..].starts_with("Infinity") {
        return if b.first() == Some(&b'-') { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        let mut j = i + 1;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        digits += j - i - 1;
        if digits > 0 {
            i = j;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let ds = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > ds {
            i = j;
        }
    }
    s[..i].parse().unwrap_or(f64::NAN)
}

struct Spec {
    ty: String,
    subtype: String,
    params: HashMap<String, String>,
    q: f64,
    i: usize,
}

fn parse_accept(header: &str) -> Vec<Spec> {
    let h: Vec<char> = header.chars().collect();
    let mut out = Vec::new();
    let mut index = 0;
    let mut i = 0;
    while index < h.len() {
        let mut start = index;
        while start < h.len() && (h[start] == ' ' || h[start] == '\t') {
            start += 1;
        }
        let (ty, mut params, end) = parse_content_type(&h, start, true);
        // Original casing of the type.
        let ty: String = h[start..(start + ty.chars().count()).min(h.len())].iter().collect();
        index = end + 1;
        let idx = i;
        i += 1;
        let Some(slash) = ty.find('/') else { continue };
        let q = match params.remove("q") {
            Some(q) if !q.is_empty() => js_parse_float(&q),
            _ => 1.0,
        };
        out.push(Spec { ty: ty[..slash].to_string(), subtype: ty[slash + 1..].to_string(), params, q, i: idx });
    }
    out
}

/// Which of `text/plain` and `text/html` `req.accepts(["text", "html"])` picks (None = neither is acceptable).
pub fn accepts_text_or_html(headers: &HeaderMap) -> Option<&'static str> {
    let accept = node_header(headers, "accept").unwrap_or_default();
    if accept.is_empty() {
        return Some("text");
    }
    let specs = parse_accept(&accept);
    // (q, s, o, i) of each provided type.
    let provided = [("text", "plain"), ("text", "html")];
    let mut best: Option<(f64, i32, i64, usize)> = None;
    for (index, (t, sub)) in provided.iter().enumerate() {
        let mut pr = (0.0f64, 0i32, -1i64);
        for spec in &specs {
            let mut s = 0;
            if spec.ty.to_lowercase() == *t {
                s |= 4;
            } else if spec.ty != "*" {
                continue;
            }
            if spec.subtype.to_lowercase() == *sub {
                s |= 2;
            } else if spec.subtype != "*" {
                continue;
            }
            if !spec.params.is_empty() {
                if spec.params.values().all(|v| v == "*" || v.is_empty()) {
                    s |= 1;
                } else {
                    continue;
                }
            }
            let cand = (spec.q, s, spec.i as i64);
            // (priority.s - spec.s || priority.q - spec.q || priority.o - spec.o) < 0
            let d = if pr.1 != cand.1 {
                (pr.1 - cand.1) as f64
            } else {
                let dq = pr.0 - cand.0;
                if dq != 0.0 && !dq.is_nan() { dq } else { (pr.2 - cand.2) as f64 }
            };
            if d < 0.0 {
                pr = cand;
            }
        }
        if pr.0 > 0.0 {
            let c = (pr.0, pr.1, pr.2, index);
            // compareSpecs: q desc, s desc, o asc, i asc.
            let better = match best {
                None => true,
                Some(b) => {
                    let dq = b.0 - c.0;
                    if dq != 0.0 && !dq.is_nan() {
                        dq < 0.0
                    } else if b.1 != c.1 {
                        c.1 > b.1
                    } else if b.2 != c.2 {
                        c.2 < b.2
                    } else {
                        c.3 < b.3
                    }
                }
            };
            if better {
                best = Some(c);
            }
        }
    }
    best.map(|b| ["text", "html"][b.3])
}

// ---------- Responses ----------

/// Weak ETag of Express (`etag` package): `W/"<length in hex>-<27 first chars of base64(sha1)>"`.
pub fn weak_etag(body: &[u8]) -> String {
    let hash = base64::engine::general_purpose::STANDARD.encode(Sha1::digest(body));
    format!("W/\"{:x}-{}\"", body.len(), &hash[..27])
}

/// `fresh` (conditional GET) with the response ETag.
fn fresh(req: &HeaderMap, etag: &str) -> bool {
    let modified_since = node_header(req, "if-modified-since");
    let none_match = node_header(req, "if-none-match");
    if modified_since.as_deref().is_none_or(str::is_empty) && none_match.as_deref().is_none_or(str::is_empty) {
        return false;
    }
    if let Some(cc) = node_header(req, "cache-control") {
        // /(?:^|,)\s*?no-cache\s*?(?:,|$)/
        if cc.split(',').any(|part| part.trim_matches(is_js_space) == "no-cache") {
            return false;
        }
    }
    if let Some(nm) = none_match.as_deref().filter(|s| !s.is_empty()) {
        if nm == "*" {
            return true;
        }
        let matched = nm.split([' ', ',']).filter(|t| !t.is_empty()).any(|m| m == etag || format!("W/{m}") == etag || m == format!("W/{etag}"));
        if !matched {
            return false;
        }
    }
    // No Last-Modified on these responses: an If-Modified-Since cannot be satisfied.
    modified_since.as_deref().is_none_or(str::is_empty)
}

/// Response under construction, like Express' `res` (headers set before the body is sent).
pub struct Res {
    pub status: StatusCode,
    pub headers: HeaderMap,
}

impl Default for Res {
    fn default() -> Self {
        Res { status: StatusCode::OK, headers: HeaderMap::new() }
    }
}

impl Res {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn status(mut self, status: u16) -> Self {
        self.status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        self
    }
    /// `res.setHeader(name, value)`.
    pub fn set(mut self, name: &'static str, value: &str) -> Self {
        if let Ok(v) = HeaderValue::from_str(value) {
            self.headers.insert(HeaderName::from_static(name), v);
        }
        self
    }

    /// `res.type(<type>).send(string)`: `content_type` without charset ("text/html", "text/plain",
    /// "application/json"); "; charset=utf-8" is added like Express does.
    pub fn send(mut self, method: &Method, req: &HeaderMap, content_type: &str, body: impl Into<String>) -> Response {
        let body: String = body.into();
        self.headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&format!("{content_type}; charset=utf-8")).unwrap());
        self.headers.insert(header::CONTENT_LENGTH, HeaderValue::from(body.len()));
        let etag = weak_etag(body.as_bytes());
        self.headers.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
        let mut body = body;
        if (*method == Method::GET || *method == Method::HEAD)
            && (self.status.is_success() || self.status == StatusCode::NOT_MODIFIED)
            && fresh(req, &etag)
        {
            self.status = StatusCode::NOT_MODIFIED;
        }
        if self.status == StatusCode::NO_CONTENT || self.status == StatusCode::NOT_MODIFIED {
            self.headers.remove(header::CONTENT_TYPE);
            self.headers.remove(header::CONTENT_LENGTH);
            self.headers.remove(header::TRANSFER_ENCODING);
            body.clear();
        }
        if *method == Method::HEAD {
            body.clear();
        }
        self.finish(body)
    }

    /// `res.json(value)`.
    pub fn json(self, method: &Method, req: &HeaderMap, value: &Value) -> Response {
        self.send(method, req, "application/json", value.to_string())
    }

    /// `res.redirect(status, url)`: Location through `encodeurl`, a short body in the negotiated format.
    pub fn redirect(mut self, method: &Method, req: &HeaderMap, status: u16, url: &str) -> Response {
        let address = encode_url(url);
        self.headers.insert(header::LOCATION, HeaderValue::from_str(&address).unwrap_or(HeaderValue::from_static("/")));
        self = self.status(status);
        let message = self.status.canonical_reason().unwrap_or("");
        self.headers.append(header::VARY, HeaderValue::from_static("Accept"));
        let body = match accepts_text_or_html(req) {
            Some("text") => {
                self.headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
                format!("{message}. Redirecting to {address}")
            }
            Some(_) => {
                self.headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
                format!("<p>{message}. Redirecting to {}</p>", escape_html(&address))
            }
            None => String::new(),
        };
        self.headers.insert(header::CONTENT_LENGTH, HeaderValue::from(body.len()));
        self.finish(if *method == Method::HEAD { String::new() } else { body })
    }

    fn finish(self, body: String) -> Response {
        let mut res = Response::new(Body::from(body));
        *res.status_mut() = self.status;
        *res.headers_mut() = self.headers;
        res
    }
}

/// The error handler of app.ts for an HTTP-layer error (body too large, bad charset…): its status, a generic text.
pub fn request_error(method: &Method, req: &HeaderMap, status: u16) -> Response {
    Res::new().status(status).json(method, req, &serde_json::json!({ "error": "requête invalide" }))
}

// ---------- Client address ----------

/// `req.ip` with `app.set("trust proxy", 1)` (the Heroku router): the right-most `X-Forwarded-For` entry, else the
/// socket address (`::ffff:1.2.3.4` for IPv4 on a dual-stack listener, as Node reports it). None when neither is known
/// (the TypeScript then used "?"). The socket address comes from `ConnectInfo<SocketAddr>`: serve with
/// `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn client_ip<B>(req: &Request<B>) -> Option<String> {
    ip_from(req.headers(), req.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0))
}

pub fn ip_from(headers: &HeaderMap, socket: Option<SocketAddr>) -> Option<String> {
    if let Some(xff) = node_header(headers, "x-forwarded-for") {
        // `forwarded`: entries split on commas, spaces trimmed, empty ones skipped; the last one is the closest.
        if let Some(last) = xff.split(',').map(|s| s.trim_matches(' ')).rfind(|s| !s.is_empty()) {
            return Some(last.to_string());
        }
    }
    socket.map(|s| match s.ip() {
        std::net::IpAddr::V6(v6) => v6.to_string(),
        std::net::IpAddr::V4(v4) => v4.to_string(),
    })
}

/// Path of the request as Express' `req.path` and the full target as `req.originalUrl` (the original URI when
/// the router is nested).
pub fn path_and_original<B>(req: &Request<B>) -> (String, String) {
    let uri = req.extensions().get::<axum::extract::OriginalUri>().map(|o| o.0.clone()).unwrap_or_else(|| req.uri().clone());
    let path = uri.path().to_string();
    let original = uri.path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| path.clone());
    (path, original)
}

/// Express route matching (case-insensitive, one optional trailing slash).
pub fn route_is(path: &str, route: &str) -> bool {
    let p = path.strip_suffix('/').filter(|p| !p.is_empty()).unwrap_or(path);
    p.eq_ignore_ascii_case(route)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_codecs() {
        assert_eq!(encode_uri_component("/app/actif?x=1&y=é"), "%2Fapp%2Factif%3Fx%3D1%26y%3D%C3%A9");
        assert_eq!(decode_uri_component("%E0%A4%A"), None);
        assert_eq!(decode_uri_component("a%20b%C3%A9"), Some("a bé".into()));
        assert_eq!(decode_uri_component("%ED%A0%80"), None);
        assert_eq!(encode_url("/app%zz"), "/app%25zz");
        assert_eq!(encode_url("/a%4"), "/a%4");
        assert_eq!(encode_url("/a%4z b"), "/a%254z%20b");
        assert_eq!(encode_url("/ok%20?x=1"), "/ok%20?x=1");
        assert_eq!(encode_url("%%41"), "%25%2541");
    }

    #[test]
    fn query_and_form() {
        let q = parse_query(Some("next=%2Fapp&next=b&x=%zz%41&y=a+b&z"));
        assert_eq!(q["next"], Field::Many(vec!["/app".into(), "b".into()]));
        assert_eq!(q["x"], Field::One("%zzA".into()));
        assert_eq!(q["y"], Field::One("a b".into()));
        assert_eq!(q["z"], Field::One("".into()));
        let f = parse_form("user=max&password=a%2Bb+c&code=1&code=2&bad=%E0%A4%A+x", false).unwrap();
        assert_eq!(f["password"].js_string(), "a+b c");
        assert_eq!(f["code"].js_string(), "1,2");
        assert_eq!(f["bad"].js_string(), "%E0%A4%A x");
        assert_eq!(parse_form(&"a&".repeat(1000), false), Err(BodyError(413)));
    }

    #[test]
    fn negotiation() {
        let h = |v: &str| {
            let mut m = HeaderMap::new();
            m.insert("accept", HeaderValue::from_str(v).unwrap());
            m
        };
        assert_eq!(accepts_text_or_html(&HeaderMap::new()), Some("text"));
        assert_eq!(accepts_text_or_html(&h("text/html,application/xhtml+xml,*/*;q=0.8")), Some("html"));
        assert_eq!(accepts_text_or_html(&h("*/*")), Some("text"));
        assert_eq!(accepts_text_or_html(&h("application/json")), None);
        assert_eq!(accepts_text_or_html(&h("text/*;q=0.5, text/html;q=0.4")), Some("text"));
    }

    #[test]
    fn etag() {
        assert_eq!(weak_etag(b""), "W/\"0-2jmj7l5rSw0yVb/vlWAYkK/YBwk\"");
    }
}

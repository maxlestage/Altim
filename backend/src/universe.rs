//! Full asset universe (port of `web/server/universe.ts`): every crypto listed against USD/USDT on the exchanges
//! Altim reads, and every stock / ETF listed in the United States (official Nasdaq Trader directory).
//! Lists are fetched from the sources themselves and cached, so nothing is hard-coded.
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use icu_normalizer::DecomposingNormalizerBorrowed;
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::express::{is_js_space, js_trim};
use crate::cache::cached;
use crate::http::{self, Error, Result};
use crate::js::number_to_string;
use crate::types::Kind;

/// Compact entry: symbol, name, rank (market cap order, 0 = unknown), flag (crypto: number of exchanges; stock:
/// 1 = ETF). Serialized as the JSON array `[symbol, name, rank, flag]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UniverseEntry(pub String, pub String, pub i64, pub i64);

/// A coin of the CoinGecko markets pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeckoCoin {
    pub symbol: String,
    pub name: String,
    pub rank: i64,
}

/// A line of the Nasdaq Trader directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub symbol: String,
    pub name: String,
    pub etf: bool,
}

const DAY: i64 = 86_400_000;
const ACCEPT: &str = "application/json, text/plain";

async fn get_text(url: &str) -> Result<String> {
    http::get_text_with(url, &[("Accept", ACCEPT)], Duration::from_secs(15)).await
}

async fn get_json(url: &str) -> Result<Value> {
    let text = get_text(url).await?;
    serde_json::from_str(&text).map_err(|_| Error("JSON invalide".into()))
}

// ---------- JavaScript values ----------

/// Where the JavaScript would throw (property of null, `.filter` of a non-array…): the caller then uses its
/// fallback, like `settled(…)` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Throws;

/// `x.key` where reading a property of null / undefined throws.
fn prop<'a>(x: Option<&'a Value>, key: &str) -> std::result::Result<Option<&'a Value>, Throws> {
    match x {
        None | Some(Value::Null) => Err(Throws),
        Some(Value::Object(o)) => Ok(o.get(key)),
        Some(_) => Ok(None),
    }
}

/// `x?.key`.
fn opt<'a>(x: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    prop(x, key).ok().flatten()
}

/// `String(v)` (None = undefined).
fn js_string(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => number_to_string(n.as_f64().unwrap_or(f64::NAN)),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a.iter().map(|x| if x.is_null() { String::new() } else { js_string(Some(x)) }).collect::<Vec<_>>().join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}

/// `v ?? ""` then String.
fn js_string_or_empty(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        v => js_string(v),
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn is(v: Option<&Value>, s: &str) -> bool {
    v.and_then(Value::as_str) == Some(s)
}

/// `Number(string)`.
fn js_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if t.len() > 2 && t[..2].eq_ignore_ascii_case(prefix) {
            let digits = &t[2..];
            return if digits.chars().all(|c| c.is_digit(radix)) {
                digits.chars().fold(0.0, |acc, c| acc * radix as f64 + c.to_digit(radix).unwrap() as f64)
            } else {
                f64::NAN
            };
        }
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    static DEC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$").unwrap());
    if DEC.is_match(t) { t.parse().unwrap_or(f64::NAN) } else { f64::NAN }
}

/// `Number(v)` for a JSON value.
fn js_number_of(v: Option<&Value>) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => js_number(s),
        Some(Value::Array(a)) if a.is_empty() => 0.0,
        Some(Value::Array(a)) if a.len() == 1 => js_number(&js_string(Some(&a[0]))),
        Some(_) => f64::NAN,
    }
}

fn items(v: Option<&Value>) -> &[Value] {
    v.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

// ---------- Parsers (formats checked against real responses; see tests/universe.rs) ----------

const CRYPTO_SYMBOL_MAX: usize = 12;

fn crypto_symbol(s: &str) -> bool {
    (1..=CRYPTO_SYMBOL_MAX).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// Kraken legacy codes.
fn kraken_alias(b: &str) -> &str {
    match b {
        "XBT" => "BTC",
        "XDG" => "DOGE",
        _ => b,
    }
}

/// `parseUniverse` of universe.ts. A parser that would throw in JavaScript returns `Err(Throws)`.
pub mod parse {
    use super::*;

    type R<T> = std::result::Result<T, Throws>;

    /// `(d?.data ?? []).filter(...)`: data must be an array (`.filter` of anything else throws).
    fn data_array(d: &Value) -> R<&[Value]> {
        match opt(Some(d), "data") {
            None | Some(Value::Null) => Ok(&[]),
            Some(Value::Array(a)) => Ok(a),
            Some(_) => Err(Throws),
        }
    }

    pub fn okx(d: &Value) -> R<Vec<String>> {
        let mut out = vec![];
        for x in data_array(d)? {
            if is(prop(Some(x), "quoteCcy")?, "USDT") && is(prop(Some(x), "state")?, "live") {
                out.push(js_string(prop(Some(x), "baseCcy")?));
            }
        }
        Ok(out)
    }

    pub fn coinbase(d: &Value) -> R<Vec<String>> {
        let mut out = vec![];
        for x in items(Some(d)) {
            let q = prop(Some(x), "quote_currency")?;
            if (is(q, "USD") || is(q, "USDT")) && is(prop(Some(x), "status")?, "online") && !truthy(prop(Some(x), "trading_disabled")?) {
                out.push(js_string(prop(Some(x), "base_currency")?));
            }
        }
        Ok(out)
    }

    pub fn kraken(d: &Value) -> R<Vec<String>> {
        let values: Vec<&Value> = match opt(Some(d), "result") {
            Some(Value::Object(o)) => o.values().collect(),
            Some(Value::Array(a)) => a.iter().collect(),
            _ => vec![],
        };
        let mut out = vec![];
        for x in values {
            let w = js_string_or_empty(prop(Some(x), "wsname")?);
            if w.ends_with("/USD") || w.ends_with("/USDT") {
                out.push(kraken_alias(w.split('/').next().unwrap_or("")).to_string());
            }
        }
        Ok(out)
    }

    pub fn kucoin(d: &Value) -> R<Vec<String>> {
        let mut out = vec![];
        for x in data_array(d)? {
            if is(prop(Some(x), "quoteCurrency")?, "USDT") && truthy(prop(Some(x), "enableTrading")?) {
                out.push(js_string(prop(Some(x), "baseCurrency")?));
            }
        }
        Ok(out)
    }

    pub fn gate(d: &Value) -> R<Vec<String>> {
        let mut out = vec![];
        for x in items(Some(d)) {
            if is(prop(Some(x), "quote")?, "USDT") && is(prop(Some(x), "trade_status")?, "tradable") {
                out.push(js_string(prop(Some(x), "base")?));
            }
        }
        Ok(out)
    }

    /// Coinbase currency list: full names, used when CoinGecko is unavailable.
    pub fn coinbase_names(d: &Value) -> R<Vec<(String, String)>> {
        items(Some(d)).iter().map(|x| Ok((js_string(prop(Some(x), "id")?).to_uppercase(), js_string_or_empty(prop(Some(x), "name")?)))).collect()
    }

    pub fn kucoin_names(d: &Value) -> R<Vec<(String, String)>> {
        data_array(d)?
            .iter()
            .map(|x| Ok((js_string(prop(Some(x), "currency")?).to_uppercase(), js_string_or_empty(prop(Some(x), "fullName")?))))
            .collect()
    }

    /// CoinGecko full coin list: a name only when the ticker belongs to a single coin (BTC is shared by 12).
    pub fn gecko_names(d: &Value) -> R<Vec<(String, String)>> {
        let mut by_symbol: IndexMap<String, Vec<String>> = IndexMap::new();
        for x in items(Some(d)) {
            let sym = js_string_or_empty(prop(Some(x), "symbol")?).to_uppercase();
            if !sym.is_empty() {
                by_symbol.entry(sym).or_default().push(js_string_or_empty(prop(Some(x), "name")?));
            }
        }
        Ok(by_symbol.into_iter().filter(|(_, n)| n.len() == 1 && !n[0].is_empty()).map(|(s, mut n)| (s, n.remove(0))).collect())
    }

    /// CoinGecko markets page: names and market cap rank.
    pub fn gecko(d: &Value) -> R<Vec<GeckoCoin>> {
        let mut out = vec![];
        for x in items(Some(d)) {
            let rank = prop(Some(x), "market_cap_rank")?;
            if truthy(rank) {
                let r = js_number_of(rank);
                out.push(GeckoCoin {
                    symbol: js_string(prop(Some(x), "symbol")?).to_uppercase(),
                    name: js_string(prop(Some(x), "name")?),
                    rank: if r.is_finite() { r as i64 } else { 0 },
                });
            }
        }
        Ok(out)
    }

    /// nasdaqlisted.txt / otherlisted.txt (pipe separated, last line = file date).
    pub fn nasdaq_directory(text: &str) -> R<Vec<Listed>> {
        static EXCLUDED: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?i-u)(?-u:\b)(warrants?|rights?|units?|notes? due|subordinated|debentures?|preferred)(?-u:\b)").unwrap()
        });
        let text = js_trim(text);
        let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
        let header: Vec<&str> = lines[0].split('|').collect();
        let col = |name: &str| header.iter().position(|h| *h == name);
        let sym = col("Symbol").or_else(|| col("ACT Symbol"));
        let (name_col, etf_col, test_col) = (col("Security Name"), col("ETF"), col("Test Issue"));
        let mut out = vec![];
        for line in &lines[1..] {
            let f: Vec<&str> = line.split('|').collect();
            if f.len() < header.len() || test_col.is_some_and(|c| f[c] == "Y") {
                continue;
            }
            // f[-1] is undefined in JavaScript: `.trim()` on it throws.
            let raw = js_trim(f[sym.ok_or(Throws)?]);
            let name = js_trim(f[name_col.ok_or(Throws)?]);
            // Preferred shares ($), warrants, rights, units and notes are not stocks you "own" in a portfolio.
            if raw.is_empty() || raw.contains('$') || EXCLUDED.is_match(name) {
                continue;
            }
            let symbol = raw.replace('.', "-"); // Yahoo format: BRK.B → BRK-B
            let b = symbol.as_bytes();
            if !(1..=10).contains(&b.len()) || !b[0].is_ascii_uppercase() || !b[1..].iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'-') {
                continue;
            }
            out.push(Listed { symbol, name: clean_stock_name(name), etf: etf_col.is_some_and(|c| f[c] == "Y") });
        }
        Ok(out)
    }

    /// Nasdaq screener: market capitalisation, used to rank the stocks.
    pub fn screener(d: &Value) -> R<IndexMap<String, f64>> {
        let rows = match opt(opt(Some(d), "data"), "rows") {
            None | Some(Value::Null) => return Ok(IndexMap::new()),
            Some(Value::Array(a)) => a,
            Some(_) => return Err(Throws),
        };
        let mut out = IndexMap::new();
        for r in rows {
            let symbol = js_trim(&js_string(prop(Some(r), "symbol")?)).replace(['.', '/'], "-");
            let cap = js_number(&js_string_or_empty(prop(Some(r), "marketCap")?).replace(',', ""));
            let cap = if cap.is_nan() { 0.0 } else { cap };
            if cap > 0.0 {
                out.insert(symbol, cap);
            }
        }
        Ok(out)
    }
}

/// "Apple Inc. - Common Stock" → "Apple Inc."; "Alphabet Inc. - Class A Common Stock" → "Alphabet Inc. Class A".
pub fn clean_stock_name(name: &str) -> String {
    static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\t\n\x0B\x0C\r \x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}]+").unwrap());
    static CLASS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?-u:\b)Class [A-Z](?-u:\b)").unwrap());
    static SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
            // After the collapse above, `\s` can only be a space.
            r"(?i-u) *-? *(?-u:\b)(Class [A-Z] )?",
            r"(Common Stock|Ordinary Shares?|Common Shares?|American Depositary Shares?|Depositary Shares?|Shares of Beneficial Interest|Capital Stock)",
            r"(?-u:\b)(?su:.*)$"
        ))
        .unwrap()
    });
    let name = SPACES.replace_all(name, " ").into_owned();
    let cls = CLASS.find(&name).map(|m| m.as_str().to_string());
    let cut = SUFFIX.find(&name).map_or(name.as_str(), |m| &name[..m.start()]);
    let mut n = js_trim(cut.trim_end_matches(|c: char| is_js_space(c) || c == ',' || c == '-')).to_string();
    if let Some(cls) = cls {
        if !n.contains(&cls) {
            n.push(' ');
            n.push_str(&cls);
        }
    }
    if n.is_empty() { js_trim(&name).to_string() } else { n }
}

// ---------- Building ----------

/// Tokens that replicate a stock or an ETF are not cryptos: the stock itself is offered instead.
static TOKENIZED_STOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i-u)(?-u:\b)(xStocks?|tokeni[sz]ed)(?-u:\b)").unwrap());

/// Leveraged tokens (BTC3L, ETH5S…) are derivatives, not coins you hold: /^[A-Z0-9]{2,}[2-9][LS]$/.
fn leveraged(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 4 && matches!(b[b.len() - 1], b'L' | b'S') && (b'2'..=b'9').contains(&b[b.len() - 2]) && b[..b.len() - 2].iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

const STABLE_OR_FIAT: [&str; 14] = ["USDT", "USD", "USDC", "DAI", "FDUSD", "TUSD", "BUSD", "USDE", "PYUSD", "USDP", "EUR", "GBP", "EURC", "EURT"];

pub fn build_crypto(exchanges: &[Vec<String>], gecko: &[GeckoCoin], names: &[(String, String)]) -> Vec<UniverseEntry> {
    let mut count: IndexMap<String, i64> = IndexMap::new();
    for list in exchanges {
        let mut seen = HashSet::new();
        for b in list.iter().map(|s| s.to_uppercase()) {
            if seen.insert(b.clone()) && crypto_symbol(&b) && !STABLE_OR_FIAT.contains(&b.as_str()) && !leveraged(&b) {
                *count.entry(b).or_insert(0) += 1;
            }
        }
    }
    // Same ticker for several coins: the best ranked one wins (it is the one the exchanges list).
    let mut info: HashMap<&str, (String, i64)> = HashMap::new();
    for g in gecko {
        info.entry(&g.symbol).or_insert_with(|| (g.name.clone(), g.rank));
    }
    for (sym, name) in names {
        let clean = js_trim(name);
        if !clean.is_empty() && clean.to_uppercase() != *sym && !info.contains_key(sym.as_str()) {
            info.insert(sym, (clean.to_string(), 0));
        }
    }
    let mut out: Vec<UniverseEntry> = count
        .into_iter()
        .map(|(s, n)| {
            let (name, rank) = info.get(s.as_str()).cloned().unwrap_or_else(|| (s.clone(), 0));
            UniverseEntry(s, name, rank, n)
        })
        // Tokenized stocks (NVDAX "xStock", SPYON "Ondo Tokenized"): the real stock is in the stock list.
        .filter(|e| !TOKENIZED_STOCK.is_match(&e.1))
        .collect();
    out.sort_by(by_rank);
    out
}

/// Largest US ETFs by assets under management (the Nasdaq screener gives no size for ETFs):
/// they are listed right after the ranked stocks instead of alphabetically.
pub const POPULAR_ETFS: [&str; 54] = [
    "VOO", "IVV", "SPY", "VTI", "QQQ", "VUG", "VEA", "IEFA", "VTV", "BND", "AGG", "IWF", "GLD", "IEMG", "VXUS", "VGT", "IJH", "VWO", "VIG", "IJR", "SPLG",
    "XLK", "IWM", "SCHD", "VO", "RSP", "ITOT", "IBIT", "BNDX", "VB", "EFA", "IWD", "SCHX", "VYM", "TLT", "XLF", "SMH", "IAU", "SCHG", "QUAL", "IVW", "MUB",
    "VCIT", "SCHF", "VT", "XLV", "VNQ", "DIA", "IWR", "XLE", "ARKK", "SOXX", "SLV", "FBTC",
];

pub fn build_stocks(directories: &[Vec<Listed>], caps: &IndexMap<String, f64>) -> Vec<UniverseEntry> {
    let mut seen: IndexMap<&str, &Listed> = IndexMap::new();
    for dir in directories {
        for s in dir {
            seen.entry(&s.symbol).or_insert(s);
        }
    }
    let mut ranked: Vec<(&String, f64)> = caps.iter().filter(|(s, _)| seen.contains_key(s.as_str())).map(|(s, c)| (s, *c)).collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    let rank: HashMap<&str, i64> = ranked.iter().enumerate().map(|(i, (s, _))| (s.as_str(), i as i64 + 1)).collect();
    let popular = |s: &str| POPULAR_ETFS.iter().position(|p| *p == s);
    let mut out: Vec<UniverseEntry> =
        seen.iter().map(|(s, v)| UniverseEntry(s.to_string(), v.name.clone(), rank.get(s).copied().unwrap_or(0), i64::from(v.etf))).collect();
    out.sort_by(|a, b| {
        if a.2 != 0 || b.2 != 0 {
            return by_rank(a, b);
        }
        match (popular(&a.0), popular(&b.0)) {
            (Some(x), Some(y)) if x != y => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            _ => locale_compare(&a.0, &b.0),
        }
    });
    out
}

/// `localeCompare` on symbols ([A-Z0-9-], uppercase): the ICU root order equals the byte order there
/// ("-" < digits < letters).
fn locale_compare(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}

/// Ranked first (by rank), then crypto listed on more exchanges, then alphabetical.
fn by_rank(a: &UniverseEntry, b: &UniverseEntry) -> Ordering {
    if a.2 != 0 && b.2 != 0 {
        return a.2.cmp(&b.2);
    }
    if a.2 != 0 || b.2 != 0 {
        return if a.2 != 0 { Ordering::Less } else { Ordering::Greater };
    }
    b.3.cmp(&a.3).then_with(|| locale_compare(&a.0, &b.0))
}

/// CoinGecko limits bursts: pages are fetched one after the other, with one retry.
static LAST_GECKO: LazyLock<Mutex<Vec<GeckoCoin>>> = LazyLock::new(|| Mutex::new(vec![]));

async fn gecko_ranks() -> Vec<GeckoCoin> {
    let mut out = vec![];
    for page in 1..=4 {
        let url = format!("https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page={page}");
        let fetch = || async { get_json(&url).await.ok().and_then(|d| parse::gecko(&d).ok()) };
        let mut rows = fetch().await;
        if rows.is_none() {
            tokio::time::sleep(Duration::from_millis(2500)).await;
            rows = fetch().await;
        }
        let Some(rows) = rows else { break };
        out.extend(rows);
    }
    // A throttled refresh keeps the previous ranking rather than losing it.
    let mut last = LAST_GECKO.lock().unwrap();
    if out.len() >= last.len() {
        *last = out;
    }
    last.clone()
}

/// `settled(getJSON(url).then(parser), fallback)`.
async fn settled<T: Default>(url: &str, parser: fn(&Value) -> std::result::Result<T, Throws>) -> T {
    get_json(url).await.ok().and_then(|d| parser(&d).ok()).unwrap_or_default()
}

async fn settled_text<T: Default>(url: &str, parser: fn(&str) -> std::result::Result<T, Throws>) -> T {
    get_text(url).await.ok().and_then(|d| parser(&d).ok()).unwrap_or_default()
}

/// Every crypto (cached 6 hours).
pub async fn crypto_universe() -> Result<Arc<Vec<UniverseEntry>>> {
    cached("universe:crypto", DAY / 4, || async {
        let (okx, coinbase, kraken, kucoin, gate, cb_names, kc_names, gecko) = tokio::join!(
            settled("https://www.okx.com/api/v5/public/instruments?instType=SPOT", parse::okx),
            settled("https://api.exchange.coinbase.com/products", parse::coinbase),
            settled("https://api.kraken.com/0/public/AssetPairs", parse::kraken),
            settled("https://api.kucoin.com/api/v2/symbols", parse::kucoin),
            settled("https://api.gateio.ws/api/v4/spot/currency_pairs", parse::gate),
            settled("https://api.exchange.coinbase.com/currencies", parse::coinbase_names),
            settled("https://api.kucoin.com/api/v3/currencies", parse::kucoin_names),
            gecko_ranks(),
        );
        // After the ranked pages (CoinGecko limits bursts): full names of the other coins.
        let gecko_names = settled("https://api.coingecko.com/api/v3/coins/list", parse::gecko_names).await;
        let names: Vec<(String, String)> = cb_names.into_iter().chain(kc_names).chain(gecko_names).collect();
        let list = build_crypto(&[okx, coinbase, kraken, kucoin, gate], &gecko, &names);
        if list.len() < 50 {
            return http::err("Liste des cryptos indisponible");
        }
        Ok(list)
    })
    .await
}

/// Every US-listed stock / ETF (cached 12 hours).
pub async fn stock_universe() -> Result<Arc<Vec<UniverseEntry>>> {
    cached("universe:stock", DAY / 2, || async {
        let (nasdaq, other, caps) = tokio::join!(
            settled_text("https://www.nasdaqtrader.com/dynamic/SymDir/nasdaqlisted.txt", parse::nasdaq_directory),
            settled_text("https://www.nasdaqtrader.com/dynamic/SymDir/otherlisted.txt", parse::nasdaq_directory),
            settled("https://api.nasdaq.com/api/screener/stocks?tableonly=true&download=true", parse::screener),
        );
        let list = build_stocks(&[nasdaq, other], &caps);
        if list.len() < 1000 {
            return http::err("Liste des actions indisponible");
        }
        Ok(list)
    })
    .await
}

pub async fn universe(kind: Kind) -> Result<Arc<Vec<UniverseEntry>>> {
    match kind {
        Kind::Crypto => crypto_universe().await,
        Kind::Stock => stock_universe().await,
    }
}

/// `s.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toUpperCase()`.
fn norm(s: &str) -> String {
    const NFD: DecomposingNormalizerBorrowed<'static> = DecomposingNormalizerBorrowed::new_nfd();
    NFD.normalize(s).chars().filter(|c| !('\u{300}'..='\u{36f}').contains(c)).collect::<String>().to_uppercase()
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `new RegExp("\\b" + escaped(q)).test(name)`: q at a position where a word boundary holds.
fn word_start(name: &str, q: &str) -> bool {
    let Some(first) = q.chars().next() else { return true };
    // Every position (overlapping ones included), like the regular expression search.
    name.char_indices().any(|(i, _)| name[i..].starts_with(q) && name[..i].chars().next_back().is_some_and(is_word) != is_word(first))
}

/// Search in a universe: exact symbol, then symbol prefix, then name (word start, then anywhere); ranked assets first.
pub fn search_universe<'a>(list: &'a [UniverseEntry], query: &str, limit: usize) -> Vec<&'a UniverseEntry> {
    let q = norm(js_trim(query));
    if q.is_empty() {
        return list.iter().take(limit).collect();
    }
    let mut scored: Vec<(u8, usize, &UniverseEntry)> = vec![];
    for (i, e) in list.iter().enumerate() {
        let name = norm(&e.1);
        let s = if e.0 == q {
            0
        } else if e.0.starts_with(&q) {
            1
        } else if word_start(&name, &q) {
            2
        } else if name.contains(&q) {
            3
        } else {
            continue;
        };
        scored.push((s, i, e));
    }
    scored.sort_by_key(|&(s, i, _)| (s, i));
    scored.into_iter().take(limit).map(|x| x.2).collect()
}

/// A result of [`search_all`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit<'a> {
    pub e: &'a UniverseEntry,
    pub kind: Kind,
}

/// Cryptos and stocks searched together: exact symbol, then symbol prefix, then the largest.
pub fn search_all<'a>(crypto: &'a [UniverseEntry], stock: &'a [UniverseEntry], query: &str, limit: usize) -> Vec<Hit<'a>> {
    let q = norm(js_trim(query));
    // Obscure tokens (no name, no rank, a single exchange) only when their exact symbol is typed.
    let known = |e: &UniverseEntry| e.0 == q || e.2 > 0 || e.3 > 1 || e.1 != e.0;
    let mut hits: Vec<(Hit, usize)> = search_universe(crypto, query, limit * 3)
        .into_iter()
        .filter(|e| known(e))
        .take(limit)
        .enumerate()
        .map(|(i, e)| (Hit { e, kind: Kind::Crypto }, i))
        .chain(search_universe(stock, query, limit).into_iter().enumerate().map(|(i, e)| (Hit { e, kind: Kind::Stock }, i)))
        .collect();
    let score = |e: &UniverseEntry| if e.0 == q { 0 } else if e.0.starts_with(&q) { 1 } else { 2 };
    let size = |e: &UniverseEntry| if e.2 != 0 { e.2 as f64 } else { 1e9 };
    hits.sort_by(|(a, ai), (b, bi)| {
        score(a.e).cmp(&score(b.e)).then_with(|| size(a.e).partial_cmp(&size(b.e)).unwrap_or(Ordering::Equal)).then_with(|| ai.cmp(bi))
    });
    hits.into_iter().take(limit).map(|(h, _)| h).collect()
}

//! Price consensus for the market panel (`web/server/quotes.ts`): each price is fetched from several independent
//! sources, compared with the median, and a divergent source is discarded.
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, Shared, join_all};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::http::{CLIENT, Error, Result, err, get_json_with};
use crate::js::median;
use crate::jsval::{encode_uri_component as enc, get, number, truthy_num};
pub use crate::types::{Asset, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SourceQuote {
    /// NaN when the source gave no usable price (`undefined` in the TypeScript).
    pub price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<f64>,
}

pub type QuoteMap = IndexMap<String, SourceQuote>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteSourceStatus {
    pub name: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsensusQuote {
    pub symbol: String,
    pub name: String,
    pub kind: Kind,
    pub price: f64,
    /// null when no agreeing source gives the 24 h change.
    pub change: Option<f64>,
    pub agreeing: usize,
    pub total: usize,
    pub sources: Vec<QuoteSourceStatus>,
}

fn asset(symbol: &str, name: &str, kind: Kind, gecko: Option<&str>) -> Asset {
    Asset { symbol: symbol.into(), name: name.into(), kind, gecko: gecko.map(String::from) }
}

/// Default assets of the market panel.
pub static ASSETS: LazyLock<Vec<Asset>> = LazyLock::new(|| {
    vec![
        asset("BTC", "Bitcoin", Kind::Crypto, Some("bitcoin")),
        asset("ETH", "Ethereum", Kind::Crypto, Some("ethereum")),
        asset("SOL", "Solana", Kind::Crypto, Some("solana")),
        asset("BNB", "BNB", Kind::Crypto, Some("binancecoin")),
        asset("XRP", "XRP", Kind::Crypto, Some("ripple")),
        asset("ADA", "Cardano", Kind::Crypto, Some("cardano")),
        asset("AAPL", "Apple", Kind::Stock, None),
        asset("NVDA", "NVIDIA", Kind::Stock, None),
    ]
});

/// CoinGecko identifiers for the common cryptos (other cryptos go through the exchanges).
pub const GECKO: &[(&str, &str)] = &[
    ("BTC", "bitcoin"),
    ("ETH", "ethereum"),
    ("SOL", "solana"),
    ("BNB", "binancecoin"),
    ("XRP", "ripple"),
    ("ADA", "cardano"),
    ("DOGE", "dogecoin"),
    ("AVAX", "avalanche-2"),
    ("DOT", "polkadot"),
    ("LINK", "chainlink"),
    ("LTC", "litecoin"),
    ("TRX", "tron"),
    ("TON", "the-open-network"),
    ("SHIB", "shiba-inu"),
    ("ATOM", "cosmos"),
    ("UNI", "uniswap"),
    ("NEAR", "near"),
    ("APT", "aptos"),
    ("ARB", "arbitrum"),
    ("OP", "optimism"),
    ("SUI", "sui"),
    ("PEPE", "pepe"),
    ("BCH", "bitcoin-cash"),
    ("XLM", "stellar"),
    ("ETC", "ethereum-classic"),
    ("FIL", "filecoin"),
];

pub fn gecko(symbol: &str) -> Option<&'static str> {
    GECKO.iter().find(|(s, _)| *s == symbol).map(|(_, g)| *g)
}

pub fn make_asset(symbol: &str, kind: Kind, name: Option<&str>) -> Asset {
    if let Some(known) = ASSETS.iter().find(|a| a.symbol == symbol && a.kind == kind) {
        return known.clone();
    }
    Asset {
        symbol: symbol.into(),
        kind,
        name: name.unwrap_or(symbol).into(),
        gecko: if kind == Kind::Crypto { gecko(symbol).map(String::from) } else { None },
    }
}

pub const UA: &str = "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0";

async fn get_json(url: &str) -> Result<Value> {
    get_json_with(url, &[("User-Agent", UA)], Duration::from_secs(8)).await
}

/// `Number(x)` (after dropping "$ , % +" from a string), kept only when finite and not 0.
pub fn n(v: Option<&Value>) -> Option<f64> {
    let x = match v {
        Some(Value::String(s)) => crate::jsval::string_to_number(&s.replace(['$', ',', '%', '+'], "")),
        _ => number(v),
    };
    (x.is_finite() && x != 0.0).then_some(x)
}
fn nan(x: Option<f64>) -> f64 {
    x.unwrap_or(f64::NAN)
}

/// Runs one request per asset, ignoring individual failures (asset not listed on that source).
async fn each<F, Fut>(assets: Vec<Asset>, f: F) -> Result<QuoteMap>
where
    F: Fn(Asset) -> Fut,
    Fut: Future<Output = Result<Option<SourceQuote>>>,
{
    let done = join_all(assets.into_iter().map(|a| {
        let symbol = a.symbol.clone();
        f(a).map(move |r| (symbol, r))
    }))
    .await;
    let mut out = QuoteMap::new();
    for (symbol, r) in done {
        if let Ok(Some(q)) = r {
            if truthy_num(q.price) {
                out.insert(symbol, q);
            }
        }
    }
    Ok(out)
}

/// Quote parsers (formats checked against real responses, see tests/quotes.rs).
pub mod parse {
    use serde_json::Value;

    use super::{QuoteMap, SourceQuote, n, nan};
    use crate::http::{Error, Result, err};
    use crate::jsval::{array, get, idx, is_str, need, nullish, number, to_string, truthy};
    use crate::types::Asset;

    fn strip_suffix<'a>(s: &'a str, suffix: &str) -> &'a str {
        s.strip_suffix(suffix).unwrap_or(s)
    }
    /// `for (const r of x)` on a non-iterable value.
    fn iterable(v: Option<&Value>, expr: &str) -> Result<Vec<Value>> {
        match v {
            None => Err(Error(format!("undefined is not an object (evaluating '{expr}')"))),
            Some(Value::Null) => Err(Error(format!("null is not an object (evaluating '{expr}')"))),
            Some(Value::Array(a)) => Ok(a.clone()),
            Some(Value::String(s)) => Ok(s.chars().map(|c| Value::String(c.into())).collect()),
            // JavaScriptCore names the value "{}" (object), "number" or the boolean.
            Some(Value::Object(_)) => Err(Error("{} is not iterable".into())),
            Some(Value::Number(_)) => Err(Error("number is not iterable".into())),
            Some(o) => Err(Error(format!("{} is not iterable", to_string(Some(o))))),
        }
    }

    const BINANCE: &str = "rows.map((r) => [String(r.symbol).replace(/USDT$/, \"\"), { price: n(r.lastPrice), change: n(r.priceChangePercent) }])";
    pub fn binance(rows: &Value) -> Result<QuoteMap> {
        let mut m = QuoteMap::new();
        for r in array(Some(rows), "rows", "map", BINANCE)? {
            need(Some(r), "r.symbol")?;
            let r = Some(r);
            m.insert(strip_suffix(&to_string(get(r, "symbol")), "USDT").into(), SourceQuote { price: nan(n(get(r, "lastPrice"))), change: n(get(r, "priceChangePercent")) });
        }
        Ok(m)
    }

    pub fn okx(d: &Value, bases: &[String]) -> Result<QuoteMap> {
        need(Some(d), "d.code")?;
        if !is_str(get(Some(d), "code"), "0") {
            return err("OKX");
        }
        let mut m = QuoteMap::new();
        for r in iterable(get(Some(d), "data"), "r of d.data")? {
            need(Some(&r), "r.instId")?;
            let r = Some(&r);
            let inst = get(r, "instId");
            let base = strip_suffix(&to_string(inst), "-USDT").to_string();
            need(inst, "r.instId.endsWith")?;
            let ends = inst.and_then(|v| v.as_str()).is_some_and(|s| s.ends_with("-USDT"));
            if ends && bases.contains(&base) {
                let last = nan(n(get(r, "last")));
                m.insert(base, SourceQuote { price: last, change: Some((last / nan(n(get(r, "open24h"))) - 1.0) * 100.0) });
            }
        }
        Ok(m)
    }

    pub fn kraken(d: &Value, bases: &[String]) -> Result<QuoteMap> {
        need(Some(d), "d.error")?;
        crate::market::parse::kraken_error(get(Some(d), "error"), "d.error")?;
        let mut m = QuoteMap::new();
        for base in bases {
            let alt = if base == "BTC" { "XBT" } else { base.as_str() };
            let result = get(Some(d), "result");
            need(result, "d.result[`${alt}USD`]")?;
            let row = get(result, &format!("{alt}USD"));
            let row = if nullish(row) { get(result, &format!("X{alt}ZUSD")) } else { row };
            if truthy(row) {
                // Kraken's "o" is the UTC-day open, not 24h: no change
                let c = get(row, "c");
                need(c, "row.c[0]")?;
                m.insert(base.clone(), SourceQuote { price: nan(n(idx(c, 0))), change: None });
            }
        }
        Ok(m)
    }

    const BITFINEX: &str = "rows.map((r) => [String(r[0]).slice(1).replace(/:?USD$/, \"\"), { price: n(r[7]), change: r[6] * 100 }])";
    pub fn bitfinex(rows: &Value) -> Result<QuoteMap> {
        let mut m = QuoteMap::new();
        for r in array(Some(rows), "rows", "map", BITFINEX)? {
            need(Some(r), "r[0]")?;
            let r = Some(r);
            let s = to_string(idx(r, 0));
            let mut chars = s.chars();
            // slice(1): one UTF-16 unit (the symbols are ASCII).
            chars.next();
            let s = chars.as_str();
            let base = s.strip_suffix(":USD").or_else(|| s.strip_suffix("USD")).unwrap_or(s);
            m.insert(base.into(), SourceQuote { price: nan(n(idx(r, 7))), change: Some(number(idx(r, 6)) * 100.0) });
        }
        Ok(m)
    }

    pub fn coingecko(d: &Value, assets: &[Asset]) -> Result<QuoteMap> {
        let mut m = QuoteMap::new();
        for a in assets {
            need(Some(d), "d[a.gecko]")?;
            let id = a.gecko.as_deref().unwrap_or("undefined");
            let x = get(Some(d), id);
            if truthy(x) {
                m.insert(a.symbol.clone(), SourceQuote { price: nan(n(get(x, "usd"))), change: n(get(x, "usd_24h_change")) });
            }
        }
        Ok(m)
    }

    pub fn nasdaq(d: &Value) -> Result<SourceQuote> {
        need(Some(d), "d.data")?;
        let data = get(Some(d), "data");
        need(data, "d.data.primaryData")?;
        let p = get(data, "primaryData");
        need(p, "d.data.primaryData.lastSalePrice")?;
        Ok(SourceQuote { price: nan(n(get(p, "lastSalePrice"))), change: n(get(p, "percentageChange")) })
    }

    pub fn cboe(d: &Value) -> Result<SourceQuote> {
        need(Some(d), "d.details")?;
        let x = get(Some(d), "details");
        need(x, "d.details.current_price")?;
        Ok(SourceQuote { price: nan(n(get(x, "current_price"))), change: n(get(x, "price_change_percent")) })
    }

    /// Robinhood batch quotes (class shares written BRK.B); unknown symbols come back as null.
    pub fn robinhood(d: &Value) -> Result<QuoteMap> {
        let list = get(Some(d), "results");
        let list = if nullish(list) { &[][..] } else { array(list, "(d?.results ?? [])", "filter", "(d?.results ?? []).filter(Boolean)")? };
        let mut m = QuoteMap::new();
        for r in list.iter().filter(|r| truthy(Some(r))) {
            let r = Some(r);
            let price = nan(n(get(r, "last_trade_price")));
            let prev = n(get(r, "adjusted_previous_close")).or_else(|| n(get(r, "previous_close")));
            m.insert(to_string(get(r, "symbol")).replace('.', "-"), SourceQuote { price, change: prev.map(|p| (price / p - 1.0) * 100.0) });
        }
        Ok(m)
    }

    /// TradingView scanner: one row per listing; the main US listing is kept.
    pub fn tradingview(d: &Value) -> Result<QuoteMap> {
        let data = get(Some(d), "data");
        let rows = if nullish(data) { vec![] } else { iterable(data, "r of d?.data ?? []")? };
        let mut m = QuoteMap::new();
        for r in &rows {
            need(Some(r), "r.d")?;
            let row = get(Some(r), "d");
            need(row, "r.d[0]")?;
            let sym = to_string(idx(row, 0)).replace('.', "-");
            if !m.contains_key(&sym) && number(idx(row, 2)) > 0.0 {
                let change = idx(row, 3).map(|v| number(Some(v)));
                m.insert(sym, SourceQuote { price: number(idx(row, 2)), change });
            }
        }
        Ok(m)
    }

    /// Webull real-time quote: regular-session price and change ratio.
    pub fn webull(d: &Value) -> Result<SourceQuote> {
        need(Some(d), "d.close")?;
        let ratio = get(Some(d), "changeRatio");
        Ok(SourceQuote { price: nan(n(get(Some(d), "close"))), change: (!nullish(ratio)).then(|| number(ratio) * 100.0) })
    }

    /// Zacks quote feed: {"AAPL": {"last": "341.07", "percent_net_change": "1.53…"}}.
    pub fn zacks(d: &Value) -> Result<QuoteMap> {
        let entries: Vec<(String, &Value)> = match d {
            Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), v)).collect(),
            Value::Array(a) => a.iter().enumerate().map(|(i, v)| (i.to_string(), v)).collect(),
            _ => vec![],
        };
        let mut m = QuoteMap::new();
        for (sym, x) in entries {
            if let Some(last) = n(get(Some(x), "last")) {
                m.insert(sym.replace('.', "-"), SourceQuote { price: last, change: n(get(Some(x), "percent_net_change")) });
            }
        }
        Ok(m)
    }

    const WEBULL_TICKER: &str = "(d?.data ?? []).find((x) => x.regionCode === \"US\" && String(x.disSymbol).replace(/[ .]/g, \"-\") === symbol)";
    /// Webull search: the US listing whose symbol is exactly the one asked (class shares written "BRK B").
    pub fn webull_ticker(d: &Value, symbol: &str) -> Result<Option<f64>> {
        let data = get(Some(d), "data");
        let list = if nullish(data) { &[][..] } else { array(data, "(d?.data ?? [])", "find", WEBULL_TICKER)? };
        for x in list {
            need(Some(x), "x.regionCode")?;
            let x = Some(x);
            if is_str(get(x, "regionCode"), "US") && to_string(get(x, "disSymbol")).replace([' ', '.'], "-") == symbol {
                return Ok(Some(number(get(x, "tickerId"))));
            }
        }
        Ok(None)
    }

    pub fn yahoo(d: &Value) -> Result<SourceQuote> {
        need(Some(d), "d.chart")?;
        let chart = get(Some(d), "chart");
        need(chart, "d.chart.result")?;
        let r = idx(get(chart, "result"), 0);
        need(r, "r.indicators")?;
        let ind = get(r, "indicators");
        need(ind, "r.indicators.quote")?;
        let q = get(ind, "quote");
        need(q, "r.indicators.quote[0]")?;
        let q0 = idx(q, 0);
        need(q0, "r.indicators.quote[0].close")?;
        let close = array(get(q0, "close"), "r.indicators.quote[0].close", "filter", "r.indicators.quote[0].close.filter((x) => x != null)")?;
        let closes: Vec<f64> = close.iter().filter(|x| !x.is_null()).map(|x| number(Some(x))).collect();
        need(r, "r.meta")?;
        let meta = get(r, "meta");
        need(meta, "r.meta.regularMarketPrice")?;
        let price = nan(n(get(meta, "regularMarketPrice")));
        let prev = if closes.len() > 1 { Some(closes[closes.len() - 2]) } else { None };
        Ok(SourceQuote { price, change: prev.filter(|p| crate::jsval::truthy_num(*p)).map(|p| (price / p - 1.0) * 100.0) })
    }
}

/// A live quote provider: `fetch(assets)` returns the quotes it has, by symbol.
#[derive(Clone)]
pub struct QuoteSource {
    pub name: String,
    pub kind: Kind,
    pub fetch: Arc<dyn Fn(Vec<Asset>) -> BoxFuture<'static, Result<QuoteMap>> + Send + Sync>,
}

impl QuoteSource {
    pub fn new<F, Fut>(name: &str, kind: Kind, fetch: F) -> Self
    where
        F: Fn(Vec<Asset>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<QuoteMap>> + Send + 'static,
    {
        QuoteSource { name: name.into(), kind, fetch: Arc::new(move |a| fetch(a).boxed()) }
    }
}

impl std::fmt::Debug for QuoteSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuoteSource").field("name", &self.name).field("kind", &self.kind).finish()
    }
}

fn cryptos(a: &[Asset]) -> Vec<Asset> {
    a.iter().filter(|x| x.kind == Kind::Crypto).cloned().collect()
}
fn stocks(a: &[Asset]) -> Vec<Asset> {
    a.iter().filter(|x| x.kind == Kind::Stock).cloned().collect()
}
fn dotted(a: &[Asset]) -> Vec<String> {
    stocks(a).iter().map(|x| x.symbol.replace('-', ".")).collect()
}

/// Webull identifies securities by a numeric id: looked up once per symbol, then kept (a network error is retried
/// next time). None = not listed.
pub async fn webull_ticker_id(symbol: &str) -> Option<f64> {
    static IDS: LazyLock<Mutex<HashMap<String, Shared<BoxFuture<'static, Option<f64>>>>>> = LazyLock::new(Default::default);
    let fut = {
        let mut ids = IDS.lock().unwrap();
        ids.entry(symbol.to_string())
            .or_insert_with(|| {
                let symbol = symbol.to_string();
                async move {
                    let keyword = symbol.replace('-', " ");
                    let url = format!("https://quotes-gw.webullfintech.com/api/search/pc/tickers?keyword={}&pageIndex=1&pageSize=10", enc(&keyword));
                    match get_json(&url).await.and_then(|d| parse::webull_ticker(&d, &symbol)) {
                        Ok(id) => id,
                        Err(_) => {
                            IDS.lock().unwrap().remove(&symbol);
                            None
                        }
                    }
                }
                .boxed()
                .shared()
            })
            .clone()
    };
    fut.await
}

/// POST JSON (TradingView scanner).
async fn post_json(url: &str, body: &Value) -> Result<Value> {
    let res = CLIENT
        .post(url)
        .timeout(Duration::from_secs(8))
        .header("User-Agent", UA)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await?;
    if !res.status().is_success() {
        return err(format!("HTTP {}", res.status().as_u16()));
    }
    let text = res.text().await?;
    serde_json::from_str(&text).map_err(|_| Error("JSON invalide".into()))
}

fn bfx_symbol(b: &str) -> String {
    if b.encode_utf16().count() > 3 { format!("t{b}:USD") } else { format!("t{b}USD") }
}

pub static QUOTE_SOURCES: LazyLock<Vec<QuoteSource>> = LazyLock::new(|| {
    let mut v = vec![
        QuoteSource::new("Binance", Kind::Crypto, |a| async move {
            let symbols: Vec<String> = cryptos(&a).iter().map(|x| format!("{}USDT", x.symbol)).collect();
            parse::binance(&get_json(&format!("https://api.binance.com/api/v3/ticker/24hr?symbols={}", enc(&serde_json::to_string(&symbols).unwrap()))).await?)
        }),
        QuoteSource::new("OKX", Kind::Crypto, |a| async move {
            let bases: Vec<String> = cryptos(&a).iter().map(|x| x.symbol.clone()).collect();
            parse::okx(&get_json("https://www.okx.com/api/v5/market/tickers?instType=SPOT").await?, &bases)
        }),
        QuoteSource::new("Coinbase", Kind::Crypto, |a| {
            each(cryptos(&a), |x| async move {
                let d = get_json(&format!("https://api.exchange.coinbase.com/products/{}-USD/stats", x.symbol)).await?;
                let last = nan(n(get(Some(&d), "last")));
                Ok(Some(SourceQuote { price: last, change: Some((last / nan(n(get(Some(&d), "open"))) - 1.0) * 100.0) }))
            })
        }),
        QuoteSource::new("Kraken", Kind::Crypto, |a| async move {
            let bases: Vec<String> = cryptos(&a).iter().map(|x| x.symbol.clone()).collect();
            let pairs = bases.iter().map(|b| format!("{}USD", if b == "BTC" { "XBT" } else { b })).collect::<Vec<_>>().join(",");
            parse::kraken(&get_json(&format!("https://api.kraken.com/0/public/Ticker?pair={pairs}")).await?, &bases)
        }),
        QuoteSource::new("KuCoin", Kind::Crypto, |a| {
            each(cryptos(&a), |x| async move {
                let d = get_json(&format!("https://api.kucoin.com/api/v1/market/stats?symbol={}-USDT", x.symbol)).await?;
                let data = get(Some(&d), "data");
                Ok(crate::jsval::truthy(get(data, "last"))
                    .then(|| SourceQuote { price: nan(n(get(data, "last"))), change: Some(number(get(data, "changeRate")) * 100.0) }))
            })
        }),
        QuoteSource::new("Gate.io", Kind::Crypto, |a| {
            each(cryptos(&a), |x| async move {
                let d = get_json(&format!("https://api.gateio.ws/api/v4/spot/tickers?currency_pair={}_USDT", x.symbol)).await?;
                let r = d.as_array().and_then(|a| a.first()).ok_or_else(|| Error("réponse inattendue".into()))?;
                Ok(Some(SourceQuote { price: nan(n(get(Some(r), "last"))), change: Some(number(get(Some(r), "change_percentage"))) }))
            })
        }),
        QuoteSource::new("Bitfinex", Kind::Crypto, |a| async move {
            let symbols = cryptos(&a).iter().map(|x| bfx_symbol(&x.symbol)).collect::<Vec<_>>().join(",");
            parse::bitfinex(&get_json(&format!("https://api-pub.bitfinex.com/v2/tickers?symbols={symbols}")).await?)
        }),
        QuoteSource::new("CoinGecko", Kind::Crypto, |a| async move {
            let known: Vec<Asset> = cryptos(&a).into_iter().filter(|x| x.gecko.is_some()).collect();
            if known.is_empty() {
                return Ok(QuoteMap::new());
            }
            let ids = known.iter().filter_map(|x| x.gecko.clone()).collect::<Vec<_>>().join(",");
            parse::coingecko(&get_json(&format!("https://api.coingecko.com/api/v3/simple/price?ids={ids}&vs_currencies=usd&include_24hr_change=true")).await?, &known)
        }),
        QuoteSource::new("Yahoo Finance", Kind::Stock, |a| {
            each(stocks(&a), |x| async move {
                Ok(Some(parse::yahoo(&get_json(&format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=5d", x.symbol)).await?)?))
            })
        }),
        QuoteSource::new("Nasdaq", Kind::Stock, |a| {
            each(stocks(&a), |x| async move {
                // Class shares: BRK-B (Yahoo) = BRK.B (Nasdaq). ETFs live in another asset class.
                let sym = x.symbol.replace('-', ".");
                let first = async { parse::nasdaq(&get_json(&format!("https://api.nasdaq.com/api/quote/{sym}/info?assetclass=stocks")).await?) }.await;
                match first {
                    Ok(q) => Ok(Some(q)),
                    Err(_) => Ok(Some(parse::nasdaq(&get_json(&format!("https://api.nasdaq.com/api/quote/{sym}/info?assetclass=etf")).await?)?)),
                }
            })
        }),
        QuoteSource::new("Robinhood", Kind::Stock, |a| async move {
            if stocks(&a).is_empty() {
                return Ok(QuoteMap::new());
            }
            let symbols = dotted(&a).iter().map(|s| enc(s)).collect::<Vec<_>>().join(",");
            parse::robinhood(&get_json(&format!("https://api.robinhood.com/quotes/?symbols={symbols}")).await?)
        }),
        QuoteSource::new("Webull", Kind::Stock, |a| {
            each(stocks(&a), |x| async move {
                let id = webull_ticker_id(&x.symbol).await.filter(|x| truthy_num(*x));
                let Some(id) = id else { return err("non coté") };
                let url = format!("https://quotes-gw.webullfintech.com/api/stock/tickerRealTime/getQuote?tickerId={}&includeSecu=1", crate::js::number_to_string(id));
                Ok(Some(parse::webull(&get_json(&url).await?)?))
            })
        }),
        QuoteSource::new("Zacks", Kind::Stock, |a| async move {
            if stocks(&a).is_empty() {
                return Ok(QuoteMap::new());
            }
            let symbols = dotted(&a).iter().map(|s| enc(s)).collect::<Vec<_>>().join(",");
            parse::zacks(&get_json(&format!("https://quote-feed.zacks.com/index?t={symbols}")).await?)
        }),
        QuoteSource::new("TradingView", Kind::Stock, |a| async move {
            if stocks(&a).is_empty() {
                return Ok(QuoteMap::new());
            }
            let body = json!({
                "filter": [
                    { "left": "name", "operation": "in_range", "right": dotted(&a) },
                    { "left": "exchange", "operation": "in_range", "right": ["NASDAQ", "NYSE", "AMEX", "CBOE"] },
                ],
                "columns": ["name", "exchange", "close", "change"],
                "range": [0, 100],
            });
            parse::tradingview(&post_json("https://scanner.tradingview.com/america/scan", &body).await?)
        }),
        QuoteSource::new("Cboe", Kind::Stock, |a| {
            each(stocks(&a), |x| async move {
                let url = format!("https://www.cboe.com/education/tools/trade-optimizer/symbol-info/?symbol={}", x.symbol.replace('-', "."));
                Ok(Some(parse::cboe(&get_json(&url).await?)?))
            })
        }),
    ];
    // Live quotes from Fidelity, StockCharts, TipRanks and Public.com (stocks_extra).
    for s in crate::stocks_extra::EXTRA_QUOTE_SOURCES.iter() {
        let fetch = s.fetch;
        v.push(QuoteSource::new(s.name, Kind::Stock, move |a| fetch(stocks(&a).into_iter().map(|x| x.symbol).collect())));
    }
    v
});

/// Outcome of one quote source for `combine` (`quotes` or `error`).
#[derive(Debug, Clone)]
pub struct QuoteResult {
    pub name: String,
    pub kind: Kind,
    pub quotes: Option<QuoteMap>,
    pub error: Option<String>,
}

/// Consensus for each asset: median of agreeing sources (0.5 % crypto, 1.5 % stocks).
pub fn combine(assets: &[Asset], results: &[QuoteResult]) -> Vec<ConsensusQuote> {
    assets
        .iter()
        .filter_map(|asset| {
            let relevant: Vec<&QuoteResult> = results.iter().filter(|r| r.kind == asset.kind).collect();
            let found: Vec<(&str, SourceQuote)> =
                relevant.iter().filter_map(|r| r.quotes.as_ref().and_then(|q| q.get(&asset.symbol)).map(|q| (r.name.as_str(), *q))).collect();
            if found.is_empty() {
                return None;
            }
            let reference = median(&found.iter().map(|f| f.1.price).collect::<Vec<_>>());
            let tol = if asset.kind == Kind::Crypto { 0.5 } else { 1.5 };
            let agree: Vec<&(&str, SourceQuote)> = found.iter().filter(|f| (f.1.price / reference - 1.0).abs() * 100.0 <= tol).collect();
            let changes: Vec<f64> = agree.iter().filter_map(|f| f.1.change).filter(|c| c.is_finite()).collect();
            Some(ConsensusQuote {
                symbol: asset.symbol.clone(),
                name: asset.name.clone(),
                kind: asset.kind,
                price: median(&agree.iter().map(|f| f.1.price).collect::<Vec<_>>()),
                change: if changes.is_empty() { None } else { Some(median(&changes)) },
                agreeing: agree.len(),
                total: relevant.len(),
                sources: relevant
                    .iter()
                    .map(|r| match r.quotes.as_ref().and_then(|q| q.get(&asset.symbol)) {
                        Some(q) => QuoteSourceStatus { name: r.name.clone(), ok: agree.iter().any(|f| f.0 == r.name), price: Some(q.price), error: None },
                        None => QuoteSourceStatus { name: r.name.clone(), ok: false, price: None, error: Some(r.error.clone().unwrap_or_else(|| "non coté".into())) },
                    })
                    .collect(),
            })
        })
        .collect()
}

/// Queries every source in parallel (a failing one is reported in `sources`) and combines their quotes.
pub async fn consensus_quotes(assets: &[Asset], sources: &[QuoteSource]) -> Vec<ConsensusQuote> {
    let results = join_all(sources.iter().map(|s| {
        let fut = (s.fetch)(assets.to_vec());
        let (name, kind) = (s.name.clone(), s.kind);
        async move {
            match fut.await {
                Ok(quotes) => QuoteResult { name, kind, quotes: Some(quotes), error: None },
                Err(e) => QuoteResult { name, kind, quotes: None, error: Some(e.0) },
            }
        }
    }))
    .await;
    combine(assets, &results)
}

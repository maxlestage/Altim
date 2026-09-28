//! Token and network figures for `/api/decision`: supply and valuation (CoinGecko), BTC dominance (CoinGecko,
//! CoinPaprika when it is rate-limited), TVL and fees (DefiLlama, free endpoints only), perpetual funding and open
//! interest (OKX, shared with the guard), Bitcoin activity (blockchain.com).
//!
//! Each figure is checked (positive, plausible, consistent with the others) and dropped otherwise: blockchain.com
//! for instance has published a negative `total_fees_btc`. Token unlocks are only sold by paid sources: never given.
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::cache::cached;
use crate::engine::decision_types::CryptoFundamentals;
use crate::fundamentals::{num, round_to};
use crate::http::{Error, Result, get_json_with};

const HOUR: i64 = 3_600_000;
const TIMEOUT: Duration = Duration::from_secs(12);

pub const UNLOCKS_PAID: &str = "Non vérifiable : les déblocages de jetons ne sont publiés que par des sources payantes";
pub const UNLOCKS_BTC: &str = "Sans objet : le bitcoin n'a pas de déblocage de jetons (émission fixée par le protocole, 21 millions au plus)";

async fn get(url: &str) -> Result<Value> {
    get_json_with(url, &[("User-Agent", crate::guard::UA)], TIMEOUT).await
}

/// CoinGecko figures of one coin.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Coin {
    pub market_cap: Option<f64>,
    pub fdv: Option<f64>,
    pub circulating: Option<f64>,
    pub total: Option<f64>,
    pub max: Option<f64>,
    /// Who gave these figures ("CoinGecko", or "CoinPaprika" when CoinGecko refuses: it limits requests hard).
    pub source: &'static str,
}

/// DefiLlama index: the chains and protocols that name a CoinGecko id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Llama {
    /// (gecko id, chain name, TVL)
    pub chains: Vec<(String, String, Option<f64>)>,
    /// (gecko id, slug): protocols grouping several versions (Uniswap V2, V3…).
    pub parents: Vec<(String, String)>,
    /// (gecko id, slug, TVL)
    pub protocols: Vec<(String, String, Option<f64>)>,
}

fn positive(x: Option<f64>) -> Option<f64> {
    x.filter(|v| v.is_finite() && *v > 0.0)
}

pub mod parse {
    use super::*;

    /// CoinGecko `/coins/{id}` (market_data).
    /// CoinPaprika `/v1/tickers`: the best-ranked coin with this symbol. Circulating supply = market cap ÷ price;
    /// FDV = price × maximum supply (or total when there is no maximum).
    pub fn paprika(d: &Value, symbol: &str) -> Option<Coin> {
        let t = d
            .as_array()?
            .iter()
            .filter(|t| t.get("symbol").and_then(Value::as_str).is_some_and(|s| s.eq_ignore_ascii_case(symbol)))
            .filter(|t| t.get("rank").and_then(Value::as_f64).is_some_and(|r| r > 0.0))
            .min_by(|a, b| a["rank"].as_f64().unwrap_or(f64::MAX).total_cmp(&b["rank"].as_f64().unwrap_or(f64::MAX)))?;
        let usd = t.pointer("/quotes/USD")?;
        let price = positive(num(usd.get("price")));
        let market_cap = positive(num(usd.get("market_cap")));
        let total = positive(num(t.get("total_supply")));
        let max = positive(num(t.get("max_supply")));
        let circulating = match (market_cap, price) {
            (Some(m), Some(p)) => Some(m / p),
            _ => None,
        };
        let fdv = match (price, max.or(total)) {
            (Some(p), Some(s)) => Some(p * s),
            _ => None,
        };
        let mut c = Coin { market_cap, fdv, circulating, total, max, source: "CoinPaprika" };
        if let (Some(circ), Some(cap)) = (c.circulating, c.max.or(c.total)) {
            if circ > cap * 1.001 {
                c.circulating = None;
            }
        }
        (c.market_cap.is_some() || c.circulating.is_some()).then_some(c)
    }

    pub fn coin(d: &Value) -> Coin {
        let Some(m) = d.get("market_data") else { return Coin::default() };
        let usd = |k: &str| positive(num(m.get(k).and_then(|x| x.get("usd"))));
        let mut c = Coin {
            market_cap: usd("market_cap"),
            fdv: usd("fully_diluted_valuation"),
            circulating: positive(num(m.get("circulating_supply"))),
            total: positive(num(m.get("total_supply"))),
            max: positive(num(m.get("max_supply"))),
            source: "CoinGecko",
        };
        // Inconsistent supplies (more in circulation than can ever exist): not shown.
        let cap = c.max.or(c.total);
        if let (Some(circ), Some(cap)) = (c.circulating, cap) {
            if circ > cap * 1.001 {
                c.circulating = None;
            }
        }
        if let (Some(t), Some(mx)) = (c.total, c.max) {
            if t > mx * 1.001 {
                c.total = None;
            }
        }
        if let (Some(mc), Some(f)) = (c.market_cap, c.fdv) {
            if mc > f * 1.001 {
                c.fdv = None;
            }
        }
        c
    }

    /// CoinGecko `/search?query=SYM`: the best ranked coin whose ticker is exactly `symbol`.
    pub fn search(d: &Value, symbol: &str) -> Option<String> {
        d.get("coins")?
            .as_array()?
            .iter()
            .filter(|c| c.get("symbol").and_then(|s| s.as_str()).is_some_and(|s| s.eq_ignore_ascii_case(symbol)))
            .filter_map(|c| Some((c.get("market_cap_rank")?.as_u64()?, c.get("id")?.as_str()?.to_string())))
            .min()
            .map(|(_, id)| id)
    }

    /// BTC share of the crypto market, % (CoinGecko `/global`, CoinPaprika `/v1/global`).
    pub fn dominance(d: &Value) -> Option<f64> {
        let v = num(d.get("data").and_then(|x| x.get("market_cap_percentage")).and_then(|x| x.get("btc")))
            .or_else(|| num(d.get("bitcoin_dominance_percentage")))?;
        (10.0..=95.0).contains(&v).then(|| round_to(v, 2))
    }

    /// DefiLlama `/v2/chains`.
    pub fn llama_chains(d: &Value) -> Vec<(String, String, Option<f64>)> {
        d.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| {
                        Some((c.get("gecko_id")?.as_str()?.to_string(), c.get("name")?.as_str()?.to_string(), positive(num(c.get("tvl")))))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// DefiLlama's slug of a protocol name ("Aave V3" → "aave-v3").
    pub fn slug(name: &str) -> String {
        name.to_lowercase().split(' ').collect::<Vec<_>>().join("-").replace('\'', "")
    }

    /// DefiLlama `/lite/protocols2`: parents (`parent#uniswap`) and protocols carrying a CoinGecko id.
    pub fn llama_protocols(d: &Value) -> (Vec<(String, String)>, Vec<(String, String, Option<f64>)>) {
        let arr = |k: &str| d.get(k).and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let parents = arr("parentProtocols")
            .iter()
            .filter_map(|p| Some((p.get("gecko_id")?.as_str()?.to_string(), p.get("id")?.as_str()?.strip_prefix("parent#")?.to_string())))
            .collect();
        let protocols = arr("protocols")
            .iter()
            .filter_map(|p| Some((p.get("geckoId")?.as_str()?.to_string(), slug(p.get("name")?.as_str()?), positive(num(p.get("tvl"))))))
            .collect();
        (parents, protocols)
    }

    /// DefiLlama `/summary/fees/{slug}`: fees of the last 30 days, only when the answer names the same CoinGecko id.
    pub fn fees30d(d: &Value, gecko: &str) -> Option<f64> {
        if d.get("gecko_id").and_then(|g| g.as_str()) != Some(gecko) {
            return None;
        }
        num(d.get("total30d")).filter(|v| *v >= 0.0)
    }

    /// blockchain.com `/stats`: transactions per day and hash rate (H/s; the API gives GH/s). Implausible values
    /// are dropped.
    pub fn blockchain(d: &Value) -> (Option<f64>, Option<f64>) {
        let tx = num(d.get("n_tx")).filter(|v| (50_000.0..=5_000_000.0).contains(v));
        let hash = num(d.get("hash_rate")).map(|gh| gh * 1e9).filter(|v| (1e18..=1e23).contains(v));
        (tx, hash)
    }
}

/// CoinGecko id of a crypto: the known ones, else CoinGecko's own search (exact ticker, best market cap rank).
async fn gecko_id(symbol: &str) -> Option<String> {
    if let Some(id) = crate::quotes::gecko(symbol) {
        return Some(id.to_string());
    }
    let sym = symbol.to_uppercase();
    cached(&format!("gecko:search:{sym}"), 24 * HOUR, move || async move {
        let d = get(&format!("https://api.coingecko.com/api/v3/search?query={}", crate::guard::encode_uri_component(&sym))).await?;
        Ok(parse::search(&d, &sym))
    })
    .await
    .ok()
    .and_then(|v| (*v).clone())
}

async fn coin(id: &str) -> Option<Coin> {
    let id = id.to_string();
    cached(&format!("gecko:coin:{id}"), 6 * HOUR, move || async move {
        let url = format!(
            "https://api.coingecko.com/api/v3/coins/{id}?localization=false&tickers=false&market_data=true&community_data=false&developer_data=false&sparkline=false"
        );
        Ok(parse::coin(&get(&url).await?))
    })
    .await
    .ok()
    .map(|c| *c)
}

/// CoinPaprika tickers (all coins, one call, cached 1 h): the fallback for market cap and supply.
async fn paprika(symbol: &str) -> Option<Coin> {
    let list = cached("paprika:tickers", HOUR, || async { get("https://api.coinpaprika.com/v1/tickers").await }).await.ok()?;
    parse::paprika(&list, symbol)
}

/// BTC dominance and the source that gave it.
async fn dominance() -> Option<(f64, &'static str)> {
    cached("crypto:dominance", HOUR, || async {
        if let Some(v) = get("https://api.coingecko.com/api/v3/global").await.ok().as_ref().and_then(parse::dominance) {
            return Ok((v, "CoinGecko"));
        }
        let v = get("https://api.coinpaprika.com/v1/global").await.ok().as_ref().and_then(parse::dominance);
        v.map(|v| (v, "CoinPaprika")).ok_or_else(|| Error("dominance indisponible".into()))
    })
    .await
    .ok()
    .map(|v| *v)
}

async fn llama() -> Result<Arc<Llama>> {
    cached("llama:index", 6 * HOUR, || async {
        let (chains, lite) = tokio::join!(get("https://api.llama.fi/v2/chains"), async {
            get_json_with("https://api.llama.fi/lite/protocols2", &[("User-Agent", crate::guard::UA)], Duration::from_secs(30)).await
        });
        if let (Err(e), Err(_)) = (&chains, &lite) {
            return Err(e.clone());
        }
        let (parents, protocols) = lite.ok().as_ref().map(parse::llama_protocols).unwrap_or_default();
        Ok(Llama { chains: chains.ok().as_ref().map(parse::llama_chains).unwrap_or_default(), parents, protocols })
    })
    .await
}

/// TVL and 30-day fees from DefiLlama, found by CoinGecko id (chain, protocol family, or single protocol).
async fn tvl_and_fees(gecko: &str) -> (Option<f64>, Option<f64>) {
    let Ok(index) = llama().await else { return (None, None) };
    let (slug, tvl) = if let Some((_, name, tvl)) = index.chains.iter().find(|c| c.0 == gecko) {
        (name.to_lowercase().replace(' ', "-"), *tvl)
    } else if let Some((_, slug)) = index.parents.iter().find(|p| p.0 == gecko) {
        let s = slug.clone();
        let tvl = cached(&format!("llama:tvl:{s}"), 6 * HOUR, move || async move {
            Ok(positive(num(Some(&get(&format!("https://api.llama.fi/tvl/{s}")).await?))))
        })
        .await
        .ok()
        .and_then(|v| *v);
        (slug.clone(), tvl)
    } else if let Some((_, slug, tvl)) =
        index.protocols.iter().filter(|p| p.0 == gecko).max_by(|a, b| a.2.unwrap_or(0.0).total_cmp(&b.2.unwrap_or(0.0)))
    {
        (slug.clone(), *tvl)
    } else {
        return (None, None);
    };
    let g = gecko.to_string();
    let fees = cached(&format!("llama:fees:{slug}"), 6 * HOUR, move || async move {
        let d = get(&format!("https://api.llama.fi/summary/fees/{}?dataType=dailyFees", crate::guard::encode_uri_component(&slug))).await?;
        Ok(parse::fees30d(&d, &g))
    })
    .await
    .ok()
    .and_then(|v| *v);
    (tvl, fees)
}

async fn bitcoin_network() -> (Option<f64>, Option<f64>) {
    cached("btc:stats", HOUR, || async { Ok(parse::blockchain(&get("https://api.blockchain.info/stats").await?)) })
        .await
        .map(|v| *v)
        .unwrap_or((None, None))
}

/// Everything together (pure part: tested offline).
pub fn assemble(
    symbol: &str,
    coin: Option<Coin>,
    dominance: Option<(f64, &'static str)>,
    tvl: Option<f64>,
    fees30d: Option<f64>,
    funding: Option<f64>,
    open_interest: Option<f64>,
    network: (Option<f64>, Option<f64>),
) -> CryptoFundamentals {
    let c = coin.unwrap_or_default();
    let mc_fdv = match (c.market_cap, c.fdv) {
        (Some(m), Some(f)) => Some(round_to((m / f).min(1.0), 3)),
        _ => None,
    };
    let circulating_pct = match (c.circulating, c.max.or(c.total)) {
        (Some(ci), Some(cap)) => Some(round_to((ci / cap * 100.0).min(100.0), 2)),
        _ => None,
    };
    let funding = funding.filter(|f| f.is_finite() && f.abs() < 0.05);
    let open_interest = positive(open_interest);
    let is_btc = symbol.eq_ignore_ascii_case("BTC");
    let mut sources = Vec::new();
    if let Some(c) = coin.filter(|c| c.market_cap.is_some() || c.circulating.is_some() || c.total.is_some() || c.max.is_some()) {
        sources.push(if c.source.is_empty() { "CoinGecko" } else { c.source });
    }
    if let Some((_, name)) = dominance {
        if !sources.contains(&name) {
            sources.push(name);
        }
    }
    if tvl.is_some() || fees30d.is_some() {
        sources.push("DefiLlama");
    }
    if funding.is_some() || open_interest.is_some() {
        sources.push("OKX");
    }
    if is_btc && (network.0.is_some() || network.1.is_some()) {
        sources.push("blockchain.com");
    }
    CryptoFundamentals {
        market_cap: c.market_cap,
        fdv: c.fdv,
        mc_fdv,
        circulating_supply: c.circulating,
        total_supply: c.total,
        max_supply: c.max,
        circulating_pct,
        tvl,
        fees30d,
        btc_dominance: dominance.map(|d| d.0),
        funding_rate: funding,
        open_interest,
        tx_per_day: if is_btc { network.0 } else { None },
        hash_rate: if is_btc { network.1 } else { None },
        unlocks: if is_btc { UNLOCKS_BTC } else { UNLOCKS_PAID }.to_string(),
        source: sources.join(", "),
    }
}

/// Fundamentals of a crypto. Each source is optional (CoinGecko's HTTP 429 only empties its fields); an error only
/// when none of them answered.
pub async fn crypto_fundamentals(symbol: &str) -> Result<CryptoFundamentals> {
    let base = symbol.trim().to_uppercase();
    let is_btc = base == "BTC";
    let gecko = gecko_id(&base).await;
    let (coin, dom, llama, pos, network) = tokio::join!(
        async {
            let from_gecko = match &gecko {
                Some(id) => coin(id).await.filter(|c| c.market_cap.is_some() || c.circulating.is_some()),
                None => None,
            };
            match from_gecko {
                Some(c) => Some(c),
                None => paprika(&base).await,
            }
        },
        dominance(),
        async {
            match &gecko {
                Some(id) => tvl_and_fees(id).await,
                None => (None, None),
            }
        },
        crate::guard::positioning(&base),
        async { if is_btc { bitcoin_network().await } else { (None, None) } },
    );
    let funding = pos.as_ref().and_then(|p| p.funding_rate);
    let oi = pos.as_ref().and_then(|p| p.open_interest.last().copied());
    let f = assemble(&base, coin, dom, llama.0, llama.1, funding, oi, network);
    if f.source.is_empty() {
        return Err(Error(format!("aucune source n'a répondu pour {base} (CoinGecko, DefiLlama, OKX)")));
    }
    Ok(f)
}

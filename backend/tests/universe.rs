//! Port of web/test/universe.test.ts (extracts of real responses, formats checked on 27/09/2026).
use altim::types::Kind;
use altim::universe::{GeckoCoin, Listed, UniverseEntry, build_crypto, build_stocks, clean_stock_name, parse, search_all, search_universe};
use indexmap::IndexMap;
use serde_json::json;

const NASDAQ_LISTED: &str = "Symbol|Security Name|Market Category|Test Issue|Financial Status|Round Lot Size|ETF|NextShares
AAPL|Apple Inc. - Common Stock|Q|N|N|100|N|N
QQQ|Invesco QQQ Trust, Series 1|G|N|N|100|Y|N
GOOG|Alphabet Inc. - Class C Capital Stock|Q|N|N|100|N|N
ZXZZT|NASDAQ TEST STOCK|G|Y|N|100|N|N
ABCDW|Some Acquisition Corp - Warrants|S|N|N|100|N|N
File Creation Time: 0925202621:31|||||||";

const OTHER_LISTED: &str = "ACT Symbol|Security Name|Exchange|CQS Symbol|ETF|Round Lot Size|Test Issue|NASDAQ Symbol
BRK.B|Berkshire Hathaway Inc. New Common Stock|N|BRK.B|N|100|N|BRK.B
BFH$A|Bread Financial Holdings, Inc. Depositary Shares 8.625% Preferred Stock|N|BFHpA|N|100|N|BFH-A
SPY|State Street SPDR S&P 500 ETF Trust|P|SPY|Y|100|N|SPY
TNL|Travel   Leisure Co. Common  Stock|N|TNL|N|100|N|TNL
File Creation Time: 0925202621:31||||||";

fn nasdaq() -> Vec<Listed> {
    parse::nasdaq_directory(NASDAQ_LISTED).unwrap()
}
fn other() -> Vec<Listed> {
    parse::nasdaq_directory(OTHER_LISTED).unwrap()
}
fn syms(list: &[Listed]) -> Vec<&str> {
    list.iter().map(|s| s.symbol.as_str()).collect()
}
fn ids(list: &[&UniverseEntry]) -> Vec<String> {
    list.iter().map(|e| e.0.clone()).collect()
}
fn caps(pairs: &[(&str, f64)]) -> IndexMap<String, f64> {
    pairs.iter().map(|(s, c)| (s.to_string(), *c)).collect()
}

#[test]
fn stock_universe_keeps_stocks_and_etf_drops_test_issues_warrants_preferred() {
    assert_eq!(syms(&nasdaq()), ["AAPL", "QQQ", "GOOG"]);
    assert_eq!(syms(&other()), ["BRK-B", "SPY", "TNL"]);
    assert!(nasdaq().iter().find(|s| s.symbol == "QQQ").unwrap().etf);
}

#[test]
fn class_shares_yahoo_format_and_clean_names() {
    assert_eq!(other()[0].symbol, "BRK-B");
    assert_eq!(clean_stock_name("Apple Inc. - Common Stock"), "Apple Inc.");
    assert_eq!(clean_stock_name("Alphabet Inc. - Class A Common Stock"), "Alphabet Inc. Class A");
    assert_eq!(nasdaq().iter().find(|s| s.symbol == "GOOG").unwrap().name, "Alphabet Inc. Class C");
    assert_eq!(other().iter().find(|s| s.symbol == "TNL").unwrap().name, "Travel Leisure Co.");
    assert_eq!(clean_stock_name("Invesco QQQ Trust, Series 1"), "Invesco QQQ Trust, Series 1");
}

#[test]
fn ranked_by_market_cap_then_alphabetical() {
    let caps = parse::screener(&json!({ "data": { "rows": [
        { "symbol": "AAPL", "marketCap": "3,500,000,000,000" },
        { "symbol": "BRK/B", "marketCap": "1000000000000.00" },
        { "symbol": "GOOG", "marketCap": "2000000000000" },
    ] } }))
    .unwrap();
    let list = build_stocks(&[nasdaq(), other()], &caps);
    let top: Vec<(&str, i64)> = list[..3].iter().map(|e| (e.0.as_str(), e.2)).collect();
    assert_eq!(top, [("AAPL", 1), ("GOOG", 2), ("BRK-B", 3)]);
    // Unranked: the largest ETFs first (SPY before QQQ by assets), then alphabetical.
    let rest: Vec<&str> = list[3..].iter().map(|e| e.0.as_str()).collect();
    assert_eq!(rest, ["SPY", "QQQ", "TNL"]);
    assert_eq!(list.iter().find(|e| e.0 == "SPY").unwrap().3, 1);
}

fn exchanges() -> [Vec<String>; 4] {
    let okx = parse::okx(&json!({ "code": "0", "data": [
        { "instId": "BTC-USDT", "baseCcy": "BTC", "quoteCcy": "USDT", "state": "live" },
        { "instId": "ETH-USDC", "baseCcy": "ETH", "quoteCcy": "USDC", "state": "live" },
        { "instId": "PEPE-USDT", "baseCcy": "PEPE", "quoteCcy": "USDT", "state": "live" },
        { "instId": "OLD-USDT", "baseCcy": "OLD", "quoteCcy": "USDT", "state": "suspend" },
    ] }))
    .unwrap();
    let coinbase = parse::coinbase(&json!([
        { "base_currency": "BTC", "quote_currency": "USD", "status": "online", "trading_disabled": false },
        { "base_currency": "ETH", "quote_currency": "USD", "status": "online", "trading_disabled": false },
        { "base_currency": "USDT", "quote_currency": "USD", "status": "online", "trading_disabled": false },
        { "base_currency": "DEAD", "quote_currency": "USD", "status": "delisted", "trading_disabled": true },
    ]))
    .unwrap();
    let kraken = parse::kraken(&json!({ "error": [], "result": {
        "XXBTZUSD": { "wsname": "XBT/USD" }, "XDGUSD": { "wsname": "XDG/USD" }, "ETHEUR": { "wsname": "ETH/EUR" },
    } }))
    .unwrap();
    let gate = parse::gate(&json!([
        { "base": "BTC3L", "quote": "USDT", "trade_status": "tradable" },
        { "base": "PEPE", "quote": "USDT", "trade_status": "tradable" },
        { "base": "NEWCOIN", "quote": "USDT", "trade_status": "tradable" },
    ]))
    .unwrap();
    [okx, coinbase, kraken, gate]
}

#[test]
fn parsers_keep_only_active_pairs_against_usd_usdt() {
    let [okx, coinbase, kraken, _] = exchanges();
    assert_eq!(okx, ["BTC", "PEPE"]);
    assert_eq!(coinbase, ["BTC", "ETH", "USDT"]);
    assert_eq!(kraken, ["BTC", "DOGE"]);
    // What JavaScript would throw on (the caller falls back to an empty list).
    assert!(parse::okx(&json!({ "data": "x" })).is_err());
    assert!(parse::coinbase(&json!([null])).is_err());
    assert_eq!(parse::okx(&json!(null)).unwrap(), Vec::<String>::new());
}

#[test]
fn union_of_exchanges_without_stablecoins_nor_leveraged_ranked_by_market_cap() {
    let [okx, coinbase, kraken, gate] = exchanges();
    let gecko = parse::gecko(&json!([
        { "symbol": "btc", "name": "Bitcoin", "market_cap_rank": 1 },
        { "symbol": "eth", "name": "Ethereum", "market_cap_rank": 2 },
        { "symbol": "doge", "name": "Dogecoin", "market_cap_rank": 9 },
        { "symbol": "pepe", "name": "Pepe", "market_cap_rank": 30 },
        { "symbol": "pepe", "name": "Pepe copycat", "market_cap_rank": 900 },
    ]))
    .unwrap();
    let names = [("NEWCOIN", "New Coin"), ("NVDAX", "NVIDIA xStock"), ("SPYON", "SPDR S&P 500 ETF (Ondo Tokenized ETF)")]
        .map(|(a, b)| (a.to_string(), b.to_string()));
    let list = build_crypto(&[okx, coinbase, kraken, gate, vec!["NVDAX".into(), "SPYON".into()]], &gecko, &names);
    let symbols: Vec<&str> = list.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(symbols, ["BTC", "ETH", "DOGE", "PEPE", "NEWCOIN"]);
    assert_eq!(list[0], UniverseEntry("BTC".into(), "Bitcoin".into(), 1, 3));
    assert_eq!(list.iter().find(|e| e.0 == "PEPE").unwrap().1, "Pepe");
    assert_eq!(list.iter().find(|e| e.0 == "NEWCOIN").unwrap(), &UniverseEntry("NEWCOIN".into(), "New Coin".into(), 0, 1));
    assert_eq!(serde_json::to_string(&list[0]).unwrap(), r#"["BTC","Bitcoin",1,3]"#);
}

fn stock_list() -> Vec<UniverseEntry> {
    build_stocks(&[nasdaq(), other()], &caps(&[("AAPL", 3e12), ("SPY", 6e11)]))
}

#[test]
fn search_exact_symbol_then_prefix_then_name_accents_ignored() {
    let list = stock_list();
    assert_eq!(ids(&search_universe(&list, "spy", 50)), ["SPY"]);
    assert_eq!(search_universe(&list, "apple", 50)[0].0, "AAPL");
    assert_eq!(search_universe(&list, "berkshire", 50)[0].0, "BRK-B");
    assert_eq!(search_universe(&list, "BRK", 50)[0].0, "BRK-B");
    assert_eq!(search_universe(&list, "tràvel", 50)[0].0, "TNL");
}

#[test]
fn empty_query_whole_list_special_characters_safe() {
    let list = stock_list();
    assert_eq!(search_universe(&list, "", 100).len(), list.len());
    assert!(search_universe(&list, "S&P (500", 50).is_empty());
    assert_eq!(search_universe(&list, "S&P 500", 50)[0].0, "SPY");
}

#[test]
fn cryptos_and_stocks_together_exact_symbols_first_then_largest() {
    let crypto = build_crypto(
        &[vec!["BTC".into(), "SOL".into(), "SPYX".into()]],
        &[GeckoCoin { symbol: "BTC".into(), name: "Bitcoin".into(), rank: 1 }, GeckoCoin { symbol: "SOL".into(), name: "Solana".into(), rank: 7 }],
        &[("SPYX".into(), "Spy Token".into())],
    );
    let stock = build_stocks(&[nasdaq(), other()], &caps(&[("AAPL", 3e12)]));
    let ids = |q: &str| search_all(&crypto, &stock, q, 5).iter().map(|h| format!("{}:{}", h.kind.as_str(), h.e.0)).collect::<Vec<_>>();
    assert_eq!(ids("spy"), ["stock:SPY", "crypto:SPYX"]);
    assert_eq!(ids("apple"), ["stock:AAPL"]);
    assert_eq!(ids("sol")[0], "crypto:SOL");
    assert_eq!(ids("bitcoin"), ["crypto:BTC"]);
    assert_eq!(search_all(&crypto, &stock, "bitcoin", 5)[0].kind, Kind::Crypto);
}

#[test]
fn coingecko_names_only_for_unambiguous_symbols_obscure_tokens_hidden() {
    let names = parse::gecko_names(&json!([
        { "id": "solv-protocol", "symbol": "solv", "name": "Solv Protocol" },
        { "id": "bitcoin", "symbol": "btc", "name": "Bitcoin" },
        { "id": "big-tom-coin", "symbol": "btc", "name": "Big Tom Coin" },
    ]))
    .unwrap();
    assert_eq!(names, [("SOLV".to_string(), "Solv Protocol".to_string())]);
    let crypto = build_crypto(&[vec!["SOLV".into(), "ZZQ".into()], vec!["SOLV".into()]], &[], &names);
    assert_eq!(crypto.iter().find(|e| e.0 == "SOLV").unwrap().1, "Solv Protocol");
    // ZZQ: no name, no rank, one exchange → hidden from the combined search unless typed exactly.
    let found = |q: &str| search_all(&crypto, &[], q, 20).iter().map(|h| h.e.0.clone()).collect::<Vec<_>>();
    assert!(found("zz").is_empty());
    assert_eq!(found("zzq"), ["ZZQ"]);
    assert_eq!(found("solv"), ["SOLV"]);
}

#[test]
fn word_boundary_search_tries_every_position() {
    let list = vec![UniverseEntry("X".into(), "Aaab Corp".into(), 0, 0), UniverseEntry("Y".into(), "Baab".into(), 0, 0)];
    // "AAB" starts inside "AAAB" (no boundary) → name contains only (3); nothing at a word start.
    assert_eq!(ids(&search_universe(&list, "aab", 50)), ["X", "Y"]);
    assert_eq!(ids(&search_universe(&list, "corp", 50)), ["X"]);
}

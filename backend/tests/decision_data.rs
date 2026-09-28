//! Data of `/api/decision` (fundamentals, tokenomics, liquidity, track record): parsers checked on real captured
//! responses (`tests/samples/decision-data/`, trimmed) against values computed by hand from the filings, plus
//! `#[ignore]` live runs: `cargo test --test decision_data -- --ignored --nocapture`.
mod common;

use altim::engine::backtest::backtest_default;
use altim::engine::metrics::{SLIPPAGE, track};
use altim::fundamentals::{self, Street, assemble, filed, parse as sec};
use altim::liquidity::{self, parse as book, spread_pct, traded};
use altim::tokenomics::{self, Coin, UNLOCKS_BTC, UNLOCKS_PAID, parse as tok};
use altim::types::{Candle, Kind};
use serde_json::Value;

fn sample(name: &str) -> Value {
    let path = format!("{}/tests/samples/decision-data/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} absent"))).unwrap()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

// ---------- SEC EDGAR ----------

#[test]
fn sec_tickers() {
    let m = sec::tickers(&sample("sec-company-tickers.json"));
    assert_eq!(m.get("AAPL"), Some(&320193));
    assert_eq!(m.get("NVDA"), Some(&1045810));
    let m = sec::ticker_txt("aapl\t320193\nbrk-b\t1067983\n");
    assert_eq!((m.get("AAPL"), m.get("BRK-B")), (Some(&320193), Some(&1067983)));
    // EDGAR search: the entity listing exactly this ticker, not "GOOGLE INC".
    let d = sample("sec-efts-googl.json");
    assert_eq!(sec::efts(&d, "GOOGL"), Some(1652044));
    assert_eq!(sec::efts(&d, "GOOG"), Some(1652044));
    assert_eq!(sec::efts(&d, "GOO"), None);
}

/// Apple, 10-Q of the quarter ended 27/06/2026 (filed 31/07/2026). Expected values computed by hand from the filed
/// figures (fiscal year ends in September):
/// revenue TTM = Q3 FY26 109.417 + Q2 FY26 111.184 + Q1 FY26 143.756 + Q4 FY25 (FY25 416.161 − 9 months 313.695)
///             = 466.823 bn$; a year earlier 94.036 + 95.359 + 124.300 + (391.035 − 296.105) = 408.625 bn$.
#[test]
fn apple_ttm_by_hand() {
    let facts = sec::company_facts(&sample("sec-aapl-companyfacts.json"));
    assert_eq!(facts.name, "Apple Inc.");
    let f = filed(&facts).expect("comptes Apple");
    assert_eq!(f.period, "12 mois au 27/06/2026 (10-Q)");
    assert_eq!(f.revenue, Some(466_823_000_000.0));
    // 466.823 / 408.625 − 1
    assert_eq!(f.revenue_growth, Some(14.24));
    // 29.789 + … : 9 months FY26 + FY25 − 9 months FY25
    assert_eq!(f.net_income, Some(128_930_000_000.0));
    // 2.02 + 2.01 + 2.84 + (7.46 − 5.62); a year earlier 1.57 + 1.65 + 2.40 + (6.08 − 5.11) = 6.59
    assert_eq!(f.eps, Some(8.71));
    assert_eq!(f.eps_growth, Some(32.17));
    // 227.123 / 466.823, 154.859 / 466.823, 128.930 / 466.823
    assert_eq!(f.gross_margin, Some(48.65));
    assert_eq!(f.operating_margin, Some(33.17));
    assert_eq!(f.net_margin, Some(27.62));
    // Operating cash flow only filed year-to-date: quarters derived by difference. 146.724 − capex 10.041
    assert_eq!(f.free_cash_flow, Some(136_683_000_000.0));
    assert_eq!(f.fcf_margin, Some(29.28));
    // Term debt 82.300 (current portion included) + commercial paper 1.997; cash 39.544
    assert_eq!(f.debt, Some(84_297_000_000.0));
    assert_eq!(f.cash, Some(39_544_000_000.0));
    assert_eq!(f.net_debt, Some(44_753_000_000.0));
    // 128.930 / ((107.520 + 65.830) / 2)
    assert_eq!(f.roe, Some(148.75));
    // Operating income 154.859 + D&A 13.100
    assert_eq!(f.ebitda, Some(167_959_000_000.0));
    // 0.27 + 0.26 + 0.26 + (1.02 − 0.76)
    assert!(close(f.dividends_per_share.unwrap(), 1.05, 1e-9));
    // Cover page 17/07/2026: 14 594 180 000 shares, 18/07/2025: 14 840 390 000
    assert_eq!(f.shares, Some(14_594_180_000.0));
    assert_eq!(f.share_change, Some(-1.66));

    let street = Street {
        next_earnings: sec::earnings_date(&sample("nasdaq-aapl-earnings-date.json")),
        surprises: sec::surprises(&sample("nasdaq-aapl-earnings-surprise.json")),
        revisions: sec::revisions(&sample("nasdaq-aapl-estimate-momentum.json")),
    };
    let now = 1_790_600_000_000; // 28/09/2026
    let s = assemble(Some(&f), Some(&street), Some(341.29), now, "");
    // 341.29 / 8.71, then ÷ 32.17
    assert_eq!(s.per, Some(39.18));
    assert_eq!(s.peg, Some(1.22));
    // (341.29 × 14.59418 bn + 84.297 − 39.544) / 167.959
    assert_eq!(s.ev_ebitda, Some(29.92));
    assert_eq!(s.dividend_yield, Some(0.31));
    assert_eq!(s.source, "SEC EDGAR (10-K, 10-Q), Nasdaq (données Zacks)");
    assert!(s.sector_note.contains("non disponible"));
    // Without a price: no valuation ratio, the filed figures stay.
    let s = assemble(Some(&f), None, None, now, "");
    assert_eq!((s.per, s.peg, s.ev_ebitda, s.dividend_yield), (None, None, None, None));
    assert_eq!(s.revenue, Some(466_823_000_000.0));
    assert_eq!(s.source, "SEC EDGAR (10-K, 10-Q)");
}

#[test]
fn ttm_uses_the_annual_figure_or_four_quarters() {
    use fundamentals::{Fact, ttm};
    let d = |s: &str| (chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days();
    let f = |start: &str, end: &str, val: f64| Fact { start: Some(d(start)), end: d(end), val, form: "10-Q".into(), filed: d(end) + 30 };
    // Latest filing is a 10-K: the annual figure.
    let facts = vec![f("2025-01-01", "2025-12-31", 400.0), f("2025-10-01", "2025-12-31", 120.0)];
    assert_eq!(ttm(&facts, d("2025-12-31")), Some(400.0));
    // Only three quarters: not twelve months.
    let facts = vec![f("2025-01-01", "2025-03-31", 90.0), f("2025-04-01", "2025-06-30", 100.0), f("2025-07-01", "2025-09-30", 110.0)];
    assert_eq!(ttm(&facts, d("2025-09-30")), None);
    // Q4 derived: FY − 9 months.
    let facts = vec![
        f("2024-01-01", "2024-09-30", 270.0),
        f("2024-01-01", "2024-12-31", 380.0),
        f("2025-01-01", "2025-03-31", 90.0),
        f("2025-01-01", "2025-06-30", 190.0),
        f("2025-01-01", "2025-09-30", 300.0),
    ];
    // Q3 = 300 − 190, Q2 = 190 − 90, Q1 = 90, Q4 2024 = 380 − 270
    assert_eq!(ttm(&facts, d("2025-09-30")), Some(410.0));
}

#[test]
fn nasdaq_street_data() {
    let e = sec::earnings_date(&sample("nasdaq-aapl-earnings-date.json")).unwrap();
    assert_eq!(e.date, 1_793_232_000_000); // 29/10/2026 00:00 UTC
    assert!(e.estimated, "« derived from an algorithm » : date estimée");
    let confirmed = serde_json::json!({"data": {"reportText": "Apple Inc. is expected to report earnings on 10/29/2026 after market close.", "announcement": "Earnings announcement* for AAPL: Oct 29, 2026"}});
    assert!(!sec::earnings_date(&confirmed).unwrap().estimated);

    let s = sec::surprises(&sample("nasdaq-aapl-earnings-surprise.json"));
    assert_eq!(s.len(), 4);
    assert_eq!((s[0].quarter.as_str(), s[0].eps, s[0].consensus, s[0].surprise_pct), ("T2 2026", 1.91, 1.88, 1.6));
    assert_eq!(s[3].quarter, "T3 2025");

    let r = sec::revisions(&sample("nasdaq-aapl-estimate-momentum.json")).unwrap();
    assert_eq!((r.month_ago, r.now, r.change_pct), (8.76, 8.74, -0.23));

    // An ETF: nothing published, no error.
    assert_eq!(sec::earnings_date(&sample("nasdaq-spy-earnings-date.json")), None);
    assert!(sec::surprises(&sample("nasdaq-spy-earnings-surprise.json")).is_empty());
    let etf = assemble(None, Some(&Street::default()), Some(765.0), 0, "Comptes non disponibles : fonds");
    assert_eq!(etf.period, "Comptes non disponibles : fonds");
    assert_eq!((etf.revenue, etf.per, etf.next_earnings.clone()), (None, None, None));
    assert_eq!(etf.source, "Aucune donnée publiée (SEC EDGAR, Nasdaq)");
}

// ---------- Tokenomics ----------

#[test]
fn coingecko_coin_and_search() {
    let btc = tok::coin(&sample("coingecko-bitcoin.json"));
    assert_eq!(btc.market_cap, Some(1_674_021_184_050.0));
    assert_eq!((btc.circulating, btc.max), (Some(20_090_481.0), Some(21_000_000.0)));
    let uni = tok::coin(&sample("coingecko-uniswap.json"));
    assert_eq!(uni.fdv, Some(7_841_692_501.0));
    assert_eq!(tok::search(&sample("coingecko-search-uni.json"), "UNI").as_deref(), Some("uniswap"));
    // Rate-limited answer: nothing, no error.
    assert_eq!(tok::coin(&serde_json::json!({"status": {"error_code": 429}})), Coin::default());
    // More in circulation than the maximum: dropped.
    let bad = serde_json::json!({"market_data": {"circulating_supply": 30.0, "max_supply": 21.0}});
    assert_eq!(tok::coin(&bad).circulating, None);
}

#[test]
fn crypto_assemble() {
    let btc = tok::coin(&sample("coingecko-bitcoin.json"));
    let net = tok::blockchain(&sample("blockchain-stats.json"));
    assert_eq!(net.0, Some(815_875.0));
    assert!(close(net.1.unwrap(), 1.0625017765427416e21, 1e9));
    let dom = tok::dominance(&sample("coinpaprika-global.json"));
    assert_eq!(dom, Some(55.99));
    let f = tokenomics::assemble("BTC", Some(btc), dom.map(|d| (d, "CoinPaprika")), None, None, Some(0.0000688), Some(3.16e9), net);
    assert_eq!(f.mc_fdv, Some(1.0));
    // 20 090 481 / 21 000 000
    assert_eq!(f.circulating_pct, Some(95.67));
    assert_eq!(f.unlocks, UNLOCKS_BTC);
    assert_eq!(f.source, "CoinGecko, CoinPaprika, OKX, blockchain.com");
    assert_eq!(f.funding_rate, Some(0.0000688));

    let uni = tok::coin(&sample("coingecko-uniswap.json"));
    let f = tokenomics::assemble("UNI", Some(uni), None, Some(3.9e9), Some(2.0e8), None, None, (Some(1.0), Some(1.0)));
    // 5 480 814 763 / 7 841 692 501; 620 420 422.6 / 1 000 000 000
    assert_eq!(f.mc_fdv, Some(0.699));
    assert_eq!(f.circulating_pct, Some(62.04));
    assert_eq!(f.unlocks, UNLOCKS_PAID);
    assert_eq!((f.tx_per_day, f.hash_rate), (None, None), "Bitcoin seulement");
    // Absurd funding rate: dropped.
    assert_eq!(tokenomics::assemble("X", None, None, None, None, Some(0.5), None, (None, None)).funding_rate, None);
}

#[test]
fn blockchain_nonsense_rejected() {
    let bad = serde_json::json!({"n_tx": -3, "hash_rate": 1e30, "total_fees_btc": -50625000000i64});
    assert_eq!(tok::blockchain(&bad), (None, None));
}

#[test]
fn defillama() {
    let chains = tok::llama_chains(&sample("defillama-chains.json"));
    assert!(chains.iter().any(|(g, n, t)| g == "ethereum" && n == "Ethereum" && t.is_some()));
    let (parents, protocols) = tok::llama_protocols(&sample("defillama-lite-protocols2.json"));
    assert!(parents.contains(&("uniswap".to_string(), "uniswap".to_string())));
    assert!(protocols.iter().any(|(g, s, t)| g == "lido-dao" && s == "lido" && t.is_some()));
    assert_eq!(tok::slug("Aave V3"), "aave-v3");
    let fees = sample("defillama-fees-uniswap.json");
    assert!(close(tok::fees30d(&fees, "uniswap").unwrap(), 207_642_487.16, 0.01));
    // Another coin's answer is never used.
    assert_eq!(tok::fees30d(&fees, "ethereum"), None);
}

// ---------- Liquidity ----------

#[test]
fn order_books() {
    let (b, a) = book::okx(&sample("okx-btc-ticker.json")).unwrap();
    assert_eq!((b, a), (83383.2, 83383.3));
    // 0.1 / 83 383.25
    assert_eq!(spread_pct(b, a), Some(0.00012));
    let (b, a) = book::coinbase(&sample("coinbase-btc-book.json")).unwrap();
    assert!(spread_pct(b, a).unwrap() < 0.001);
    let (b, a) = book::nasdaq(&sample("nasdaq-aapl-quote-info.json")).unwrap();
    assert_eq!((b, a), (341.33, 341.36));
    // 0.03 / 341.345
    assert_eq!(spread_pct(b, a), Some(0.008789));
    let (b, a) = book::robinhood(&sample("robinhood-aapl-quote.json")).unwrap();
    assert!(b > 0.0 && a >= b);
    // Crossed, empty or absurd books.
    assert_eq!(spread_pct(10.0, 9.0), None);
    assert_eq!(spread_pct(0.0, 1.0), None);
    assert_eq!(spread_pct(1.0, 2.0), None);
    assert_eq!(book::nasdaq(&sample("nasdaq-spy-earnings-date.json")), None);
}

#[test]
fn traded_value_and_relative_volume() {
    let c = |i: i64, close: f64, volume: f64| Candle { time: i * 86_400_000, open: close, high: close, low: close, close, volume };
    let mut v: Vec<Candle> = (0..20).map(|i| c(i, 10.0, 100.0)).collect();
    assert_eq!(traded(&v), (Some(1000.0), None), "20 bougies : valeur, pas de volume relatif");
    v.push(c(20, 20.0, 300.0));
    // Last 20: 19 × 1000 + 6000 → 1250; 300 ÷ 100
    assert_eq!(traded(&v), (Some(1250.0), Some(3.0)));
    assert_eq!(traded(&v[..5]), (None, None));
    v[10].volume = f64::NAN;
    assert_eq!(traded(&v), (None, None));
}

// ---------- Track record ----------

#[test]
fn track_on_real_candles() {
    for (sym, kind) in [("BTC", Kind::Crypto), ("AAPL", Kind::Stock), ("SPY", Kind::Stock)] {
        let input = common::find(sym, "long").unwrap();
        let t = track(&input.candles, kind).unwrap();
        let r = backtest_default(&input.candles);
        assert_eq!(t.trades, r.trades.len());
        // Fully invested each time: the final equity is the product of the trade returns, slippage included.
        let product = r.trades.iter().fold(1.0, |acc, x| acc * (1.0 + x.return_percent / 100.0) * (1.0 - SLIPPAGE) / (1.0 + SLIPPAGE));
        assert!(close(t.total_return, (product - 1.0) * 100.0, 0.01), "{sym} {} {}", t.total_return, (product - 1.0) * 100.0);
        assert!(t.total_return <= r.total_return_percent + 1e-9, "le glissement ne peut qu'enlever");
        assert!(t.max_drawdown <= 0.0);
        assert_eq!((t.fees_pct, t.slippage_pct), (0.1, 0.05));
        assert!(t.note.contains("bougies déjà clôturées"));
        if t.total_return < t.buy_and_hold {
            assert!(t.note.contains("moins bien que la simple détention"));
        }
        if let Some(l) = t.avg_loss {
            assert!(l <= 0.0);
        }
        println!("{sym} : {t:?}");
    }
    assert_eq!(track(&common::find("BTC", "long").unwrap().candles[..50], Kind::Crypto), None);
}

// ---------- Live ----------

#[tokio::test]
#[ignore]
async fn live_stocks() {
    for sym in ["AAPL", "NVDA", "SPY"] {
        let candles = altim::market::long_daily(sym, Kind::Stock).await.map(|c| c.candles).unwrap_or_default();
        let price = candles.last().map(|c| c.close);
        let f = fundamentals::stock_fundamentals(sym, price).await.unwrap();
        println!("\n{sym} (cours {price:?}) : {}", serde_json::to_string_pretty(&f).unwrap());
        let l = liquidity::liquidity(sym, Kind::Stock, &candles).await;
        println!("{sym} liquidité : {l:?}");
        println!("{sym} historique : {:?}", track(&candles, Kind::Stock));
        if sym == "AAPL" {
            let rev = f.revenue.unwrap();
            assert!((3e11..7e11).contains(&rev), "chiffre d'affaires d'Apple sur 12 mois : {rev}");
        }
        if sym == "SPY" {
            assert_eq!(f.revenue, None);
        }
    }
}

#[tokio::test]
#[ignore]
async fn live_cryptos() {
    for sym in ["BTC", "ETH", "SOL", "UNI"] {
        let candles = altim::market::long_daily(sym, Kind::Crypto).await.map(|c| c.candles).unwrap_or_default();
        let f = tokenomics::crypto_fundamentals(sym).await;
        println!("\n{sym} : {f:#?}");
        let l = liquidity::liquidity(sym, Kind::Crypto, &candles).await;
        println!("{sym} liquidité : {l:?}");
        println!("{sym} historique : {:?}", track(&candles, Kind::Crypto));
        if sym == "BTC" {
            if let Some(c) = f.as_ref().ok().and_then(|f| f.circulating_supply) {
                assert!((19.5e6..21e6).contains(&c));
            }
        }
    }
}

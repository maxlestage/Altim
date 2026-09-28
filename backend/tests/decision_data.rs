//! Data of `/api/decision` (fundamentals, tokenomics, liquidity, track record): parsers checked on real captured
//! responses (`tests/samples/decision-data/`, trimmed) against values computed by hand from the filings, plus
//! `#[ignore]` live runs: `cargo test --test decision_data -- --ignored --nocapture`.
mod common;

use altim::engine::backtest::backtest_default;
use altim::engine::metrics::{SLIPPAGE, SPREAD_CRYPTO, SPREAD_STOCK, track, track_with_spread};
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
        // Fully invested each time: the final equity is the product of the trade returns, slippage and half the
        // default spread included on each side.
        let spread = if kind == Kind::Crypto { SPREAD_CRYPTO } else { SPREAD_STOCK };
        assert_eq!((t.details.spread_pct, t.details.spread_measured), (spread, false));
        assert!(t.details.spread_note.starts_with("Hypothèse"));
        let side = SLIPPAGE + spread / 200.0;
        let product = r.trades.iter().fold(1.0, |acc, x| acc * (1.0 + x.return_percent / 100.0) * (1.0 - side) / (1.0 + side));
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
        // Expectancy = win rate × average win − loss rate × |average loss| (rounded inputs: loose tolerance).
        if let (Some(e), Some(w), Some(l)) = (t.details.expectancy, t.avg_win, t.avg_loss) {
            let p = t.win_rate / 100.0;
            assert!(close(e, p * w + (1.0 - p) * l, 0.05), "{sym} espérance {e}");
        }
        assert!(t.details.avg_r.is_some() == (t.trades > 0));
        // Every trade lands in exactly one regime; the four main regimes are always listed.
        assert_eq!(t.details.regimes.iter().map(|g| g.trades).sum::<usize>(), t.trades);
        assert!(t.details.regimes.len() >= 4);
        assert!(t.details.regimes.iter().all(|g| g.low_sample == (g.trades < 5)));
        assert_eq!(t.details.tested_bars, r.equity.len());
        assert!(t.details.bias_notes.iter().any(|n| n.contains("aucune optimisation")));
        // A measured spread replaces the assumption and can only cost more when it is wider.
        let wide = track_with_spread(&input.candles, kind, Some(0.5)).unwrap();
        assert!(wide.details.spread_measured && wide.details.spread_note.contains("mesuré"));
        assert!(wide.total_return <= t.total_return + 1e-9);
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
        let f = fundamentals::stock_fundamentals(sym, price, &candles).await.unwrap();
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

/// CoinPaprika, fallback of CoinGecko for market cap and supply (real response, trimmed).
#[test]
fn paprika_fallback() {
    let d: serde_json::Value = serde_json::json!([
        { "id": "btc-bitcoin", "symbol": "BTC", "rank": 1, "total_supply": 20090681, "max_supply": 21000000,
          "quotes": { "USD": { "price": 83741.30302596571, "market_cap": 1682419805619.0 } } },
        { "id": "btc-fake", "symbol": "BTC", "rank": 900, "total_supply": 1, "max_supply": 1,
          "quotes": { "USD": { "price": 1.0, "market_cap": 1.0 } } }
    ]);
    let c = altim::tokenomics::parse::paprika(&d, "btc").unwrap();
    assert_eq!(c.source, "CoinPaprika");
    assert_eq!(c.market_cap, Some(1682419805619.0));
    let circ = c.circulating.unwrap();
    assert!((circ - 20_090_681.0).abs() < 1_000.0, "{circ}");
    assert!((c.fdv.unwrap() - 83741.30302596571 * 21e6).abs() < 1.0);
    assert!(altim::tokenomics::parse::paprika(&d, "ZZZ").is_none());
}

// ---------- Valuation history, ROIC, sector, peers ----------

/// Apple's facts since mid-2019 (companyfacts trimmed to the concepts read, 10-K / 10-Q only) and 5 years of
/// StockAnalysis daily closes (to 25/09/2026), both captured 28/09/2026.
#[test]
fn ratios_roic_and_valuation_history() {
    let facts = sec::company_facts(&sample("sec-aapl-companyfacts-5y.json"));
    let f = filed(&facts).unwrap();
    assert_eq!(f.period, "12 mois au 27/06/2026 (10-Q)");
    let s = assemble(Some(&f), None, Some(341.07), 1_790_600_000_000, "");
    // 341.07 × 14 594 180 000 = 4 977.6 bn; ÷ revenue 466.823 bn, ÷ equity 107.520 bn
    assert_eq!(s.ps, Some(10.66));
    assert_eq!(s.pb, Some(46.29));
    // Effective tax = income tax ÷ pre-tax income (12 months); NOPAT = 154.859 × (1 − t); ÷ (84.297 + 107.520 − 39.544)
    let t = f.tax_rate.unwrap();
    assert!((0.15..0.2).contains(&t), "{t}");
    let expected = 154.859 * (1.0 - t) / (84.297 + 107.520 - 39.544) * 100.0;
    assert!(close(s.roic.unwrap(), expected, 0.01), "{:?} vs {expected}", s.roic);
    assert_eq!((s.roic_tax_rate, s.roic_tax_statutory), (Some(17.3), false));
    // Dates of the figures: period end 27/06/2026, 10-Q filed 31/07/2026 (midnight UTC).
    assert_eq!(s.period_end, Some(1_782_518_400_000));
    assert_eq!(s.filed_at, Some(1_785_456_000_000));
    assert!(s.guidance.contains("non disponibles"));
    // Without effective rate: the statutory 21 %, said so.
    let mut no_tax = f.clone();
    no_tax.tax_rate = None;
    assert_eq!(fundamentals::roic(&no_tax).map(|r| (r.1, r.2)), Some((21.0, true)));

    let closes: Vec<(i64, f64)> =
        altim::market::parse_stock::stockanalysis(&sample("stockanalysis-aapl-5y.json")).unwrap().iter().map(|c| (c.time, c.close)).collect();
    let h = fundamentals::valuation_history(&facts, &closes, s.per, s.ps, "StockAnalysis").unwrap();
    let per = h.per.as_ref().unwrap();
    // No split in the window: every day kept (27/09/2021 → 25/09/2026).
    assert_eq!(per.days, 1255);
    assert_eq!((per.current, per.median, per.min, per.max, per.percentile), (39.16, 31.29, 20.46, 42.6, 95.0));
    let ps = h.ps.as_ref().unwrap();
    assert_eq!((ps.median, ps.percentile), (7.7, 99.0));
    assert!(h.source.ends_with("StockAnalysis"));
    let v = fundamentals::valuation_verdict(Some(&h), s.peg, s.eps_growth).unwrap();
    assert!(v.starts_with("Valorisation élevée par rapport à sa propre histoire (PER de 39,2 ; plus haut que 95 % des jours sur 5 ans)"), "{v}");
    assert!(v.ends_with("croissance qui la justifie en partie (PEG 1,22)"), "{v}");
    assert!(fundamentals::valuation_verdict(Some(&h), Some(3.1), Some(8.0)).unwrap().ends_with("croissance qui ne la justifie pas (PEG 3,1)"));
    assert!(fundamentals::valuation_verdict(Some(&h), None, Some(-4.0)).unwrap().contains("bénéfice par action en recul"));

    // A 4-for-1 split not restated in the older filings: the days on the old basis are left out, never adjusted.
    let mut split = facts.clone();
    for n in ["EarningsPerShareDiluted", "EntityCommonStockSharesOutstanding"] {
        for x in split.concepts.get_mut(n).unwrap().iter_mut().filter(|x| x.end < 19_000) {
            x.val *= if n.starts_with("Earnings") { 4.0 } else { 0.25 };
        }
    }
    let h2 = fundamentals::valuation_history(&split, &closes, s.per, s.ps, "StockAnalysis").unwrap();
    assert!(h2.per.as_ref().unwrap().days < 1255 && h2.per.as_ref().unwrap().days >= 250, "{h2:?}");
    // Under a year of history: nothing.
    assert_eq!(fundamentals::valuation_history(&facts, &closes[closes.len() - 200..], s.per, s.ps, "x"), None);
}

#[test]
fn sector_from_the_sic_code() {
    let s = sec::submissions(&sample("sec-aapl-submissions.json")).unwrap();
    assert_eq!((s.label.as_str(), s.sic.as_str(), s.sic_description.as_str()), ("Industrie", "3571", "Electronic Computers"));
    assert_eq!(sec::sic_division("6022"), Some("Finance et immobilier"));
    assert_eq!(sec::sic_division("7372"), Some("Services"));
    assert_eq!(sec::sic_division(""), None);
    assert_eq!(sec::submissions(&serde_json::json!({"sic": ""})), None);
}

#[test]
fn peers_of_the_same_activity() {
    let rows = sec::screener(&sample("nasdaq-screener-computers.json"));
    let apple = rows.iter().find(|r| r.symbol == "AAPL").unwrap();
    assert_eq!((apple.name.as_str(), apple.industry.as_str(), apple.price), ("Apple Inc.", "Computer Manufacturing", Some(341.07)));
    let (group, peers) = fundamentals::pick_peers(&rows, "aapl", 4).unwrap();
    assert_eq!(group, "Computer Manufacturing");
    // Closest market caps first; Microsoft is in another activity.
    let syms: Vec<&str> = peers.iter().map(|p| p.symbol.as_str()).collect();
    // Super Micro's preferred depositary shares (SMCIP) are not its common stock: left out.
    assert_eq!(syms, ["DELL", "IBM", "SMCI", "HPQ"]);
    assert!(fundamentals::pick_peers(&rows, "ZZZZ", 4).is_none());

    let facts = sec::company_facts(&sample("sec-aapl-companyfacts-5y.json"));
    let f = filed(&facts).unwrap();
    let p = fundamentals::peer(apple, &f);
    assert_eq!((p.per, p.ps, p.operating_margin), (Some(39.16), Some(10.66), Some(33.17)));
    let mk = |sym: &str, per: f64, margin: f64| altim::engine::decision_types::Peer {
        symbol: sym.into(),
        name: sym.into(),
        per: Some(per),
        ps: None,
        operating_margin: Some(margin),
        net_margin: None,
        revenue_growth: None,
        period_end: 0,
    };
    assert_eq!(fundamentals::compare("X", vec![mk("A", 10.0, 5.0), mk("B", 20.0, 7.0)], 0), None, "2 peers: no comparison");
    let c = fundamentals::compare("Computer Manufacturing", vec![mk("A", 10.0, 5.0), mk("B", 20.0, 7.0), mk("C", 16.0, 9.0)], 0).unwrap();
    assert_eq!((c.median_per, c.median_operating_margin, c.median_ps), (Some(16.0), Some(7.0), None));
    let s = assemble(Some(&f), None, Some(341.07), 0, "");
    let note = fundamentals::peer_note(&s, &c);
    assert_eq!(
        note,
        "Comparée à 3 sociétés de même activité (Computer Manufacturing, classement Nasdaq), en médiane : PER 39,2 contre 16, marge opérationnelle 33,2 % contre 7 %"
    );
}

// ---------- Developer activity, stablecoins ----------

#[test]
fn developer_activity() {
    // CoinGecko's documented `developer_data` (no longer returned by the public API on 28/09/2026: read when present).
    let d = serde_json::json!({"categories": ["Smart Contract Platform", "Layer 1 (L1)"], "developer_data": {
        "forks": 21000, "stars": 48000, "subscribers": 2300, "total_issues": 9000, "closed_issues": 8700,
        "pull_requests_merged": 11200, "pull_request_contributors": 850,
        "code_additions_deletions_4_weeks": {"additions": 5210, "deletions": -3120}, "commit_count_4_weeks": 97}});
    let a = tok::developer(&d).unwrap();
    assert_eq!((a.commits4w, a.pull_requests_merged, a.contributors, a.stars), (Some(97.0), Some(11200.0), Some(850.0), Some(48000.0)));
    assert_eq!((a.additions4w, a.deletions4w, a.smart_contract_platform), (Some(5210.0), Some(3120.0), true));
    assert_eq!(tok::developer(&sample("coingecko-ethereum-links.json")), None);
    assert_eq!(tok::developer(&serde_json::json!({"developer_data": {"commit_count_4_weeks": null}})), None);
    // Real CoinGecko page (trimmed): repositories and categories.
    let eth = sample("coingecko-ethereum-links.json");
    assert_eq!(tok::github_repo(&eth).as_deref(), Some("ethereum/go-ethereum"));
    assert!(tok::smart_contract_platform(&eth));
    assert_eq!(tok::github_repo(&serde_json::json!({"links": {"repos_url": {"github": ["https://github.com/org"]}}})), None);
    // GitHub REST formats (documented shapes: GitHub is not reachable from the test machine).
    let weeks: Vec<Value> =
        (0..52).map(|i| serde_json::json!({"week": 1_700_000_000 + i * 604_800, "total": i, "days": [0, 0, 0, 0, 0, 0, 0]})).collect();
    assert_eq!(tok::github_commits4w(&Value::Array(weeks)), Some((48 + 49 + 50 + 51) as f64));
    assert_eq!(tok::github_commits4w(&serde_json::json!({})), None, "HTTP 202: statistics being computed");
    let freq = serde_json::json!([[1, 10, -1], [2, 20, -2], [3, 30, -3], [4, 40, -4], [5, 50, -5]]);
    assert_eq!(tok::github_lines4w(&freq), Some((140.0, 14.0)));
    assert_eq!(tok::github_merged(&serde_json::json!({"total_count": 5321, "incomplete_results": false, "items": []})), Some(5321.0));
    assert_eq!(tok::github_merged(&serde_json::json!({"total_count": 5321, "incomplete_results": true})), None);
    assert_eq!(tok::github_stars(&serde_json::json!({"stargazers_count": 49000, "archived": false})), Some(49000.0));
    assert_eq!(tok::github_stars(&serde_json::json!({"stargazers_count": 49000, "archived": true})), None);
}

#[test]
fn stablecoin_flows() {
    // DefiLlama stablecoincharts (last 40 days, captured 28/09/2026): every peg's USD value summed.
    let s = tok::stablecoins(&sample("defillama-stablecoincharts-all.json"), "Tous réseaux").unwrap();
    assert_eq!(s.date, 1_790_553_600_000);
    assert_eq!(s.total, 313_088_296_545.0);
    assert_eq!((s.change7d, s.change7d_pct), (Some(2_801_975_209.0), Some(0.9)));
    assert_eq!((s.change30d, s.change30d_pct), (Some(4_225_368_391.0), Some(1.37)));
    let e = tok::stablecoins(&sample("defillama-stablecoincharts-ethereum.json"), "Ethereum").unwrap();
    assert_eq!((e.scope.as_str(), e.total, e.change30d_pct), ("Ethereum", 148_452_954_020.0, Some(-0.18)));
    assert_eq!(tok::stablecoins(&serde_json::json!([]), "x"), None);

    let f = tokenomics::assemble("ETH", None, None, Some(1.0), None, None, None, (None, None));
    assert!(f.not_covered.contains("baleines") && f.not_covered.contains("liquidations") && f.not_covered.contains("staking"));
    let f = tokenomics::with_onchain(f, None, Some(s), Some(e));
    assert_eq!(f.source, "DefiLlama, DefiLlama (stablecoins)");
    assert!(f.stablecoins.is_some() && f.chain_stablecoins.is_some() && f.dev_activity.is_none());
}

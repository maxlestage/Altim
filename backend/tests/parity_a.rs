//! Engines A (backtest, history, brief, news) vs the TypeScript ones: golden files from `bun parity/golden.ts`
//! (`parity/golden-engines-a.ts`), plus the unit tests of `web/test/{signal,history,brief,news}.test.ts`.
mod common;
use std::collections::HashMap;

use altim::engine::backtest::{
    self, BacktestResult, backtest, backtest_default, backtest_with_risk, regime_at, regime_split, track_record, trade_stats,
};
use altim::engine::brief::{BriefBuy, MarketLevel, Mover, PriceNow, headline, movers};
use altim::engine::history::{Close, HistoryLine, portfolio_history};
use altim::engine::news::{
    self, FeedItems, NewsCategory, NewsTheme, NewsTone, RawNews, WatchAsset, aggregate, decode_text, mentions, news_digest, parse_feed, safe_link,
    same_story, top_stories,
};
use altim::js::to_value;
use altim::types::Kind;
use common::*;
use serde_json::{Value, json};

fn from<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap()
}

// ---------- backtest ----------

#[test]
fn parity_backtest() {
    let cases = golden("backtest");
    assert!(cases.len() >= 40);
    for c in cases {
        let a = &c.args;
        let i = find(a["symbol"].as_str().unwrap(), a["interval"].as_str().unwrap()).unwrap();
        let r = if a.get("feeRate").is_some() {
            backtest(
                &i.candles,
                a["feeRate"].as_f64().unwrap(),
                a["lookback"].as_u64().unwrap() as usize,
                a["rewardRisk"].as_f64().unwrap(),
                a["warmup"].as_u64().unwrap() as usize,
            )
        } else {
            backtest_default(&i.candles)
        };
        let out = json!({ "r": r, "track": track_record(&r) });
        assert_same(&to_value(&out), &c.output, &format!("backtest {a}"));
    }
}

#[test]
fn parity_backtest_stats() {
    let cases = golden("backtest-stats");
    assert!(cases.len() >= 15);
    for c in cases {
        let a = &c.args;
        if a["symbol"] == "none" {
            let out = json!({ "stats": trade_stats(&[], &[]), "mixed": trade_stats(&[2.0, -1.0, 0.0, 3.5], &[1.0, 0.0, 2.0]) });
            assert_same(&to_value(&out), &c.output, "backtest-stats constantes");
            continue;
        }
        let i = find(a["symbol"].as_str().unwrap(), a["interval"].as_str().unwrap()).unwrap();
        let (r, risks) = backtest_with_risk(&i.candles, backtest::FEE_RATE, backtest::LOOKBACK, backtest::REWARD_RISK, backtest::WARMUP);
        assert_eq!(r, backtest_default(&i.candles));
        assert_eq!(risks.len(), r.trades.len());
        let returns: Vec<f64> = r.trades.iter().map(|t| t.return_percent).collect();
        let net: Vec<f64> = returns.iter().map(|x| (1.0 + x / 100.0) * (1.0 - 0.0006) / (1.0 + 0.0006) * 100.0 - 100.0).collect();
        let candles = altim::engine::signal::sanitize(&i.candles);
        let regimes: Vec<_> = (0..candles.len()).map(|k| regime_at(&candles, k)).collect();
        let out = json!({
            "risks": risks,
            "stats": trade_stats(&returns, &risks),
            "statsNet": trade_stats(&net, &risks),
            "split": regime_split(&i.candles, &r.trades, &net),
            "regimes": regimes,
        });
        assert_same(&to_value(&out), &c.output, &format!("backtest-stats {a}"));
    }
}

fn fixture_candles(rows: &Value) -> Vec<altim::types::Candle> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let r: Vec<f64> = r.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            altim::types::Candle { time: r[0] as i64, open: r[1], high: r[2], low: r[3], close: r[4], volume: r[5] }
        })
        .collect()
}

/// `web/test/signal.test.ts`: "backtest i identique trade par trade" (fixture shared with Swift and Kotlin).
#[test]
fn backtest_reference_fixture() {
    let fixture: Value = serde_json::from_str(include_str!("samples/swift-fixture.json")).unwrap();
    let mut n = 0;
    for c in fixture.as_array().unwrap().iter().filter(|c| c.get("backtest").is_some()) {
        let r: BacktestResult = backtest_default(&fixture_candles(&c["candles"]));
        let e = &c["backtest"];
        let close = |a: f64, b: &Value| assert!((a - b.as_f64().unwrap()).abs() < 0.5e-8, "{a} ≠ {b}");
        let trades = e["trades"].as_array().unwrap();
        assert_eq!(r.trades.len(), trades.len());
        for (t, e) in r.trades.iter().zip(trades) {
            assert_eq!(t.entry_time, e["entryTime"].as_i64().unwrap());
            assert_eq!(t.exit_time, e["exitTime"].as_i64().unwrap());
            close(t.entry_price, &e["entryPrice"]);
            close(t.exit_price, &e["exitPrice"]);
            close(t.return_percent, &e["returnPercent"]);
            assert_eq!(t.exit_reason, e["exitReason"].as_str().unwrap());
        }
        close(r.total_return_percent, &e["totalReturnPercent"]);
        close(r.buy_and_hold_percent, &e["buyAndHoldPercent"]);
        close(r.win_rate_percent, &e["winRatePercent"]);
        close(r.max_drawdown_percent, &e["maxDrawdownPercent"]);
        close(r.exposure_percent, &e["exposurePercent"]);
        n += 1;
    }
    assert!(n > 0);
    // Too short: nothing tested.
    let short = backtest_default(&[]);
    assert!(short.trades.is_empty() && short.equity.is_empty() && short.total_return_percent == 0.0);
    assert_eq!(backtest::WARMUP, 60);
}

// ---------- history ----------

fn input_series() -> HashMap<String, Vec<Close>> {
    let mut s: HashMap<String, Vec<Close>> = HashMap::new();
    for i in INPUTS.iter().filter(|i| i.interval == "long") {
        s.insert(format!("{}:{}", i.kind.as_str(), i.symbol), i.candles.iter().map(|c| (c.time, c.close)).collect());
    }
    let sol = &s["crypto:SOL"];
    let mut odd: Vec<Close> = sol[sol.len() - 60..].to_vec();
    odd.reverse();
    let odd = odd
        .into_iter()
        .enumerate()
        .map(|(k, (t, c))| {
            (
                t,
                if k % 17 == 0 {
                    0.0
                } else if k % 23 == 0 {
                    -c
                } else {
                    c
                },
            )
        })
        .collect();
    s.insert("crypto:ODD".into(), odd);
    let nvda = &s["stock:NVDA"];
    s.insert("stock:LATE".into(), nvda[nvda.len() - 20..].to_vec());
    s.insert("crypto:FUTURE".into(), s["crypto:ETH"].iter().map(|(t, c)| (t + 5 * 86_400_000, *c)).collect());
    s
}

#[derive(serde::Deserialize)]
struct HistorySample {
    #[serde(rename = "asOf")]
    as_of: i64,
    series: Vec<SampleSeries>,
}
#[derive(serde::Deserialize)]
struct SampleSeries {
    symbol: String,
    kind: String,
    closes: Vec<Close>,
}

fn history_sample() -> (i64, HashMap<String, Vec<Close>>) {
    let s: HistorySample = serde_json::from_str(include_str!("samples/history-sample.json")).unwrap();
    (s.as_of, s.series.into_iter().map(|x| (format!("{}:{}", x.kind, x.symbol), x.closes)).collect())
}

#[test]
fn parity_history() {
    let inputs = input_series();
    let (_, sample) = history_sample();
    let cases = golden("history");
    assert!(cases.len() >= 40);
    for c in cases {
        let a = &c.args;
        let lines: Vec<HistoryLine> = from(&a["lines"]);
        let inline: HashMap<String, Vec<Close>>;
        let series = match a["series"].as_str() {
            Some("inputs") => &inputs,
            Some("sample") => &sample,
            _ => {
                inline = from(&a["series"]);
                &inline
            }
        };
        let out = portfolio_history(&lines, series, a["days"].as_u64().unwrap() as usize, a["now"].as_i64().unwrap());
        assert_same(&to_value(&out), &c.output, &format!("history {}", a["lines"]));
    }
}

const DAY: i64 = 86_400_000;
fn d(iso: &str) -> i64 {
    altim::js::parse_date(&format!("{iso}T00:00:00Z")).unwrap()
}
fn line(id: &str, quantity: f64) -> HistoryLine {
    HistoryLine { id: id.into(), quantity }
}

/// `web/test/history.test.ts`.
#[test]
fn history_real_closes() {
    let (as_of, real) = history_sample();
    let h = portfolio_history(&[line("crypto:ETH", 2.0), line("stock:AAPL", 10.0)], &real, 90, as_of).unwrap();
    assert_eq!(h.points.len(), 91);
    assert!(!h.shortened);
    assert!(h.missing.is_empty());
    let last = h.points.last().unwrap();
    assert_eq!(last.t, d("2026-09-27"));
    assert!((last.value - (2.0 * 2688.32 + 10.0 * 341.07)).abs() < 0.05);
    for w in h.points.windows(2) {
        assert_eq!(w[1].t - w[0].t, DAY);
    }
    assert!((h.change - (last.value / h.points[0].value - 1.0) * 100.0).abs() < 0.5e-6);
    assert!(h.max_drawdown <= 0.0);
    assert_eq!(h.benchmarks.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["Bitcoin", "S&P 500 (SPY)"]);
    let btc = &h.benchmarks[0];
    assert_eq!(btc.points.len(), h.points.len());
    assert_eq!(btc.points[0].pct, 0.0);
    let btc_start = real["crypto:BTC"].iter().find(|(t, _)| *t == h.points[0].t).unwrap().1;
    assert!((btc.change - (84464.605 / btc_start - 1.0) * 100.0).abs() < 0.5e-6);
    assert!((h.change - 45.5).abs() < 0.05);
    assert!((btc.change - 40.3).abs() < 0.05);
    assert!((h.max_drawdown + 6.8).abs() < 0.05);
}

#[test]
fn history_weekend_late_listing_drawdown() {
    let series = HashMap::from([("stock:X".to_string(), vec![(d("2026-09-18"), 100.0), (d("2026-09-21"), 110.0)])]);
    let h = portfolio_history(&[line("stock:X", 1.0)], &series, 3, d("2026-09-21") + 3_600_000).unwrap();
    assert_eq!(h.points.iter().map(|p| p.value).collect::<Vec<_>>(), [100.0, 100.0, 100.0, 110.0]);
    assert_eq!(h.best.unwrap().t, d("2026-09-21"));
    assert!((h.best.unwrap().change - 10.0).abs() < 1e-9);
    assert!(h.worst.is_none());

    let series = HashMap::from([
        ("crypto:A".to_string(), (0..11).map(|i| (d("2026-09-01") + i * DAY, 10.0)).collect::<Vec<Close>>()),
        ("crypto:NEW".to_string(), (0..4).map(|i| (d("2026-09-08") + i * DAY, 5.0)).collect()),
    ]);
    let h = portfolio_history(&[line("crypto:A", 1.0), line("crypto:NEW", 2.0)], &series, 10, d("2026-09-11") + 1).unwrap();
    assert!(h.shortened);
    assert_eq!(h.points[0].t, d("2026-09-08"));
    assert!(h.points.iter().all(|p| p.value == 20.0));
    assert_eq!(h.change, 0.0);

    let closes: Vec<Close> =
        [100.0, 120.0, 90.0, 108.0, 60.0, 80.0].iter().enumerate().map(|(i, c)| (d("2026-09-01") + i as i64 * DAY, *c)).collect();
    let h = portfolio_history(
        &[line("crypto:A", 1.0), line("crypto:GONE", 3.0)],
        &HashMap::from([("crypto:A".to_string(), closes)]),
        5,
        d("2026-09-06") + 1,
    )
    .unwrap();
    assert!((h.max_drawdown + 50.0).abs() < 0.5e-6);
    assert!((h.worst.unwrap().change + 44.444).abs() < 0.005);
    assert_eq!(h.missing, ["crypto:GONE"]);
    assert!(h.benchmarks.is_empty());
    assert!(portfolio_history(&[line("crypto:GONE", 1.0)], &HashMap::new(), 30, altim::js::now_ms()).is_none());
}

// ---------- brief ----------

#[test]
fn parity_brief() {
    let cases = golden("brief");
    assert!(cases.len() > 100);
    for c in cases {
        let a = &c.args;
        let out = match a["fn"].as_str().unwrap() {
            "movers" => {
                let prices: Vec<PriceNow> = from(&a["prices"]);
                let prev: HashMap<String, Option<f64>> = from(&a["prev"]);
                to_value(&movers(&prices, &prev))
            }
            _ => {
                let level: Option<MarketLevel> = from(&a["level"]);
                let buyable: Vec<BriefBuy> = from(&a["buyable"]);
                let moves: Vec<Mover> = from(&a["moves"]);
                to_value(&headline(level, &buyable, &moves))
            }
        };
        assert_same(&out, &c.output, &format!("brief {a}"));
    }
}

/// `web/test/brief.test.ts`.
#[test]
fn brief_unit() {
    let p = |symbol: &str, kind: Kind, price: Option<f64>| PriceNow { symbol: symbol.into(), kind, price };
    let m = movers(
        &[p("BTC", Kind::Crypto, Some(102.0)), p("NVDA", Kind::Stock, Some(96.6)), p("ETH", Kind::Crypto, None), p("SOL", Kind::Crypto, Some(50.0))],
        &HashMap::from([
            ("crypto:BTC".to_string(), Some(100.0)),
            ("stock:NVDA".to_string(), Some(100.0)),
            ("crypto:ETH".to_string(), Some(2000.0)),
            ("crypto:SOL".to_string(), None),
        ]),
    );
    assert_eq!(m.iter().map(|x| x.symbol.as_str()).collect::<Vec<_>>(), ["NVDA", "BTC"]);
    assert!((m[0].change + 3.4).abs() < 0.5e-9);
    assert!((m[1].change - 2.0).abs() < 0.5e-9);

    let buy = |symbol: &str, strong: bool| BriefBuy { symbol: symbol.into(), kind: Kind::Crypto, strong, title: String::new() };
    let mv = |symbol: &str, kind: Kind, change: f64| Mover { symbol: symbol.into(), kind, price: 1.0, change };
    let moves = [mv("NVDA", Kind::Stock, -3.4), mv("SOL", Kind::Crypto, 2.8), mv("BTC", Kind::Crypto, 0.4)];
    assert_eq!(
        headline(Some(MarketLevel::Calm), &[buy("BTC", true), buy("ETH", false), buy("AAPL", false)], &moves),
        "Contexte calme · 3 achetables (BTC, ETH, AAPL) · NVDA −3,4\u{a0}%, SOL +2,8\u{a0}%"
    );
    assert_eq!(headline(Some(MarketLevel::High), &[], &moves[2..]), "Tension élevée · rien d'achetable pour l'instant");
    assert_eq!(headline(None, &[buy("A", false), buy("B", false), buy("C", false), buy("D", false)], &[]), "4 achetables (A, B, C…)");
    assert_eq!(headline(None, &[buy("BTC", false)], &[]), "1 achetable (BTC)");
}

// ---------- news ----------

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/samples/news-samples/{name}.xml", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn attempt<T: serde::Serialize>(r: altim::http::Result<T>) -> Value {
    match r {
        Ok(v) => json!({ "ok": to_value(&v) }),
        Err(e) => json!({ "error": e.0 }),
    }
}

#[test]
fn parity_news() {
    let cases = golden("news");
    assert!(cases.len() > 90);
    let assets_all = [
        ("crypto:BTC", "BTC", "Bitcoin"),
        ("stock:AAPL", "AAPL", "Apple Inc."),
        ("crypto:SOL", "SOL", "Solana"),
        ("stock:NVDA", "NVDA", "NVIDIA Corporation"),
        ("stock:BRK-B", "BRK-B", "Berkshire Hathaway Holdings"),
        ("crypto:ETH", "ETH", "Ethereum"),
        ("stock:MC.PA", "MC.PA", "LVMH, SA"),
        ("crypto:XRP", "XRP", "XRP"),
        ("stock:SAN", "SAN", "Société Générale"),
        ("stock:FOO", "FOO", "Foo Bar Holdingſ"),
        ("stock:KEL", "KEL", "\u{212A}elvin Corp."),
    ]
    .map(|(id, symbol, name)| WatchAsset { id: id.into(), symbol: symbol.into(), name: name.into() });
    for c in cases {
        let a = &c.args;
        let out = match a["fn"].as_str().unwrap() {
            "parseFeed" => {
                let xml = match a["sample"].as_str() {
                    Some(s) => sample(s),
                    None => a["xml"].as_str().unwrap().to_string(),
                };
                attempt(parse_feed(&xml, a["source"].as_str().unwrap()))
            }
            "decodeText" => attempt(decode_text(a["s"].as_str().unwrap())),
            "safeLink" => to_value(&safe_link(a["s"].as_str().unwrap())),
            "sameStory" => json!(same_story(&from(&a["a"]), &from(&a["b"]))),
            "mentions" => json!(assets_all.iter().map(|x| mentions(a["title"].as_str().unwrap(), x)).collect::<Vec<_>>()),
            "aggregate" => {
                #[derive(serde::Deserialize)]
                struct F {
                    category: NewsCategory,
                    fallback: Option<NewsCategory>,
                    items: Vec<RawNews>,
                }
                let feeds: Vec<F> = from(&a["feeds"]);
                let feeds: Vec<FeedItems> = feeds.iter().map(|f| FeedItems { category: f.category, fallback: f.fallback, items: &f.items }).collect();
                let assets: Vec<WatchAsset> = from(&a["assets"]);
                let now = a["now"].as_i64().unwrap();
                let out = aggregate(&feeds, &assets, now, a.get("maxAgeMs").and_then(Value::as_i64).unwrap_or(news::MAX_AGE_MS));
                json!({ "out": out, "top": top_stories(&out, 5), "top2": top_stories(&out, 2), "digest": news_digest(&out, now) })
            }
            f => panic!("{f}"),
        };
        assert_same(&to_value(&out), &c.output, &format!("news {}", a.to_string().chars().take(200).collect::<String>()));
    }
}

/// `web/test/news.test.ts`: real feeds.
#[test]
fn news_real_feeds() {
    let tag = regex::Regex::new(r"(?i)<[a-z/]").unwrap();
    for (name, source) in [
        ("bfm", "BFM Économie"),
        ("cointelegraph", "Cointelegraph"),
        ("google-fr", "Google Actualités"),
        ("yahoo-aapl", "Yahoo Finance"),
        ("decrypt", "Decrypt"),
    ] {
        let items = parse_feed(&sample(name), source).unwrap();
        assert_eq!(items.len(), 6, "{name}");
        for it in &items {
            assert!(it.title.encode_utf16().count() > 5);
            assert!(!it.title.contains('<') && !it.title.contains("&amp;") && !it.title.contains("CDATA"));
            assert!(it.link.starts_with("http://") || it.link.starts_with("https://"));
            assert!(it.source.chars().count() > 1);
            if let Some(s) = &it.summary {
                assert!(!tag.is_match(s));
            }
        }
    }
    for it in parse_feed(&sample("google-fr"), "Google Actualités").unwrap() {
        assert!(!it.title.ends_with(&format!(" - {}", it.source)));
        assert!(it.summary.is_none());
        assert_ne!(it.source, "Google Actualités");
    }
}

#[test]
fn news_entities_links_atom() {
    assert_eq!(decode_text("<![CDATA[L&#8217;or &amp; le <b>dollar</b>]]>").unwrap(), "L’or & le dollar");
    assert_eq!(decode_text("&lt;a href=&quot;x&quot;&gt;Titre&lt;/a&gt;&nbsp;Source").unwrap(), "Titre Source");
    assert_eq!(safe_link("javascript:alert(1)"), None);
    assert_eq!(safe_link("data:text/html,x"), None);
    assert_eq!(safe_link("https://example.com/a?b=1").as_deref(), Some("https://example.com/a?b=1"));
    let feed = "<rss><channel><item><title>Titre piégé</title><link>javascript:alert(1)</link><pubDate>Mon, 28 Sep 2026 06:00:00 GMT</pubDate></item></channel></rss>";
    assert!(parse_feed(feed, "X").unwrap().is_empty());
    let atom = r#"<feed><entry><title>Fed holds rates</title><link href="https://ex.com/1"/><updated>2026-09-28T06:00:00Z</updated></entry></feed>"#;
    assert_eq!(
        parse_feed(atom, "Atom").unwrap(),
        [RawNews {
            title: "Fed holds rates".into(),
            link: "https://ex.com/1".into(),
            time: altim::js::parse_date("2026-09-28T06:00:00Z").unwrap(),
            source: "Atom".into(),
            summary: None
        }]
    );
}

fn now8() -> i64 {
    altim::js::parse_date("2026-09-28T08:00:00Z").unwrap()
}
fn item(title: &str, source: &str, hours_ago: f64) -> RawNews {
    let link = format!("https://ex.com/{}/{source}", altim::news::encode_uri_component(title));
    RawNews { title: title.into(), link, source: source.into(), time: now8() - (hours_ago * 3_600_000.0) as i64, summary: None }
}
fn feed(category: NewsCategory, items: &[RawNews]) -> FeedItems<'_> {
    FeedItems { category, fallback: None, items }
}

#[test]
fn news_grouping() {
    let now = now8();
    let a = item("Fed holds interest rates steady as inflation cools", "Reuters", 3.0);
    let b = item("Fed holds interest rates steady while inflation cools", "CNBC", 2.0);
    let c = item("Fed holds interest rates steady, inflation cools", "MarketWatch", 1.0);
    let d = item("Bitcoin ETF inflows reach record high", "CoinDesk", 1.0);
    assert!(same_story(&a, &b));
    assert!(!same_story(&a, &d));
    let fed_items = [a, b, c];
    let crypto_items = [d];
    let out = aggregate(&[feed(NewsCategory::Monde, &fed_items), feed(NewsCategory::Crypto, &crypto_items)], &[], now, news::MAX_AGE_MS);
    assert_eq!(out.len(), 2);
    let fed = out.iter().find(|i| i.title.starts_with("Fed")).unwrap();
    assert_eq!(fed.source, "Reuters");
    let mut also = fed.also_in.clone();
    also.sort();
    assert_eq!(also, ["CNBC", "MarketWatch"]);
    assert!(fed.themes.contains(&NewsTheme::Monetary));
    assert_eq!(top_stories(&out, 5)[0].id, fed.id);
}

#[test]
fn news_assets() {
    let w = |id: &str, symbol: &str, name: &str| WatchAsset { id: id.into(), symbol: symbol.into(), name: name.into() };
    let btc = w("crypto:BTC", "BTC", "Bitcoin");
    let apple = w("stock:AAPL", "AAPL", "Apple Inc.");
    let sol = w("crypto:SOL", "SOL", "Solana");
    assert!(mentions("Bitcoin tops $90,000", &btc));
    assert!(mentions("BTC dominance rises", &btc));
    assert!(mentions("Apple unveils new iPhone", &apple));
    assert!(!mentions("Pineapple prices rise", &apple));
    assert!(!mentions("Le sol est gelé", &sol));
    assert!(mentions("$SOL rallies", &sol));

    let items = [item("Bitcoin miners expand in Texas", "Google News", 1.0), item("Crypto market wobbles on macro fears", "Google News", 1.0)];
    let out = aggregate(
        &[FeedItems { category: NewsCategory::Actifs, fallback: Some(NewsCategory::Crypto), items: &items }],
        &[btc],
        now8(),
        news::MAX_AGE_MS,
    );
    let b = out.iter().find(|i| i.title.starts_with("Bitcoin")).unwrap();
    assert_eq!((b.category, b.assets.clone()), (NewsCategory::Actifs, vec!["crypto:BTC".to_string()]));
    let c = out.iter().find(|i| i.title.starts_with("Crypto")).unwrap();
    assert_eq!((c.category, c.assets.len()), (NewsCategory::Crypto, 0));
}

#[test]
fn news_tone_lang_alert_cap() {
    let now = now8();
    let items = [
        item("Stocks plunge as bank collapse sparks contagion fears", "A", 1.0),
        item("La Bourse de Paris grimpe après la décision de la BCE", "B", 1.0),
        item("Russia declares war on neighbour, markets slide", "C", 1.0),
        item("Old story from last week", "D", 24.0 * 7.0),
    ];
    let out = aggregate(&[feed(NewsCategory::Monde, &items)], &[], now, news::MAX_AGE_MS);
    assert_eq!(out.len(), 3);
    let crash = out.iter().find(|i| i.source == "A").unwrap();
    assert_eq!(crash.tone, NewsTone::Negative);
    assert!(crash.themes.contains(&NewsTheme::Stress));
    assert!(crash.alert);
    let paris = out.iter().find(|i| i.source == "B").unwrap();
    assert_eq!((paris.lang, paris.tone), (news::Lang::Fr, NewsTone::Positive));
    assert!(paris.themes.contains(&NewsTheme::Monetary));
    assert!(out.iter().find(|i| i.source == "C").unwrap().alert);
    assert!(top_stories(&out, 5).is_empty());
    let hedged_items = [
        item("Les agents IA pourraient déclencher un bank run, selon cet économiste", "X", 1.0),
        item("AI agents could trigger a bank run", "Y", 1.0),
    ];
    assert!(aggregate(&[feed(NewsCategory::Monde, &hedged_items)], &[], now, news::MAX_AGE_MS).iter().all(|i| !i.alert));
    let war_items = [
        item("Russia declares war on neighbour, markets slide", "A", 1.0),
        item("Russia declares war on its neighbour as markets slide", "B", 1.0),
        item("Fed holds interest rates steady as inflation cools", "C", 1.0),
        item("Fed holds interest rates steady while inflation cools", "D", 1.0),
    ];
    let war = aggregate(&[feed(NewsCategory::Monde, &war_items)], &[], now, news::MAX_AGE_MS);
    assert!(top_stories(&war, 5)[0].alert);
    let w = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet"];
    let many: Vec<RawNews> = (0..100)
        .map(|i| item(&format!("{} {}token {i}x{}chain news", w[i % 10], w[i / 10], w[(i * 3) % 10]), &format!("S{i}"), i as f64 / 10.0))
        .collect();
    let many = aggregate(&[feed(NewsCategory::Crypto, &many)], &[], now, news::MAX_AGE_MS);
    assert_eq!(many.len(), NewsCategory::Crypto.per_category());
    assert_eq!(news_digest(&out, now).tone.negative, 2);
}

/// `web/server/news.ts`: the feed list and the searches made for the user's assets (URLs byte for byte).
#[test]
fn parity_news_feeds() {
    for c in golden("news-feeds") {
        let assets: Vec<altim::types::Asset> = from(&c.args["assets"]);
        let out = json!({ "feeds": *altim::news::FEEDS, "assetFeeds": altim::news::asset_feeds(&assets) });
        assert_same(&to_value(&out), &c.output, "news-feeds");
    }
}

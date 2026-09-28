//! "Résumé intelligent" of the news (`engine::news_summary`) on the real feeds of `web/test/news-samples`.
use altim::engine::guard::Trend;
use altim::engine::news::{self, FeedItems, NewsCategory, NewsTone, RawNews, WatchAsset, aggregate, parse_feed};
use altim::engine::news_summary::{
    Agreement, CachedMarket, Impact, ImpactBasis, MAX_EVENTS, StorySummary, measured_level, move_since, summarize, technical_text,
};

const HOUR: i64 = 3_600_000;
/// Mon 28 Sep 2026 07:00 UTC, just after the samples were captured.
const NOW: i64 = 1_790_578_800_000;

fn sample(name: &str) -> Vec<RawNews> {
    let xml = std::fs::read_to_string(format!("{}/../web/test/news-samples/{name}.xml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let source = match name {
        "bfm" => "BFM Économie",
        "cointelegraph" => "Cointelegraph",
        "google-fr" => "Google Actualités",
        "yahoo-aapl" => "Yahoo Finance",
        _ => "Decrypt",
    };
    parse_feed(&xml, source).unwrap()
}

fn watch() -> Vec<WatchAsset> {
    vec![
        WatchAsset { id: "stock:AAPL".into(), symbol: "AAPL".into(), name: "Apple Inc.".into() },
        WatchAsset { id: "crypto:BTC".into(), symbol: "BTC".into(), name: "Bitcoin".into() },
    ]
}

struct Feeds {
    monde: Vec<RawNews>,
    crypto: Vec<RawNews>,
    actifs: Vec<RawNews>,
    extra: Vec<RawNews>,
}

fn feeds(extra: Vec<RawNews>) -> Feeds {
    let mut crypto = sample("cointelegraph");
    crypto.extend(sample("decrypt"));
    let mut actifs = sample("yahoo-aapl");
    actifs.extend(sample("google-fr"));
    Feeds { monde: sample("bfm"), crypto, actifs, extra }
}

fn run(f: &Feeds, market: &dyn Fn(&str) -> Option<CachedMarket>) -> Vec<StorySummary> {
    let list = [
        FeedItems { category: NewsCategory::Monde, fallback: None, items: &f.monde },
        FeedItems { category: NewsCategory::Crypto, fallback: None, items: &f.crypto },
        FeedItems { category: NewsCategory::Actifs, fallback: Some(NewsCategory::Marches), items: &f.actifs },
        FeedItems { category: NewsCategory::Crypto, fallback: None, items: &f.extra },
    ];
    let items = aggregate(&list, &watch(), NOW, news::MAX_AGE_MS);
    summarize(&items, &list, &watch(), NOW, market)
}

fn none(_: &str) -> Option<CachedMarket> {
    None
}

fn twin(title: &str, source: &str, time: i64) -> RawNews {
    RawNews { title: title.into(), link: format!("https://example.com/{}", source.replace(' ', "-")), time, source: source.into(), summary: None }
}

/// Real feeds plus other outlets' versions of two real headlines: Apple's (reworded) and Zano's (reworded twice).
fn with_twins() -> Feeds {
    let apple = sample("yahoo-aapl").into_iter().find(|n| n.title.contains("Premium iPhone")).unwrap();
    let zano = sample("cointelegraph").into_iter().find(|n| n.title.contains("Zano")).unwrap();
    feeds(vec![
        twin("Apple's Premium iPhone Strategy Faces a Test Beyond Its Early Adopters", "Barron's", apple.time + HOUR),
        twin("Zano rolls blockchain back one month after Gateway Address exploit", "The Block", zano.time + HOUR),
        twin("Zano rolls its blockchain back a month after Gateway Address attack, token rebounds", "CoinDesk", zano.time + 2 * HOUR),
    ])
}

#[test]
fn real_feeds_rule_only() {
    let s = run(&with_twins(), &none);
    assert!(!s.is_empty() && s.len() <= MAX_EVENTS);
    for e in &s {
        // Today only, and something that makes it an event: several sources, an escalation or the user's assets.
        assert!(e.time >= NOW - 24 * HOUR, "{}", e.title);
        assert!(e.independent_sources >= 2 || e.alert || (!e.assets.is_empty() && e.impact_points >= 2), "{}", e.title);
        // Nothing cached: the level is the rule's, and the reasons add up to the points.
        assert_eq!(e.impact_basis, ImpactBasis::Rule);
        assert_eq!(e.impact, e.rule_impact);
        assert!(e.moves.is_empty() && e.technical.is_empty());
        let sum: u32 =
            e.impact_reasons.iter().filter_map(|r| r.rsplit_once("(+").and_then(|(_, p)| p.trim_end_matches(')').parse::<u32>().ok())).sum();
        assert_eq!(sum, e.impact_points, "{:?}", e.impact_reasons);
        assert_eq!(e.links[0].link, e.link);
        assert!(e.links.iter().all(|l| l.link.starts_with("http")));
        assert_eq!(e.consensus.agreement, if e.sources < 2 { Agreement::Single } else { e.consensus.agreement });
    }
    // Ordered by impact, then points.
    assert!(s.windows(2).all(|w| (w[0].impact, w[0].impact_points) >= (w[1].impact, w[1].impact_points)));
    // Apple's story told by two outlets names the user's asset: 2 sources (+1) and AAPL (+1).
    let apple = s.iter().find(|e| e.title.contains("Premium iPhone")).expect("Apple story");
    assert_eq!((apple.assets.as_slice(), apple.impact_points, apple.impact), (&["stock:AAPL".to_string()][..], 2, Impact::Medium));
    // Apple's other stories, told by Yahoo alone, are not events of the day.
    assert!(!s.iter().any(|e| e.title.contains("Foldable")));
}

#[test]
fn syndicated_headline_counts_once() {
    // One Motley Fool article carried word for word by three outlets: 3 sources, 1 independent.
    let t = "Where Will the S&P 500 Be in 30 Years? History Offers a Clear Answer for Investors";
    let f =
        feeds(vec![twin(t, "The Motley Fool", NOW - 3 * HOUR), twin(t, "Yahoo Finance", NOW - 2 * HOUR), twin(t, "The Globe and Mail", NOW - HOUR)]);
    let list = [FeedItems { category: NewsCategory::Marches, fallback: None, items: &f.extra }];
    let items = aggregate(&list, &[], NOW, news::MAX_AGE_MS);
    assert_eq!(items[0].also_in.len(), 2);
    // Not an event of the day: nothing independent confirms it.
    assert!(summarize(&items, &list, &[], NOW, &none).is_empty());
}

#[test]
fn merged_story_hack_and_divergent_sources() {
    // Cointelegraph's real headline, retold by two other sources (one of them upbeat).
    let s = run(&with_twins(), &none);
    let e = s.iter().find(|e| e.title.contains("Zano")).expect("Zano story");
    assert_eq!((e.sources, e.independent_sources), (3, 3));
    assert_eq!(e.links.iter().map(|l| l.source.as_str()).collect::<Vec<_>>(), ["Cointelegraph", "The Block", "CoinDesk"]);
    // 3 sources (+1) and a hack (+1): moyen, by rule.
    assert_eq!(e.impact_points, 2);
    assert_eq!(e.impact, Impact::Medium);
    assert!(e.impact_reasons.iter().any(|r| r == "Piratage (+1)"));
    // "exploit" reads negative, "rebounds" positive: the sources diverge.
    assert_eq!(e.consensus.agreement, Agreement::Divergent);
    assert_eq!((e.consensus.negative, e.consensus.positive), (2, 1));
    assert_eq!(e.consensus.tone, NewsTone::Negative);
}

/// Hourly closes from `start` (open time), one per hour.
fn closes(start: i64, prices: &[f64]) -> Vec<(i64, f64)> {
    prices.iter().enumerate().map(|(i, p)| (start + i as i64 * HOUR, *p)).collect()
}

#[test]
fn measured_impact_and_technical_line() {
    let f = with_twins();
    let s0 = run(&f, &none);
    let apple = s0.iter().find(|e| e.title.contains("Premium iPhone")).expect("Apple story").clone();
    let pub_time = apple.time;
    // Hourly candles around publication: the last candle closed before it at 100, then +4 %.
    let start = (pub_time / HOUR - 3) * HOUR;
    let market = move |id: &str| {
        (id == "stock:AAPL").then(|| CachedMarket {
            closes: closes(start, &[99.0, 100.0, 100.0, 102.0, 104.0]),
            step_ms: HOUR,
            source: "consensus".into(),
            crypto: false,
            trend: Some(Trend::Up),
        })
    };
    let s = run(&f, &market);
    let e = s.iter().find(|e| e.id == apple.id).unwrap();
    assert_eq!(e.impact_basis, ImpactBasis::Measured);
    assert_eq!(e.impact, Impact::High);
    assert_eq!(e.rule_impact, apple.rule_impact);
    let m = &e.moves[0];
    // Reference: the close of the last candle that ended at or before publication (no look-ahead).
    assert!(m.from_time <= pub_time && m.from_time > pub_time - HOUR);
    assert_eq!((m.from_price, m.to_price), (100.0, 104.0));
    assert!((m.change_pct - 4.0).abs() < 1e-9);
    assert!(e.impact_reasons.iter().any(|r| r == "AAPL +4,0 % depuis la publication"));
    assert_eq!(e.technical.len(), 1);
    assert_eq!(e.technical[0].text, technical_text(e.consensus.tone, Trend::Up));

    // Candles that stop before publication: nothing measurable, the rule stays; the trend alone still gives its line.
    let early = move |_: &str| {
        Some(CachedMarket {
            closes: closes(start - 10 * HOUR, &[100.0, 101.0]),
            step_ms: HOUR,
            source: "x".into(),
            crypto: false,
            trend: Some(Trend::Down),
        })
    };
    let s = run(&f, &early);
    let e = s.iter().find(|e| e.id == apple.id).unwrap();
    assert_eq!(e.impact_basis, ImpactBasis::Rule);
    assert!(e.moves.is_empty());
    assert_eq!(e.technical[0].trend, Trend::Down);
}

#[test]
fn thresholds_and_texts() {
    assert_eq!(measured_level(0.9, false), Impact::Low);
    assert_eq!(measured_level(-1.0, false), Impact::Medium);
    assert_eq!(measured_level(3.0, false), Impact::High);
    assert_eq!(measured_level(1.9, true), Impact::Low);
    assert_eq!(measured_level(-4.9, true), Impact::Medium);
    assert_eq!(measured_level(5.0, true), Impact::High);
    let m = CachedMarket { closes: closes(0, &[10.0, 11.0]), step_ms: HOUR, ..Default::default() };
    // Published after the last cached candle closed: no candle after the reference yet.
    assert!(move_since(&m, "x", 2 * HOUR + 10).is_none());
    // Published during it: from the close preceding publication (1 h) to its close.
    let mv = move_since(&m, "x", HOUR + 10).unwrap();
    assert_eq!((mv.from_time, mv.to_time, mv.from_price, mv.to_price), (HOUR, 2 * HOUR, 10.0, 11.0));
    // Before any cached candle closed: no reference.
    assert!(move_since(&m, "x", HOUR - 1).is_none());
    assert_eq!(technical_text(NewsTone::Negative, Trend::Up), "Actualité négative alors que la tendance technique est haussière : à surveiller.");
}

#[test]
fn json_contract() {
    let s = run(&with_twins(), &none);
    let v = serde_json::to_value(&s[0]).unwrap();
    for k in [
        "id",
        "title",
        "link",
        "source",
        "time",
        "sources",
        "independentSources",
        "links",
        "assets",
        "impact",
        "impactBasis",
        "ruleImpact",
        "impactPoints",
        "impactReasons",
        "consensus",
        "moves",
        "technical",
    ] {
        assert!(v.get(k).is_some(), "{k}");
    }
    assert!(["low", "medium", "high"].contains(&v["impact"].as_str().unwrap()));
    assert_eq!(v["impactBasis"], "rule");
    assert!(["convergent", "divergent", "single"].contains(&v["consensus"]["agreement"].as_str().unwrap()));
}

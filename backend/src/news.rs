//! Sources of the news section (`web/server/news.ts`): specialised feeds in French and English plus Google News
//! searches, each fetched with a timeout and cached 10 minutes; a feed that fails is left out (and reported), the
//! others still come.
use std::sync::LazyLock;
use std::time::Duration;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Serialize;

use crate::cache::cached;
use crate::engine::news::{NewsCategory, RawNews, WatchAsset, parse_feed};
use crate::http::{CLIENT, Error, Result, err};
use crate::types::{Asset, Kind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Feed {
    pub name: String,
    pub url: String,
    pub category: NewsCategory,
    /// For the searches of one asset: where a story that does not name it goes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<NewsCategory>,
}

/// `encodeURIComponent`.
const URI_COMPONENT: &AsciiSet =
    &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'!').remove(b'~').remove(b'*').remove(b'\'').remove(b'(').remove(b')');

pub fn encode_uri_component(s: &str) -> String {
    utf8_percent_encode(s, URI_COMPONENT).to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Fr,
    En,
}

fn google(q: &str, lang: Lang, when: &str) -> String {
    let q = encode_uri_component(&format!("{q} when:{when}"));
    match lang {
        Lang::Fr => format!("https://news.google.com/rss/search?q={q}&hl=fr&gl=FR&ceid=FR:fr"),
        Lang::En => format!("https://news.google.com/rss/search?q={q}&hl=en-US&gl=US&ceid=US:en"),
    }
}

fn feed(name: &str, url: String, category: NewsCategory) -> Feed {
    Feed { name: name.into(), url, category, fallback: None }
}

pub static FEEDS: LazyLock<Vec<Feed>> = LazyLock::new(|| {
    use NewsCategory::*;
    vec![
        // World economy and geopolitics
        feed("Le Monde Économie", "https://www.lemonde.fr/economie/rss_full.xml".into(), Monde),
        feed("BFM Économie", "https://www.bfmtv.com/rss/economie/".into(), Monde),
        feed("La Tribune", "https://www.latribune.fr/feed.xml".into(), Monde),
        feed(
            "Google Actualités (économie)",
            google("(économie OR bourse OR BCE OR Fed OR inflation OR guerre OR sanctions OR droits de douane)", Lang::Fr, "1d"),
            Monde,
        ),
        feed(
            "Google News (world)",
            google(r#"(war OR sanctions OR "Federal Reserve" OR inflation OR tariffs OR recession OR ceasefire)"#, Lang::En, "1d"),
            Monde,
        ),
        // Markets and stocks
        feed("MarketWatch", "https://feeds.content.dowjones.io/public/rss/mw_topstories".into(), Marches),
        feed("CNBC", "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id=100003114".into(), Marches),
        feed("Investing.com", "https://www.investing.com/rss/news_25.rss".into(), Marches),
        feed("Google News (markets)", google(r#"("stock market" OR "Wall Street" OR Nasdaq OR "S&P 500")"#, Lang::En, "1d"), Marches),
        // Crypto
        feed("CoinDesk", "https://www.coindesk.com/arc/outboundfeeds/rss/".into(), Crypto),
        feed("Cointelegraph", "https://cointelegraph.com/rss".into(), Crypto),
        feed("Decrypt", "https://decrypt.co/feed".into(), Crypto),
        feed("The Block", "https://www.theblock.co/rss.xml".into(), Crypto),
        feed("Cryptoast", "https://cryptoast.fr/feed/".into(), Crypto),
        feed("Journal du Token", "https://journaldutoken.com/feed/".into(), Crypto),
    ]
});

const UA: &str = "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0";
const ACCEPT: &str = "application/rss+xml, application/xml, text/xml, */*";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeedResult {
    pub feed: Feed,
    pub items: Vec<RawNews>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `fetch(url).text()`: follows redirects, 8 s, the body read as UTF-8 whatever the charset (BOM dropped).
async fn fetch_text(url: &str) -> Result<String> {
    let res = CLIENT.get(url).header("User-Agent", UA).header("Accept", ACCEPT).timeout(Duration::from_secs(8)).send().await?;
    if !res.status().is_success() {
        return err(format!("HTTP {}", res.status().as_u16()));
    }
    let bytes = res.bytes().await?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.strip_prefix('\u{FEFF}').unwrap_or(&text).to_string())
}

/// `message.slice(0, 80)` (UTF-16 code units).
fn slice80(s: &str) -> String {
    let u: Vec<u16> = s.encode_utf16().take(80).collect();
    String::from_utf16_lossy(&u)
}

async fn fetch_feed(feed: Feed) -> FeedResult {
    let url = feed.url.clone();
    let name = feed.name.clone();
    let r = cached(&format!("feed:{}", feed.url), 600_000, move || async move {
        let text = fetch_text(&url).await?;
        if text.len() > 5_000_000 && text.encode_utf16().count() > 5_000_000 {
            return Err(Error("flux trop gros".into()));
        }
        parse_feed(&text, &name)
    })
    .await;
    match r {
        Ok(items) => {
            let ok = !items.is_empty();
            FeedResult { feed, items: (*items).clone(), ok, error: if ok { None } else { Some("flux vide".into()) } }
        }
        Err(e) => FeedResult { feed, items: vec![], ok: false, error: Some(slice80(&e.0)) },
    }
}

/// Feeds of the user's own assets: Yahoo Finance for each stock, Google News (FR and EN) for each asset.
pub fn asset_feeds(assets: &[Asset]) -> Vec<Feed> {
    static SUFFIX: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i),? (Inc|Corp|Corporation|Ltd|plc)\.?$").expect("regex"));
    assets
        .iter()
        .flat_map(|a| {
            // `/i` without the `u` flag: `ſ` and `K` do not fold onto ASCII letters in JavaScript.
            let probe = a.name.replace('\u{17F}', "\u{D7}").replace('\u{212A}', "\u{2020}");
            let name = match SUFFIX.find(&probe) {
                Some(m) => format!("{}{}", &a.name[..m.start()], &a.name[m.end()..]),
                None => a.name.clone(),
            };
            let symbol = a.symbol.replace('-', ".");
            let crypto = a.kind == Kind::Crypto;
            let q = if crypto { format!("\"{name}\" crypto") } else { format!("\"{name}\" OR {symbol} action") };
            let fallback = if crypto { NewsCategory::Crypto } else { NewsCategory::Marches };
            let en = if crypto { format!("\"{name}\" crypto") } else { format!("{symbol} stock \"{name}\"") };
            let mut list = vec![
                Feed { name: "Google News".into(), url: google(&en, Lang::En, "2d"), category: NewsCategory::Actifs, fallback: Some(fallback) },
                Feed { name: "Google Actualités".into(), url: google(&q, Lang::Fr, "2d"), category: NewsCategory::Actifs, fallback: Some(fallback) },
            ];
            if a.kind == Kind::Stock {
                list.push(Feed {
                    name: "Yahoo Finance".into(),
                    url: format!("https://feeds.finance.yahoo.com/rss/2.0/headline?s={}&region=US&lang=en-US", encode_uri_component(&a.symbol)),
                    category: NewsCategory::Actifs,
                    fallback: Some(fallback),
                });
            }
            list
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchedNews {
    pub results: Vec<FeedResult>,
    pub watch: Vec<WatchAsset>,
}

pub async fn fetch_news(assets: &[Asset]) -> FetchedNews {
    let mut feeds: Vec<Feed> = FEEDS.clone();
    feeds.extend(asset_feeds(&assets[..assets.len().min(20)]));
    let results = futures::future::join_all(feeds.into_iter().map(fetch_feed)).await;
    let watch = assets
        .iter()
        .map(|a| WatchAsset { id: format!("{}:{}", a.kind.as_str(), a.symbol), symbol: a.symbol.clone(), name: a.name.clone() })
        .collect();
    FetchedNews { results, watch }
}

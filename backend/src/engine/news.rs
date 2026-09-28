//! News section (`web/src/engine/news.ts`): aggregation of many RSS feeds (world economy and geopolitics, crypto,
//! stocks, the user's own assets), in French and English. Pure functions, tested on real feeds:
//! - robust RSS / Atom parsing (CDATA, numeric entities, HTML in descriptions, odd dates);
//! - merge of the same story told by several sources (its coverage = how important it is);
//! - classification by category, theme (war, central banks, tariffs, crisis) and tone, detection of the user's assets.
//!
//! Titles are kept in their language: nothing is machine-translated nor rewritten.
//!
//! JavaScript regex semantics kept: `\b` is the ASCII word boundary (`(?-u:\b)`), `\s` the JavaScript white space set,
//! lengths and slices count UTF-16 code units, and the case-insensitive patterns without the `u` flag do not match
//! `ſ` (U+017F), `K` (U+212A) or `Å` (U+212B) with ASCII letters (see [`js_ci`]).
use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::http::{Error, Result};
use crate::js::parse_date;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NewsCategory {
    Monde,
    Marches,
    Crypto,
    Actifs,
}

impl NewsCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            NewsCategory::Monde => "monde",
            NewsCategory::Marches => "marches",
            NewsCategory::Crypto => "crypto",
            NewsCategory::Actifs => "actifs",
        }
    }
    /// `PER_CATEGORY[category]`: items kept per category, so that one big feed does not crowd out the others.
    pub fn per_category(self) -> usize {
        match self {
            NewsCategory::Monde => 70,
            NewsCategory::Marches => 60,
            NewsCategory::Crypto => 60,
            NewsCategory::Actifs => 80,
        }
    }
}

/// `PER_CATEGORY`.
pub const PER_CATEGORY: [(NewsCategory, usize); 4] =
    [(NewsCategory::Monde, 70), (NewsCategory::Marches, 60), (NewsCategory::Crypto, 60), (NewsCategory::Actifs, 80)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NewsTheme {
    Geopolitics,
    Monetary,
    Trade,
    Stress,
    Regulation,
    Earnings,
}

impl NewsTheme {
    /// `THEME_LABEL[theme]`.
    pub fn label(self) -> &'static str {
        match self {
            NewsTheme::Geopolitics => "Géopolitique / guerre",
            NewsTheme::Monetary => "Banques centrales / taux",
            NewsTheme::Trade => "Commerce / droits de douane",
            NewsTheme::Stress => "Crise / krach",
            NewsTheme::Regulation => "Régulation",
            NewsTheme::Earnings => "Résultats d'entreprises",
        }
    }
}

/// `THEME_LABEL`, in the key order of the TypeScript object.
pub const THEME_LABEL: [(NewsTheme, &str); 6] = [
    (NewsTheme::Geopolitics, "Géopolitique / guerre"),
    (NewsTheme::Monetary, "Banques centrales / taux"),
    (NewsTheme::Trade, "Commerce / droits de douane"),
    (NewsTheme::Stress, "Crise / krach"),
    (NewsTheme::Regulation, "Régulation"),
    (NewsTheme::Earnings, "Résultats d'entreprises"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NewsTone {
    Negative,
    Positive,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Fr,
    En,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawNews {
    pub title: String,
    pub link: String,
    pub time: i64,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub id: String,
    pub title: String,
    pub link: String,
    pub time: i64,
    pub source: String,
    pub summary: Option<String>,
    pub lang: Lang,
    pub category: NewsCategory,
    pub themes: Vec<NewsTheme>,
    pub tone: NewsTone,
    /// "crypto:BTC"… assets of the user named in the title.
    pub assets: Vec<String>,
    /// Other sources that told the same story (same event, near-identical title).
    pub also_in: Vec<String>,
    /// Serious escalation (war declared, invasion, bank run…).
    pub alert: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchAsset {
    pub id: String,
    pub symbol: String,
    pub name: String,
}

// ---------- JavaScript string and regex semantics ----------

/// JavaScript `\s` (WhiteSpace + LineTerminator), for use inside a character class.
const JS_WS: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

fn is_js_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// `String.prototype.trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

/// `s.length` (UTF-16 code units).
fn js_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.slice(0, n)` in UTF-16 code units (a split surrogate pair becomes U+FFFD, JavaScript keeps the lone half).
fn js_slice_to(s: &str, n: usize) -> String {
    let u: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&u)
}

/// For the case-insensitive patterns without the `u` flag: JavaScript does not fold `ſ`, `K` or `Å` onto ASCII letters
/// (Rust's Unicode case folding does), so they are replaced by characters of the same UTF-8 length no pattern matches;
/// match offsets stay valid in the original string.
fn js_ci(s: &str) -> Cow<'_, str> {
    if s.contains(['\u{17F}', '\u{212A}', '\u{212B}']) {
        Cow::Owned(s.replace('\u{17F}', "\u{D7}").replace(['\u{212A}', '\u{212B}'], "\u{2020}"))
    } else {
        Cow::Borrowed(s)
    }
}

fn re(p: &str) -> Regex {
    Regex::new(p).expect("regex")
}

/// `/\b(…)\b/i` of the TypeScript: ASCII word boundaries, case-insensitive.
fn word_re(alternatives: &str) -> Regex {
    re(&format!(r"(?i)(?-u:\b)({alternatives})(?-u:\b)"))
}

static THEMES: LazyLock<Vec<(NewsTheme, Regex)>> = LazyLock::new(|| {
    vec![
        (
            NewsTheme::Geopolitics,
            word_re(
                r"wars?|invasion|invades?|invaded|missiles?|air ?strikes?|drones?|military|troops|nuclear|sanctions?|ceasefire|hostages?|coup|blockade|guerre|frappes?|armée|militaires?|nucléaire|cessez-le-feu|otages?|invasion",
            ),
        ),
        (
            NewsTheme::Monetary,
            word_re(
                r"fed|federal reserve|fomc|powell|ecb|bce|lagarde|rate (hikes?|cuts?)|interest rates?|inflation|cpi|treasury yields?|taux (directeurs?|d'intérêt)|banque centrale|baisse des taux|hausse des taux",
            ),
        ),
        (NewsTheme::Trade, word_re(r"tariffs?|trade war|export (ban|controls?)|embargo|droits de douane|guerre commerciale|taxes douanières")),
        (
            NewsTheme::Stress,
            word_re(
                r"recession|default(s|ed)?|bank (runs?|collapse|failures?)|financial crisis|market crash|crash|sell-?off|bankruptcy|contagion|récession|krach|faillite|crise financière|effondrement|défaut de paiement",
            ),
        ),
        (
            NewsTheme::Regulation,
            word_re(r"sec|cftc|regulators?|regulation|lawsuit|etf approval|mica|amf|régulateur|régulation|réglementation|plainte|procès"),
        ),
        (
            NewsTheme::Earnings,
            word_re(
                r"earnings|quarterly results|revenue|guidance|eps|profit warning|résultats (trimestriels|annuels|semestriels)|chiffre d'affaires|bénéfice",
            ),
        ),
    ]
});

/// A conditional or an opinion ("could trigger a bank run", "pourrait déclencher") is not an event that happened.
static HEDGED: LazyLock<Regex> =
    LazyLock::new(|| word_re(r"could|might|may|would|risks?|fears? of|what if|pourrai(t|ent)|risquer?ai(t|ent)|risque de|et si|selon (cet|un|une)"));
static ESCALATION: LazyLock<Regex> = LazyLock::new(|| {
    word_re(
        r"declar(es|ed|ing) war|invades?|invaded|invasion of|nuclear (strike|attack|threat|test)|martial law|state of emergency|bank runs?|bank collapse|circuit breaker|trading halted|defaults? on (its )?debt|déclare la guerre|déclaration de guerre|loi martiale|état d'urgence|panique bancaire|cotations suspendues",
    )
});
static NEGATIVE: LazyLock<Regex> = LazyLock::new(|| {
    word_re(
        r"hack(ed)?|exploit|breach|stolen|lawsuit|sues|sued|fraud|bankrupt(cy)?|insolvency|liquidat(ed|ion)|delist(ed|ing)?|ban(ned)?|crackdown|crash(es)?|plunges?|tumbles?|sinks?|slumps?|slides?|sell-?off|downgraded?|misses|layoffs|recall|outage|warning|indictment|falls?|drops?|losses?|piratage|fraude|faillite|chute|plonge|recule|recul|dégringole|effondre|baisse|pertes?|licenciements?|panne|avertissement|sanctions?",
    )
});
static POSITIVE: LazyLock<Regex> = LazyLock::new(|| {
    word_re(
        r"approv(al|ed|es)|inflows|record high|all-time high|surges?|soars?|rall(y|ies)|upgraded?|beats|raises guidance|buyback|partnership|adoption|breakthrough|jumps?|climbs?|gains?|rebounds?|hausse|bondit|grimpe|record|rebond|progresse|s'envole|partenariat|rachat|approbation",
    )
});
static FRENCH: LazyLock<Regex> =
    LazyLock::new(|| word_re(r"le|la|les|des|du|une|pour|dans|sur|avec|est|sont|pas|plus|qui|après|selon|français|bourse|marchés?"));
static ACCENTED: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)[éèàùç]"));

static CDATA: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)<!\[CDATA\[(.*?)\]\]>"));
static TAGS: LazyLock<Regex> = LazyLock::new(|| re(r"<[^>]*>"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(&format!("[{JS_WS}]+")));
static HEX_ENTITY: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)&#x([0-9a-f]+);"));
static DEC_ENTITY: LazyLock<Regex> = LazyLock::new(|| re(r"&#([0-9]+);"));
static ITEM: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)<item(?-u:\b)[^>]*>(.*?)</item>"));
static ENTRY: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)<entry(?-u:\b)[^>]*>(.*?)</entry>"));
static LINK_HREF: LazyLock<Regex> = LazyLock::new(|| re(r#"<link(?-u:\b)[^>]*href="([^"]+)""#));
const TAG_NAMES: [&str; 9] = ["title", "link", "pubDate", "dc:date", "updated", "published", "source", "description", "summary"];
static TAG: LazyLock<Vec<Regex>> = LazyLock::new(|| TAG_NAMES.iter().map(|n| re(&format!(r"(?s)<{n}(?-u:\b)[^>]*>(.*?)</{n}>"))).collect());
static TRAILING_WORD: LazyLock<Regex> = LazyLock::new(|| re(&format!("[{JS_WS}]+[^{JS_WS}]*$")));
static COMPANY_SUFFIX: LazyLock<Regex> = LazyLock::new(|| re(r"(?i),? (Inc|Corp|Corporation|Ltd|plc|SA|NV|Holdings?)\.?$"));

/// `String.fromCodePoint(n)` over the digits of an entity (RangeError beyond U+10FFFF), as UTF-16 code units.
fn code_point(digits: &str, radix: u32, out: &mut Vec<u16>) -> Result<()> {
    let d = digits.trim_start_matches('0');
    let n = if d.is_empty() {
        Some(0)
    } else if d.len() > 7 {
        None
    } else {
        u32::from_str_radix(d, radix).ok()
    };
    match n {
        Some(n) if n <= 0x10FFFF => {
            if (0xD800..=0xDFFF).contains(&n) {
                out.push(n as u16);
            } else {
                let mut buf = [0u16; 2];
                out.extend_from_slice(char::from_u32(n).unwrap().encode_utf16(&mut buf));
            }
            Ok(())
        }
        _ => Err(Error("Arguments contain a value that is out of range of code points".into())),
    }
}

/// `s.replace(re, (_, digits) => String.fromCodePoint(...))`, surrogate pairs from two entities joined.
fn replace_code_points(s: &str, re: &Regex, radix: u32) -> Result<String> {
    let mut out: Vec<u16> = Vec::with_capacity(s.len());
    let mut last = 0;
    for c in re.captures_iter(s) {
        let m = c.get(0).unwrap();
        out.extend(s[last..m.start()].encode_utf16());
        code_point(&c[1], radix, &mut out)?;
        last = m.end();
    }
    if last == 0 {
        return Ok(s.to_string());
    }
    out.extend(s[last..].encode_utf16());
    Ok(String::from_utf16_lossy(&out))
}

fn entities(t: &str) -> Result<String> {
    let t = replace_code_points(t, &HEX_ENTITY, 16)?;
    let t = replace_code_points(&t, &DEC_ENTITY, 10)?;
    Ok(t.replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&"))
}

/// Decodes XML text: CDATA, named and numeric entities, tags (descriptions are often HTML). Err = the RangeError of
/// `String.fromCodePoint` on an entity beyond U+10FFFF.
pub fn decode_text(s: &str) -> Result<String> {
    // Entities first: some feeds encode their HTML (&lt;a href…&gt;), which must go with the tags.
    let s = CDATA.replace_all(s, "$1");
    let s = entities(&s)?;
    let s = TAGS.replace_all(&s, " ");
    let s = entities(&s)?;
    let s = TAGS.replace_all(&s, " ");
    let s = SPACES.replace_all(&s, " ");
    Ok(js_trim(&s).to_string())
}

/// Only http(s) links are kept (a feed could carry javascript: or data: URLs).
pub fn safe_link(s: &str) -> Option<String> {
    let u = url::Url::parse(js_trim(s)).ok()?;
    if u.scheme() == "https" || u.scheme() == "http" { Some(u.to_string()) } else { None }
}

/// `Date.parse` on a feed date (`js::parse_date`, plus an RFC 822 date without zone, read as UTC like Bun does on
/// the server).
fn feed_date(s: &str) -> Option<i64> {
    parse_date(s).or_else(|| {
        let t = s.trim();
        let with_zone = format!("{} +0000", t.strip_suffix(" Z").unwrap_or(t));
        chrono::DateTime::parse_from_rfc2822(&with_zone).ok().map(|d| d.timestamp_millis())
    })
}

/// RSS 2.0 and Atom. `fallback_source` names the feed when items do not (Google News gives each item's source).
/// Err when the TypeScript throws (an entity beyond U+10FFFF).
pub fn parse_feed(xml: &str, fallback_source: &str) -> Result<Vec<RawNews>> {
    let blocks: Vec<&str> = ITEM.captures_iter(xml).chain(ENTRY.captures_iter(xml)).map(|c| c.get(1).unwrap().as_str()).collect();
    let mut out = Vec::new();
    for b in blocks {
        let tag = |name: &str| -> Option<&str> {
            let i = TAG_NAMES.iter().position(|n| *n == name).unwrap();
            TAG[i].captures(b).map(|c| c.get(1).unwrap().as_str())
        };
        let mut title = decode_text(tag("title").unwrap_or(""))?;
        let link_text = decode_text(tag("link").unwrap_or(""))?;
        let link = if link_text.is_empty() {
            safe_link(LINK_HREF.captures(b).map(|c| c.get(1).unwrap().as_str()).unwrap_or(""))
        } else {
            safe_link(&link_text)
        };
        let date = tag("pubDate").or_else(|| tag("dc:date")).or_else(|| tag("updated")).or_else(|| tag("published")).unwrap_or("");
        let time = feed_date(&decode_text(date)?);
        let source_text = decode_text(tag("source").unwrap_or(""))?;
        let mut source = if source_text.is_empty() { fallback_source.to_string() } else { source_text };
        // Google News appends " - Source" to the title.
        let suffix = format!(" - {source}");
        if title.ends_with(&suffix) {
            title.truncate(title.len() - suffix.len());
        } else if source == fallback_source && fallback_source.starts_with("Google") {
            if let Some(cut) = title.rfind(" - ") {
                if js_len(&title[..cut]) > 20 {
                    source = title[cut + 3..].to_string();
                    title.truncate(cut);
                }
            }
        }
        let mut summary = decode_text(tag("description").or_else(|| tag("summary")).unwrap_or(""))?;
        // Google News' description only repeats the title and the source.
        let head: Vec<u16> = title.encode_utf16().take(30).collect();
        if summary.encode_utf16().take(head.len()).eq(head.iter().copied()) && js_len(&summary) >= head.len() {
            summary.clear();
        }
        if js_len(&summary) > 280 {
            summary = TRAILING_WORD.replace(&js_slice_to(&summary, 277), "").into_owned() + "…";
        }
        let (Some(link), Some(time)) = (link, time) else { continue };
        if title.is_empty() {
            continue;
        }
        out.push(RawNews { title, link, time, source, summary: if summary.is_empty() { None } else { Some(summary) } });
    }
    Ok(out)
}

fn words(s: &str) -> Vec<String> {
    let lower = s.to_lowercase();
    let nfd = icu_normalizer::DecomposingNormalizerBorrowed::new_nfd().normalize(&lower);
    let mut t = String::with_capacity(nfd.len());
    for c in nfd.chars() {
        if ('\u{300}'..='\u{36F}').contains(&c) {
            continue;
        }
        t.push(if c.is_ascii_lowercase() || c.is_ascii_digit() || c == ' ' { c } else { ' ' });
    }
    t.split(' ').filter(|w| w.len() > 2).map(String::from).collect()
}

/// Same story: at least 60 % of the words in common (Jaccard), told within 36 h.
pub fn same_story(a: &RawNews, b: &RawNews) -> bool {
    if (a.time - b.time).abs() > 36 * 3_600_000 {
        return false;
    }
    let dedup = |v: Vec<String>| {
        let mut out: Vec<String> = Vec::new();
        for w in v {
            if !out.contains(&w) {
                out.push(w);
            }
        }
        out
    };
    let wa = dedup(words(&a.title));
    let wb = dedup(words(&b.title));
    if wa.len() < 3 || wb.len() < 3 {
        return false;
    }
    let common = wa.iter().filter(|w| wb.contains(w)).count();
    common as f64 / (wa.len() + wb.len() - common) as f64 >= 0.6
}

/// The two tests of [`mentions`] for one asset, compiled once.
pub struct AssetMatcher {
    name: Option<Regex>,
    symbol: Option<Regex>,
}

impl AssetMatcher {
    pub fn new(a: &WatchAsset) -> AssetMatcher {
        let name = match COMPANY_SUFFIX.find(&js_ci(&a.name)) {
            Some(m) => format!("{}{}", &a.name[..m.start()], &a.name[m.end()..]),
            None => a.name.clone(),
        };
        let name = js_trim(&name);
        AssetMatcher {
            name: (js_len(name) >= 4).then(|| re(&format!(r"(?i)(?:^|\P{{L}}){}(?:\P{{L}}|$)", regex::escape(name)))),
            symbol: (js_len(&a.symbol) >= 3).then(|| re(&format!(r"(?:^|[^A-Za-z$])\$?{}(?:[^A-Za-z]|$)", regex::escape(&a.symbol)))),
        }
    }

    pub fn is_match(&self, title: &str) -> bool {
        self.name.as_ref().is_some_and(|r| r.is_match(title)) || self.symbol.as_ref().is_some_and(|r| r.is_match(title))
    }
}

/// Does the title name this asset? Name as a whole word (4 letters at least), or the ticker in capitals (3+ letters).
pub fn mentions(title: &str, a: &WatchAsset) -> bool {
    AssetMatcher::new(a).is_match(title)
}

/// FNV-1a over the UTF-16 code units, base 36.
fn hash(s: &str) -> String {
    let mut h: u32 = 2166136261;
    for u in s.encode_utf16() {
        h = (h ^ u as u32).wrapping_mul(16777619);
    }
    if h == 0 {
        return "0".into();
    }
    let mut digits = Vec::new();
    while h > 0 {
        digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(h % 36) as usize]);
        h /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap()
}

/// Items of one feed with the category of the feed (`{ category, fallback?, items }`).
#[derive(Debug, Clone, Copy)]
pub struct FeedItems<'a> {
    pub category: NewsCategory,
    /// For the searches of one asset: where a story that does not name it goes.
    pub fallback: Option<NewsCategory>,
    pub items: &'a [RawNews],
}

/// Default `maxAgeMs` of `aggregate`.
pub const MAX_AGE_MS: i64 = 48 * 3_600_000;

/// From the raw items of every feed (tagged with the category of their feed) to the news section: duplicates merged
/// (the earliest version is kept, the others listed in `alsoIn`), most recent first, limited to `max_age_ms` and to
/// [PER_CATEGORY] items per category. A story is filed under "actifs" only if it names one of the user's assets; the
/// searches made for an asset that bring unrelated stories fall back to their market (`fallback`).
pub fn aggregate(feeds: &[FeedItems], assets: &[WatchAsset], now: i64, max_age_ms: i64) -> Vec<NewsItem> {
    let mut all: Vec<(&RawNews, NewsCategory)> = feeds
        .iter()
        .flat_map(|f| {
            let category = if f.category == NewsCategory::Actifs { f.fallback.unwrap_or(NewsCategory::Marches) } else { f.category };
            f.items.iter().filter(move |i| i.time <= now + 600_000 && i.time >= now - max_age_ms).map(move |i| (i, category))
        })
        .collect();
    all.sort_by_key(|(i, _)| i.time);
    struct Kept<'a> {
        news: &'a RawNews,
        category: NewsCategory,
        also_in: Vec<String>,
    }
    let matchers: Vec<(&WatchAsset, AssetMatcher)> = assets.iter().map(|a| (a, AssetMatcher::new(a))).collect();
    let mut kept: Vec<Kept> = Vec::new();
    for (it, category) in all {
        if let Some(twin) = kept.iter_mut().find(|k| k.news.link == it.link || same_story(k.news, it)) {
            if twin.news.source != it.source && !twin.also_in.contains(&it.source) {
                twin.also_in.push(it.source.clone());
            }
            continue;
        }
        kept.push(Kept { news: it, category, also_in: Vec::new() });
    }
    let mut items: Vec<NewsItem> = kept
        .into_iter()
        .map(|k| {
            let n = k.news;
            let text = format!("{} {}", n.title, n.summary.as_deref().unwrap_or(""));
            let text = js_ci(&text);
            let title = js_ci(&n.title);
            let asset_ids: Vec<String> = matchers.iter().filter(|(_, m)| m.is_match(&n.title)).map(|(a, _)| a.id.clone()).collect();
            let neg = NEGATIVE.is_match(&title);
            let pos = POSITIVE.is_match(&title);
            let french_words = FRENCH.find_iter(&title).count();
            NewsItem {
                id: hash(&n.link),
                title: n.title.clone(),
                link: n.link.clone(),
                time: n.time,
                source: n.source.clone(),
                summary: n.summary.clone(),
                lang: if french_words >= 2 || ACCENTED.is_match(&n.title) { Lang::Fr } else { Lang::En },
                category: if asset_ids.is_empty() { k.category } else { NewsCategory::Actifs },
                themes: THEMES.iter().filter(|(_, r)| r.is_match(&text)).map(|(t, _)| *t).collect(),
                tone: if neg && !pos {
                    NewsTone::Negative
                } else if pos && !neg {
                    NewsTone::Positive
                } else {
                    NewsTone::Neutral
                },
                assets: asset_ids,
                also_in: k.also_in.into_iter().take(8).collect(),
                alert: ESCALATION.is_match(&title) && !HEDGED.is_match(&title),
            }
        })
        .collect();
    items.sort_by_key(|a| std::cmp::Reverse(a.time));
    let mut count = [0usize; 4];
    items.retain(|i| {
        let c = &mut count[i.category as usize];
        *c += 1;
        *c <= i.category.per_category()
    });
    items
}

/// "À la une": serious escalations first, then the stories told by the most sources (at least 2), recent first.
pub fn top_stories(items: &[NewsItem], n: usize) -> Vec<NewsItem> {
    // A story on the front page is told by 2 sources at least, a serious escalation too (it comes first).
    let mut v: Vec<NewsItem> = items.iter().filter(|i| !i.also_in.is_empty()).cloned().collect();
    v.sort_by(|a, b| b.alert.cmp(&a.alert).then(b.also_in.len().cmp(&a.also_in.len())).then(b.time.cmp(&a.time)));
    v.truncate(n);
    v
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DigestTheme {
    pub theme: NewsTheme,
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToneCount {
    pub negative: usize,
    pub positive: usize,
    pub neutral: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsDigest {
    pub total: usize,
    pub themes: Vec<DigestTheme>,
    pub tone: ToneCount,
}

/// Counts per theme and tone over the last 24 h (what dominates the news right now).
pub fn news_digest(items: &[NewsItem], now: i64) -> NewsDigest {
    let day: Vec<&NewsItem> = items.iter().filter(|i| i.time >= now - 86_400_000).collect();
    let mut themes: Vec<DigestTheme> = THEME_LABEL
        .iter()
        .map(|(t, label)| DigestTheme { theme: *t, label: (*label).into(), count: day.iter().filter(|i| i.themes.contains(t)).count() })
        .filter(|t| t.count > 0)
        .collect();
    themes.sort_by_key(|a| std::cmp::Reverse(a.count));
    let tone_count = |t: NewsTone| day.iter().filter(|i| i.tone == t).count();
    NewsDigest {
        total: day.len(),
        themes,
        tone: ToneCount {
            negative: tone_count(NewsTone::Negative),
            positive: tone_count(NewsTone::Positive),
            neutral: tone_count(NewsTone::Neutral),
        },
    }
}

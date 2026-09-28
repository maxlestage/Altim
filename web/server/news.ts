/**
 * Sources of the news section: specialised feeds in French and English plus Google News searches, each fetched with a
 * timeout and cached 10 minutes; a feed that fails is left out (and reported), the others still come.
 */
import { cached } from "./cache";
import { parseFeed, type NewsCategory, type RawNews, type WatchAsset } from "../src/engine/news";
import type { Asset } from "./quotes";

export interface Feed {
  name: string;
  url: string;
  category: NewsCategory;
  /** For the searches of one asset: where a story that does not name it goes. */
  fallback?: NewsCategory;
}

const google = (q: string, lang: "fr" | "en", when = "1d") =>
  lang === "fr"
    ? `https://news.google.com/rss/search?q=${encodeURIComponent(`${q} when:${when}`)}&hl=fr&gl=FR&ceid=FR:fr`
    : `https://news.google.com/rss/search?q=${encodeURIComponent(`${q} when:${when}`)}&hl=en-US&gl=US&ceid=US:en`;

export const FEEDS: Feed[] = [
  // World economy and geopolitics
  { name: "Le Monde Économie", url: "https://www.lemonde.fr/economie/rss_full.xml", category: "monde" },
  { name: "BFM Économie", url: "https://www.bfmtv.com/rss/economie/", category: "monde" },
  { name: "La Tribune", url: "https://www.latribune.fr/feed.xml", category: "monde" },
  { name: "Google Actualités (économie)", url: google("(économie OR bourse OR BCE OR Fed OR inflation OR guerre OR sanctions OR droits de douane)", "fr"), category: "monde" },
  { name: "Google News (world)", url: google('(war OR sanctions OR "Federal Reserve" OR inflation OR tariffs OR recession OR ceasefire)', "en"), category: "monde" },
  // Markets and stocks
  { name: "MarketWatch", url: "https://feeds.content.dowjones.io/public/rss/mw_topstories", category: "marches" },
  { name: "CNBC", url: "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id=100003114", category: "marches" },
  { name: "Investing.com", url: "https://www.investing.com/rss/news_25.rss", category: "marches" },
  { name: "Google News (markets)", url: google('("stock market" OR "Wall Street" OR Nasdaq OR "S&P 500")', "en"), category: "marches" },
  // Crypto
  { name: "CoinDesk", url: "https://www.coindesk.com/arc/outboundfeeds/rss/", category: "crypto" },
  { name: "Cointelegraph", url: "https://cointelegraph.com/rss", category: "crypto" },
  { name: "Decrypt", url: "https://decrypt.co/feed", category: "crypto" },
  { name: "The Block", url: "https://www.theblock.co/rss.xml", category: "crypto" },
  { name: "Cryptoast", url: "https://cryptoast.fr/feed/", category: "crypto" },
  { name: "Journal du Token", url: "https://journaldutoken.com/feed/", category: "crypto" },
];

const UA = { "User-Agent": "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0", Accept: "application/rss+xml, application/xml, text/xml, */*" };

export interface FeedResult { feed: Feed; items: RawNews[]; ok: boolean; error?: string }

async function fetchFeed(feed: Feed): Promise<FeedResult> {
  try {
    const items = await cached(`feed:${feed.url}`, 600_000, async () => {
      const r = await fetch(feed.url, { headers: UA, redirect: "follow", signal: AbortSignal.timeout(8_000) });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const text = await r.text();
      if (text.length > 5_000_000) throw new Error("flux trop gros");
      return parseFeed(text, feed.name);
    });
    return { feed, items, ok: items.length > 0, error: items.length ? undefined : "flux vide" };
  } catch (e) {
    return { feed, items: [], ok: false, error: e instanceof Error ? e.message.slice(0, 80) : "échec" };
  }
}

/** Feeds of the user's own assets: Yahoo Finance for each stock, Google News (FR and EN) for each asset. */
export function assetFeeds(assets: Asset[]): Feed[] {
  return assets.flatMap((a) => {
    const name = a.name.replace(/,? (Inc|Corp|Corporation|Ltd|plc)\.?$/i, "");
    const q = a.kind === "crypto" ? `"${name}" crypto` : `"${name}" OR ${a.symbol.replace(/-/g, ".")} action`;
    const fallback: NewsCategory = a.kind === "crypto" ? "crypto" : "marches";
    const list: Feed[] = [
      { name: "Google News", url: google(a.kind === "crypto" ? `"${name}" crypto` : `${a.symbol.replace(/-/g, ".")} stock "${name}"`, "en", "2d"), category: "actifs", fallback },
      { name: "Google Actualités", url: google(q, "fr", "2d"), category: "actifs", fallback },
    ];
    if (a.kind === "stock") {
      list.push({ name: "Yahoo Finance", url: `https://feeds.finance.yahoo.com/rss/2.0/headline?s=${encodeURIComponent(a.symbol)}&region=US&lang=en-US`, category: "actifs", fallback });
    }
    return list;
  });
}

export async function fetchNews(assets: Asset[]): Promise<{ results: FeedResult[]; watch: WatchAsset[] }> {
  const feeds = [...FEEDS, ...assetFeeds(assets.slice(0, 20))];
  const results = await Promise.all(feeds.map(fetchFeed));
  const watch = assets.map((a) => ({ id: `${a.kind}:${a.symbol}`, symbol: a.symbol, name: a.name }));
  return { results, watch };
}

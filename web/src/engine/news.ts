/**
 * News section: aggregation of many RSS feeds (world economy and geopolitics, crypto, stocks, the user's own assets),
 * in French and English. Pure functions, tested on real feeds:
 * - robust RSS / Atom parsing (CDATA, numeric entities, HTML in descriptions, odd dates);
 * - merge of the same story told by several sources (its coverage = how important it is);
 * - classification by category, theme (war, central banks, tariffs, crisis) and tone, detection of the user's assets.
 * Titles are kept in their language: nothing is machine-translated nor rewritten.
 */

export type NewsCategory = "monde" | "marches" | "crypto" | "actifs";
export type NewsTheme = "geopolitics" | "monetary" | "trade" | "stress" | "regulation" | "earnings";
export type NewsTone = "negative" | "positive" | "neutral";

export interface RawNews {
  title: string;
  link: string;
  time: number;
  source: string;
  summary?: string;
}

export interface NewsItem {
  id: string;
  title: string;
  link: string;
  time: number;
  source: string;
  summary: string | null;
  lang: "fr" | "en";
  category: NewsCategory;
  themes: NewsTheme[];
  tone: NewsTone;
  /** "crypto:BTC"… assets of the user named in the title. */
  assets: string[];
  /** Other sources that told the same story (same event, near-identical title). */
  alsoIn: string[];
  /** Serious escalation (war declared, invasion, bank run…). */
  alert: boolean;
}

export const THEME_LABEL: Record<NewsTheme, string> = {
  geopolitics: "Géopolitique / guerre",
  monetary: "Banques centrales / taux",
  trade: "Commerce / droits de douane",
  stress: "Crise / krach",
  regulation: "Régulation",
  earnings: "Résultats d'entreprises",
};

const THEMES: { theme: NewsTheme; re: RegExp }[] = [
  { theme: "geopolitics", re: /\b(wars?|invasion|invades?|invaded|missiles?|air ?strikes?|drones?|military|troops|nuclear|sanctions?|ceasefire|hostages?|coup|blockade|guerre|frappes?|armée|militaires?|nucléaire|cessez-le-feu|otages?|invasion)\b/i },
  { theme: "monetary", re: /\b(fed|federal reserve|fomc|powell|ecb|bce|lagarde|rate (hikes?|cuts?)|interest rates?|inflation|cpi|treasury yields?|taux (directeurs?|d'intérêt)|banque centrale|baisse des taux|hausse des taux)\b/i },
  { theme: "trade", re: /\b(tariffs?|trade war|export (ban|controls?)|embargo|droits de douane|guerre commerciale|taxes douanières)\b/i },
  { theme: "stress", re: /\b(recession|default(s|ed)?|bank (runs?|collapse|failures?)|financial crisis|market crash|crash|sell-?off|bankruptcy|contagion|récession|krach|faillite|crise financière|effondrement|défaut de paiement)\b/i },
  { theme: "regulation", re: /\b(sec|cftc|regulators?|regulation|lawsuit|etf approval|mica|amf|régulateur|régulation|réglementation|plainte|procès)\b/i },
  { theme: "earnings", re: /\b(earnings|quarterly results|revenue|guidance|eps|profit warning|résultats (trimestriels|annuels|semestriels)|chiffre d'affaires|bénéfice)\b/i },
];

const ESCALATION = /\b(declar(es|ed|ing) war|invades?|invaded|invasion of|nuclear (strike|attack|threat|test)|martial law|state of emergency|bank runs?|bank collapse|circuit breaker|trading halted|defaults? on (its )?debt|déclare la guerre|déclaration de guerre|loi martiale|état d'urgence|panique bancaire|cotations suspendues)\b/i;

const NEGATIVE = /\b(hack(ed)?|exploit|breach|stolen|lawsuit|sues|sued|fraud|bankrupt(cy)?|insolvency|liquidat(ed|ion)|delist(ed|ing)?|ban(ned)?|crackdown|crash(es)?|plunges?|tumbles?|sinks?|slumps?|slides?|sell-?off|downgraded?|misses|layoffs|recall|outage|warning|indictment|falls?|drops?|losses?|piratage|fraude|faillite|chute|plonge|recule|recul|dégringole|effondre|baisse|pertes?|licenciements?|panne|avertissement|sanctions?)\b/i;
const POSITIVE = /\b(approv(al|ed|es)|inflows|record high|all-time high|surges?|soars?|rall(y|ies)|upgraded?|beats|raises guidance|buyback|partnership|adoption|breakthrough|jumps?|climbs?|gains?|rebounds?|hausse|bondit|grimpe|record|rebond|progresse|s'envole|partenariat|rachat|approbation)\b/i;

const FRENCH = /\b(le|la|les|des|du|une|pour|dans|sur|avec|est|sont|pas|plus|qui|après|selon|français|bourse|marchés?)\b/gi;

/** Decodes XML text: CDATA, named and numeric entities, tags (descriptions are often HTML). */
export function decodeText(s: string): string {
  const entities = (t: string) =>
    t
      .replace(/&#x([0-9a-f]+);/gi, (_, h) => String.fromCodePoint(parseInt(h, 16)))
      .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(Number(d)))
      .replace(/&nbsp;/g, " ").replace(/&quot;/g, '"').replace(/&#39;|&apos;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");
  // Entities first: some feeds encode their HTML (&lt;a href…&gt;), which must go with the tags.
  return entities(entities(s.replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1")).replace(/<[^>]*>/g, " "))
    .replace(/<[^>]*>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/** Only http(s) links are kept (a feed could carry javascript: or data: URLs). */
export function safeLink(s: string): string | null {
  try {
    const u = new URL(s.trim());
    return u.protocol === "https:" || u.protocol === "http:" ? u.toString() : null;
  } catch {
    return null;
  }
}

/** RSS 2.0 and Atom. `fallbackSource` names the feed when items do not (Google News gives each item's source). */
export function parseFeed(xml: string, fallbackSource: string): RawNews[] {
  const blocks = [...xml.matchAll(/<item\b[^>]*>([\s\S]*?)<\/item>/g), ...xml.matchAll(/<entry\b[^>]*>([\s\S]*?)<\/entry>/g)].map((m) => m[1]!);
  return blocks.flatMap((b) => {
    const tag = (name: string) => b.match(new RegExp(`<${name}\\b[^>]*>([\\s\\S]*?)</${name}>`))?.[1];
    let title = decodeText(tag("title") ?? "");
    const link = safeLink(decodeText(tag("link") ?? "") || (b.match(/<link\b[^>]*href="([^"]+)"/)?.[1] ?? ""));
    const date = tag("pubDate") ?? tag("dc:date") ?? tag("updated") ?? tag("published") ?? "";
    const time = Date.parse(decodeText(date));
    let source = decodeText(tag("source") ?? "") || fallbackSource;
    // Google News appends " - Source" to the title.
    const suffix = ` - ${source}`;
    if (title.endsWith(suffix)) title = title.slice(0, -suffix.length);
    else if (source === fallbackSource && fallbackSource.startsWith("Google")) {
      const cut = title.lastIndexOf(" - ");
      if (cut > 20) {
        source = title.slice(cut + 3);
        title = title.slice(0, cut);
      }
    }
    let summary = decodeText(tag("description") ?? tag("summary") ?? "");
    // Google News' description only repeats the title and the source.
    if (summary.startsWith(title.slice(0, 30))) summary = "";
    if (summary.length > 280) summary = summary.slice(0, 277).replace(/\s+\S*$/, "") + "…";
    if (!title || !link || !Number.isFinite(time)) return [];
    return [{ title, link, time, source, summary: summary || undefined }];
  });
}

const words = (s: string) =>
  s.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9 ]/g, " ").split(/\s+/).filter((w) => w.length > 2);

/** Same story: at least 60 % of the words in common (Jaccard), told within 36 h. */
export function sameStory(a: RawNews, b: RawNews): boolean {
  if (Math.abs(a.time - b.time) > 36 * 3_600_000) return false;
  const wa = new Set(words(a.title)), wb = new Set(words(b.title));
  if (wa.size < 3 || wb.size < 3) return false;
  let common = 0;
  for (const w of wa) if (wb.has(w)) common++;
  return common / (wa.size + wb.size - common) >= 0.6;
}

export interface WatchAsset { id: string; symbol: string; name: string }

/** Does the title name this asset? Name as a whole word (4 letters at least), or the ticker in capitals (3+ letters). */
export function mentions(title: string, a: WatchAsset): boolean {
  const name = a.name.replace(/,? (Inc|Corp|Corporation|Ltd|plc|SA|NV|Holdings?)\.?$/i, "").trim();
  const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (name.length >= 4 && new RegExp(`(^|[^\\p{L}])${esc(name)}([^\\p{L}]|$)`, "iu").test(title)) return true;
  return a.symbol.length >= 3 && new RegExp(`(^|[^A-Za-z$])\\$?${esc(a.symbol)}([^A-Za-z]|$)`).test(title);
}

function hash(s: string): string {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return (h >>> 0).toString(36);
}

/** Items kept per category, so that one big feed does not crowd out the others. */
export const PER_CATEGORY: Record<NewsCategory, number> = { monde: 70, marches: 60, crypto: 60, actifs: 80 };

/**
 * From the raw items of every feed (tagged with the category of their feed) to the news section: duplicates merged
 * (the earliest version is kept, the others listed in `alsoIn`), most recent first, limited to `maxAgeMs` and to
 * [PER_CATEGORY] items per category. A story is filed under "actifs" only if it names one of the user's assets; the
 * searches made for an asset that bring unrelated stories fall back to their market (`fallback`).
 */
export function aggregate(
  feeds: { category: NewsCategory; fallback?: NewsCategory; items: RawNews[] }[],
  assets: WatchAsset[],
  now: number,
  maxAgeMs = 48 * 3_600_000,
): NewsItem[] {
  const all = feeds.flatMap((f) =>
    f.items.filter((i) => i.time <= now + 600_000 && i.time >= now - maxAgeMs).map((i) => ({ ...i, category: f.category === "actifs" ? (f.fallback ?? "marches") : f.category })),
  );
  all.sort((a, b) => a.time - b.time);
  const kept: (RawNews & { category: NewsCategory; alsoIn: Set<string>; assets: Set<string> })[] = [];
  for (const it of all) {
    const twin = kept.find((k) => k.link === it.link || sameStory(k, it));
    if (twin) {
      if (twin.source !== it.source) twin.alsoIn.add(it.source);
      continue;
    }
    kept.push({ ...it, alsoIn: new Set(), assets: new Set() });
  }
  const items = kept
    .map((k) => {
      const text = `${k.title} ${k.summary ?? ""}`;
      const assetIds = assets.filter((a) => mentions(k.title, a)).map((a) => a.id);
      const neg = NEGATIVE.test(k.title), pos = POSITIVE.test(k.title);
      const frenchWords = (k.title.match(FRENCH) ?? []).length;
      return {
        id: hash(k.link),
        title: k.title,
        link: k.link,
        time: k.time,
        source: k.source,
        summary: k.summary ?? null,
        lang: frenchWords >= 2 || /[éèàùç]/i.test(k.title) ? "fr" : "en",
        category: assetIds.length ? "actifs" : k.category,
        themes: THEMES.filter((t) => t.re.test(text)).map((t) => t.theme),
        tone: neg && !pos ? "negative" : pos && !neg ? "positive" : "neutral",
        assets: assetIds,
        alsoIn: [...k.alsoIn].slice(0, 8),
        alert: ESCALATION.test(k.title),
      } satisfies NewsItem;
    })
    .sort((a, b) => b.time - a.time);
  const count: Partial<Record<NewsCategory, number>> = {};
  return items.filter((i) => (count[i.category] = (count[i.category] ?? 0) + 1) <= PER_CATEGORY[i.category]);
}

/** "À la une": serious escalations first, then the stories told by the most sources (at least 2), recent first. */
export function topStories(items: NewsItem[], n = 5): NewsItem[] {
  return items
    .filter((i) => i.alert || i.alsoIn.length >= 1)
    .sort((a, b) => Number(b.alert) - Number(a.alert) || b.alsoIn.length - a.alsoIn.length || b.time - a.time)
    .slice(0, n);
}

/** Counts per theme and tone over the last 24 h (what dominates the news right now). */
export function newsDigest(items: NewsItem[], now: number) {
  const day = items.filter((i) => i.time >= now - 86_400_000);
  const themes = (Object.keys(THEME_LABEL) as NewsTheme[])
    .map((t) => ({ theme: t, label: THEME_LABEL[t], count: day.filter((i) => i.themes.includes(t)).length }))
    .filter((t) => t.count > 0)
    .sort((a, b) => b.count - a.count);
  const tone = {
    negative: day.filter((i) => i.tone === "negative").length,
    positive: day.filter((i) => i.tone === "positive").length,
    neutral: day.filter((i) => i.tone === "neutral").length,
  };
  return { total: day.length, themes, tone };
}

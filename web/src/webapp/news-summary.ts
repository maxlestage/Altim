/** "Résumé intelligent" of the news (`summary` of /api/news, computed by the server in engine/news_summary.rs):
 * types and the pure helpers of the card. */
import type { NewsCategory, NewsTheme, NewsTone } from "../engine/news";

export type Impact = "low" | "medium" | "high";

export type StorySummary = {
  /** Id of the merged story in `items`. */
  id: string;
  title: string;
  link: string;
  source: string;
  time: number;
  category: NewsCategory;
  themes: NewsTheme[];
  alert: boolean;
  /** Distinct sources that told the story. */
  sources: number;
  /** Distinct headlines among them (one article syndicated word for word counts once): what the rule counts. */
  independentSources: number;
  /** Each source's own headline (8 at most), the first one first. */
  links: { source: string; title: string; link: string; time: number; tone: NewsTone }[];
  /** The user's assets named in the headlines: "stock:AAPL"… */
  assets: string[];
  impact: Impact;
  /** "measured": from the move of the named assets since publication; "rule": sources, theme, the user's assets. */
  impactBasis: "measured" | "rule";
  ruleImpact: Impact;
  impactPoints: number;
  /** French, one line per point of the rule, and the measured moves. */
  impactReasons: string[];
  consensus: { agreement: "convergent" | "divergent" | "single"; tone: NewsTone; negative: number; positive: number; neutral: number };
  /** Hourly closes of the server's cache: from the close preceding publication to the last one (ms, prices, %). */
  moves: { asset: string; fromTime: number; fromPrice: number; toTime: number; toPrice: number; changePct: number; source: string }[];
  /** The story against the asset's technical trend (the guard's, when cached). */
  technical: { asset: string; trend: "up" | "down" | "range"; text: string }[];
};

export const IMPACT_LABEL: Record<Impact, string> = { low: "faible", medium: "moyen", high: "important" };

export function basisLabel(s: StorySummary): string {
  return s.impactBasis === "measured" ? "impact mesuré" : "impact estimé par règle";
}

/** Counts the events of moyen or important impact; the faible ones are "autres sujets". */
export function summaryHeading(list: StorySummary[]): { title: string; others: string | null } {
  const n = list.filter((s) => s.impact !== "low").length;
  const rest = list.length - n;
  const title = n === 0 ? "Aucun événement important aujourd'hui" : n === 1 ? "1 événement important aujourd'hui" : `${n} événements importants aujourd'hui`;
  const others = rest === 0 ? null : `${n === 0 ? "" : "Et "}${rest} sujet${rest > 1 ? "s" : ""} repris par plusieurs sources, à impact faible.`;
  return { title, others };
}

const plural = (n: number, one: string, many: string) => `${n} ${n > 1 ? many : one}`;

/** "Convergent · 3 sources · ton des titres : 2 négatifs, 1 neutre" (tones counted over the distinct headlines). */
export function consensusText(s: StorySummary): string {
  const c = s.consensus;
  if (c.agreement === "single") {
    return s.sources > 1 ? `Même titre repris par ${s.sources} sources : pas de consensus mesurable` : "Une seule source : pas de consensus mesurable";
  }
  const parts = [
    c.negative && plural(c.negative, "négatif", "négatifs"),
    c.positive && plural(c.positive, "positif", "positifs"),
    c.neutral && plural(c.neutral, "neutre", "neutres"),
  ].filter(Boolean);
  const titles = c.negative + c.positive + c.neutral;
  const head = `${c.agreement === "convergent" ? "Convergent" : "Divergent"} · ${plural(s.sources, "source", "sources")}`;
  return `${head}${titles < s.sources ? ` (${titles} titres distincts)` : ""} · ton des titres : ${parts.join(", ")}`;
}

/** "stock:AAPL" → { kind, symbol, href }. */
export function assetLink(id: string): { kind: string; symbol: string; href: string } {
  const [kind = "", symbol = id] = id.includes(":") ? id.split(":") : ["", id];
  return { kind, symbol, href: `/app/actif/${kind}/${encodeURIComponent(symbol)}` };
}

/** "AAPL +4,0 % depuis la publication". */
export function moveText(m: StorySummary["moves"][number]): string {
  const pct = `${m.changePct >= 0 ? "+" : "−"}${Math.abs(m.changePct).toFixed(1).replace(".", ",")} %`;
  return `${assetLink(m.asset).symbol} ${pct} depuis la publication`;
}

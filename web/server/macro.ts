/**
 * Macro-economic and geopolitical context (market-wide, shared by every asset): VIX, S&P 500, oil, gold, dollar and
 * US 10-year yields (Yahoo Finance, 5 years of daily closes), plus world headlines (Google News).
 */
import { macroReport, type MacroKey, type MacroReport, type MacroSeries } from "../src/engine/macro";
import type { NewsItem } from "../src/engine/guard";
import { cached } from "./cache";
import { parseStock } from "./market";
import { parseGuard } from "./guard";

const UA = { "User-Agent": "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0" };

export const MACRO_SYMBOLS: Record<MacroKey, string> = {
  vix: "^VIX", spx: "^GSPC", oil: "CL=F", gold: "GC=F", dollar: "DX-Y.NYB", rates: "^TNX",
};

const QUERIES = [
  '(war OR invasion OR missile OR airstrike OR sanctions OR military OR nuclear OR ceasefire OR strait)',
  '("Federal Reserve" OR Fed OR inflation OR tariffs OR recession OR "interest rates" OR "stock market" OR "sell-off" OR "trade war" OR "bank")',
];

async function series(symbol: string): Promise<{ time: number; close: number }[]> {
  const r = await fetch(`https://query1.finance.yahoo.com/v8/finance/chart/${encodeURIComponent(symbol)}?interval=1d&range=5y`, { headers: UA, signal: AbortSignal.timeout(8000) });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return parseStock.yahoo(await r.json()).map((c) => ({ time: c.time, close: c.close }));
}

export function macroSeries(): Promise<MacroSeries> {
  return cached("macro:series", 3_600_000, async () => {
    const entries = await Promise.all(
      (Object.entries(MACRO_SYMBOLS) as [MacroKey, string][]).map(async ([k, s]) => [k, await series(s).catch(() => [])] as const),
    );
    const out: MacroSeries = Object.fromEntries(entries.filter(([, v]) => v.length));
    if (!out.vix && !out.spx) throw new Error("données macro indisponibles");
    return out;
  });
}

export function worldNews(): Promise<NewsItem[]> {
  return cached("macro:news", 600_000, async () => {
    const lists = await Promise.all(QUERIES.map(async (q) => {
      const r = await fetch(`https://news.google.com/rss/search?q=${encodeURIComponent(`${q} when:1d`)}&hl=en-US&gl=US&ceid=US:en`, { headers: UA, signal: AbortSignal.timeout(8000) });
      return r.ok ? parseGuard.rss(await r.text()) : [];
    }));
    const seen = new Set<string>();
    return lists.flat().filter((n) => !seen.has(n.title) && seen.add(n.title));
  }).catch(() => []);
}

export async function macro(now = Date.now()): Promise<MacroReport> {
  const [s, news] = await Promise.all([macroSeries(), worldNews()]);
  return macroReport(s, news, now);
}

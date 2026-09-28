/**
 * "Opportunités du moment" (/api/opportunities) and anomalies of one asset (/api/anomalies): JSON contracts and
 * the pure filtering of the scan on the page (category chips, market cap or rank, liquidity, volatility).
 */

export type OppCategory = "setup" | "reversal" | "breakout" | "volume" | "oversold" | "fundamentals";
export const OPP_CATEGORIES: OppCategory[] = ["setup", "reversal", "breakout", "volume", "oversold", "fundamentals"];
export const OPP_SHORT: Record<OppCategory, string> = {
  setup: "Configurations",
  reversal: "Retournements",
  breakout: "Cassures",
  volume: "Volume anormal",
  oversold: "Survendus",
  fundamentals: "Fondamentaux",
};

export type OppHit = { category: OppCategory; reason: string; strength: number };
export type OppItem = {
  symbol: string; name: string; sector: string;
  /** Stocks: Nasdaq market cap, USD. Cryptos: null (rank instead). */
  marketCap: number | null;
  /** Cryptos: CoinGecko market-cap rank. */
  rank: number | null;
  price: number; time: number; change1d: number | null; rsi14: number | null; volumeRatio: number | null;
  /** ATR(14) ÷ price, % per day. */
  volatility: number | null;
  /** Average daily traded value over 20 sessions, USD. */
  liquidity: number | null;
  distanceAtr: number | null;
  hits: OppHit[];
};
export type OppCategoryInfo = { id: OppCategory; label: string; rule: string; analyzed: number; note: string | null; error: string | null };
export type NotCovered = { label: string; reason: string };
export type OpportunityReport = {
  kind: "stock" | "crypto"; asOf: number; scanned: number; universe: string; topN: number;
  categories: OppCategoryInfo[]; items: OppItem[]; notCovered: NotCovered[]; source: string;
};

export type OppFilters = {
  categories: OppCategory[];
  /** Stocks: minimum market cap (USD). */
  minCap: number | null;
  /** Cryptos: best rank allowed (top N by market cap). */
  maxRank: number | null;
  /** Minimum average daily traded value (USD). */
  minLiquidity: number | null;
  /** Maximum daily volatility (ATR %). */
  maxVolatility: number | null;
};

export const DEFAULT_FILTERS: OppFilters = { categories: [...OPP_CATEGORIES], minCap: null, maxRank: null, minLiquidity: null, maxVolatility: null };

/** Whether an asset passes the numeric filters (an unknown value fails a filter that is set). */
export function passes(item: OppItem, f: OppFilters): boolean {
  if (f.minCap != null && !(item.marketCap != null && item.marketCap >= f.minCap)) return false;
  if (f.maxRank != null && !(item.rank != null && item.rank <= f.maxRank)) return false;
  if (f.minLiquidity != null && !(item.liquidity != null && item.liquidity >= f.minLiquidity)) return false;
  if (f.maxVolatility != null && !(item.volatility != null && item.volatility <= f.maxVolatility)) return false;
  return true;
}

/** Assets that pass the filters, with only the hits of the chosen categories; the most hits (then strongest) first. */
export function filterItems(items: OppItem[], f: OppFilters): OppItem[] {
  const wanted = new Set(f.categories);
  const best = (i: OppItem) => Math.max(0, ...i.hits.map((h) => h.strength));
  return items
    .filter((i) => passes(i, f))
    .map((i) => ({ ...i, hits: i.hits.filter((h) => wanted.has(h.category)) }))
    .filter((i) => i.hits.length > 0)
    .sort((a, b) => b.hits.length - a.hits.length || best(b) - best(a));
}

/** Number of assets per category once the numeric filters are applied (for the chips). */
export function countByCategory(items: OppItem[], f: OppFilters): Record<OppCategory, number> {
  const out = Object.fromEntries(OPP_CATEGORIES.map((c) => [c, 0])) as Record<OppCategory, number>;
  for (const i of items) {
    if (!passes(i, f)) continue;
    for (const c of new Set(i.hits.map((h) => h.category))) out[c]++;
  }
  return out;
}

// ---------- Anomalies of one asset ----------

export type Severity = "normal" | "warning" | "high";
export type Anomaly = {
  code: "volume" | "priceVolume" | "zScore" | "openInterest" | "funding" | "longShort";
  severity: Severity; triggered: boolean; title: string; value: number; threshold: number; unit: string;
  measured: string; meaning: string; source: string;
};
export type LiquidationSummary = {
  longUsd: number; shortUsd: number; longCount: number; shortCount: number;
  largest: { usd: number; long: boolean; price: number; time: number } | null;
  from: number; to: number; hours: number; complete: boolean; scope: string;
};
export type Derivatives = {
  source: string;
  liquidations: LiquidationSummary | null;
  openInterest: { usd: number; time: number; change24h: number | null; change7d: number | null } | null;
  /** Rates in % per settlement period. */
  funding: { rate: number; p5: number; p95: number; samples: number; periodHours: number | null; time: number } | null;
  longShort: { ratio: number; p5: number; p95: number; samples: number } | null;
  errors: string[];
  notCovered: NotCovered[];
};
export type AnomalyReport = {
  symbol: string; kind: "stock" | "crypto"; asOf: number; price: number | null; session: number | null;
  anomalies: Anomaly[]; normal: Anomaly[]; derivatives: Derivatives | null; errors: string[]; source: string;
};

/** Share of the liquidated value that was long positions (%), null when nothing was liquidated. */
export function longShare(l: LiquidationSummary): number | null {
  const total = l.longUsd + l.shortUsd;
  return total > 0 ? (l.longUsd / total) * 100 : null;
}

/** "12,3 M$", "850 k$", "420 $". */
export function compactUsd(v: number): string {
  const a = Math.abs(v);
  const f = (x: number, d: number) => x.toLocaleString("fr-FR", { maximumFractionDigits: d });
  if (a >= 1e9) return `${f(v / 1e9, 1)} Md$`;
  if (a >= 1e6) return `${f(v / 1e6, 1)} M$`;
  if (a >= 1e3) return `${f(v / 1e3, 0)} k$`;
  return `${f(v, 0)} $`;
}

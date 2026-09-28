/**
 * "Point du jour": the day in a few lines for the user's radar and holdings — the market climate (macro), what can
 * be bought now (same rule as the notifications), the biggest moves since the last daily close and the top story.
 */

export type MarketLevel = "calm" | "tense" | "high";

export const MARKET_LABEL: Record<MarketLevel, string> = { calm: "Contexte calme", tense: "Contexte tendu", high: "Tension élevée" };

export interface Mover {
  symbol: string;
  kind: "crypto" | "stock";
  price: number;
  /** Change since the last daily close, in %. */
  change: number;
}

export interface BriefBuy {
  symbol: string;
  kind: "crypto" | "stock";
  strong: boolean;
  title: string;
}

const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })}\u00a0%`;

/** Moves since the last daily close, largest first (either direction); an asset without both prices is left out. */
export function movers(prices: { symbol: string; kind: "crypto" | "stock"; price: number | null }[], previousClose: Record<string, number | null>): Mover[] {
  const out: Mover[] = [];
  for (const p of prices) {
    const prev = previousClose[`${p.kind}:${p.symbol}`];
    if (p.price == null || !(p.price > 0) || prev == null || !(prev > 0)) continue;
    out.push({ symbol: p.symbol, kind: p.kind, price: p.price, change: (p.price / prev - 1) * 100 });
  }
  return out.sort((a, b) => Math.abs(b.change) - Math.abs(a.change));
}

/** One sentence: "Contexte calme · 3 achetables (BTC, ETH, AAPL) · NVDA −3,4 %, SOL +2,8 %". */
export function headline(level: MarketLevel | null, buyable: BriefBuy[], moves: Mover[]): string {
  const parts: string[] = [];
  if (level) parts.push(MARKET_LABEL[level]);
  if (buyable.length === 0) parts.push("rien d'achetable pour l'instant");
  else {
    const names = buyable.slice(0, 3).map((b) => b.symbol).join(", ") + (buyable.length > 3 ? "…" : "");
    parts.push(`${buyable.length} achetable${buyable.length > 1 ? "s" : ""} (${names})`);
  }
  const big = moves.filter((m) => Math.abs(m.change) >= 1).slice(0, 2);
  if (big.length) parts.push(big.map((m) => `${m.symbol} ${pct(m.change)}`).join(", "));
  const s = parts.join(" · ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

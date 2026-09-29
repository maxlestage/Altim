/**
 * Sector exposure of the portfolio held: stocks grouped by sector (as returned by /api/sectors), cryptos and cash
 * as their own blocks, stocks without a known sector as "Secteur inconnu". Pure functions.
 *
 * JSON contract of GET /api/sectors?symbols=AAPL,NVDA (≤ 50 stocks, ":crypto" entries ignored):
 * { asOf: ms, items: SectorItem[], sources: { name, ok, error: string | null }[] }
 */
import type { Insight, InsightLevel } from "./holdings";
import type { Kind } from "./reliability";

/** "nasdaq": sector of the Nasdaq screener; "sec": SIC division filed at the SEC; "etf": fund spanning several sectors. */
export type SectorClassification = "nasdaq" | "sec" | "etf";

export interface SectorItem {
  symbol: string;
  /** French label ("Technologie", "Finance et immobilier", "ETF / fonds indiciel (plusieurs secteurs)"), null when unknown. */
  sector: string | null;
  classification: SectorClassification | null;
  /** Named source of the label, null when unknown. */
  source: string | null;
  /** Why no source covers it (null when classified). */
  reason: string | null;
  etf: boolean;
  sec: { label: string; sic: string; sicDescription: string; source: string } | null;
  nasdaq: { sector: string; sectorFr: string; industry: string } | null;
}

export interface SectorsReport {
  asOf: number;
  items: SectorItem[];
  sources: { name: string; ok: boolean; error: string | null }[];
}

export type BlockKind = "sector" | "etf" | "crypto" | "cash" | "unknown";

export interface ExposureBlock {
  key: string;
  label: string;
  kind: BlockKind;
  value: number;
  /** % of the whole portfolio (cash included). */
  weight: number;
  /** % of the stock part (stocks only, else null). */
  stockWeight: number | null;
  symbols: string[];
}

export interface SectorExposure {
  total: number;
  stockValue: number;
  /** Sorted by weight, heaviest first. */
  blocks: ExposureBlock[];
  /** 1 / Σ w² over the stocks with an operating sector (ETFs and unknown left out); null without any. */
  effectiveSectors: number | null;
  /** % of the stock part whose operating sector is known (ETFs and unknown excluded). */
  classifiedShare: number;
  unknown: { symbol: string; reason: string }[];
  /** Number of stocks per classification, for the source line. */
  bySource: Record<SectorClassification, number>;
  insights: Insight[];
}

/** A sector above this share of the whole portfolio is flagged. */
export const SECTOR_MAX_WEIGHT = 35;
/** A sector above this share of the stock part is flagged (from 2 stock lines). */
export const SECTOR_MAX_STOCK_SHARE = 50;

export const ETF_BLOCK_LABEL = "ETF / fonds (plusieurs secteurs)";
export const UNKNOWN_LABEL = "Secteur inconnu";

/**
 * Exposure by sector. `lines`: the portfolio lines with their current value (USD); `sectors`: items of /api/sectors
 * by symbol, null when the call failed (`failure` then says why: every stock is "Secteur inconnu").
 */
export function sectorExposure(
  lines: { symbol: string; kind: Kind; value: number }[],
  cash: number,
  sectors: Record<string, SectorItem | undefined> | null,
  failure = "classement sectoriel indisponible",
): SectorExposure {
  const safeCash = Math.max(0, cash);
  const total = lines.reduce((a, l) => a + Math.max(0, l.value), 0) + safeCash;
  const blocks = new Map<string, Omit<ExposureBlock, "weight" | "stockWeight">>();
  const unknown = new Map<string, string>();
  const bySource: Record<SectorClassification, number> = { nasdaq: 0, sec: 0, etf: 0 };
  const counted = new Set<string>();
  const add = (key: string, label: string, kind: BlockKind, value: number, symbol?: string) => {
    const b = blocks.get(key) ?? { key, label, kind, value: 0, symbols: [] };
    b.value += value;
    if (symbol && !b.symbols.includes(symbol)) b.symbols.push(symbol);
    blocks.set(key, b);
  };
  let stockValue = 0;
  for (const l of lines) {
    const value = Math.max(0, l.value);
    if (l.kind === "crypto") {
      add("crypto", "Crypto", "crypto", value, l.symbol);
      continue;
    }
    stockValue += value;
    const s = sectors ? sectors[l.symbol] : undefined;
    if (s?.sector && s.classification) {
      if (!counted.has(l.symbol)) bySource[s.classification]++;
      counted.add(l.symbol);
      if (s.classification === "etf") add("etf", ETF_BLOCK_LABEL, "etf", value, l.symbol);
      // Nasdaq and SEC are two classifications: never merged, the SEC's labelled "(SIC)".
      else add(`${s.classification}:${s.sector}`, s.classification === "sec" ? `${s.sector} (SIC)` : s.sector, "sector", value, l.symbol);
    } else {
      add("unknown", UNKNOWN_LABEL, "unknown", value, l.symbol);
      unknown.set(l.symbol, s?.reason ?? (sectors ? "absent de la réponse du serveur" : failure));
    }
  }
  if (safeCash > 0) add("cash", "Liquidités", "cash", safeCash);

  const pct = (v: number, of: number) => (of > 0 ? (v / of) * 100 : 0);
  const out: ExposureBlock[] = [...blocks.values()]
    .filter((b) => b.value > 0)
    .map((b) => ({ ...b, weight: pct(b.value, total), stockWeight: ["sector", "etf", "unknown"].includes(b.kind) ? pct(b.value, stockValue) : null }))
    .sort((a, b) => b.weight - a.weight || a.label.localeCompare(b.label, "fr"));

  const sectorBlocks = out.filter((b) => b.kind === "sector");
  const classified = sectorBlocks.reduce((a, b) => a + b.value, 0);
  const hhi = classified > 0 ? sectorBlocks.reduce((a, b) => a + (b.value / classified) ** 2, 0) : 0;
  const effectiveSectors = hhi > 0 ? 1 / hhi : null;
  const stockLines = new Set(lines.filter((l) => l.kind === "stock" && l.value > 0).map((l) => l.symbol)).size;

  const insights: Insight[] = [];
  const say = (level: InsightLevel, code: string, values: Insight["values"] = {}) => insights.push({ level, code, values });
  const top = sectorBlocks[0];
  if (top && top.weight > SECTOR_MAX_WEIGHT) say("warning", "sector_heavy", { sector: top.label, weight: top.weight });
  else if (top && stockLines >= 2 && top.stockWeight! > SECTOR_MAX_STOCK_SHARE) say("warning", "sector_heavy_stocks", { sector: top.label, share: top.stockWeight! });
  if (effectiveSectors !== null && sectorBlocks.reduce((a, b) => a + b.symbols.length, 0) >= 2 && effectiveSectors < 2)
    say("info", "sector_effective", { effective: effectiveSectors });
  const etf = out.find((b) => b.kind === "etf");
  if (etf) say("info", "sector_etf", { weight: etf.weight, symbols: etf.symbols.join(", ") });
  const unk = out.find((b) => b.kind === "unknown");
  if (unk) say("info", "sector_unknown", { weight: unk.weight, symbols: unk.symbols.join(", ") });

  return {
    total, stockValue, blocks: out, effectiveSectors, classifiedShare: pct(classified, stockValue),
    unknown: [...unknown].map(([symbol, reason]) => ({ symbol, reason })), bySource, insights,
  };
}

const pc = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;

export function sectorInsightText(i: Insight): string {
  const v = i.values;
  switch (i.code) {
    case "sector_heavy": return `${v.sector} pèse ${pc(v.weight as number)} de votre patrimoine : une mauvaise passe de ce secteur pourrait toucher plusieurs lignes à la fois.`;
    case "sector_heavy_stocks": return `${v.sector} représente ${pc(v.share as number)} de vos actions : leur diversification sectorielle est faible.`;
    case "sector_effective": return `Vos actions classées équivalent à ${(v.effective as number).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} secteur(s) de même poids.`;
    case "sector_etf": return `ETF (${v.symbols}, ${pc(v.weight as number)}) : leur répartition par secteur n'est pas couverte (composition non lue).`;
    case "sector_unknown": return `Secteur non couvert pour ${v.symbols} (${pc(v.weight as number)}).`;
    default: return i.code;
  }
}

export const CLASSIFICATION_SOURCE: Record<SectorClassification, string> = {
  nasdaq: "Nasdaq (secteur du screener)",
  sec: "SEC EDGAR (code SIC, grandes divisions)",
  etf: "Nasdaq Trader / SEC (ETF)",
};

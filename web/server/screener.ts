/**
 * Stock selection over the largest US companies (Nasdaq screener: market cap, sector).
 * Stage 1: every stock is scored on its daily candles (one source, fast) and ranked by relative strength.
 * Stage 2: the best candidates are checked with the full multi-source consensus, the market guard and their trend;
 * a candidate that fails a check is set aside or put on watch, with the reason.
 */
import type { Candle } from "../src/engine/signal";
import { backtest, trackRecord } from "../src/engine/backtest";
import { alignSeries, explain, factorsAt, pick, scoreUniverse, validate, CRITERIA, HOLD, ROLE, STOP_ATR, type Criterion, type RawFactors, type ScreenHorizon, type Validation } from "../src/engine/screener";
import { cached } from "./cache";
import { parseStock } from "./market";
import { parseExtra } from "./stocks-extra";

const UA = { "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15", Accept: "application/json" };
async function getJSON(url: string): Promise<any> {
  const r = await fetch(url, { headers: UA, signal: AbortSignal.timeout(12_000) });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export interface Listed { symbol: string; name: string; sector: string; marketCap: number }

/** Company names without the listing suffix ("Apple Inc. Common Stock" → "Apple Inc."). */
const cleanName = (s: string) => s.replace(/\s+(Common Stock|Ordinary Shares|American Depositary Shares?|Class [A-C] (Common|Ordinary|Capital) Stock|Capital Stock|Common Shares).*$/i, "").trim();

/** Largest US-listed companies, one share class per company (GOOGL kept, GOOG dropped). */
export function parseListed(d: any, size = 150): Listed[] {
  const rows = ((d?.data?.rows ?? []) as any[])
    .map((r) => ({ symbol: String(r.symbol).trim().replace(/[./]/g, "-"), name: cleanName(String(r.name ?? "")), sector: String(r.sector || "Autre"), marketCap: Number(String(r.marketCap ?? "").replace(/,/g, "")) || 0 }))
    .filter((r) => r.marketCap > 0 && /^[A-Z][A-Z0-9-]{0,6}$/.test(r.symbol))
    .sort((a, b) => b.marketCap - a.marketCap);
  const seen = new Set<string>();
  const out: Listed[] = [];
  for (const r of rows) {
    const company = r.name.replace(/\b(Class|Series) [A-C]\b/gi, "").toLowerCase().replace(/[^a-z0-9]/g, "");
    if (seen.has(company)) continue;
    seen.add(company);
    out.push(r);
    if (out.length >= size) break;
  }
  return out;
}

export const SECTOR_FR: Record<string, string> = {
  Technology: "Technologie", Finance: "Finance", "Health Care": "Santé", "Consumer Discretionary": "Consommation",
  Industrials: "Industrie", Energy: "Énergie", Telecommunications: "Télécoms", "Consumer Staples": "Biens de base",
  "Basic Materials": "Matériaux", "Real Estate": "Immobilier", Utilities: "Services publics", Miscellaneous: "Divers", Autre: "Autre",
};

export const listed = () => cached("screener:listed", 12 * 3_600_000, async () => parseListed(await getJSON("https://api.nasdaq.com/api/screener/stocks?tableonly=true&download=true")));

/** Daily candles for selection: Finviz (10 years in one call), Yahoo as fallback; ≈ 3 years kept. */
export function dailyFor(symbol: string): Promise<Candle[]> {
  return cached(`screener:daily:${symbol}`, 3 * 3_600_000, async () => {
    try {
      const c = parseExtra.finviz(await getJSON(`https://finviz.com/api/quote.ashx?instrument=stock&ticker=${encodeURIComponent(symbol)}&timeframe=d`));
      if (c.length > 300) return c.slice(-2600);
    } catch {}
    return parseStock.yahoo(await getJSON(`https://query1.finance.yahoo.com/v8/finance/chart/${encodeURIComponent(symbol)}?interval=1d&range=10y`)).slice(-2600);
  });
}

async function mapLimit<T, R>(items: T[], limit: number, fn: (t: T) => Promise<R>): Promise<R[]> {
  const out = new Array<R>(items.length);
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (next < items.length) {
      const i = next++;
      out[i] = await fn(items[i]!);
    }
  }));
  return out;
}

export interface Candidate {
  rank: number;
  symbol: string;
  name: string;
  sector: string;
  marketCap: number;
  price: number;
  scores: Record<Criterion, number>;
  why: Record<Criterion, string>;
  action: string;
  zoneStatus: string;
  /** Buy now, or a limit order at the top of the buy zone when the price is a little above it. */
  plan: { entry: number; limit: number | null; stop: number; target: number; atrPct: number } | null;
  track: { trades: number; winRate: number; avgReturn: number } | null;
  checks: { label: string; ok: boolean; detail: string }[];
}

export interface ScreenResult {
  horizon: ScreenHorizon;
  asOf: number;
  scanned: number;
  criteria: Record<Criterion, string>;
  roles: Record<Criterion, string>;
  holdDays: number;
  buy: Candidate[];
  watch: (Candidate & { reason: string })[];
  setAside: { symbol: string; name: string; reason: string }[];
  validation: Validation | null;
}

type Verify = (symbol: string) => Promise<{ reliability: "high" | "medium" | "low"; shock: string; reversalDown: boolean; daily: Candle[] }>;

/** Stage 1 on the whole universe (ranking), stage 2 on the best candidates (checks), then the final lists. */
export async function screen(h: ScreenHorizon, verify: Verify, size = 150, topN = 10): Promise<ScreenResult> {
  const universe = (await listed()).slice(0, size);
  const candles = await mapLimit(universe, 10, (s) => dailyFor(s.symbol).catch(() => [] as Candle[]));
  const factors: (RawFactors | null)[] = candles.map((c) => (c.length > 300 ? factorsAt(c, c.length - 1, h) : null));
  const scored = scoreUniverse(factors, h);
  const rows = universe.map((s, i) => ({ s, f: factors[i], sc: scored[i], c: candles[i]! })).filter((x) => x.f && x.sc);
  // A few more than needed: some will fail the checks.
  const finalists = pick(rows, (x) => x.sc!.total, (x) => x.s.sector, topN + 6);

  const checked = await mapLimit(finalists, 4, async ({ s, f, sc, c }) => {
    let v: Awaited<ReturnType<Verify>> | null = null;
    try {
      v = await verify(s.symbol);
    } catch {}
    const bt = trackRecord(backtest(v?.daily.length ? v.daily : c.slice(-500)));
    const checks: Candidate["checks"] = [
      { label: "Données recoupées", ok: !!v && v.reliability !== "low", detail: v ? `fiabilité ${v.reliability === "high" ? "élevée" : v.reliability === "medium" ? "moyenne" : "faible"} sur les sources indépendantes` : "sources indisponibles" },
      { label: "Garde-fou marché", ok: !!v && v.shock !== "shock" && !v.reversalDown, detail: !v ? "indisponible" : v.shock === "shock" ? "marché en choc sur ce titre" : v.reversalDown ? "risque de retournement à la baisse élevé" : v.shock === "agitated" ? "agité : taille divisée par deux" : "conditions normales" },
      { label: "Tendance de fond", ok: f!.trend >= 40, detail: f!.trend >= 70 ? "haussière" : f!.trend >= 40 ? "mitigée" : "baissière" },
    ];
    const atrAbs = f!.atrPct != null ? (f!.atrPct / 100) * f!.price : null;
    const entry = f!.price;
    const zoneTop = f!.zoneStatus === "above" && f!.zoneDistance != null && f!.zoneDistance <= 10 ? entry * (1 - f!.zoneDistance / 100) : null;
    // Stop and target from the price actually paid: the limit when there is one.
    const base = zoneTop ?? entry;
    const stop = atrAbs ? base - STOP_ATR[h] * atrAbs : null;
    return {
      s, v, cand: {
        rank: 0, symbol: s.symbol, name: s.name, sector: SECTOR_FR[s.sector] ?? s.sector, marketCap: s.marketCap, price: f!.price,
        scores: sc!.scores, why: explain(f!, sc!, h), action: f!.action, zoneStatus: f!.zoneStatus,
        plan: stop && f!.atrPct != null ? { entry, limit: zoneTop, stop, target: base + 2 * (base - stop), atrPct: f!.atrPct } : null,
        track: bt.trades ? bt : null, checks,
      } satisfies Candidate,
    };
  });

  const buy: Candidate[] = [], watch: ScreenResult["watch"] = [], setAside: ScreenResult["setAside"] = [];
  for (const { cand } of checked) {
    const data = cand.checks[0]!, guard = cand.checks[1]!, trend = cand.checks[2]!;
    if (!data.ok) setAside.push({ symbol: cand.symbol, name: cand.name, reason: `${data.label} : ${data.detail}` });
    else if (!guard.ok || !trend.ok) watch.push({ ...cand, reason: !guard.ok ? `${guard.label} : ${guard.detail}` : `${trend.label} ${trend.detail}` });
    else if (buy.length < topN) buy.push({ ...cand, rank: buy.length + 1 });
  }
  const validation = await cached(`screener:validation:${h}:${size}:${topN}`, 12 * 3_600_000, async () => {
    // Stocks with ≈ 6 years of history: the replay then covers 2020 onwards, not only the last months.
    const usable = universe.map((s, i) => ({ s, c: candles[i]! })).filter((x) => x.c.length > 1500);
    return validate(alignSeries(usable.map((x) => x.c)), h, topN, 21, usable.map((x) => x.s.sector));
  }).catch(() => null);
  return { horizon: h, asOf: Date.now(), scanned: rows.length, criteria: CRITERIA, roles: ROLE, holdDays: HOLD[h], buy, watch, setAside, validation };
}

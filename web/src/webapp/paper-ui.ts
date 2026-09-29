/**
 * Pure helpers of the "Simulation" screens (paper trading): reading the saved state, default amounts, labels,
 * notices. No React, no fetch, no storage access, so they are tested with `bun test`. The rules themselves live
 * in engine/paper.ts (shared with the iPhone and Android apps) and are not changed here.
 */
import { currencySymbol, money, toDisplay } from "../money";
import { DEFAULT_CAPITAL, isPaperState, type DailyCandle, type PaperPosition, type PaperReason, type PaperState, type PaperTrade, type VerdictStats } from "../engine/paper";

export const PAPER_KEY = "altim.paper.v1";

/** Result of reading localStorage: a valid state, nothing saved yet, or unreadable data (ignored, with a message). */
export type SavedPaper = { state: PaperState | null; error: string | null };

export const BAD_DATA_MESSAGE =
  "Les données de simulation enregistrées dans ce navigateur sont illisibles ou abîmées : elles ont été ignorées. Recommencez une simulation pour repartir de zéro.";

export function parseSavedPaper(raw: string | null): SavedPaper {
  if (raw == null || raw === "") return { state: null, error: null };
  try {
    const v: unknown = JSON.parse(raw);
    return isPaperState(v) ? { state: v, error: null } : { state: null, error: BAD_DATA_MESSAGE };
  } catch {
    return { state: null, error: BAD_DATA_MESSAGE };
  }
}

/** French decimal input ("1 234,5", "1234.5", "10 000 $") → number, NaN when unreadable. Empty → null. */
export function parseAmount(text: string): number | null {
  const t = text.replace(/[\s  $€]/g, "").replace(",", ".");
  if (t === "") return null;
  return /^\d*\.?\d+$|^\d+\.$/.test(t) ? Number(t) : NaN;
}

/** Default amount of a simulated purchase: 10 % of the simulated value, never more than the cash (cents, rounded down). */
export function defaultAmount(equity: number, cash: number): number {
  const v = Math.min(equity * 0.1, cash);
  return v > 0 && Number.isFinite(v) ? Math.floor(v * 100) / 100 : 0;
}

/** A starting capital typed by the user: positive and finite, else the default (10 000 $). */
export function startCapital(text: string): number {
  const v = parseAmount(text);
  return v != null && Number.isFinite(v) && v > 0 ? v : DEFAULT_CAPITAL;
}

/** A price put in an input: French decimal comma, no grouping, 2 decimals from 1 $ up, 6 significant digits below. */
export function inputPrice(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v) || v <= 0) return "";
  const digits = v >= 1 ? 2 : Math.min(12, 5 - Math.floor(Math.log10(v)));
  return v.toFixed(digits).replace(/\.?0+$/, "").replace(".", ",");
}

/** Verdicts considered as "buy" by the decision: simulating another one is simulating against it. */
export const BUYING_VERDICTS = ["buy", "buyZone"];
export const againstDecision = (verdict: string) => !BUYING_VERDICTS.includes(verdict);
export const againstText = (label: string) => `La décision actuelle est « ${label} » : vous simulez contre elle.`;

/**
 * Warnings on the stop and target typed in the form, mirroring what openPosition does with them
 * (a stop at or above the fill price, or a target at or below it, is dropped).
 */
export function levelWarnings(fill: number, stop: number | null, target: number | null): string[] {
  const w: string[] = [];
  if (stop != null && Number.isFinite(stop) && (stop <= 0 || stop >= fill)) w.push("Le stop doit être sous le prix d'achat : il sera ignoré.");
  if (target != null && Number.isFinite(target) && target <= fill) w.push("L'objectif doit être au-dessus du prix d'achat : il sera ignoré.");
  return w;
}

export const REASON_LABEL: Record<PaperReason, string> = { target: "objectif atteint", stop: "stop touché", manual: "vente manuelle" };

/** Candle day (UTC midnight for daily candles) or moment, in French. */
export function frDate(ms: number, withTime = false): string {
  const day = ms % 86_400_000 === 0;
  return new Date(ms).toLocaleString("fr-FR", {
    day: "numeric", month: "long", year: "numeric",
    ...(withTime && !day ? { hour: "2-digit", minute: "2-digit" } : {}),
    timeZone: day ? "UTC" : "Europe/Paris",
  });
}

/** Notice for a position just closed by checkExits: "Stop touché le 3 septembre 2026 : Bitcoin, −212,40 $". */
export function exitNotice(t: PaperTrade): string {
  const head = t.reason === "stop" ? "Stop touché" : t.reason === "target" ? "Objectif atteint" : "Vente manuelle";
  return `${head} le ${frDate(t.closedAt)} : ${t.name} (${t.symbol}), ${signedUsd(t.pnl)} (${signedPct(t.pnlPct)}).`;
}

/** The open positions whose exits must be checked (a stop or a target), one entry per asset. */
export function assetsToCheck(positions: PaperPosition[]): { symbol: string; kind: PaperPosition["kind"] }[] {
  const m = new Map<string, { symbol: string; kind: PaperPosition["kind"] }>();
  for (const p of positions) if (p.stop != null || p.target != null) m.set(`${p.kind}:${p.symbol}`, { symbol: p.symbol, kind: p.kind });
  return [...m.values()];
}

/** Server candles (any extra fields) → the engine's daily candles, bad rows dropped. */
export function toDaily(candles: { time: number; open: number; high: number; low: number; close: number }[]): DailyCandle[] {
  return candles
    .filter((c) => [c.time, c.open, c.high, c.low, c.close].every(Number.isFinite))
    .map(({ time, open, high, low, close }) => ({ time, open, high, low, close }));
}

/** Journal order: most recent exit first. */
export const journal = (trades: PaperTrade[]) => [...trades].sort((a, b) => b.closedAt - a.closedAt || b.openedAt - a.openedAt);

const VERDICT_ORDER = ["buy", "buyZone", "wait", "noPosition", "trim", "sell"];
/** "Résultats par décision" rows in the order of the decision scale, "Sans décision" last. */
export function verdictRows(rows: VerdictStats[]): VerdictStats[] {
  const rank = (v: string) => (v === "none" ? 99 : VERDICT_ORDER.includes(v) ? VERDICT_ORDER.indexOf(v) : 50);
  return [...rows].sort((a, b) => rank(a.verdict) - rank(b.verdict) || a.verdict.localeCompare(b.verdict));
}

/** Under this many closed trades the statistics mean little. */
export const FEW_TRADES = 20;
export const FEW_TRADES_NOTE =
  "Sous une vingtaine de trades clôturés, ces chiffres veulent dire peu : quelques trades chanceux ou malchanceux suffisent à les renverser. Ils deviennent parlants avec le temps.";

// ---------- French formatting ----------

const NNBSP = "\u202f";
const fr = (v: number, min: number, max: number) => v.toLocaleString("fr-FR", { minimumFractionDigits: min, maximumFractionDigits: max });
/** Dollar amounts of the simulation, shown in the display currency (money.ts). */
export const usd = (v: number) => money(v, 2, 2, NNBSP);
/** Prices: more decimals for small values (0,000012 €). */
export const price = (v: number) => {
  const x = toDisplay(v);
  return `${fr(x, x >= 1 ? 2 : 4, x >= 1 ? 2 : 8)}${NNBSP}${currencySymbol()}`;
};
export const signedUsd = (v: number) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${usd(Math.abs(v))}`;
export const signedPct = (v: number, digits = 2) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${fr(Math.abs(v), digits, digits)} %`;
export const pct = (v: number, digits = 0) => `${fr(v, digits, digits)} %`;
export const qty = (v: number) => fr(v, 0, v >= 1 ? 6 : 10);

/** Unique enough id for a position (crypto.randomUUID needs a secure context). */
export function newId(): string {
  try {
    if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") return crypto.randomUUID();
  } catch {}
  return `p-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

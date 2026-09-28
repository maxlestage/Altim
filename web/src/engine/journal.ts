/**
 * Automatic trading journal: every simulated purchase (paper) and every real purchase or sale recorded in
 * « Mes avoirs » is written down with WHY it was taken (the decision shown at that moment), the price, the stop,
 * the targets, the signal used and the market conditions. Everything stays on the device (like the holdings).
 *
 * Later, when the journal is opened, each entry is reviewed on the daily candles AFTER the entry (never the entry
 * day itself: its low may be earlier than the purchase, same rule as paper.ts), at 3, 10 and 30 calendar days:
 * max favourable / adverse excursion, whether the stop or a target was reached first (a candle reaching both counts
 * as the stop: the worst case, the order inside the day is not known), result against the plan in R (multiples of the
 * risk taken: entry − stop), facts on what worked or not, and whether the entry was coherent with the data available
 * at that moment. The profile groups the results by rating at entry, plan respected or not, and market regime, in R
 * and drawdown (never in raw returns only: a gain obtained by taking more risk does not count more).
 * Pure, deterministic functions (same inputs, same results).
 */
import type { Decision, RegimeKind } from "../webapp/decision";
import type { Kind } from "./reliability";
import { atr, type Candle } from "./signal";

export type JournalSource = "paper" | "real";
export type JournalSide = "buy" | "sell";

/** The decision shown when the entry was written (a snapshot: the full decision is not kept). */
export interface JournalDecision {
  asOf: number;
  verdict: string;
  label: string;
  rating: string | null;
  ratingLabel: string | null;
  levelLabel: string;
  headline: string;
  confidence: number;
  /** Composite score /100 (null: not computed or older server). */
  score: number | null;
  /** First three pros / cons. */
  pros: string[];
  cons: string[];
  degraded: boolean;
  degradedHeadline: string | null;
  setup: { name: string; met: number; total: number; steps: { label: string; state: string }[] };
  /** Labels of the buy vetoes active at that moment. */
  vetoes: string[];
  plan: { zoneFrom: number; zoneTo: number; entry: number; stop: number; target1: number; target2: number | null; riskReward: number; minRiskReward: number; acceptable: boolean } | null;
  horizon: string | null;
}

/** Market conditions at the entry; null = not known (not loaded, source failed). */
export interface JournalMarket {
  regime: RegimeKind | null;
  regimeLabel: string | null;
  /** Macro stress /100 and its level (calm / tense / high), from /api/macro. */
  macroScore: number | null;
  macroLevel: string | null;
  /** Daily ATR (14) in % of the price, on the daily candles closed before the entry. */
  atrPct: number | null;
  /** Volume of the last closed day ÷ average of the 20 before (or the decision's liquidity figure). */
  relativeVolume: number | null;
  /** Events of the next 7 days in the decision (economy, central banks, company). */
  events: number | null;
}

export interface JournalEntry {
  id: string;
  createdAt: number;
  source: JournalSource;
  side: JournalSide;
  symbol: string;
  kind: Kind;
  name: string;
  /** Price of the purchase or sale (paper: fill price with slippage; real: price entered or live price). */
  price: number;
  quantity: number | null;
  amount: number | null;
  /** The user's own stop and target(s) (paper order, holding's stop); the decision's plan is kept apart. */
  stop: number | null;
  targets: number[];
  /** "Signal utilisé" in words. */
  signal: string;
  /** "Pourquoi je suis entré" (optional). */
  note: string;
  /** Paper position or holding line this entry is about. */
  refId: string | null;
  decision: JournalDecision | null;
  market: JournalMarket;
}

export interface JournalState {
  version: 1;
  entries: JournalEntry[];
}

export const DAY = 86_400_000;
export const REVIEW_DAYS = [3, 10, 30] as const;
export type ReviewDays = (typeof REVIEW_DAYS)[number];
/** Under this many entries a group's figures are shown with "échantillon trop faible". */
export const MIN_SAMPLE = 5;
/** Beyond this age the decision is not "the one of the moment" anymore. */
export const STALE_DECISION_MS = 24 * 3_600_000;
export const MAX_ENTRIES = 500;
const BUYING = ["buy", "buyZone"];

const round = (v: number, d = 4) => Math.round(v * 10 ** d) / 10 ** d;
const fr = (v: number, d = 1) => v.toLocaleString("fr-FR", { minimumFractionDigits: 0, maximumFractionDigits: d });
const signed = (v: number, d = 1) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${fr(Math.abs(v), d)}`;
const plural = (n: number, w: string) => `${n} ${w}${n > 1 ? "s" : ""}`;

export const emptyJournal = (): JournalState => ({ version: 1, entries: [] });
export const EMPTY_MARKET: JournalMarket = { regime: null, regimeLabel: null, macroScore: null, macroLevel: null, atrPct: null, relativeVolume: null, events: null };

// ---------- Writing an entry ----------

/** Snapshot of the decision (only what the journal shows and checks). */
export function snapshotDecision(d: Decision): JournalDecision {
  const p = d.plan;
  return {
    asOf: d.asOf,
    verdict: d.verdict,
    label: d.label,
    rating: d.rating ?? null,
    ratingLabel: d.ratingLabel ?? null,
    levelLabel: d.levelLabel,
    headline: d.headline,
    confidence: d.confidence,
    score: d.score?.value ?? null,
    pros: d.pros.slice(0, 3),
    cons: d.cons.slice(0, 3),
    degraded: !!d.degraded?.active,
    degradedHeadline: d.degraded?.active ? d.degraded.headline : null,
    setup: { name: d.setup.name, met: d.setup.met, total: d.setup.total, steps: d.setup.steps.map((s) => ({ label: s.label, state: s.state })) },
    vetoes: d.vetoes.filter((v) => v.active).map((v) => v.label),
    plan: p
      ? { zoneFrom: p.zoneFrom, zoneTo: p.zoneTo, entry: p.entry, stop: p.stop, target1: p.target1, target2: p.target2, riskReward: p.riskReward, minRiskReward: p.minRiskReward, acceptable: p.acceptable }
      : null,
    horizon: d.horizon?.label ?? p?.horizon ?? null,
  };
}

/** "Signal utilisé": the decision, its level and the setup's progress. */
export function signalText(d: JournalDecision | null): string {
  if (!d) return "Aucune décision chargée pour cet actif à ce moment-là";
  const parts = [`${d.ratingLabel ?? d.label} (${d.levelLabel})`, `configuration « ${d.setup.name} » ${d.setup.met}/${d.setup.total}`];
  if (d.horizon) parts.push(`horizon ${d.horizon}`);
  return parts.join(" · ");
}

/** Market conditions the decision already carries (regime, relative volume, events). */
export function marketFromDecision(d: Decision | null): JournalMarket {
  if (!d) return { ...EMPTY_MARKET };
  return {
    ...EMPTY_MARKET,
    regime: d.marketRegime?.kind ?? null,
    regimeLabel: d.marketRegime?.label ?? null,
    relativeVolume: d.liquidity?.relativeVolume ?? null,
    events: Array.isArray(d.events) ? d.events.length : null,
  };
}

/**
 * ATR % and relative volume on the daily candles CLOSED before `at` (no look-ahead: the entry day's candle is
 * excluded). null when the history is too short.
 */
export function marketFromCandles(candles: Candle[], at: number): { atrPct: number | null; relativeVolume: number | null } {
  const before = [...candles].filter((c) => c.time + DAY <= at && c.close > 0).sort((a, b) => a.time - b.time);
  const last = before[before.length - 1];
  const a = before.length >= 15 ? atr(before)[before.length - 1] ?? null : null;
  const prev = before.slice(-21, -1).map((c) => c.volume).filter((v) => Number.isFinite(v) && v > 0);
  const avg = prev.length >= 20 ? prev.reduce((s, v) => s + v, 0) / prev.length : 0;
  return {
    atrPct: a != null && last ? round((a / last.close) * 100, 3) : null,
    relativeVolume: last && avg > 0 && last.volume > 0 ? round(last.volume / avg, 3) : null,
  };
}

export interface NewEntry {
  id: string;
  now: number;
  source: JournalSource;
  side: JournalSide;
  symbol: string;
  kind: Kind;
  name: string;
  price: number;
  quantity?: number | null;
  amount?: number | null;
  stop?: number | null;
  targets?: (number | null | undefined)[];
  note?: string;
  refId?: string | null;
  decision?: Decision | null;
}

/** A new entry from what the screen knows at that moment (the decision already loaded, if any). */
export function createEntry(n: NewEntry): JournalEntry {
  const decision = n.decision ? snapshotDecision(n.decision) : null;
  const ok = (v: number | null | undefined): v is number => v != null && Number.isFinite(v) && v > 0;
  return {
    id: n.id,
    createdAt: n.now,
    source: n.source,
    side: n.side,
    symbol: n.symbol,
    kind: n.kind,
    name: n.name,
    price: n.price,
    quantity: ok(n.quantity) ? n.quantity : null,
    amount: ok(n.amount) ? n.amount : null,
    stop: n.side === "buy" && ok(n.stop) && n.stop < n.price ? n.stop : null,
    targets: n.side === "buy" ? (n.targets ?? []).filter(ok).filter((t) => t > n.price).sort((a, b) => a - b) : [],
    signal: signalText(decision),
    note: (n.note ?? "").trim().slice(0, 1000),
    refId: n.refId ?? null,
    decision,
    market: marketFromDecision(n.decision ?? null),
  };
}

/**
 * A holding line edited in « Mes avoirs » → the purchase or sale it records (null: no quantity change, or no usable
 * price). Purchase price: the one implied by the new average cost ((q2 × PRU2 − q1 × PRU1) ÷ (q2 − q1)), else the
 * live price; a sale is priced at the live price (the average cost does not change on a sale).
 */
export function holdingChange(
  before: { quantity: number; averagePrice: number },
  after: { quantity: number; averagePrice: number },
  livePrice: number | null,
): { side: JournalSide; quantity: number; price: number; implied: boolean } | null {
  const dq = after.quantity - before.quantity;
  if (!(Math.abs(dq) > 1e-12)) return null;
  const live = livePrice != null && Number.isFinite(livePrice) && livePrice > 0 ? livePrice : null;
  if (dq < 0) return live ? { side: "sell", quantity: -dq, price: live, implied: false } : null;
  const implied = (after.quantity * after.averagePrice - before.quantity * before.averagePrice) / dq;
  if (after.averagePrice !== before.averagePrice && Number.isFinite(implied) && implied > 0) return { side: "buy", quantity: dq, price: implied, implied: true };
  return live ? { side: "buy", quantity: dq, price: live, implied: false } : null;
}

export function addEntry(s: JournalState, e: JournalEntry): JournalState {
  return { version: 1, entries: [...s.entries.filter((x) => x.id !== e.id), e].sort((a, b) => a.createdAt - b.createdAt).slice(-MAX_ENTRIES) };
}

/** Completes an entry (market data arrived after it was written, the user's note). */
export function patchEntry(s: JournalState, id: string, patch: { note?: string; market?: Partial<JournalMarket>; decision?: JournalDecision }): JournalState {
  return {
    version: 1,
    entries: s.entries.map((e) => {
      if (e.id !== id) return e;
      const decision = patch.decision ?? e.decision;
      return {
        ...e,
        note: patch.note != null ? patch.note.trim().slice(0, 1000) : e.note,
        market: { ...e.market, ...patch.market },
        decision,
        signal: patch.decision ? signalText(decision) : e.signal,
      };
    }),
  };
}

export const removeEntry = (s: JournalState, id: string): JournalState => ({ version: 1, entries: s.entries.filter((e) => e.id !== id) });

export function isJournalState(v: unknown): v is JournalState {
  const s = v as JournalState;
  return !!s && s.version === 1 && Array.isArray(s.entries) && s.entries.every(
    (e) => !!e && typeof e.id === "string" && typeof e.symbol === "string" && (e.kind === "crypto" || e.kind === "stock")
      && (e.side === "buy" || e.side === "sell") && Number.isFinite(e.createdAt) && Number.isFinite(e.price) && e.price > 0 && !!e.market,
  );
}

// ---------- Review ----------

export type LevelSource = "user" | "plan" | "none";
export interface Levels { stop: number | null; stopSource: LevelSource; targets: number[]; targetsSource: LevelSource }

/** Stop and targets the review uses: the user's own, else the plan of the decision shown at the entry. */
export function levelsOf(e: JournalEntry): Levels {
  const p = e.decision?.plan;
  const stop = e.stop ?? (p && p.stop < e.price ? p.stop : null);
  const planTargets = p ? [p.target1, p.target2].filter((t): t is number => t != null && t > e.price) : [];
  const targets = e.targets.length ? e.targets : planTargets;
  return {
    stop,
    stopSource: e.stop != null ? "user" : stop != null ? "plan" : "none",
    targets,
    targetsSource: e.targets.length ? "user" : targets.length ? "plan" : "none",
  };
}

export type FirstHit = "stop" | "target1" | "none";

export interface HorizonReview {
  days: ReviewDays;
  /** pending: not reached yet; noData: no candle after the entry in the history; ready: computed. */
  status: "pending" | "noData" | "ready";
  availableAt: number;
  candles: number;
  lastClose: number | null;
  /** Price change since the entry (a sale: since the sale), %. */
  returnPct: number | null;
  /** Max favourable / adverse excursion since the entry, % (highs and lows of the window). */
  mfePct: number | null;
  maePct: number | null;
  mfeR: number | null;
  maeR: number | null;
  /** First level reached and after how many days (calendar days since the entry). */
  first: FirstHit;
  firstDays: number | null;
  /** Target 2 reached before the stop within the window. */
  target2: boolean;
  /** Result following the plan (exit at the first level reached, else the last close), in R; null without stop. */
  resultR: number | null;
}

export interface Check { code: string; label: string; ok: boolean | null; detail: string }

export interface ClosedInfo { at: number; price: number; reason: string }

export interface EntryReview {
  entry: JournalEntry;
  levels: Levels;
  /** Risk per unit (entry − stop), null without stop. */
  risk: number | null;
  /** Planned reward at target 1, in R. */
  planR: number | null;
  horizons: HorizonReview[];
  /** Most advanced ready horizon (the facts are built on it). */
  latest: HorizonReview | null;
  worked: string[];
  failed: string[];
  coherence: Check[];
  /** true: every verifiable check passes; false: at least one fails; null: nothing verifiable. */
  coherent: boolean | null;
  /** Plan respected at the entry (same checks). */
  planRespected: boolean | null;
  closed: ClosedInfo | null;
  realizedR: number | null;
}

/** Checks of the entry against the data available at that moment (a sale is checked against the decision only). */
export function coherenceChecks(e: JournalEntry, levels: Levels = levelsOf(e)): Check[] {
  const d = e.decision;
  if (!d) {
    return [{ code: "decision", label: "Décision disponible", ok: null, detail: "Aucune décision chargée pour cet actif à ce moment-là : cohérence non vérifiable." }];
  }
  const out: Check[] = [];
  const age = e.createdAt - d.asOf;
  out.push(age > STALE_DECISION_MS
    ? { code: "fresh", label: "Décision récente", ok: false, detail: `La décision affichée datait de ${plural(Math.round(age / 3_600_000), "heure")} : les données avaient pu changer.` }
    : { code: "fresh", label: "Décision récente", ok: true, detail: "Décision calculée moins de 24 h avant." });
  if (e.side === "sell") {
    const selling = ["trim", "sell"].includes(d.verdict);
    out.push({ code: "verdict", label: "Vente conforme à la décision", ok: selling, detail: selling ? `Décision « ${d.label} » : la vente allait dans son sens.` : `Décision « ${d.label} » : la vente allait contre elle.` });
    return out;
  }
  const buying = BUYING.includes(d.verdict);
  out.push({ code: "verdict", label: "Achat conforme à la décision", ok: buying, detail: buying ? `Décision « ${d.label} ».` : `Achat alors que la décision était « ${d.label} ».` });
  out.push(d.vetoes.length
    ? { code: "vetoes", label: "Aucune interdiction d'achat active", ok: false, detail: `Entrée malgré ${d.vetoes.length > 1 ? "des interdictions d'achat actives" : "une interdiction d'achat active"} : ${d.vetoes.join(", ")}.` }
    : { code: "vetoes", label: "Aucune interdiction d'achat active", ok: true, detail: "Aucune interdiction d'achat active." });
  out.push(d.degraded
    ? { code: "degraded", label: "Signal non dégradé", ok: false, detail: d.degradedHeadline ?? "Le signal était dégradé à l'entrée." }
    : { code: "degraded", label: "Signal non dégradé", ok: true, detail: "Signal complet à l'entrée." });
  const p = d.plan;
  if (!p) {
    out.push({ code: "rr", label: "Rapport gain / risque ≥ 2", ok: null, detail: "Pas de plan chiffré dans la décision." });
    out.push({ code: "zone", label: "Entrée dans la zone d'achat", ok: null, detail: "Pas de zone d'achat dans la décision." });
  } else {
    const min = p.minRiskReward > 0 ? p.minRiskReward : 2;
    out.push(p.riskReward >= min
      ? { code: "rr", label: `Rapport gain / risque ≥ ${fr(min)}`, ok: true, detail: `Rapport du plan : ${fr(p.riskReward)}.` }
      : { code: "rr", label: `Rapport gain / risque ≥ ${fr(min)}`, ok: false, detail: `Rapport du plan : ${fr(p.riskReward)}, sous le minimum de ${fr(min)}.` });
    const lo = Math.min(p.zoneFrom, p.zoneTo);
    const hi = Math.max(p.zoneFrom, p.zoneTo);
    if (e.price > hi) out.push({ code: "zone", label: "Entrée dans la zone d'achat", ok: false, detail: `Entrée hors zone d'achat : ${signed((e.price / hi - 1) * 100)} % au-dessus du haut de la zone.` });
    else if (e.price < lo) out.push({ code: "zone", label: "Entrée dans la zone d'achat", ok: false, detail: `Entrée hors zone d'achat : ${fr((1 - e.price / lo) * 100)} % sous la zone (support peut-être cassé).` });
    else out.push({ code: "zone", label: "Entrée dans la zone d'achat", ok: true, detail: "Entrée dans la zone d'achat." });
  }
  out.push(levels.stop == null
    ? { code: "stop", label: "Stop défini", ok: false, detail: "Aucun stop : risque non borné, résultat en R non mesurable." }
    : { code: "stop", label: "Stop défini", ok: true, detail: levels.stopSource === "user" ? "Stop fixé à l'entrée." : "Pas de stop saisi : celui du plan de la décision est utilisé." });
  const r = e.market.regime;
  out.push(r == null
    ? { code: "regime", label: "Pas contre le régime de marché", ok: null, detail: "Régime de marché inconnu à l'entrée." }
    : r === "riskOff"
      ? { code: "regime", label: "Pas contre le régime de marché", ok: false, detail: `Achat en régime ${e.market.regimeLabel ?? "risk-off"} : à contre-courant du marché.` }
      : { code: "regime", label: "Pas contre le régime de marché", ok: true, detail: `Régime ${e.market.regimeLabel ?? r}.` });
  return out;
}

function horizonReview(e: JournalEntry, days: ReviewDays, after: Candle[], now: number, levels: Levels, risk: number | null): HorizonReview {
  const availableAt = e.createdAt + days * DAY;
  const base: HorizonReview = {
    days, status: "pending", availableAt, candles: 0, lastClose: null, returnPct: null, mfePct: null, maePct: null, mfeR: null, maeR: null,
    first: "none", firstDays: null, target2: false, resultR: null,
  };
  if (now < availableAt) return base;
  const w = after.filter((c) => c.time <= availableAt);
  if (!w.length) return { ...base, status: "noData" };
  const last = w[w.length - 1]!;
  // Excursions are measured from the entry: never below it for the favourable one, never above it for the adverse one.
  const hi = Math.max(e.price, ...w.map((c) => c.high));
  const lo = Math.min(e.price, ...w.map((c) => c.low));
  const R = (p: number) => (risk ? round((p - e.price) / risk, 3) : null);
  let first: FirstHit = "none";
  let firstDays: number | null = null;
  let exit: number | null = null;
  let target2 = false;
  if (e.side === "buy") {
    const [t1, t2] = levels.targets;
    for (const c of w) {
      const stopHit = levels.stop != null && c.low <= levels.stop;
      if (stopHit) {
        if (first === "none") {
          first = "stop";
          firstDays = Math.max(1, Math.ceil((c.time - e.createdAt) / DAY));
          exit = Math.min(levels.stop!, c.open);
        }
        break;
      }
      if (t1 != null && c.high >= t1 && first === "none") {
        first = "target1";
        firstDays = Math.max(1, Math.ceil((c.time - e.createdAt) / DAY));
        exit = t1;
      }
      if (t2 != null && c.high >= t2) target2 = true;
    }
  }
  const mark = exit ?? last.close;
  return {
    ...base,
    status: "ready",
    candles: w.length,
    lastClose: last.close,
    returnPct: round((last.close / e.price - 1) * 100, 3),
    mfePct: round((hi / e.price - 1) * 100, 3),
    maePct: round((lo / e.price - 1) * 100, 3),
    mfeR: R(hi),
    maeR: R(lo),
    first,
    firstDays,
    target2,
    resultR: e.side === "buy" ? R(mark) : null,
  };
}

/**
 * Review of one entry on its asset's daily candles (any order). `closed`: the position was actually closed (paper
 * trade, recorded sale) — its realized result in R is added.
 */
export function reviewEntry(e: JournalEntry, candles: Candle[], now: number, closed: ClosedInfo | null = null): EntryReview {
  const levels = levelsOf(e);
  const risk = levels.stop != null && e.price > levels.stop ? e.price - levels.stop : null;
  const planR = risk && levels.targets[0] != null ? round((levels.targets[0] - e.price) / risk, 3) : null;
  // Candles starting after the entry (the entry day's own candle may predate it).
  // A history starting after the entry would leave a hole at its start: nothing is computed then.
  const covers = candles.some((c) => c.time <= e.createdAt);
  const after = covers ? candles.filter((c) => c.time > e.createdAt && c.low > 0 && c.high >= c.low).sort((a, b) => a.time - b.time) : [];
  const horizons = REVIEW_DAYS.map((d) => horizonReview(e, d, after, now, levels, risk));
  const latest = [...horizons].reverse().find((h) => h.status === "ready") ?? null;
  const coherence = coherenceChecks(e, levels);
  const verifiable = coherence.filter((c) => c.ok != null);
  const coherent = verifiable.length ? verifiable.every((c) => c.ok) : null;
  const realizedR = closed && risk ? round((closed.price - e.price) / risk, 3) : null;
  const { worked, failed } = facts(e, latest, coherence, levels, planR, closed, realizedR);
  return { entry: e, levels, risk, planR, horizons, latest, worked, failed, coherence, coherent, planRespected: e.side === "buy" ? coherent : null, closed, realizedR };
}

const rText = (r: number) => `${signed(r)} R`;

/** "Qu'est-ce qui a fonctionné ? / Qu'est-ce qui n'a pas fonctionné ?" — facts only, from the review. */
function facts(e: JournalEntry, h: HorizonReview | null, checks: Check[], levels: Levels, planR: number | null, closed: ClosedInfo | null, realizedR: number | null) {
  const worked: string[] = [];
  const failed: string[] = [];
  const failing = (code: string) => checks.find((c) => c.code === code && c.ok === false);
  if (e.side === "sell") {
    if (h?.returnPct != null) {
      const txt = `${h.days} jours après la vente, le cours est à ${signed(h.returnPct)} % du prix de vente (plus haut ${signed(h.mfePct!)} %, plus bas ${signed(h.maePct!)} %).`;
      (h.returnPct <= 0 ? worked : failed).push(txt);
    }
    const v = failing("verdict");
    if (v) failed.push(v.detail);
    return { worked, failed };
  }
  if (h) {
    if (h.first === "target1") {
      worked.push(`L'objectif 1 a été atteint en ${plural(h.firstDays!, "jour")}${h.resultR != null ? ` (${rText(h.resultR)})` : ""}${h.target2 ? ", puis l'objectif 2" : ""}.`);
    } else if (h.first === "stop") {
      const why = [failing("degraded") && "le signal était dégradé à l'entrée", failing("vetoes") && "des interdictions d'achat étaient actives", failing("zone") && "l'entrée était hors zone d'achat"]
        .filter(Boolean);
      failed.push(`Le stop a été touché en ${plural(h.firstDays!, "jour")}${h.resultR != null ? ` (${rText(h.resultR)})` : ""}${why.length ? ` alors que ${why.join(" et que ")}` : ""}.`);
    } else if (h.resultR != null) {
      const txt = `Ni stop ni objectif en ${h.days} jours : ${rText(h.resultR)} au dernier cours.`;
      (h.resultR > 0 ? worked : failed).push(txt);
    } else if (h.returnPct != null) {
      const txt = `${signed(h.returnPct)} % en ${h.days} jours (sans stop, résultat en R non mesurable).`;
      (h.returnPct > 0 ? worked : failed).push(txt);
    }
    if (h.first !== "target1" && h.mfeR != null && h.mfeR >= 1 && (h.resultR ?? 0) < 0) {
      failed.push(`Le cours est monté jusqu'à ${rText(h.mfeR)} avant de repasser sous l'entrée : le gain latent n'a pas été conservé.`);
    }
    if (h.first !== "stop" && h.maeR != null && h.maeR <= -0.8) failed.push(`Recul jusqu'à ${rText(h.maeR)} : le stop a failli être touché.`);
    if (h.first !== "stop" && h.maeR != null && h.maeR > -0.3 && h.resultR != null && h.resultR > 0) worked.push(`Recul limité à ${rText(h.maeR)} depuis l'entrée.`);
  }
  if (closed && realizedR != null) (realizedR > 0 ? worked : failed).push(`Position clôturée (${closed.reason}) : ${rText(realizedR)} réalisé${planR != null ? ` pour ${rText(planR)} prévu à l'objectif 1` : ""}.`);
  for (const code of ["zone", "rr", "regime", "verdict", "stop"]) {
    const c = failing(code);
    if (c) failed.push(c.detail);
  }
  const zone = checks.find((c) => c.code === "zone" && c.ok);
  if (zone && checks.every((c) => c.ok !== false)) worked.push("Entrée dans la zone d'achat, plan respecté (aucune interdiction, signal complet, rapport gain / risque suffisant).");
  if (levels.stopSource === "plan") failed.push("Aucun stop saisi : la revue utilise le stop du plan de la décision.");
  return { worked, failed };
}

// ---------- Profile ----------

export interface GroupStat {
  key: string;
  label: string;
  /** Entries reviewed at this horizon. */
  n: number;
  /** Entries with a stop (results in R). */
  withStop: number;
  lowSample: boolean;
  avgR: number | null;
  medianR: number | null;
  /** Share of results above 0 R (or above 0 % without stop), %. */
  winRate: number | null;
  /** Average and worst adverse excursion (drawdown during the trade), in R. */
  avgMaeR: number | null;
  worstMaeR: number | null;
  avgReturnPct: number | null;
  avgMaePct: number | null;
}

export interface JournalProfile {
  horizon: ReviewDays;
  /** Purchases reviewed at this horizon / all purchases. */
  reviewed: number;
  purchases: number;
  pending: number;
  all: GroupStat | null;
  byRating: GroupStat[];
  byPlan: GroupStat[];
  byRegime: GroupStat[];
}

const mean = (v: number[]) => (v.length ? round(v.reduce((a, b) => a + b, 0) / v.length, 3) : null);
function median(v: number[]): number | null {
  if (!v.length) return null;
  const s = [...v].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return round(s.length % 2 ? s[m]! : (s[m - 1]! + s[m]!) / 2, 3);
}

function stat(key: string, label: string, rows: HorizonReview[]): GroupStat {
  const inR = rows.filter((h) => h.resultR != null);
  const rs = inR.map((h) => h.resultR!);
  const wins = rows.filter((h) => (h.resultR ?? h.returnPct ?? 0) > 0).length;
  const maes = inR.map((h) => h.maeR!).filter((v) => v != null);
  return {
    key, label, n: rows.length, withStop: inR.length, lowSample: rows.length < MIN_SAMPLE,
    avgR: mean(rs), medianR: median(rs),
    winRate: rows.length ? round((wins / rows.length) * 100, 2) : null,
    avgMaeR: mean(maes), worstMaeR: maes.length ? Math.min(...maes) : null,
    avgReturnPct: mean(rows.map((h) => h.returnPct!).filter((v) => v != null)),
    avgMaePct: mean(rows.map((h) => h.maePct!).filter((v) => v != null)),
  };
}

const REGIME_LABEL: Record<string, string> = { riskOn: "Risk-on", neutral: "Neutre", riskOff: "Risk-off", unknown: "Régime inconnu" };
const RATING_ORDER = ["strongBuy", "buy", "hold", "reduce", "sell", "strongSell"];

/**
 * Results of the purchases at one horizon, grouped by rating at entry, plan respected or not, and market regime.
 * Groups are in R and drawdown; ordered by a fixed scale, never by performance.
 */
export function journalProfile(reviews: EntryReview[], horizon: ReviewDays): JournalProfile {
  const buys = reviews.filter((r) => r.entry.side === "buy");
  const rows = buys.flatMap((r) => {
    const h = r.horizons.find((x) => x.days === horizon);
    return h && h.status === "ready" ? [{ r, h }] : [];
  });
  const group = (keyOf: (r: EntryReview) => { key: string; label: string }, order: (k: string) => number) => {
    const m = new Map<string, { label: string; hs: HorizonReview[] }>();
    for (const { r, h } of rows) {
      const k = keyOf(r);
      const g = m.get(k.key) ?? { label: k.label, hs: [] };
      g.hs.push(h);
      m.set(k.key, g);
    }
    return [...m.entries()].map(([k, g]) => stat(k, g.label, g.hs)).sort((a, b) => order(a.key) - order(b.key) || a.key.localeCompare(b.key));
  };
  return {
    horizon,
    reviewed: rows.length,
    purchases: buys.length,
    pending: buys.filter((r) => r.horizons.find((x) => x.days === horizon)?.status === "pending").length,
    all: rows.length ? stat("all", "Tous les achats", rows.map((x) => x.h)) : null,
    byRating: group(
      (r) => {
        const d = r.entry.decision;
        return d ? { key: d.rating ?? d.verdict, label: d.ratingLabel ?? d.label } : { key: "none", label: "Sans décision" };
      },
      (k) => (RATING_ORDER.includes(k) ? RATING_ORDER.indexOf(k) : k === "none" ? 99 : 50),
    ),
    byPlan: group(
      (r) => (r.planRespected == null ? { key: "unknown", label: "Non vérifiable" } : r.planRespected ? { key: "yes", label: "Plan respecté" } : { key: "no", label: "Plan non respecté" }),
      (k) => ["yes", "no", "unknown"].indexOf(k),
    ),
    byRegime: group(
      (r) => {
        const k = r.entry.market.regime ?? "unknown";
        return { key: k, label: REGIME_LABEL[k] ?? k };
      },
      (k) => ["riskOn", "neutral", "riskOff", "unknown"].indexOf(k),
    ),
  };
}

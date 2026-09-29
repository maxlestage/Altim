/**
 * « Bot Altim » (/api/bot, /api/bot/views, `bot` of /api/decision): JSON contract and the pure helpers of the screen
 * and of the decision line, kept apart from the React views so they can be tested and ported to the mobile apps.
 * Returns and probabilities are in %, "points" are differences of % (signal − random day), times in ms. The server
 * computes everything; nothing here recomputes a statistic.
 */
import type { Kind } from "../engine/reliability";
import type { AssetClass, Verdict } from "./model-validation";
import { NNBSP, plain, signedPct } from "./model-validation";

export type BotGroup = "stock" | "crypto";
export type BotAction = "buy" | "wait" | "sell";

/** Probability range [from, to) in %, rows = labelled test days, predicted = mean probability, realised = share. */
export type Bucket = { from: number; to: number; label: string; rows: number; predicted: number | null; realised: number | null };

/** ACHETER signals out of sample (non-overlapping per asset). excess = meanNet − baselineNet (same asset and period). */
export type BuyStats = {
  signals: number; meanNet: number | null; baselineNet: number | null; allDaysNet: number | null; excess: number | null;
  tStat: number | null; rawTStat: number | null; hitRate: number | null; baselineHitRate: number | null;
  medianBotReturn: number | null; medianHoldReturn: number | null; beatHold: number; assets: number;
  verdict: Verdict | null; verdictLabel: string;
};
/** VENDRE signals: what holding made over the next 20 days after them vs a random day; avoided = baseline − after. */
export type SellStats = {
  signals: number; meanAfter: number | null; baselineAfter: number | null; allDaysAfter: number | null; avoided: number | null;
  tStat: number | null; fallRate: number | null; baselineFallRate: number | null; meanDrawdown: number | null; baselineDrawdown: number | null;
  verdict: Verdict | null; verdictLabel: string;
};
export type WaitStats = { days: number; share: number | null; meanNet: number | null; baselineNet: number | null };

export type BotStats = {
  testRows: number; labelled: number; buy: BuyStats; sell: SellStats; wait: WaitStats;
  brierSkillUp: number | null; brierSkillDown: number | null; calibrationUp: Bucket[]; calibrationDown: Bucket[];
};

export type ModelOut = { baseRate: number; threshold: number; intercept: number; weights: { id: string; coef: number; mean: number; sd: number }[] };
export type LiveModel = { trainedRows: number; trainedFrom: number | null; trainedTo: number | null; up: ModelOut; down: ModelOut };

export type BotGroupStat = BotStats & {
  id: BotGroup; label: string; assets: number; testFrom: number | null; testTo: number | null; blocks: number; trainedBlocks: number;
  model: LiveModel | null; text: string;
};

export type NowView = { time: number | null; action: BotAction | null; up: number | null; down: number | null };

export type BotAssetRow = {
  symbol: string; name: string; kind: Kind; class: AssetClass; group: BotGroup;
  testRows: number; testFrom: number | null; testTo: number | null;
  buys: number; buyMean: number | null; buyExcess: number | null; sells: number; sellAvoided: number | null; waitShare: number | null;
  botReturn: number | null; holdReturn: number | null; now: NowView; source: string;
};

export type BotReport = {
  asOf: number; basketFixedOn: string; headline: string; overall: BotStats; groups: BotGroupStat[];
  /** Basket order, never ranked by performance. */
  assets: BotAssetRow[];
  failures: { symbol: string; name: string; kind: Kind; class: AssetClass; error: string }[];
  features: { id: string; label: string; help: string }[];
  parameters: {
    horizonDays: number; retrainEvery: number; purgeDays: number; minTrainDays: number; warmup: number; l2: number;
    thresholdMargin: number; costStockPct: number; costCryptoPct: number; minSignals: number; tEdge: number; nudge: number;
  };
  method: string[]; limits: string[]; source: string;
};

export type Contribution = { id: string; label: string; value: number; valueText: string; weight: number; effect: "up" | "down"; text: string };

/** The bot's view of one asset now (`bot` of /api/decision, items of /api/bot/views with symbol and kind added). */
export type BotView = {
  available: boolean; group: BotGroup; groupLabel: string; inBasket: boolean;
  action: BotAction | null; actionLabel: string | null;
  up: number | null; down: number | null; thresholdUp: number | null; thresholdDown: number | null; baseUp: number | null; baseDown: number | null;
  buyVerdict: Verdict | null; sellVerdict: Verdict | null;
  /** Today's action is on a side with an out-of-sample edge: it counts (a little) in the decision. */
  counts: boolean; time: number | null; contributions: Contribution[]; text: string; note: string; asOf: number | null; link: string;
};
export type BotViews = { asOf: number | null; views: (BotView & { symbol: string; kind: Kind })[] };

export const botUrl = () => "/api/bot";
export const botViewsUrl = (items: { symbol: string; kind: Kind }[]) =>
  `/api/bot/views?symbols=${encodeURIComponent(items.slice(0, 20).map((i) => `${i.symbol}:${i.kind}`).join(","))}`;

export const ACTION_UI: Record<BotAction, { label: string; tone: "buy" | "wait" | "sell" }> = {
  buy: { label: "ACHETER", tone: "buy" },
  wait: { label: "ATTENDRE", tone: "wait" },
  sell: { label: "VENDRE", tone: "sell" },
};

const fr = (v: number, max: number) => v.toLocaleString("fr-FR", { minimumFractionDigits: 0, maximumFractionDigits: max });

/** "+0,52 point", "−2,4 points", "—". */
export function points(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return `${v < 0 ? "−" : v > 0 ? "+" : ""}${fr(Math.abs(v), 2)} point${Math.abs(v) >= 2 ? "s" : ""}`;
}

export function pct0(v: number | null | undefined): string {
  return v == null || !Number.isFinite(v) ? "—" : `${fr(v, 0)}${NNBSP}%`;
}

const tText = (t: number | null) => (t == null ? "t non calculable" : `t = ${plain(t, 1)}`);

/** "108 achats : +1,7 % en moyenne contre +1,85 % pour une entrée au hasard (−0,15 point, t = −0,1)". */
export function buyText(b: BuyStats): string {
  if (b.signals === 0) return "Aucun achat pendant les périodes de test.";
  return `${b.signals} achat${b.signals > 1 ? "s" : ""} : ${signedPct(b.meanNet, 2)} en moyenne sur 20 jours, contre ${signedPct(b.baselineNet, 2)} pour une entrée au hasard sur le même actif et la même période (${points(b.excess)}, ${tText(b.tStat)}).`;
}

/** What followed the VENDRE signals, said without a sign to decode. */
export function sellText(s: SellStats): string {
  if (s.signals === 0) return "Aucune vente pendant les périodes de test.";
  const diff = s.avoided == null ? "" : s.avoided >= 0 ? `cours ensuite inférieur de ${points(s.avoided).replace("+", "")}` : `cours ensuite supérieur de ${points(-s.avoided).replace("+", "")}`;
  return `${s.signals} vente${s.signals > 1 ? "s" : ""} : le cours a fait ${signedPct(s.meanAfter, 2)} dans les 20 jours suivants, contre ${signedPct(s.baselineAfter, 2)} après un jour au hasard (${diff}, ${tText(s.tStat)}).`;
}

export function waitText(w: WaitStats): string {
  if (w.days === 0) return "Jamais sur ATTENDRE pendant les tests.";
  return `ATTENDRE ${pct0(w.share)} des jours testés, suivis en moyenne de ${signedPct(w.meanNet, 2)} (tous les jours : ${signedPct(w.baselineNet, 2)}).`;
}

/** Calibration buckets that have days in them. */
export function calibrationRows(b: Bucket[]): Bucket[] {
  return b.filter((x) => x.rows > 0);
}

/** "meilleur que la fréquence de base" / "moins bon…" from a Brier skill (%). */
export function skillText(skill: number | null): string {
  if (skill == null) return "non calculable";
  if (Math.abs(skill) < 0.5) return "pas mieux que la fréquence de base";
  return skill > 0 ? `${fr(skill, 1)}${NNBSP}% mieux que la fréquence de base` : `${fr(-skill, 1)}${NNBSP}% moins bien que la fréquence de base`;
}

/** Decision card line: "ATTENDRE · hausse 54 %, baisse 44 %". */
export function botSummary(v: BotView): string {
  if (!v.available || !v.action) return v.text;
  return `${ACTION_UI[v.action].label} · hausse ${pct0(v.up)}, baisse ${pct0(v.down)}`;
}

/** Whether any group has an edge on either side (then the headline says which one counts). */
export function anyEdge(r: BotReport): boolean {
  return r.groups.some((g) => g.buy.verdict === "edge" || g.sell.verdict === "edge");
}

/** Checks the fields the card relies on (the decision's parse throws on a bad `bot`). */
export function isBotView(v: unknown): v is BotView {
  const b = v as Partial<BotView> | null;
  return !!b && typeof b.available === "boolean" && typeof b.text === "string" && typeof b.counts === "boolean";
}

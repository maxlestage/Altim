/**
 * « Bot Altim » (/api/bot, /api/bot/views, `bot` of /api/decision): JSON contract and the pure helpers of the screen
 * and of the decision line, kept apart from the React views so they can be tested and ported to the mobile apps.
 * Returns and probabilities are in %, "points" are differences of % (signal − random day), times in ms. The server
 * computes everything; nothing here recomputes a statistic. v2 fields are additive and optional (a v1 answer still
 * reads): `version`, `changes`, per group `universe`, `dataYears`, `selection`, `candidates`, `holdout`, `extra`,
 * `market`, `clustered` t in each side, `exit` of the sell side, the live model's `candidate`.
 */
import type { Kind } from "../engine/reliability";
import type { AssetClass, Verdict } from "./model-validation";
import { NNBSP, plain, signedPct } from "./model-validation";

export type BotGroup = "stock" | "crypto";
export type BotAction = "buy" | "wait" | "sell";
/** The fixed candidate models of v2: v1's logistic regression (16 features), the same on 24, boosted trees, the trend rule. */
export type Candidate = "v1" | "logit" | "trees" | "trend";

/** t of a side's excesses: by date (the verdict's since v2), by asset, per signal (v1's). */
export type Clustered = { byDate: number | null; dates: number; byAsset: number | null; assets: number; perSignal: number | null };
/** Holding vs leaving for 20 days at each VENDRE signal (medians over the assets; drawdowns ≤ 0 in %). */
export type ExitStats = {
  assets: number; outShare: number | null; medianHoldMaxDrawdown: number | null; medianBotMaxDrawdown: number | null;
  medianDrawdownAvoided: number | null; medianHoldReturn: number | null; medianBotReturn: number | null; beatHold: number;
};

/** Probability range [from, to) in %, rows = labelled test days, predicted = mean probability, realised = share. */
export type Bucket = { from: number; to: number; label: string; rows: number; predicted: number | null; realised: number | null };

/** ACHETER signals out of sample (non-overlapping per asset). excess = meanNet − baselineNet (same asset and period). */
export type BuyStats = {
  signals: number; meanNet: number | null; baselineNet: number | null; allDaysNet: number | null; excess: number | null;
  tStat: number | null; rawTStat: number | null; clustered?: Clustered; hitRate: number | null; baselineHitRate: number | null;
  medianBotReturn: number | null; medianHoldReturn: number | null; beatHold: number; assets: number;
  verdict: Verdict | null; verdictLabel: string;
};
/** VENDRE signals: what holding made over the next 20 days after them vs a random day; avoided = baseline − after. */
export type SellStats = {
  signals: number; meanAfter: number | null; baselineAfter: number | null; allDaysAfter: number | null; avoided: number | null;
  tStat: number | null; clustered?: Clustered; fallRate: number | null; baselineFallRate: number | null; meanDrawdown: number | null; baselineDrawdown: number | null;
  exit?: ExitStats; verdict: Verdict | null; verdictLabel: string;
};
export type WaitStats = { days: number; share: number | null; meanNet: number | null; baselineNet: number | null };

export type BotStats = {
  testRows: number; labelled: number; buy: BuyStats; sell: SellStats; wait: WaitStats;
  brierSkillUp: number | null; brierSkillDown: number | null; calibrationUp: Bucket[]; calibrationDown: Bucket[];
};

export type ModelOut = { baseRate: number; threshold: number; intercept: number; weights: { id: string; coef: number; mean: number; sd: number }[] };
export type CandidateScore = { id: Candidate; logLoss: number | null };
/** A live side model: `logit` weights, `trees` (compact nodes [feature, threshold, left, right, value]) or `trend` probabilities. */
export type LiveSide = { baseRate: number; threshold: number; logit: ModelOut | null; trees: unknown | null; trend: { probs: number[]; baseRate: number; rows: number } | null };
export type LiveModel = {
  trainedRows: number; trainedFrom: number | null; trainedTo: number | null; up: ModelOut; down: ModelOut;
  candidate?: Candidate; scores?: CandidateScore[]; upModel?: LiveSide; downModel?: LiveSide;
};
/** One candidate's own walk-forward result: for information only, never used to choose. */
export type CandidateStat = BotStats & { id: Candidate; label: string; description: string; trainedBlocks: number; chosenBlocks: number };
/** One retraining: test period [start, end), training rows, inner-validation log-loss of each candidate, choice. */
export type BlockOut = { start: number; end: number | null; trainRows: number; chosen: Candidate | null; scores: CandidateScore[] };
export type Universe = {
  basket: number; extra: number; extraFailed: number; rows: number; dataFrom: number | null; medianYears: number | null; maxYears: number | null;
};
export type Holdout = BotStats & { from: number | null; to: number | null };

export type BotGroupStat = BotStats & {
  id: BotGroup; label: string; assets: number; testFrom: number | null; testTo: number | null; blocks: number; trainedBlocks: number;
  model: LiveModel | null; text: string;
  universe?: Universe; dataYears?: number | null; selection?: BlockOut[]; candidates?: CandidateStat[]; holdout?: Holdout | null;
  /** The nested result on the extra training assets (out of sample too, not the headline). */
  extra?: BotStats | null; market?: string;
};

export type NowView = { time: number | null; action: BotAction | null; up: number | null; down: number | null };

export type BotAssetRow = {
  symbol: string; name: string; kind: Kind; class: AssetClass; group: BotGroup;
  testRows: number; testFrom: number | null; testTo: number | null;
  buys: number; buyMean: number | null; buyExcess: number | null; sells: number; sellAvoided: number | null; waitShare: number | null;
  botReturn: number | null; holdReturn: number | null; now: NowView; source: string;
  years?: number | null; dataFrom?: number | null; outShare?: number | null; holdMaxDrawdown?: number | null; botMaxDrawdown?: number | null;
};

type Failure = { symbol: string; name: string; kind: Kind; class: AssetClass; error: string };

export type BotReport = {
  asOf: number; basketFixedOn: string; headline: string; overall: BotStats; groups: BotGroupStat[];
  /** Basket order, never ranked by performance. */
  assets: BotAssetRow[];
  failures: Failure[];
  features: { id: string; label: string; help: string }[];
  parameters: {
    horizonDays: number; retrainEvery: number; purgeDays: number; minTrainDays: number; warmup: number; l2: number;
    thresholdMargin: number; costStockPct: number; costCryptoPct: number; minSignals: number; tEdge: number; nudge: number;
    innerValidationDays?: number; trainStride?: number; holdoutDays?: number; stockYears?: number; trees?: number; treeDepth?: number;
    shrinkage?: number; minLeaf?: number; selection?: string;
  };
  method: string[]; limits: string[]; source: string;
  version?: number; changes?: string[]; extraFailures?: Failure[]; extraFixedOn?: string;
  timing?: { fetchMs: number; computeMs: number; threads: number } | null;
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
  model?: Candidate | null; modelLabel?: string | null;
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
/** v2 (clustered present): the t is by date. */
const sideT = (t: number | null, c: Clustered | undefined) => (c && t != null ? `t par jour = ${plain(t, 1)}` : tText(t));

export const CANDIDATE_SHORT: Record<Candidate, string> = { v1: "Logistique v1", logit: "Logistique 24", trees: "Arbres", trend: "Tendance" };

/** "t par jour −2,4 · par actif −4,5 · par signal −2,8" (the verdict reads the first). */
export function clusteredText(c: Clustered | undefined, t: number | null): string {
  if (!c) return `t = ${t == null ? "—" : plain(t, 1)}`;
  const f = (v: number | null) => (v == null ? "—" : plain(v, 1));
  return `t par jour ${f(c.byDate)} (${c.dates} jours) · par actif ${f(c.byAsset)} · par signal ${f(c.perSignal)}`;
}

/** What leaving at each VENDRE did to the holding: time out, worst fall, return (medians over the assets). */
export function exitText(e: ExitStats | undefined): string | null {
  if (!e || e.assets === 0) return null;
  return `En sortant 20 jours à chaque VENDRE : hors marché ${pct0(e.outShare)} du temps ; pire baisse médiane ${signedPct(e.medianBotMaxDrawdown)} contre ${signedPct(e.medianHoldMaxDrawdown)} en gardant ; rendement médian ${signedPct(e.medianBotReturn, 0)} contre ${signedPct(e.medianHoldReturn, 0)} (mieux que garder : ${e.beatHold} actif${e.beatHold > 1 ? "s" : ""} sur ${e.assets}).`;
}

/** "22 actifs du panier et 72 de plus à l'entraînement · historique médian 20,1 ans (le plus long 20,1) · marché : S&P 500 (SPY)". */
export function dataText(g: BotGroupStat): string | null {
  const u = g.universe;
  if (!u) return null;
  const y = (v: number | null) => (v == null ? "—" : `${plain(v, 1)} ans`);
  const failed = u.extraFailed > 0 ? ` (${u.extraFailed} indisponible${u.extraFailed > 1 ? "s" : ""})` : "";
  return `${u.basket} actif${u.basket > 1 ? "s" : ""} du panier et ${u.extra} de plus à l'entraînement${failed} · historique médian ${y(u.medianYears)} (le plus long ${y(u.maxYears)})${g.market ? ` · marché : ${g.market}` : ""}`;
}

/** Retrainings grouped by consecutive identical choices: `from` / `to` = start of the first / last retraining of the run. */
export function selectionRuns(blocks: BlockOut[]): { from: number; to: number; chosen: Candidate | null; count: number }[] {
  const out: { from: number; to: number; chosen: Candidate | null; count: number }[] = [];
  for (const b of blocks) {
    const last = out[out.length - 1];
    if (last && last.chosen === b.chosen) {
      last.to = b.start;
      last.count++;
    } else out.push({ from: b.start, to: b.start, chosen: b.chosen, count: 1 });
  }
  return out;
}

/** "108 achats : +1,7 % en moyenne contre +1,85 % pour une entrée au hasard (−0,15 point, t = −0,1)". */
export function buyText(b: BuyStats): string {
  if (b.signals === 0) return "Aucun achat pendant les périodes de test.";
  return `${b.signals} achat${b.signals > 1 ? "s" : ""} : ${signedPct(b.meanNet, 2)} en moyenne sur 20 jours, contre ${signedPct(b.baselineNet, 2)} pour une entrée au hasard sur le même actif et la même période (${points(b.excess)}, ${sideT(b.tStat, b.clustered)}).`;
}

/** What followed the VENDRE signals, said without a sign to decode. */
export function sellText(s: SellStats): string {
  if (s.signals === 0) return "Aucune vente pendant les périodes de test.";
  const diff = s.avoided == null ? "" : s.avoided >= 0 ? `cours ensuite inférieur de ${points(s.avoided).replace("+", "")}` : `cours ensuite supérieur de ${points(-s.avoided).replace("+", "")}`;
  return `${s.signals} vente${s.signals > 1 ? "s" : ""} : le cours a fait ${signedPct(s.meanAfter, 2)} dans les 20 jours suivants, contre ${signedPct(s.baselineAfter, 2)} après un jour au hasard (${diff}, ${sideT(s.tStat, s.clustered)}).`;
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

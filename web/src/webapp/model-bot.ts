/**
 * « Bot Altim » (/api/bot, /api/bot/views, `bot` of /api/decision): JSON contract and the pure helpers of the screen
 * and of the decision line, kept apart from the React views so they can be tested and ported to the mobile apps.
 * Returns and probabilities are in %, "points" are differences of % (signal − random day), times in ms. The server
 * computes everything; nothing here recomputes a statistic. v2 fields are additive and optional (a v1 answer still
 * reads): `version`, `changes`, per group `universe`, `dataYears`, `selection`, `candidates`, `holdout`, `extra`,
 * `market`, `clustered` t in each side, `exit` of the sell side, the live model's `candidate`. v3 (additive, optional):
 * `v3` of the report (horizons 20 / 60, peers ranking, corrected threshold, forward test), `v3` of an asset row (today's
 * actions of the 4 headline configurations) and of a view.
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
  v3?: V3AssetNow;
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
  timing?: { fetchMs: number; computeMs: number; threads: number; rssBeforeMb?: number | null; peakRssMb?: number | null } | null;
  /** Since v3: the pre-registered v3; the v2-shaped fields then hold v2's selection at 20 days, at the corrected threshold. */
  v3?: V3Report;
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
  /** Since v3: the 4 headline configurations today; `counts` is then theirs. */
  v3?: V3View;
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

// ---------- v3 (additive: `v3` of the report, of an asset row and of a view) ----------

/** v3's candidates: family « absolute » (rise / fall of the asset) then « peers » (vs the group's median). */
export type V3Candidate = "trend" | "v1" | "logit" | "trees" | "treesLong" | "xsMomentum" | "xsLogit" | "xsTrees";
export type Family = "absolute" | "peers" | "v2";
/**
 * One side of a configuration: excess in points of % (buy: mean − baseline; sell: baseline − mean), `t` by date (the
 * verdict's), verdict at the corrected threshold and at the raw t ≥ 2 (for comparison only).
 */
export type SideStats = {
  signals: number; mean: number | null; baseline: number | null; excess: number | null; t: number | null; dates: number;
  tByAsset: number | null; assets: number; tPerSignal: number | null; beatShare: number | null; verdict: Verdict | null; rawVerdict: Verdict | null;
};
export type ConfigStats = {
  testRows: number; labelled: number; from: number | null; to: number | null; buy: SideStats; sell: SideStats; waitShare: number | null;
  medianBotReturn: number | null; medianHoldReturn: number | null; beatHold: number; holdAssets: number; exit: ExitStats | null;
  brierSkillUp: number | null; brierSkillDown: number | null;
  /** Family B: the same sides against the group's equal-weight mean (control added after the first real run). */
  buyVsMean?: SideStats | null; sellVsMean?: SideStats | null;
};
/** `main`: basket, signals dated up to the pre-registration; `extra`: training assets (nested only); `forward`: after it. */
export type V3Config = {
  id: string; family: Family; candidate: V3Candidate | null; nested: boolean; headline: boolean; label: string;
  trainedBlocks: number; chosenBlocks: number; main: ConfigStats; extra: ConfigStats | null; forward: ConfigStats;
};
export type EconScore = { id: V3Candidate; score: number | null; signals: number };
export type V3BlockOut = {
  start: number; end: number | null; trainRows: number; v2: Candidate | null; absolute: V3Candidate | null; peers: V3Candidate | null;
  absoluteScores: EconScore[]; peersScores: EconScore[]; roundsUp: number | null; roundsDown: number | null; roundsPeers: number | null;
};
export type Rounds = { fits: number; min: number | null; median: number | null; max: number | null };
export type V3Horizon = {
  horizon: number; blocks: number; testFrom: number | null; testTo: number | null; configs: V3Config[]; selection: V3BlockOut[];
  roundsUp: Rounds; roundsDown: Rounds; roundsPeers: Rounds;
};
/** Annualised (%, Sharpe without risk-free rate), max drawdown ≤ 0 (%), mean exposure 0-1. */
export type SeriesStats = {
  days: number; annualReturn: number | null; annualVol: number | null; sharpe: number | null; maxDrawdown: number | null; totalReturn: number | null; meanExposure: number | null;
};
export type VolManaged = {
  assets: number; from: number | null; to: number | null; periods: number; hold: SeriesStats; managed: SeriesStats;
  medianSharpeHold: number | null; medianSharpeManaged: number | null; medianDrawdownHold: number | null; medianDrawdownManaged: number | null;
  betterSharpe: number; shallowerDrawdown: number; forwardHold: SeriesStats; forwardManaged: SeriesStats; text: string;
};
export type V3Group = {
  id: BotGroup; label: string; market: string; universe: Universe; peersFrom: number | null; horizons: V3Horizon[]; volManaged: VolManaged | null; text: string;
};
export type V3Report = {
  version: number; preregDate: string; forwardFrom: number; afterPrereg: string[]; k: { v1: number; v2: number; v3: number; total: number };
  alpha: number; tRequired: number; headline: string; forwardHeadline: string; groups: V3Group[];
  candidates: { id: V3Candidate; family: Family; label: string; description: string }[];
  changes: string[]; method: string[]; limits: string[];
  parameters: {
    horizons: number[]; stockFrom: string; maxTreeRows: number; longMaxRounds: number; longDepth: number; longShrinkage: number; longMinLeaf: number;
    patience: number; minPeers: number; minValSignals: number; minSignals: number; nudge: number;
  };
  compute: { maxRows: number; blocks: number; rowBytes: number };
};
export type V3AssetNow = { time: number | null; absolute20: BotAction | null; absolute60: BotAction | null; peers20: BotAction | null; peers60: BotAction | null };
export type V3Signal = {
  family: Family; horizon: number; candidate: V3Candidate | null; action: BotAction | null; buyVerdict: Verdict | null; sellVerdict: Verdict | null;
  forwardBuySignals: number; forwardSellSignals: number; contradicted: boolean; counts: boolean; text: string;
  /** Proven on the past but awaiting 30 confirming forward signals: shown only (since the rule of 30/09/2026). */
  pending?: boolean;
};
export type V3View = {
  available: boolean; signals: V3Signal[]; counts: boolean; nudge: number; tRequired: number; note: string;
  pro: string | null; con: string | null; conHeld: string | null;
};

export const V3_SHORT: Record<V3Candidate, string> = {
  trend: "Tendance", v1: "Logistique v1", logit: "Logistique 24", trees: "Arbres v2", treesLong: "Arbres longs",
  xsMomentum: "Momentum entre pairs", xsLogit: "Logistique entre pairs", xsTrees: "Arbres longs entre pairs",
};
export const FAMILY_LABEL: Record<Family, string> = { absolute: "Hausse ou baisse de l'actif", peers: "Classement entre pairs", v2: "Sélection v2 (log-loss)" };

/** "12/10/2026" from "2026-10-12". */
export const frIso = (s: string) => s.split("-").reverse().join("/");

/** "t = −1,2 (requis 3,52)". */
export function tVsRequired(t: number | null, required: number): string {
  return `t = ${t == null ? "—" : plain(t, 1)} (requis ${plain(required, 2)})`;
}

/** "t par jour 3,7 (requis 3,52) · par actif 0,6": the verdict's t, the one it needs, and the t by asset. */
export function tLine(s: SideStats, required: number): string {
  return `t par jour ${s.t == null ? "—" : plain(s.t, 1)} (requis ${plain(required, 2)}) · par actif ${s.tByAsset == null ? "—" : plain(s.tByAsset, 1)}`;
}

/** Verdict at the corrected threshold, in French; says when only the raw t ≥ 2 was reached. */
export function v3VerdictLabel(s: SideStats, required: number): string {
  switch (s.verdict) {
    case "edge": return `Avantage au seuil corrigé (t ≥ ${plain(required, 2)}), à confirmer`;
    case "negative": return "Pire que la référence au seuil corrigé";
    case "insufficient": return "Trop peu de signaux pour conclure";
    default: return s.rawVerdict === "edge" ? "t ≥ 2 atteint, pas le seuil corrigé : non démontré" : "Non démontré";
  }
}

/** A side of a configuration in one sentence: "412 achats : +0,31 point face à la médiane du groupe (t par jour 1,1 (requis 3,52) · par actif 0,4)." */
export function v3SideText(family: Family, side: "buy" | "sell", s: SideStats, required: number): string {
  if (s.signals === 0) return side === "buy" ? "Aucun achat." : "Aucune vente.";
  const ref = family === "peers" ? "la médiane du groupe" : "une entrée au hasard";
  const plural = s.signals > 1 ? "s" : "";
  if (side === "buy") return `${s.signals} achat${plural} : ${points(s.excess)} face à ${ref} (${tLine(s, required)}).`;
  const verb = family === "peers" ? "gain à passer sur l'actif médian" : "baisse évitée";
  return `${s.signals} vente${plural} : ${verb} ${points(s.excess)} (${tLine(s, required)}).`;
}

/** Forward test of a configuration so far. */
export function forwardText(c: ConfigStats, required: number): string {
  const n = c.buy.signals + c.sell.signals;
  if (n === 0) return "Aucun signal jugé pour l'instant (il faut 20 à 60 jours de bourse après le signal).";
  return `${c.buy.signals} achat${c.buy.signals > 1 ? "s" : ""} (${points(c.buy.excess)}, ${tVsRequired(c.buy.t, required)}) · ${c.sell.signals} vente${c.sell.signals > 1 ? "s" : ""} (${points(c.sell.excess)}).`;
}

/** Sharpe, max drawdown, yearly return and volatility of the managed trend vs holding (equal-weight portfolio). */
export function volRows(v: VolManaged): { label: string; managed: string; hold: string }[] {
  return [
    { label: "Ratio de Sharpe", managed: plain(v.managed.sharpe, 2), hold: plain(v.hold.sharpe, 2) },
    { label: "Pire baisse", managed: signedPct(v.managed.maxDrawdown), hold: signedPct(v.hold.maxDrawdown) },
    { label: "Rendement annuel", managed: signedPct(v.managed.annualReturn), hold: signedPct(v.hold.annualReturn) },
    { label: "Volatilité annuelle", managed: pct0(v.managed.annualVol), hold: pct0(v.hold.annualVol) },
  ];
}

/** "7 min 12 s de calcul sur 1 cœur, pic mémoire 243 Mo (téléchargement 15 s)". */
export function computeText(t: BotReport["timing"]): string | null {
  if (!t) return null;
  const s = Math.round(t.computeMs / 1000);
  const dur = s >= 60 ? `${Math.floor(s / 60)} min ${s % 60} s` : `${s} s`;
  const mem = t.peakRssMb != null ? `, pic mémoire ${Math.round(t.peakRssMb)} Mo` : "";
  return `${dur} de calcul sur ${t.threads} cœur${t.threads > 1 ? "s" : ""}${mem} (téléchargement ${Math.round(t.fetchMs / 1000)} s)`;
}

/** The 4 headline configurations of a group: (horizon, config). */
export function headlineConfigs(g: V3Group): { horizon: number; c: V3Config }[] {
  return g.horizons.flatMap((h) => h.configs.filter((c) => c.headline).map((c) => ({ horizon: h.horizon, c })));
}

/** The figures that judge a side: family B's control against the group's mean when measured, else the side itself. */
export function judged(c: ConfigStats, side: "buy" | "sell"): SideStats {
  return (side === "buy" ? c.buyVsMean : c.sellVsMean) ?? c[side];
}

/** A side proven at the corrected threshold (family B: against the median and the mean). */
export function proven(c: ConfigStats, side: "buy" | "sell"): boolean {
  return c[side].verdict === "edge" && judged(c, side).verdict === "edge";
}

/** A side proven on the past and confirmed by the forward test (≥ 30 signals, excess ≥ 0 against each reference): only then does it count. */
export function confirmed(c: V3Config, side: "buy" | "sell"): boolean {
  const refs = [c.forward[side], side === "buy" ? c.forward.buyVsMean : c.forward.sellVsMean].filter((s): s is SideStats => s != null);
  return proven(c.main, side) && refs.every((s) => s.signals >= 30 && s.excess != null && s.excess >= 0);
}

/** « Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : … » for a proven side awaiting its forward test; null otherwise. */
export function pendingText(c: V3Config, side: "buy" | "sell", required: number): string | null {
  if (!proven(c.main, side) || confirmed(c, side)) return null;
  const t = judged(c.main, side).t;
  return `Avantage mesuré sur le passé (t = ${t == null ? "—" : plain(t, 2)} contre ${plain(required, 2)} exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment (${c.forward[side].signals} à ce jour).`;
}

/** Whether a v3 headline configuration has an edge on either side (at the corrected threshold, both references). */
export function v3AnyEdge(r: V3Report): boolean {
  return r.groups.some((g) => headlineConfigs(g).some(({ c }) => proven(c.main, "buy") || proven(c.main, "sell")));
}

/** Signals judged so far in the forward test (headline configurations). */
export function forwardSignals(r: V3Report): number {
  return r.groups.reduce((s, g) => s + headlineConfigs(g).reduce((t, { c }) => t + c.forward.buy.signals + c.forward.sell.signals, 0), 0);
}

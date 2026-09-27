/**
 * Ports of DataQuality.swift and ReliabilityGate: candle-series quality check and
 * reliability score (quality capped by the number of independent sources).
 */
import { sanitize, type Candle, type Signal } from "./signal";

export type Kind = "crypto" | "stock";
export type ReliabilityLevel = "high" | "medium" | "low";

export interface QualityReport { score: number; issues: string[]; gaps: number; badTicks: number; stale: boolean }
export interface Reliability { score: number; level: ReliabilityLevel; independent: number; conflict: boolean }

const median = (v: number[]) => {
  if (!v.length) return 0;
  const s = [...v].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2]! : (s[s.length / 2 - 1]! + s[s.length / 2]!) / 2;
};

export function assessQuality(raw: Candle[], intervalMs: number, kind: Kind, now = Date.now()): QualityReport {
  const candles = sanitize(raw);
  let score = 100;
  const issues: string[] = [];
  const invalid = raw.length - candles.length;
  if (invalid > 0) {
    score -= Math.min(20, invalid * 2);
    issues.push(`${invalid} bougie(s) incohérente(s) écartée(s).`);
  }
  if (candles.length < 2) return { score: 0, issues: [...issues, "Aucune donnée exploitable."], gaps: 0, badTicks: 0, stale: true };
  if (candles.length < 60) {
    score -= 40;
    issues.push(`Historique trop court (${candles.length} bougies).`);
  }
  const recent = candles.slice(-300);
  let gaps = 0;
  for (let i = 1; i < recent.length; i++) {
    const delta = recent[i]!.time - recent[i - 1]!.time;
    if (kind === "crypto" ? delta > intervalMs * 1.5 : delta > Math.max(intervalMs * 1.5, 4.5 * 86_400_000)) gaps++;
  }
  if (gaps) {
    score -= Math.min(30, gaps * 5);
    issues.push(`${gaps} trou(s) dans l'historique.`);
  }
  const returns = recent.slice(1).map((c, i) => Math.log(c.close / recent[i]!.close));
  let badTicks = 0;
  if (returns.length > 20) {
    const med = median(returns);
    const mad = median(returns.map((r) => Math.abs(r - med))) * 1.4826;
    if (mad > 0) {
      for (let i = 0; i < returns.length - 1; i++) {
        const r = returns[i]!;
        const next = returns[i + 1]!;
        if (Math.abs(r - med) > 12 * mad && Math.abs(r) > 0.03 && next * r < 0 && Math.abs(next) > Math.abs(r) * 0.6) badTicks++;
      }
    }
  }
  if (badTicks) {
    score -= Math.min(30, badTicks * 10);
    issues.push(`${badTicks} pic(s) de prix aberrant(s) aussitôt annulé(s).`);
  }
  const last = candles[candles.length - 1]!.time;
  const allowed = kind === "crypto" ? intervalMs * 3 : Math.max(intervalMs * 3, 4 * 86_400_000);
  const stale = now - last > allowed + intervalMs;
  if (stale) {
    score -= 40;
    issues.push("Données périmées.");
  }
  const withVolume = candles.filter((c) => c.volume > 0).length;
  if (withVolume > 0 && (candles.length - withVolume) / candles.length > 0.2) {
    score -= 10;
    issues.push("Volume absent sur une partie de l'historique.");
  }
  if (recent.filter((c) => c.high === c.low).length / recent.length > 0.3) {
    score -= 15;
    issues.push("Marché très peu liquide (prix figés).");
  }
  return { score: Math.max(0, score), issues, gaps, badTicks, stale };
}

/** Score cap by number of independent agreeing sources (index = sources, 5 and more = 100). */
export const RELIABILITY_CAP = [40, 40, 60, 75, 90, 100];

export function reliability(quality: number, independentSources: number, conflict: boolean): Reliability {
  // A decision needs several independent confirmations: 1 source = no advice, 3 minimum for "high".
  const cap = RELIABILITY_CAP[Math.min(independentSources, 5)]!;
  const score = conflict ? Math.min(quality, 30) : Math.min(quality, cap);
  return { score, level: score >= 75 ? "high" : score >= 50 ? "medium" : "low", independent: independentSources, conflict };
}

export const LEVEL_LABEL: Record<ReliabilityLevel, string> = {
  high: "Fiabilité élevée",
  medium: "Fiabilité moyenne",
  low: "Fiabilité faible",
};

/** Never a BUY/SELL on doubtful data (same as ReliabilityGate.swift). */
export function gate(signal: Signal, rel: Reliability, qualityIssues: string[] = []): Signal {
  let action = signal.action;
  const warnings = [...signal.warnings];
  if (rel.level === "low") {
    if (action !== "hold") {
      action = "hold";
      warnings.unshift(`Données insuffisamment fiables (${Math.round(rel.score)}/100) : signal suspendu.`);
    }
  } else if (rel.level === "medium") {
    if (action === "strongBuy") action = "buy";
    if (action === "strongSell") action = "sell";
    warnings.push(`Fiabilité moyenne des données (${rel.independent} source(s) indépendante(s)).`);
  }
  return { ...signal, action, warnings: [...warnings, ...qualityIssues], confidence: (signal.confidence * rel.score) / 100 };
}

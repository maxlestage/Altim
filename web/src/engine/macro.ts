/**
 * Macro-economic and geopolitical context: an asset can sit in a buy zone while everything collapses because of a
 * war, a central bank or a crisis. No one can predict such an event; what can be measured is the stress it causes
 * in the markets, as soon as it starts:
 * - fear (VIX), fall of the S&P 500 from its recent high;
 * - unusual moves (compared with their own last year) of oil (supply shock, wars in producing regions), gold (safe
 *   haven), the dollar (flight to safety) and US 10-year yields (central banks, inflation);
 * - escalation headlines (war declared, invasion, nuclear threat, bank run…), which cannot be checked on history.
 * The market part is checked on each asset: does high stress precede a fall of the asset more often than usual?
 */
import { atr, sanitize, type Candle } from "./signal";
import type { Evidence, NewsItem } from "./guard";

export type MacroLevel = "calm" | "tense" | "high";
export type MacroKey = "vix" | "spx" | "oil" | "gold" | "dollar" | "rates";
export type MacroSeries = Partial<Record<MacroKey, { time: number; close: number }[]>>;

export interface MacroFactor { code: string; points: number; text: string }
export interface MacroTheme { theme: "geopolitics" | "monetary" | "trade" | "stress"; label: string; count: number; examples: string[] }
export interface MacroReport {
  score: number;
  level: MacroLevel;
  /** Market part only (measurable, checked on history). */
  marketScore: number;
  factors: MacroFactor[];
  themes: MacroTheme[];
  /** Latest values, for display. */
  values: Partial<Record<MacroKey, { value: number; change5d: number }>>;
  asOf: number | null;
}

export const MACRO = { tense: 25, high: 50, window: 250 } as const;

const day = (t: number) => new Date(t).toISOString().slice(0, 10);
const one = (v: number) => v.toLocaleString("fr-FR", { maximumFractionDigits: 1 });
const signed = (v: number) => `${v >= 0 ? "+" : "−"}${one(Math.abs(v))} %`;

/** Series aligned on the dates of the VIX (or the S&P 500): value of each series on each date (last known). */
export function align(series: MacroSeries): { dates: string[]; times: number[]; values: Record<MacroKey, (number | null)[]> } {
  const ref = series.vix?.length ? series.vix : series.spx ?? [];
  const dates = ref.map((p) => day(p.time));
  const times = ref.map((p) => p.time);
  const values = {} as Record<MacroKey, (number | null)[]>;
  for (const k of ["vix", "spx", "oil", "gold", "dollar", "rates"] as MacroKey[]) {
    const byDay = new Map((series[k] ?? []).map((p) => [day(p.time), p.close]));
    let lastV: number | null = null;
    values[k] = dates.map((d) => (byDay.has(d) ? (lastV = byDay.get(d)!) : lastV));
  }
  return { dates, times, values };
}

/** z-score of the 5-day change at index i against the previous `window` 5-day changes (no look-ahead). */
function z5(v: (number | null)[], i: number, window: number = MACRO.window): { z: number; change: number } | null {
  const ch = (j: number) => (v[j] != null && v[j - 5] != null && v[j - 5]! > 0 ? (v[j]! / v[j - 5]! - 1) * 100 : null);
  const now = ch(i);
  if (now == null) return null;
  const past: number[] = [];
  for (let j = Math.max(5, i - window); j < i - 5; j++) {
    const c = ch(j);
    if (c != null) past.push(c);
  }
  if (past.length < 60) return null;
  const m = past.reduce((a, b) => a + b, 0) / past.length;
  const sd = Math.sqrt(past.reduce((a, b) => a + (b - m) ** 2, 0) / past.length);
  return sd > 0 ? { z: (now - m) / sd, change: now } : null;
}

/** Market stress on day i (0–100) and its factors. */
export function marketStress(values: Record<MacroKey, (number | null)[]>, i: number): { score: number; factors: MacroFactor[] } {
  const f: MacroFactor[] = [];
  const vix = values.vix[i];
  if (vix != null) {
    if (vix >= 30) f.push({ code: "vixHigh", points: 25, text: `Peur généralisée : VIX à ${one(vix)} (au-delà de 30).` });
    else if (vix >= 25) f.push({ code: "vixHigh", points: 15, text: `Nervosité élevée : VIX à ${one(vix)}.` });
  }
  const zv = z5(values.vix, i);
  if (zv && zv.z >= 2) f.push({ code: "vixJump", points: 15, text: `Le VIX a bondi de ${signed(zv.change)} en 5 séances, inhabituel.` });
  const spx = values.spx;
  if (spx[i] != null) {
    let peak = 0;
    for (let j = Math.max(0, i - 20); j <= i; j++) if (spx[j] != null) peak = Math.max(peak, spx[j]!);
    const dd = peak > 0 ? (1 - spx[i]! / peak) * 100 : 0;
    if (dd >= 7) f.push({ code: "spxDrawdown", points: 20, text: `Le S&P 500 a perdu ${one(dd)} % depuis son plus haut du mois.` });
    else if (dd >= 4) f.push({ code: "spxDrawdown", points: 10, text: `Le S&P 500 recule de ${one(dd)} % depuis son plus haut du mois.` });
  }
  const unusual: [MacroKey, "up" | "both", number, (c: number) => string][] = [
    ["oil", "up", 15, (c) => `Pétrole ${signed(c)} en 5 séances : choc d'offre possible (tensions géopolitiques).`],
    ["gold", "up", 10, (c) => `Or ${signed(c)} en 5 séances : les investisseurs cherchent un refuge.`],
    ["dollar", "up", 10, (c) => `Dollar ${signed(c)} en 5 séances : fuite vers la sécurité.`],
    ["rates", "both", 10, (c) => `Taux américains à 10 ans ${signed(c)} en 5 séances : banques centrales / inflation sous tension.`],
  ];
  for (const [k, dir, pts, text] of unusual) {
    const z = z5(values[k], i);
    if (z && (dir === "both" ? Math.abs(z.z) >= 2 : z.z >= 2)) f.push({ code: k, points: pts, text: text(z.change) });
  }
  return { score: Math.min(100, f.reduce((a, x) => a + x.points, 0)), factors: f };
}

// ---------- Headlines ----------

const THEMES: { theme: MacroTheme["theme"]; label: string; re: RegExp }[] = [
  { theme: "geopolitics", label: "Géopolitique / guerre", re: /\b(wars?|invasion|invades?|invaded|missiles?|air ?strikes?|drone strikes?|military|troops|nuclear|sanctions?|ceasefire|hostages?|coup|blockade)\b/i },
  { theme: "monetary", label: "Banques centrales / inflation", re: /\b(fed|federal reserve|fomc|powell|ecb|rate (hikes?|cuts?)|interest rates?|inflation|cpi|treasury yields?)\b/i },
  { theme: "trade", label: "Commerce / droits de douane", re: /\b(tariffs?|trade war|export (ban|controls?)|embargo)\b/i },
  { theme: "stress", label: "Crise financière", re: /\b(recession|default(s|ed)?|bank (runs?|collapse|failures?)|financial crisis|market crash|sell-?off|bankruptcy|contagion)\b/i },
];

/** Rare escalation events: each distinct one in the last 12 h adds points (not checkable on history). */
const ESCALATION: { re: RegExp; text: string }[] = [
  { re: /\bdeclar(es|ed|ing) war\b/i, text: "déclaration de guerre" },
  { re: /\b(invades?|invaded|invasion of)\b/i, text: "invasion" },
  { re: /\bnuclear (strike|attack|threat|test)\b/i, text: "menace nucléaire" },
  { re: /\b(missile|air) ?strikes? on\b/i, text: "frappes militaires" },
  { re: /\b(closes?|closed|blockade of) (the )?strait\b/i, text: "blocage d'un détroit (pétrole)" },
  { re: /\bmartial law|state of emergency\b/i, text: "état d'urgence" },
  { re: /\bbank runs?|bank collapse\b/i, text: "panique bancaire" },
  { re: /\bcircuit breaker|trading halted\b/i, text: "cotations suspendues" },
  { re: /\bdefaults? on (its )?debt\b/i, text: "défaut de paiement d'un État" },
];

export function headlineThemes(items: NewsItem[], now: number): { themes: MacroTheme[]; factors: MacroFactor[] } {
  const recent = items.filter((n) => n.time <= now && n.time >= now - 86_400_000);
  const themes = THEMES.map(({ theme, label, re }) => {
    const hits = recent.filter((n) => re.test(n.title));
    return { theme, label, count: hits.length, examples: hits.slice(0, 3).map((n) => n.title) };
  }).filter((t) => t.count > 0).sort((a, b) => b.count - a.count);
  // A question ("Is a bank run coming?") is speculation, not an event.
  const last12 = recent.filter((n) => n.time >= now - 12 * 3_600_000 && !/\?\s*(-\s*[^-]+)?$/.test(n.title));
  const found = ESCALATION.filter((e) => last12.some((n) => e.re.test(n.title)));
  const factors = found.length
    ? [{ code: "escalation", points: Math.min(30, 10 * found.length), text: `Actualité : ${found.map((e) => e.text).join(", ")} (dernières 12 h, non vérifiable sur l'historique).` }]
    : [];
  return { themes, factors };
}

export function macroReport(series: MacroSeries, news: NewsItem[] = [], now = Date.now()): MacroReport {
  const a = align(series);
  const i = a.dates.length - 1;
  const market = i >= 0 ? marketStress(a.values, i) : { score: 0, factors: [] };
  const h = headlineThemes(news, now);
  const score = Math.min(100, market.score + h.factors.reduce((s, f) => s + f.points, 0));
  const values: MacroReport["values"] = {};
  for (const k of Object.keys(a.values) as MacroKey[]) {
    const v = a.values[k];
    if (i >= 5 && v[i] != null && v[i - 5] != null) values[k] = { value: v[i]!, change5d: (v[i]! / v[i - 5]! - 1) * 100 };
  }
  return {
    score,
    level: score >= MACRO.high ? "high" : score >= MACRO.tense ? "tense" : "calm",
    marketScore: market.score,
    factors: [...market.factors, ...h.factors],
    themes: h.themes,
    values,
    asOf: i >= 0 ? a.times[i]! : null,
  };
}

/**
 * Does market stress precede a fall of this asset? Days with stress ≥ `MACRO.tense`: share followed within 5 days by
 * a fall of at least twice the asset's usual daily range (median ATR of the whole history, so that the yardstick does
 * not grow with the stress itself), against all days (base).
 * Measured in September 2026 on 5 years: SPY 49 % vs 24 %, QQQ 43 % vs 23 %, NVDA 34 % vs 20 %, AAPL 33 % vs 21 %;
 * no effect on BTC, ETH, SOL (16 % vs 16 %).
 */
export function macroEvidence(assetDaily: Candle[], series: MacroSeries): Evidence | null {
  const c = sanitize(assetDaily);
  const a = align(series);
  const idx = new Map(a.dates.map((d, i) => [d, i]));
  const at = atr(c);
  const ratios = at.map((r, j) => (r == null ? null : r / c[j]!.close)).filter((x): x is number => x != null).sort((x, y) => x - y);
  if (!ratios.length) return null;
  const usual = ratios[Math.floor(ratios.length / 2)]!;
  let n = 0, hits = 0, baseN = 0, baseHits = 0;
  for (let j = 20; j < c.length - 5; j++) {
    const k = idx.get(day(c[j]!.time));
    if (k == null) continue;
    let low = Infinity;
    for (let t = j + 1; t <= j + 5; t++) low = Math.min(low, c[t]!.low);
    const fell = 1 - low / c[j]!.close >= 2 * usual ? 1 : 0;
    baseN++;
    baseHits += fell;
    if (marketStress(a.values, k).score >= MACRO.tense) {
      n++;
      hits += fell;
    }
  }
  if (!n || !baseN) return null;
  const rate = (hits / n) * 100, base = (baseHits / baseN) * 100;
  return { samples: n, rate, base, lift: base > 0 ? rate / base : 1 };
}

/** What the macro context changes for each horizon (a buy zone does not protect from a crisis). */
export function macroAdvice(level: MacroLevel, horizon: "short" | "medium" | "long"): string | null {
  if (level === "calm") return null;
  const high = level === "high";
  return {
    short: high ? "Contexte macro très tendu : éviter d'entrer à court terme, les zones techniques sautent facilement." : "Contexte macro tendu : réduire la taille et garder un stop serré.",
    medium: high ? "Contexte macro très tendu : attendre le bas de la zone, ou entrer par petites tranches." : "Contexte macro tendu : entrer en deux ou trois fois plutôt qu'en une.",
    long: high ? "Contexte macro très tendu : les crises offrent parfois de bons points d'entrée long terme, mais seulement par achats échelonnés sur plusieurs semaines." : "Contexte macro tendu : échelonner les achats long terme.",
  }[horizon];
}

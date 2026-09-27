/**
 * Moteur de signaux Altim. `signal.test.ts` le vérifie sur 17 scénarios de référence (score, confiance,
 * facteurs, avertissements, backtest trade par trade).
 */

export interface Candle {
  time: number; // ms
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export type Action = "strongBuy" | "buy" | "hold" | "sell" | "strongSell";

export interface Factor {
  name: string;
  score: number;
  weight: number;
  detail: string;
}

export interface Signal {
  action: Action;
  score: number;
  confidence: number;
  price: number;
  time: number;
  factors: Factor[];
  stopLoss: number;
  takeProfit: number;
  /** false if the ATR is unavailable (no stop/target plan). */
  hasPlan: boolean;
  warnings: string[];
}

export interface AnalyzeOptions {
  /** Higher-timeframe candles, to confirm the underlying trend. */
  higher?: Candle[];
  /** Candle duration (ms) and current time, to detect stale data. */
  intervalMs?: number;
  now?: number;
}

const isValid = (c: Candle) =>
  [c.open, c.high, c.low, c.close, c.volume].every(Number.isFinite) &&
  c.low > 0 && c.high >= c.low && c.high >= Math.max(c.open, c.close) && c.low <= Math.min(c.open, c.close) && c.volume >= 0;

/** Valid candles, sorted, without duplicates. */
export function sanitize(candles: Candle[]): Candle[] {
  const seen = new Set<number>();
  return candles
    .filter(isValid)
    .sort((a, b) => a.time - b.time)
    .filter((c) => (seen.has(c.time) ? false : (seen.add(c.time), true)));
}

type Series = (number | null)[];

const sign = (x: number) => (x > 0 ? 1 : x < 0 ? -1 : 0);
const clamp = (x: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, x));

export function sma(values: number[], period: number): Series {
  const out: Series = new Array(values.length).fill(null);
  if (values.length < period) return out;
  let sum = 0;
  for (let i = 0; i < period; i++) sum += values[i]!;
  out[period - 1] = sum / period;
  for (let i = period; i < values.length; i++) {
    sum += values[i]! - values[i - period]!;
    out[i] = sum / period;
  }
  return out;
}

export function ema(values: Series, period: number): Series {
  const out: Series = new Array(values.length).fill(null);
  const start = values.findIndex((v) => v !== null);
  if (start < 0 || values.length - start < period) return out;
  const k = 2 / (period + 1);
  let prev = 0;
  for (let j = 0; j < period; j++) prev += values[start + j]!;
  prev /= period;
  out[start + period - 1] = prev;
  for (let i = start + period; i < values.length; i++) {
    const v = values[i];
    if (v === null || v === undefined) return out;
    prev = v * k + prev * (1 - k);
    out[i] = prev;
  }
  return out;
}

export function rsi(closes: number[], period = 14): Series {
  const out: Series = new Array(closes.length).fill(null);
  if (closes.length <= period) return out;
  let gain = 0;
  let loss = 0;
  for (let i = 1; i <= period; i++) {
    const c = closes[i]! - closes[i - 1]!;
    if (c >= 0) gain += c;
    else loss -= c;
  }
  let ag = gain / period;
  let al = loss / period;
  const value = (g: number, l: number) => (l === 0 ? (g === 0 ? 50 : 100) : 100 - 100 / (1 + g / l));
  out[period] = value(ag, al);
  for (let i = period + 1; i < closes.length; i++) {
    const c = closes[i]! - closes[i - 1]!;
    ag = (ag * (period - 1) + Math.max(c, 0)) / period;
    al = (al * (period - 1) + Math.max(-c, 0)) / period;
    out[i] = value(ag, al);
  }
  return out;
}

function trueRange(c: Candle[]): number[] {
  return c.map((x, i) =>
    i === 0 ? x.high - x.low : Math.max(x.high - x.low, Math.abs(x.high - c[i - 1]!.close), Math.abs(x.low - c[i - 1]!.close)),
  );
}

export function atr(c: Candle[], period = 14): Series {
  const out: Series = new Array(c.length).fill(null);
  if (c.length < period) return out;
  const tr = trueRange(c);
  let prev = tr.slice(0, period).reduce((a, b) => a + b, 0) / period;
  out[period - 1] = prev;
  for (let i = period; i < c.length; i++) {
    prev = (prev * (period - 1) + tr[i]!) / period;
    out[i] = prev;
  }
  return out;
}

export function adx(c: Candle[], period = 14): Series {
  const n = c.length;
  const out: Series = new Array(n).fill(null);
  if (n <= period * 2) return out;
  const tr = trueRange(c);
  const pdm = new Array(n).fill(0);
  const mdm = new Array(n).fill(0);
  for (let i = 1; i < n; i++) {
    const up = c[i]!.high - c[i - 1]!.high;
    const down = c[i - 1]!.low - c[i]!.low;
    pdm[i] = up > down && up > 0 ? up : 0;
    mdm[i] = down > up && down > 0 ? down : 0;
  }
  let sTR = 0;
  let sP = 0;
  let sM = 0;
  for (let i = 1; i <= period; i++) {
    sTR += tr[i]!;
    sP += pdm[i];
    sM += mdm[i];
  }
  const dx: Series = new Array(n).fill(null);
  const update = (i: number) => {
    const p = sTR === 0 ? 0 : (100 * sP) / sTR;
    const m = sTR === 0 ? 0 : (100 * sM) / sTR;
    dx[i] = p + m === 0 ? 0 : (100 * Math.abs(p - m)) / (p + m);
  };
  update(period);
  for (let i = period + 1; i < n; i++) {
    sTR = sTR - sTR / period + tr[i]!;
    sP = sP - sP / period + pdm[i];
    sM = sM - sM / period + mdm[i];
    update(i);
  }
  const first = 2 * period - 1;
  let prev = 0;
  for (let i = period; i <= first; i++) prev += dx[i] ?? 0;
  prev /= period;
  out[first] = prev;
  for (let i = first + 1; i < n; i++) {
    prev = (prev * (period - 1) + (dx[i] ?? 0)) / period;
    out[i] = prev;
  }
  return out;
}

function smaOptional(values: Series, period: number): Series {
  const out: Series = new Array(values.length).fill(null);
  for (let i = period - 1; i < values.length; i++) {
    let sum = 0;
    let ok = true;
    for (let j = i - period + 1; j <= i; j++) {
      const v = values[j];
      if (v === null || v === undefined) {
        ok = false;
        break;
      }
      sum += v;
    }
    if (ok) out[i] = sum / period;
  }
  return out;
}

function stochastic(c: Candle[], period = 14, smoothK = 3, smoothD = 3) {
  const raw: Series = new Array(c.length).fill(null);
  for (let i = period - 1; i < c.length; i++) {
    let hh = -Infinity;
    let ll = Infinity;
    for (let j = i - period + 1; j <= i; j++) {
      hh = Math.max(hh, c[j]!.high);
      ll = Math.min(ll, c[j]!.low);
    }
    raw[i] = hh - ll === 0 ? 50 : ((c[i]!.close - ll) / (hh - ll)) * 100;
  }
  const k = smaOptional(raw, smoothK);
  return { k, d: smaOptional(k, smoothD) };
}

function obv(c: Candle[]): number[] {
  const out = new Array(c.length).fill(0);
  for (let i = 1; i < c.length; i++) {
    const d = c[i]!.close - c[i - 1]!.close;
    out[i] = out[i - 1] + (d > 0 ? c[i]!.volume : d < 0 ? -c[i]!.volume : 0);
  }
  return out;
}

function rawSlope(values: number[]): number | null {
  if (values.length < 2) return null;
  const n = values.length;
  const mx = (n - 1) / 2;
  const my = values.reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let den = 0;
  values.forEach((y, i) => {
    num += (i - mx) * (y - my);
    den += (i - mx) * (i - mx);
  });
  return den > 0 ? num / den : null;
}

const W = { trend: 2, macd: 1.5, rsi: 1.5, stoch: 1, boll: 0.75, volume: 1, higher: 1.5 };

export function analyze(raw: Candle[], options: AnalyzeOptions = {}): Signal | null {
  const candles = sanitize(raw);
  if (candles.length < 60) return null;
  const closes = candles.map((c) => c.close);
  const last = candles.length - 1;
  const price = closes[last]!;
  const factors: Factor[] = [];
  const warnings: string[] = [];

  const adxNow = adx(candles)[last] ?? null;
  const atrNow = atr(candles)[last] ?? 0;

  // 1. Trend
  const e20 = ema(closes, 20)[last] ?? null;
  const e50 = ema(closes, 50)[last] ?? null;
  const e200 = ema(closes, 200)[last] ?? null;
  const [fast, slow, label] = e200 !== null ? [e50, e200, "EMA 50/200"] : [e20, e50, "EMA 20/50"];
  if (e200 === null) warnings.push("Moins de 200 bougies : tendance de long terme estimée sur EMA 20/50.");
  if (fast !== null && slow !== null) {
    const rawTrend = 0.5 * sign(fast - slow) + 0.5 * sign(price - slow);
    const strength = adxNow !== null ? clamp((adxNow - 15) / 20, 0.3, 1) : 0.6;
    const dir = rawTrend > 0 ? "haussière" : rawTrend < 0 ? "baissière" : "neutre";
    factors.push({
      name: "Tendance",
      score: clamp(rawTrend * strength, -1, 1),
      weight: W.trend,
      detail: `${label} ${dir}${adxNow !== null ? ` · ADX ${adxNow.toFixed(0)}` : ""}`,
    });
  }
  if (adxNow !== null && adxNow < 18) warnings.push("Marché sans tendance (ADX < 18) : signaux moins fiables.");

  // 2. MACD
  const f12 = ema(closes, 12);
  const s26 = ema(closes, 26);
  const line: Series = f12.map((f, i) => (f !== null && s26[i] !== null ? f - s26[i]! : null));
  const sig = ema(line, 9);
  const hist: Series = line.map((l, i) => (l !== null && sig[i] !== null ? l - sig[i]! : null));
  const h = hist[last];
  const hp = hist[last - 1];
  if (h != null && hp != null) {
    let score = 0.5 * sign(h);
    let detail = h >= 0 ? "Histogramme positif" : "Histogramme négatif";
    let cross: number | null = null;
    for (let i = Math.max(1, last - 3); i <= last; i++) {
      const a = hist[i - 1];
      const b = hist[i];
      if (a == null || b == null) continue;
      if (a <= 0 && b > 0) cross = 1;
      else if (a >= 0 && b < 0) cross = -1;
    }
    if (cross !== null) {
      score = cross;
      detail = cross > 0 ? "Croisement haussier récent" : "Croisement baissier récent";
    } else if (h > hp) {
      score += 0.25;
      detail += ", en hausse";
    } else if (h < hp) {
      score -= 0.25;
      detail += ", en baisse";
    }
    factors.push({ name: "MACD", score: clamp(score, -1, 1), weight: W.macd, detail });
  }

  // 3. RSI
  const r = rsi(closes);
  const rv = r[last];
  if (rv != null) {
    const prev = r[last - 1] ?? rv;
    let score: number;
    if (rv < 30) score = Math.min(1, 0.5 + (30 - rv) / 20) * (rv > prev ? 1 : 0.7);
    else if (rv > 70) score = -Math.min(1, 0.5 + (rv - 70) / 20) * (rv < prev ? 1 : 0.7);
    else score = (rv - 50) / 40;
    const zone = rv < 30 ? "survente" : rv > 70 ? "surachat" : "zone neutre";
    factors.push({ name: "RSI", score: clamp(score, -1, 1), weight: W.rsi, detail: `RSI ${rv.toFixed(1)} (${zone})` });
  }

  // 4. Stochastic
  const st = stochastic(candles);
  const k = st.k[last];
  const d = st.d[last];
  const kp = st.k[last - 1];
  const dp = st.d[last - 1];
  if (k != null && d != null && kp != null && dp != null) {
    let score = 0;
    let detail = `%K ${k.toFixed(0)} / %D ${d.toFixed(0)}`;
    if (kp <= dp && k > d && k < 30) {
      score = 1;
      detail += " · croisement haussier en survente";
    } else if (kp >= dp && k < d && k > 70) {
      score = -1;
      detail += " · croisement baissier en surachat";
    } else if (k < 20) score = 0.5;
    else if (k > 80) score = -0.5;
    factors.push({ name: "Stochastique", score, weight: W.stoch, detail });
  }

  // 5. Bollinger
  const mid = sma(closes, 20)[last];
  if (mid != null) {
    let variance = 0;
    for (let i = last - 19; i <= last; i++) variance += (closes[i]! - mid) ** 2;
    const sd = Math.sqrt(variance / 20);
    const up = mid + 2 * sd;
    const lo = mid - 2 * sd;
    if (up > lo) {
      const pb = (price - lo) / (up - lo);
      const score = pb < 0 ? 0.8 : pb > 1 ? -0.8 : (0.5 - pb) * 0.6;
      factors.push({ name: "Bollinger", score, weight: W.boll, detail: `%B ${pb.toFixed(2)}` });
    }
  }

  // 6. Volume
  const o = obv(candles).slice(-20);
  const avgVol = candles.slice(-20).reduce((a, c) => a + c.volume, 0) / 20;
  const slope = rawSlope(o);
  if (avgVol > 0 && slope !== null) {
    const perBar = slope / avgVol;
    factors.push({
      name: "Volume (OBV)",
      score: clamp(perBar / 0.3, -1, 1),
      weight: W.volume,
      detail: perBar >= 0 ? "Accumulation" : "Distribution",
    });
  } else {
    warnings.push("Volume indisponible : facteur volume ignoré.");
  }

  // 7. Higher-timeframe confirmation
  let higherScore: number | null = null;
  const higher = options.higher ? sanitize(options.higher) : [];
  if (higher.length >= 50) {
    const hc = higher.map((c) => c.close);
    const hl = hc.length - 1;
    const hf = ema(hc, 20)[hl];
    const hs = ema(hc, 50)[hl];
    if (hf != null && hs != null) {
      const sc = 0.5 * sign(hf - hs) + 0.5 * sign(hc[hl]! - hs);
      higherScore = sc;
      factors.push({
        name: "UT supérieure",
        score: sc,
        weight: W.higher,
        detail: sc > 0 ? "Tendance de fond haussière" : sc < 0 ? "Tendance de fond baissière" : "Neutre",
      });
    }
  }

  const tw = factors.reduce((a, f) => a + f.weight, 0);
  const score = tw > 0 ? (factors.reduce((a, f) => a + f.score * f.weight, 0) / tw) * 100 : 0;
  const directional = factors.filter((f) => f.score !== 0);
  const dw = directional.reduce((a, f) => a + f.weight, 0);
  const aw = directional.filter((f) => sign(f.score) === sign(score)).reduce((a, f) => a + f.weight, 0);
  const agreement = dw > 0 ? aw / dw : 0;
  const confidence = clamp(Math.abs(score) * 1.25, 0, 100) * (0.5 + 0.5 * agreement);

  let action: Action =
    score >= 50 ? "strongBuy" : score >= 25 ? "buy" : score <= -50 ? "strongSell" : score <= -25 ? "sell" : "hold";

  // Never buy against the underlying trend: the signal is downgraded.
  if (higherScore !== null) {
    if ((action === "buy" || action === "strongBuy") && higherScore < 0) {
      action = action === "strongBuy" ? "buy" : "hold";
      warnings.push("Achat contre la tendance de l'unité supérieure : signal rétrogradé.");
    } else if ((action === "sell" || action === "strongSell") && higherScore > 0) {
      action = action === "strongSell" ? "sell" : "hold";
      warnings.push("Vente contre la tendance de l'unité supérieure : signal rétrogradé.");
    }
  }

  if (atrNow > 0 && atrNow / price > 0.08) {
    warnings.push(`Volatilité extrême (ATR ${((atrNow / price) * 100).toFixed(1)} % du prix) : réduisez la taille.`);
  }
  const lastTime = candles[last]!.time;
  if (options.intervalMs && (options.now ?? Date.now()) - lastTime > options.intervalMs * 3) {
    warnings.push("Données possiblement périmées (dernière bougie ancienne).");
  }

  const dist = 2 * atrNow;
  const isSell = action === "sell" || action === "strongSell";
  return {
    action,
    score,
    confidence,
    price,
    time: lastTime,
    factors,
    hasPlan: atrNow > 0,
    stopLoss: isSell ? price + dist : Math.max(price - dist, price * 0.01),
    takeProfit: isSell ? Math.max(price - dist * 2, price * 0.01) : price + dist * 2,
    warnings,
  };
}

export const isBuy = (a: Action) => a === "buy" || a === "strongBuy";
export const isSell = (a: Action) => a === "sell" || a === "strongSell";

export const ACTION_LABEL: Record<Action, string> = {
  strongBuy: "ACHAT FORT",
  buy: "ACHAT",
  hold: "ATTENDRE",
  sell: "VENTE",
  strongSell: "VENTE FORTE",
};

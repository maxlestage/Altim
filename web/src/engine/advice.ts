/**
 * Altim's advice on an asset, in plain language. Altim never places orders: it advises.
 * - Asset already held → recommendation from the holdings analysis (holdings.ts).
 * - Asset not held → consider buying / wait / avoid, with plan and prudent amount.
 */
import type { Signal } from "./signal";
import type { ReliabilityLevel } from "./reliability";
import { positionSize, riskReward, type RiskSettings } from "./risk";
import type { TrackRecord } from "./backtest";
import { REASON_TEXT, RECOMMENDATION_LABEL, type LineAnalysis } from "./holdings";

export type AdviceTone = "buy" | "hold" | "sell" | "unknown";
export interface Advice {
  tone: AdviceTone;
  title: string;
  points: string[];
  plan?: { entry: number; stop: number; target: number; riskReward: number };
  /** Prudent amount (USD) to allocate, if the user's wealth is known. */
  amount?: number;
  /** Corresponding quantity (whole shares for a stock). */
  quantity?: number;
}

/** Below this track record on the asset itself, a buy signal is not advised (same rule in Advisor.swift). */
export const MIN_TRACK_TRADES = 5;
/** Same threshold as GUARD.reversalHigh (guard.ts). */
export const REVERSAL_HIGH = 50;
export const MIN_WIN_RATE = 40;

const usd = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: v >= 100 ? 0 : 2 })} $`;
/** Price to the cent above 1 $, 4 significant digits below (0,000009312 $ for PEPE, not 0,00001 $). */
export const px = (v: number) =>
  `${v.toLocaleString("fr-FR", v >= 1 ? { minimumFractionDigits: 2, maximumFractionDigits: 2 } : { maximumSignificantDigits: 4 })} $`;
const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;

/** Quantity to buy or sell: whole shares for a stock, 6 significant digits (rounded down) for a crypto. */
export function adviceQuantity(amount: number, price: number, kind: "crypto" | "stock"): number {
  if (!(amount > 0) || !(price > 0)) return 0;
  const raw = amount / price;
  if (kind === "stock") return Math.floor(raw + 1e-9);
  const step = 10 ** (Math.floor(Math.log10(raw)) - 5);
  return Math.floor(raw / step + 1e-9) * step;
}

export function quantityText(q: number, kind: "crypto" | "stock", symbol: string): string {
  if (kind === "stock") return `${q} action${q > 1 ? "s" : ""} ${symbol}`;
  return `${q.toLocaleString("fr-FR", { maximumSignificantDigits: 6 })} ${symbol}`;
}

export function adviseAsset(input: {
  signal: Signal | null;
  reliability: ReliabilityLevel | null;
  price: number | null;
  line?: LineAnalysis | null;
  capital?: number;
  risk: RiskSettings;
  /** Backtest of the buy signals on this asset (same candles). */
  track?: TrackRecord | null;
  symbol?: string;
  kind?: "crypto" | "stock";
  /** Market guard (guard.ts): shock level and counter-trend reversal risk. */
  guard?: { shock: "calm" | "agitated" | "shock"; reversalScore: number; reversalDirection: "down" | "up" | null } | null;
}): Advice {
  const { signal, reliability, price, line, capital, risk, track, symbol = "", kind = "crypto", guard } = input;
  const reversalDown = !!guard && guard.reversalDirection === "down" && guard.reversalScore >= REVERSAL_HIGH;

  // Asset already held: the portfolio analysis takes precedence.
  if (line) {
    const tone: AdviceTone = line.recommendation === "sell" || line.recommendation === "protect" ? "sell"
      : line.recommendation === "strengthen" ? "buy" : line.recommendation === "unknown" ? "unknown" : "hold";
    const points = [
      `Vous en détenez ${line.quantity.toLocaleString("fr-FR", { maximumFractionDigits: 8 })} (${usd(line.value)}, ${line.pnl >= 0 ? "+" : "−"}${usd(Math.abs(line.pnl))} depuis l'achat), soit ${line.weight.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} % de votre patrimoine.`,
      ...line.reasons.map((r) => REASON_TEXT[r] ?? r),
    ];
    if (line.recommendation === "lighten" && line.trimValue > 0) {
      const q = line.price ? adviceQuantity(line.trimValue, line.price, line.kind) : 0;
      points.push(`Montant à alléger conseillé : environ ${usd(line.trimValue)}${q > 0 ? `, soit ${quantityText(q, line.kind, line.symbol)}` : ""}.`);
    }
    if (line.stop) points.push(`Stop de protection conseillé : ${px(line.stop)} (si le cours passe dessous, sortir limite la perte à ≈ ${usd(line.lossAtStop ?? 0)}).`);
    if (guard?.shock === "shock") points.push("Marché en choc (mouvements anormaux) : ne renforcez pas maintenant, vérifiez que votre stop est bien en place.");
    if (reversalDown) points.push(`Risque de retournement à la baisse élevé (${guard!.reversalScore}/100) : resserrez votre stop ou prenez une partie de vos gains.`);
    return { tone: tone === "buy" && (guard?.shock === "shock" || reversalDown) ? "hold" : tone, title: RECOMMENDATION_LABEL[line.recommendation], points };
  }

  if (!signal || !price) {
    return { tone: "unknown", title: "Pas de conseil pour l'instant", points: ["Historique ou cours insuffisant pour analyser cet actif."] };
  }
  if (reliability === "low") {
    return {
      tone: "unknown",
      title: "Pas de conseil pour l'instant",
      points: ["Les sources de données sont absentes ou en désaccord : Altim préfère ne rien conseiller plutôt que de mal conseiller."],
    };
  }

  const caution = reliability === "medium" ? ["Fiabilité des données moyenne (peu de sources indépendantes) : restez prudent."] : [];
  const a = signal.action;
  // Market guard: no buy in a shock or when a reversal against the rise is likely.
  if ((a === "buy" || a === "strongBuy") && guard?.shock === "shock") {
    return {
      tone: "hold",
      title: "Attendre : marché en choc",
      points: ["Les indicateurs sont à l'achat, mais le marché fait des mouvements anormaux (volatilité, sauts de prix) : attendez que la tempête passe.", ...caution],
    };
  }
  if ((a === "buy" || a === "strongBuy") && reversalDown) {
    return {
      tone: "hold",
      title: "Attendre : risque de retournement",
      points: [`Les indicateurs sont à l'achat, mais un retournement à la baisse est probable (${guard!.reversalScore}/100 : excès, foule trop optimiste ou actualités défavorables). Attendez qu'il se produise ou soit écarté.`, ...caution],
    };
  }
  // A buy signal must have worked on this very asset: otherwise, wait.
  if ((a === "buy" || a === "strongBuy") && track && track.trades >= MIN_TRACK_TRADES && (track.winRate < MIN_WIN_RATE || track.avgReturn <= 0)) {
    return {
      tone: "hold",
      title: "Attendre : signal peu fiable sur cet actif",
      points: [
        `Les indicateurs sont à l'achat, mais sur l'historique de cet actif ce signal n'a réussi que ${Math.round(track.winRate)} % du temps (${track.trades} signaux, ${pct(track.avgReturn)} en moyenne par signal, frais inclus).`,
        "Altim préfère attendre un meilleur point d'entrée plutôt que de suivre un signal qui a surtout échoué ici.",
        ...caution,
      ],
    };
  }
  if (a === "buy" || a === "strongBuy") {
    const plan = signal.hasPlan ? { entry: price, stop: price - (signal.price - signal.stopLoss), target: price + (signal.price - signal.stopLoss) * 2 } : null;
    const points: string[] = [];
    let amount: number | undefined;
    let quantity: number | undefined;
    if (plan && plan.stop > 0) {
      points.push(`Zone d'entrée : autour de ${px(plan.entry)}. Stop conseillé : ${px(plan.stop)}. Objectif : ${px(plan.target)} (gain potentiel ${riskReward({ entry: plan.entry, stopLoss: plan.stop, takeProfit: plan.target }).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} fois le risque).`);
      const size = capital && capital > 0 ? positionSize(risk, capital, { entry: plan.entry, stopLoss: plan.stop, takeProfit: plan.target }) : null;
      if (size) {
        quantity = adviceQuantity(size.notional, plan.entry, kind);
        // Agitated market: half the amount (same rule as the guard's policy for bots).
        if (guard?.shock === "agitated") quantity = adviceQuantity(size.notional * 0.5, plan.entry, kind);
        // Whole shares: the amount and the loss at the stop are those of the rounded quantity.
        const scale = quantity > 0 && (kind === "stock" || guard?.shock === "agitated") ? (quantity * plan.entry) / size.notional : guard?.shock === "agitated" ? 0.5 : 1;
        amount = size.notional * scale;
        const qty = quantity > 0 && symbol ? `, soit ${quantityText(quantity, kind, symbol)}` : kind === "stock" && symbol ? " (moins d'une action entière : il faudrait des fractions d'action)" : "";
        points.push(`Avec votre patrimoine (${usd(capital!)}), n'y consacrez pas plus d'environ ${usd(amount)}${qty} : si le stop est touché, la perte resterait limitée à ≈ ${usd(size.riskAmount * scale)} (${(risk.riskPerTradePercent * scale).toLocaleString("fr-FR", { maximumFractionDigits: 2 })} % du patrimoine, frais inclus).`);
      } else {
        points.push("Renseignez vos avoirs dans « Mes avoirs » pour obtenir un montant adapté à votre patrimoine.");
      }
    }
    if (guard?.shock === "agitated") points.push("Marché agité : le montant conseillé est divisé par deux, et un stop plus large évite d'être sorti par le bruit.");
    points.push(`Confiance du signal : ${Math.round(signal.confidence)} %. ${a === "strongBuy" ? "Les indicateurs sont largement d'accord." : "Signal modéré : entrez progressivement."}`);
    if (track && track.trades >= MIN_TRACK_TRADES) {
      points.push(`Sur l'historique de cet actif : ${track.trades} signaux d'achat, ${Math.round(track.winRate)} % gagnants, ${pct(track.avgReturn)} en moyenne par signal (frais inclus). Les performances passées ne préjugent pas des performances futures.`);
    } else if (track) {
      points.push(`Seulement ${track.trades} signal${track.trades > 1 ? "s" : ""} d'achat dans l'historique de cet actif : pas assez pour juger de sa fiabilité, prudence.`);
    }
    return {
      tone: "buy",
      title: a === "strongBuy" ? "Achat envisageable (signal fort)" : "Achat envisageable",
      points: [...points, ...caution],
      plan: plan ? { ...plan, riskReward: 2 } : undefined,
      amount,
      quantity,
    };
  }
  if (a === "sell" || a === "strongSell") {
    return {
      tone: "sell",
      title: "À éviter pour l'instant",
      points: [
        "Tendance baissière : ce n'est pas le moment d'acheter.",
        "Si vous en détenez, ajoutez-le dans « Mes avoirs » pour savoir s'il faut protéger ou alléger votre position.",
        ...caution,
      ],
    };
  }
  return {
    tone: "hold",
    title: "Attendre",
    points: [
      `Pas de signal clair (score ${signal.score >= 0 ? "+" : ""}${signal.score.toFixed(0)}/100, ${signal.score > 10 ? "légère orientation haussière" : signal.score < -10 ? "légère orientation baissière" : "neutre"}).`,
      "Mieux vaut attendre un signal d'achat confirmé par plusieurs indicateurs.",
      ...caution,
    ],
  };
}

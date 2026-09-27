/**
 * Altim's advice on an asset, in plain language. Altim never places orders: it advises.
 * - Asset already held → recommendation from the holdings analysis (holdings.ts).
 * - Asset not held → consider buying / wait / avoid, with plan and prudent amount.
 */
import type { Signal } from "./signal";
import type { ReliabilityLevel } from "./reliability";
import { positionSize, riskReward, type RiskSettings } from "./risk";
import { REASON_TEXT, RECOMMENDATION_LABEL, type LineAnalysis } from "./holdings";

export type AdviceTone = "buy" | "hold" | "sell" | "unknown";
export interface Advice {
  tone: AdviceTone;
  title: string;
  points: string[];
  plan?: { entry: number; stop: number; target: number; riskReward: number };
  /** Prudent amount (USD) to allocate, if the user's wealth is known. */
  amount?: number;
}

const usd = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: v >= 100 ? 0 : 2 })} $`;
const px = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: v >= 1 ? 2 : 6 })} $`;

export function adviseAsset(input: {
  signal: Signal | null;
  reliability: ReliabilityLevel | null;
  price: number | null;
  line?: LineAnalysis | null;
  capital?: number;
  risk: RiskSettings;
}): Advice {
  const { signal, reliability, price, line, capital, risk } = input;

  // Asset already held: the portfolio analysis takes precedence.
  if (line) {
    const tone: AdviceTone = line.recommendation === "sell" || line.recommendation === "protect" ? "sell"
      : line.recommendation === "strengthen" ? "buy" : line.recommendation === "unknown" ? "unknown" : "hold";
    const points = [
      `Vous en détenez ${line.quantity.toLocaleString("fr-FR", { maximumFractionDigits: 8 })} (${usd(line.value)}, ${line.pnl >= 0 ? "+" : "−"}${usd(Math.abs(line.pnl))} depuis l'achat), soit ${line.weight.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} % de votre patrimoine.`,
      ...line.reasons.map((r) => REASON_TEXT[r] ?? r),
    ];
    if (line.recommendation === "lighten" && line.trimValue > 0) points.push(`Montant à alléger conseillé : environ ${usd(line.trimValue)}.`);
    if (line.stop) points.push(`Stop de protection conseillé : ${px(line.stop)} (si le cours passe dessous, sortir limite la perte à ≈ ${usd(line.lossAtStop ?? 0)}).`);
    return { tone, title: RECOMMENDATION_LABEL[line.recommendation], points };
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
  if (a === "buy" || a === "strongBuy") {
    const plan = signal.hasPlan ? { entry: price, stop: price - (signal.price - signal.stopLoss), target: price + (signal.price - signal.stopLoss) * 2 } : null;
    const points: string[] = [];
    let amount: number | undefined;
    if (plan && plan.stop > 0) {
      points.push(`Zone d'entrée : autour de ${px(plan.entry)}. Stop conseillé : ${px(plan.stop)}. Objectif : ${px(plan.target)} (gain potentiel ${riskReward({ entry: plan.entry, stopLoss: plan.stop, takeProfit: plan.target }).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} fois le risque).`);
      const size = capital && capital > 0 ? positionSize(risk, capital, { entry: plan.entry, stopLoss: plan.stop, takeProfit: plan.target }) : null;
      if (size) {
        amount = size.notional;
        points.push(`Avec votre patrimoine (${usd(capital!)}), n'y consacrez pas plus d'environ ${usd(size.notional)} : si le stop est touché, la perte resterait limitée à ≈ ${usd(size.riskAmount)} (${risk.riskPerTradePercent} % du patrimoine).`);
      } else {
        points.push("Renseignez vos avoirs dans « Mes avoirs » pour obtenir un montant adapté à votre patrimoine.");
      }
    }
    points.push(`Confiance du signal : ${Math.round(signal.confidence)} %. ${a === "strongBuy" ? "Les indicateurs sont largement d'accord." : "Signal modéré : entrez progressivement."}`);
    return {
      tone: "buy",
      title: a === "strongBuy" ? "Achat envisageable (signal fort)" : "Achat envisageable",
      points: [...points, ...caution],
      plan: plan ? { ...plan, riskReward: 2 } : undefined,
      amount,
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

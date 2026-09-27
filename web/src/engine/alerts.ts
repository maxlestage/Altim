/**
 * "Can I buy now?" — one rule shared by the notifications of the iPhone, the Apple Watch and Android.
 *
 * An asset is buyable when at least one measured reason says so and nothing blocks it:
 * - reasons: the 4 h signal is ACHAT / ACHAT FORT, or the price is inside a Fibonacci buy zone (or its golden pocket);
 * - blockers: data sources disagree (low reliability), the guard sees a shock, or the move that drew the zone has
 *   been broken (price under its low).
 * Cautions (downtrend, very tense macro context) do not block, they are written in the notification.
 */
import type { Action } from "./signal";
import type { FibZone } from "./fibonacci";
import { formatPrice } from "../market";

export interface AlertInput {
  symbol: string;
  name: string;
  price: number | null;
  signal: { action: Action; confidence: number } | null;
  reliability: "high" | "medium" | "low" | null;
  zones: Pick<FibZone, "horizon" | "label" | "status" | "zone" | "golden" | "invalidation">[];
  shock: "calm" | "agitated" | "shock" | null;
  trend: "up" | "down" | "range" | null;
  macro: "calm" | "tense" | "high" | null;
}

export interface BuyAlert {
  buy: boolean;
  /** Both a buy signal and the price in a zone. */
  strong: boolean;
  reasons: string[];
  blockers: string[];
  cautions: string[];
  /** Stable identifier of the situation: a new notification only when it changes (e.g. "signal+zone:medium"). */
  key: string;
  title: string;
  body: string;
}

const usd = (v: number) => `${formatPrice(v)} $`;
const IN_ZONE = new Set(["inZone", "golden"]);

export function buyAlert(a: AlertInput): BuyAlert {
  const reasons: string[] = [];
  const blockers: string[] = [];
  const cautions: string[] = [];
  const parts: string[] = [];

  const signalBuy = a.signal?.action === "buy" || a.signal?.action === "strongBuy";
  if (signalBuy) {
    reasons.push(`Signal ${a.signal!.action === "strongBuy" ? "ACHAT FORT" : "ACHAT"} en 4 h (confiance ${Math.round(a.signal!.confidence)} %).`);
    parts.push("signal");
  }
  // Longest horizon first: a long-term zone matters more than a short one.
  const order = ["long", "medium", "short"];
  const hit = [...a.zones].sort((x, y) => order.indexOf(x.horizon) - order.indexOf(y.horizon)).find((z) => IN_ZONE.has(z.status) && z.zone);
  if (hit) {
    const band = hit.status === "golden" && hit.golden ? hit.golden : hit.zone!;
    reasons.push(`Prix dans la ${hit.status === "golden" ? "zone d'or" : "zone d'achat"} ${hit.label.toLowerCase()} (${usd(band.to)} – ${usd(band.from)}).`);
    parts.push(`zone:${hit.horizon}`);
  }

  if (a.reliability === "low") blockers.push("Sources de prix en désaccord : données peu fiables.");
  if (a.shock === "shock") blockers.push("Garde-fou : risque de choc élevé, mieux vaut attendre.");
  if (hit && a.price != null && hit.invalidation != null && a.price < hit.invalidation) blockers.push("Le plus bas du mouvement est cassé : zone invalidée.");

  if (a.trend === "down") cautions.push("Tendance de fond baissière : taille réduite.");
  if (a.macro === "high") cautions.push("Contexte macro très tendu : taille réduite.");
  if (a.shock === "agitated") cautions.push("Marché agité : stop plus large.");

  const buy = reasons.length > 0 && blockers.length === 0;
  const strong = buy && signalBuy && !!hit;
  const stop = hit?.invalidation != null ? ` Invalidé sous ${usd(hit.invalidation)}.` : "";
  const price = a.price != null ? ` à ${usd(a.price)}` : "";
  return {
    buy,
    strong,
    reasons,
    blockers,
    cautions,
    key: buy ? parts.join("+") : "",
    title: buy ? `${a.symbol} : achat ${strong ? "conseillé" : "possible"}${price}` : `${a.symbol} : pas d'achat pour l'instant`,
    body: buy ? [...reasons, ...cautions].join(" ") + stop : (blockers[0] ?? "Ni signal d'achat ni prix dans une zone d'achat."),
  };
}

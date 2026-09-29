/**
 * Display currency of every amount: engines and the server compute in dollars (the quote currency of the sources),
 * the display converts to euros with the EUR/USD rate of /api/fx. Pure module state, set once per render by the app
 * (`setMoneyDisplay`); without a rate the amounts stay in dollars with "$" (a rate is never made up).
 */
export type Currency = "EUR" | "USD";

/** `GET /api/fx` with a rate (euros for 1 dollar). */
export interface FxRate {
  rate: number;
  usdPerEur: number;
  /** Time of the quote (ms). */
  time: number;
  /** "Yahoo Finance", "BCE" or "Frankfurter (BCE)". */
  source: string;
  fetchedAt: number;
  stale: boolean;
}

let wanted: Currency = "USD";
let fx: FxRate | null = null;

/** Sets what the formatters below use: the currency chosen in Réglages and the last known rate. */
export function setMoneyDisplay(want: Currency, rate: FxRate | null) {
  wanted = want;
  fx = rate && Number.isFinite(rate.rate) && rate.rate > 0 ? rate : null;
}

export const currentFx = (): FxRate | null => fx;

/** Currency actually shown: euros only when chosen and a rate is known. */
export function displayCurrency(): Currency {
  return wanted === "EUR" && fx ? "EUR" : "USD";
}

export const currencySymbol = (c: Currency = displayCurrency()) => (c === "EUR" ? "€" : "$");

/** `v` from one currency to another at the current rate; NaN when no rate allows it. */
export function convert(v: number, from: Currency, to: Currency): number {
  if (from === to) return v;
  if (!fx) return NaN;
  return from === "USD" ? v * fx.rate : v / fx.rate;
}

/** Dollars → display currency. */
export const toDisplay = (usd: number) => convert(usd, "USD", displayCurrency());
/** Display currency → dollars (what the user typed, for the engines and the server). */
export const fromDisplay = (v: number) => convert(v, displayCurrency(), "USD");

const fr = (v: number, min: number, max: number) => v.toLocaleString("fr-FR", { minimumFractionDigits: min, maximumFractionDigits: max });

/** A dollar amount in the display currency with `min`–`max` decimals: "212,40 €" (or "$" without a rate). */
export function money(usd: number, min = 2, max = min, sep = "\u00a0"): string {
  return `${fr(toDisplay(usd), min, max)}${sep}${currencySymbol()}`;
}

/** A dollar amount in the display currency with the caller's number format (applied to the converted value). */
export function moneyFmt(usd: number, fmt: (v: number) => string, sep = "\u00a0"): string {
  return `${fmt(toDisplay(usd))}${sep}${currencySymbol()}`;
}

/** A price with `formatPrice` digits (2 from 1, 4 from 0.01, 8 below), chosen on the converted value. */
export function moneyPrice(usd: number, sep = "\u00a0"): string {
  const v = toDisplay(usd);
  const digits = v >= 1 ? 2 : v >= 0.01 ? 4 : 8;
  return `${fr(v, digits, digits)}${sep}${currencySymbol()}`;
}

/** Large amounts: "421 Md€", "3,16 Md€", "850 M€", "12,5 k€". */
export function moneyCompact(usd: number, sep = "\u00a0"): string {
  const v = toDisplay(usd);
  const a = Math.abs(v);
  const sym = currencySymbol();
  const [div, unit] = a >= 1e9 ? [1e9, `Md${sym}`] : a >= 1e6 ? [1e6, `M${sym}`] : a >= 1e4 ? [1e3, `k${sym}`] : [1, sym];
  const x = v / div;
  const digits = Math.abs(x) >= 100 ? 0 : Math.abs(x) >= 10 ? 1 : 2;
  return `${fr(x, 0, digits)}${sep}${unit}`;
}

/** "1 $ = 0,881 € · Yahoo Finance, 14:05" (or the date when not today); null without a rate. */
export function fxLine(r: FxRate | null = fx, now = Date.now()): string | null {
  if (!r) return null;
  const d = new Date(r.time);
  const sameDay = new Date(now).toDateString() === d.toDateString();
  const when = sameDay
    ? d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleDateString("fr-FR", { day: "numeric", month: "short" }) + " " + d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  return `1 $ = ${fr(r.rate, 3, 4)} € · ${r.source}, ${when}`;
}

/** Tagged stored amount (localStorage): the currency it was typed in; old data without the tag is in dollars. */
export const storedCurrency = (c: unknown): Currency => (c === "EUR" ? "EUR" : "USD");

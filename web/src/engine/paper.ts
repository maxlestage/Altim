/**
 * Paper trading: a virtual portfolio that follows Altim's decisions with no real money, to check whether
 * "signal → exécution → résultat" holds. Everything stays on the device (like the holdings).
 *
 * Honest rules, the same on the web, iPhone and Android (checked on the same cases, test/paper-fixture.json):
 * - every order pays fees (0,1 %) and slippage (0,05 %, against you) on each side, like the signal's track record;
 * - a position with a stop or a target is closed automatically when a daily candle AFTER the opening reaches it;
 *   a candle that reaches both counts as the stop (the worst case: the order inside the day is not known); a gap
 *   below the stop is filled at the open, a target at the target (never better than planned);
 * - the candle of the opening day is not used (its low may be earlier than the purchase): a stop or target
 *   reached that same day is only seen the next day;
 * - results are grouped by the decision shown at the purchase, to see which verdicts actually worked.
 */

export type PaperKind = "crypto" | "stock";
export type PaperReason = "stop" | "target" | "manual";

/** The decision shown when the position was opened (null: opened without one). */
export interface PaperDecision {
  verdict: string;
  label: string;
  confidence: number;
  asOf: number;
}

export interface PaperPosition {
  id: string;
  symbol: string;
  kind: PaperKind;
  name: string;
  openedAt: number;
  /** Fill price (market price + slippage). */
  entry: number;
  quantity: number;
  /** Amount taken from the cash, fees included. */
  invested: number;
  stop: number | null;
  target: number | null;
  decision: PaperDecision | null;
}

export interface PaperTrade extends PaperPosition {
  closedAt: number;
  /** Fill price (market price − slippage, or the stop / gap open). */
  exit: number;
  reason: PaperReason;
  /** Cash received, fees deducted. */
  proceeds: number;
  pnl: number;
  pnlPct: number;
}

export interface PaperState {
  version: 1;
  startCapital: number;
  cash: number;
  startedAt: number;
  positions: PaperPosition[];
  trades: PaperTrade[];
}

export const FEE_RATE = 0.001;
export const SLIPPAGE = 0.0005;
export const DEFAULT_CAPITAL = 10_000;

const round = (v: number, d = 8) => Math.round(v * 10 ** d) / 10 ** d;

export function newPaper(capital: number, now: number): PaperState {
  const c = Number.isFinite(capital) && capital > 0 ? round(capital, 2) : DEFAULT_CAPITAL;
  return { version: 1, startCapital: c, cash: c, startedAt: now, positions: [], trades: [] };
}

export interface OpenOrder {
  id: string;
  symbol: string;
  kind: PaperKind;
  name: string;
  /** Market price now. */
  price: number;
  /** Amount to invest, fees included. */
  amount: number;
  stop?: number | null;
  target?: number | null;
  decision?: PaperDecision | null;
}

/** Buys `amount` $ of the asset at `price` + slippage, fees deducted. Error text when refused. */
export function openPosition(s: PaperState, o: OpenOrder, now: number): { state: PaperState; error?: string } {
  if (!(o.price > 0) || !Number.isFinite(o.price)) return { state: s, error: "Prix indisponible." };
  if (!(o.amount > 0) || !Number.isFinite(o.amount)) return { state: s, error: "Montant invalide." };
  if (o.amount > s.cash + 1e-9) return { state: s, error: `Liquidités simulées insuffisantes (${round(s.cash, 2)} $ disponibles).` };
  const entry = o.price * (1 + SLIPPAGE);
  const stop = o.stop != null && o.stop > 0 && o.stop < entry ? o.stop : null;
  const target = o.target != null && o.target > entry ? o.target : null;
  const fee = o.amount * FEE_RATE;
  const position: PaperPosition = {
    id: o.id, symbol: o.symbol, kind: o.kind, name: o.name, openedAt: now,
    entry: round(entry), quantity: round((o.amount - fee) / entry, 10), invested: round(o.amount, 2),
    stop, target, decision: o.decision ?? null,
  };
  return { state: { ...s, cash: round(s.cash - o.amount, 2), positions: [...s.positions, position] } };
}

function close(s: PaperState, p: PaperPosition, fill: number, at: number, reason: PaperReason): PaperState {
  const gross = p.quantity * fill;
  const proceeds = round(gross - gross * FEE_RATE, 2);
  const pnl = round(proceeds - p.invested, 2);
  const trade: PaperTrade = { ...p, closedAt: at, exit: round(fill), reason, proceeds, pnl, pnlPct: round((pnl / p.invested) * 100, 4) };
  return {
    ...s,
    cash: round(s.cash + proceeds, 2),
    positions: s.positions.filter((x) => x.id !== p.id),
    trades: [...s.trades, trade],
  };
}

/** Sells the whole position at the market price − slippage, fees deducted. */
export function closePosition(s: PaperState, id: string, price: number, now: number): { state: PaperState; error?: string } {
  const p = s.positions.find((x) => x.id === id);
  if (!p) return { state: s, error: "Position introuvable." };
  if (!(price > 0) || !Number.isFinite(price)) return { state: s, error: "Prix indisponible." };
  return { state: close(s, p, price * (1 - SLIPPAGE), now, "manual") };
}

export interface DailyCandle { time: number; open: number; high: number; low: number; close: number }

/**
 * Closes the positions whose stop or target was reached by a daily candle that started after the opening
 * (candles of `key` = "kind:symbol", any order). Returns the new state and the trades just closed.
 */
export function checkExits(s: PaperState, candles: Record<string, DailyCandle[]>): { state: PaperState; closed: PaperTrade[] } {
  let state = s;
  const closed: PaperTrade[] = [];
  for (const p of s.positions) {
    if (p.stop == null && p.target == null) continue;
    const list = (candles[`${p.kind}:${p.symbol}`] ?? []).filter((c) => c.time > p.openedAt && c.low > 0 && c.high >= c.low).sort((a, b) => a.time - b.time);
    for (const c of list) {
      let fill: number | null = null;
      let reason: PaperReason | null = null;
      if (p.stop != null && c.low <= p.stop) {
        fill = Math.min(p.stop, c.open) * (1 - SLIPPAGE);
        reason = "stop";
      } else if (p.target != null && c.high >= p.target) {
        fill = p.target * (1 - SLIPPAGE);
        reason = "target";
      }
      if (fill != null && reason) {
        state = close(state, p, fill, c.time, reason);
        closed.push(state.trades[state.trades.length - 1]!);
        break;
      }
    }
  }
  return { state, closed };
}

export interface OpenLine extends PaperPosition {
  price: number | null;
  value: number | null;
  pnl: number | null;
  pnlPct: number | null;
}

export interface PaperValuation {
  cash: number;
  /** Value of the open positions at the given prices (sale fees and slippage deducted, as if sold now). */
  positionsValue: number;
  equity: number;
  pnl: number;
  pnlPct: number;
  lines: OpenLine[];
  /** Positions without a price: valued at their cost. */
  unpriced: number;
}

export function valuation(s: PaperState, prices: Record<string, number | null | undefined>): PaperValuation {
  let positionsValue = 0;
  let unpriced = 0;
  const lines = s.positions.map((p): OpenLine => {
    const price = prices[`${p.kind}:${p.symbol}`];
    if (!(price != null && price > 0)) {
      unpriced++;
      positionsValue += p.invested;
      return { ...p, price: null, value: null, pnl: null, pnlPct: null };
    }
    const gross = p.quantity * price * (1 - SLIPPAGE);
    const value = round(gross - gross * FEE_RATE, 2);
    positionsValue += value;
    const pnl = round(value - p.invested, 2);
    return { ...p, price, value, pnl, pnlPct: round((pnl / p.invested) * 100, 4) };
  });
  const equity = round(s.cash + positionsValue, 2);
  const pnl = round(equity - s.startCapital, 2);
  return { cash: s.cash, positionsValue: round(positionsValue, 2), equity, pnl, pnlPct: round((pnl / s.startCapital) * 100, 4), lines, unpriced };
}

export interface VerdictStats { verdict: string; label: string; trades: number; wins: number; winRate: number; avgPnlPct: number }

export interface PaperStats {
  trades: number;
  wins: number;
  winRate: number;
  avgWinPct: number | null;
  avgLossPct: number | null;
  /** Sum of gains ÷ sum of losses (null without a losing trade). */
  profitFactor: number | null;
  realizedPnl: number;
  best: PaperTrade | null;
  worst: PaperTrade | null;
  /** Worst fall of the realized capital (start + closed trades in order), %. */
  maxDrawdownPct: number;
  byReason: Record<PaperReason, number>;
  /** Results grouped by the decision shown at the purchase ("sans décision" when opened without one). */
  byVerdict: VerdictStats[];
}

export function paperStats(s: PaperState): PaperStats {
  const t = [...s.trades].sort((a, b) => a.closedAt - b.closedAt);
  const wins = t.filter((x) => x.pnl > 0);
  const losses = t.filter((x) => x.pnl <= 0);
  const mean = (v: number[]) => (v.length ? round(v.reduce((a, b) => a + b, 0) / v.length, 4) : null);
  const gains = wins.reduce((a, x) => a + x.pnl, 0);
  const lost = -losses.reduce((a, x) => a + x.pnl, 0);
  let capital = s.startCapital;
  let peak = capital;
  let maxDd = 0;
  for (const x of t) {
    capital += x.pnl;
    peak = Math.max(peak, capital);
    maxDd = Math.min(maxDd, (capital / peak - 1) * 100);
  }
  const groups = new Map<string, PaperTrade[]>();
  for (const x of t) {
    const k = x.decision?.verdict ?? "none";
    groups.set(k, [...(groups.get(k) ?? []), x]);
  }
  const byVerdict = [...groups.entries()]
    .map(([verdict, list]): VerdictStats => {
      const w = list.filter((x) => x.pnl > 0).length;
      return {
        verdict,
        label: list[0]!.decision?.label ?? "Sans décision",
        trades: list.length,
        wins: w,
        winRate: round((w / list.length) * 100, 4),
        avgPnlPct: mean(list.map((x) => x.pnlPct))!,
      };
    })
    .sort((a, b) => b.trades - a.trades || a.verdict.localeCompare(b.verdict));
  const byPct = [...t].sort((a, b) => b.pnlPct - a.pnlPct);
  return {
    trades: t.length,
    wins: wins.length,
    winRate: t.length ? round((wins.length / t.length) * 100, 4) : 0,
    avgWinPct: mean(wins.map((x) => x.pnlPct)),
    avgLossPct: mean(losses.map((x) => x.pnlPct)),
    profitFactor: lost > 0 ? round(gains / lost, 4) : null,
    realizedPnl: round(t.reduce((a, x) => a + x.pnl, 0), 2),
    best: byPct[0] ?? null,
    worst: byPct[byPct.length - 1] ?? null,
    maxDrawdownPct: round(maxDd, 4),
    byReason: { stop: t.filter((x) => x.reason === "stop").length, target: t.filter((x) => x.reason === "target").length, manual: t.filter((x) => x.reason === "manual").length },
    byVerdict,
  };
}

/** Accepts only a well-formed saved state (localStorage / files can be edited by hand). */
export function isPaperState(v: unknown): v is PaperState {
  const s = v as PaperState;
  return !!s && s.version === 1 && Number.isFinite(s.startCapital) && s.startCapital > 0 && Number.isFinite(s.cash)
    && Array.isArray(s.positions) && Array.isArray(s.trades)
    && s.positions.every((p) => typeof p.id === "string" && typeof p.symbol === "string" && p.quantity > 0 && p.entry > 0 && p.invested > 0);
}

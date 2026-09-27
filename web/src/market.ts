import type { Candle } from "./engine/signal";

export type Coin = { symbol: string; name: string; gecko: string };

export const COINS: Coin[] = [
  { symbol: "BTCUSDT", name: "Bitcoin", gecko: "bitcoin" },
  { symbol: "ETHUSDT", name: "Ethereum", gecko: "ethereum" },
  { symbol: "SOLUSDT", name: "Solana", gecko: "solana" },
  { symbol: "BNBUSDT", name: "BNB", gecko: "binancecoin" },
  { symbol: "XRPUSDT", name: "XRP", gecko: "ripple" },
  { symbol: "ADAUSDT", name: "Cardano", gecko: "cardano" },
];

export type Tick = {
  symbol: string;
  name: string;
  kind: "crypto" | "stock";
  price: number;
  change: number | null;
  /** Sources that agree on the price / sources queried. */
  agreeing: number;
  total: number;
};

async function json<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status}`);
  return (await res.json()) as T;
}

/** Consolidated prices (server consensus of up to 8 crypto sources and 3 stock sources). */
export async function fetchTicks(): Promise<Tick[]> {
  try {
    const rows = await json<Tick[]>("/api/tickers");
    if (!Array.isArray(rows) || !rows.length) throw new Error("vide");
    return rows;
  } catch {
    // Fallback if the Altim server is unreachable: two public sources queried directly.
    const [binance, gecko] = await Promise.allSettled([
      json<{ symbol: string; lastPrice: string; priceChangePercent: string }[]>(
        `https://api.binance.com/api/v3/ticker/24hr?symbols=${encodeURIComponent(JSON.stringify(COINS.map((c) => c.symbol)))}`,
      ),
      json<Record<string, { usd: number; usd_24h_change: number }>>(
        `https://api.coingecko.com/api/v3/simple/price?ids=${COINS.map((c) => c.gecko).join(",")}&vs_currencies=usd&include_24hr_change=true`,
      ),
    ]);
    return COINS.flatMap((c) => {
      const prices: number[] = [];
      const changes: number[] = [];
      if (binance.status === "fulfilled") {
        const r = binance.value.find((x) => x.symbol === c.symbol);
        if (r) { prices.push(Number(r.lastPrice)); changes.push(Number(r.priceChangePercent)); }
      }
      if (gecko.status === "fulfilled" && gecko.value[c.gecko]) {
        prices.push(gecko.value[c.gecko]!.usd);
        changes.push(gecko.value[c.gecko]!.usd_24h_change);
      }
      if (!prices.length) return [];
      const agree = prices.length === 2 && Math.abs(prices[0]! / prices[1]! - 1) < 0.005 ? 2 : 1;
      return [{
        symbol: c.symbol.replace("USDT", ""), name: c.name, kind: "crypto" as const,
        price: prices[0]!, change: changes[0] ?? null, agreeing: agree, total: 2,
      }];
    });
  }
}

export type Interval = "1h" | "4h" | "1d";

export type SourceStatus = { name: string; ok: boolean; deviation?: number; error?: string };

/** Candles validated by multi-source consensus (Altim server), falling back to Binance directly. */
export async function fetchCandles(
  coin: Coin,
  interval: Interval,
): Promise<{ candles: Candle[]; source: string; sources: SourceStatus[]; agreeing: number }> {
  try {
    const d = await json<{ candles: Candle[]; source: string; sources: SourceStatus[]; agreeing: number }>(
      `/api/candles?base=${coin.symbol.replace("USDT", "")}&interval=${interval}`,
    );
    if (!d.candles?.length) throw new Error("vide");
    return d;
  } catch {
    const rows = await json<(string | number)[][]>(
      `https://api.binance.com/api/v3/klines?symbol=${coin.symbol}&interval=${interval}&limit=500`,
    );
    const now = Date.now();
    const candles = rows
      .filter((r) => Number(r[6]) < now)
      .map((r) => ({
        time: Number(r[0]),
        open: Number(r[1]),
        high: Number(r[2]),
        low: Number(r[3]),
        close: Number(r[4]),
        volume: Number(r[5]),
      }));
    return { candles, source: "Binance", sources: [{ name: "Binance", ok: true }], agreeing: 1 };
  }
}

export function formatPrice(v: number): string {
  const digits = v >= 1 ? 2 : v >= 0.01 ? 4 : 8;
  return v.toLocaleString("fr-FR", { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

export function formatPercent(v: number): string {
  return `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 2, minimumFractionDigits: 2 })} %`;
}

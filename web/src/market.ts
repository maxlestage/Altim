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

export type Tick = { symbol: string; name: string; price: number; change: number };

async function json<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status}`);
  return (await res.json()) as T;
}

/** Cours en direct : Binance, avec repli CoinGecko. */
export async function fetchTicks(): Promise<Tick[]> {
  try {
    const symbols = encodeURIComponent(JSON.stringify(COINS.map((c) => c.symbol)));
    const rows = await json<{ symbol: string; lastPrice: string; priceChangePercent: string }[]>(
      `https://api.binance.com/api/v3/ticker/24hr?symbols=${symbols}`,
    );
    return COINS.map((c) => {
      const r = rows.find((x) => x.symbol === c.symbol);
      return { symbol: c.symbol, name: c.name, price: Number(r?.lastPrice ?? 0), change: Number(r?.priceChangePercent ?? 0) };
    });
  } catch {
    const ids = COINS.map((c) => c.gecko).join(",");
    const data = await json<Record<string, { usd: number; usd_24h_change: number }>>(
      `https://api.coingecko.com/api/v3/simple/price?ids=${ids}&vs_currencies=usd&include_24hr_change=true`,
    );
    return COINS.map((c) => ({
      symbol: c.symbol,
      name: c.name,
      price: data[c.gecko]?.usd ?? 0,
      change: data[c.gecko]?.usd_24h_change ?? 0,
    }));
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

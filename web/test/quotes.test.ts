import { expect, test } from "bun:test";
import { ASSETS, combine, parseQuotes } from "../server/quotes";

// Excerpts of real responses (27/09/2026).
test("parseurs de cours", () => {
  expect(parseQuotes.binance([{ symbol: "BTCUSDT", lastPrice: "84700.10", priceChangePercent: "0.81" }]).get("BTC")).toEqual({ price: 84700.1, change: 0.81 });
  const okx = parseQuotes.okx({ code: "0", data: [{ instId: "BTC-USDT", last: "84699.2", open24h: "84005.8" }, { instId: "BTC-USDC", last: "1", open24h: "1" }] }, ["BTC"]);
  expect(okx.get("BTC")!.price).toBe(84699.2);
  expect(okx.get("BTC")!.change!).toBeCloseTo(0.8254, 3);
  const kraken = parseQuotes.kraken({ error: [], result: { XXBTZUSD: { c: ["84687.40000", "0.1"] }, SOLUSD: { c: ["123.90000", "1"] } } }, ["BTC", "SOL", "BNB"]);
  expect(kraken.get("BTC")!.price).toBe(84687.4);
  expect(kraken.get("SOL")!.price).toBe(123.9);
  expect(kraken.has("BNB")).toBe(false);
  const bfx = parseQuotes.bitfinex([["tBTCUSD", 84623, 2.05, 84638, 1.41, 639, 0.00760741, 84636, 305.5, 84846, 83781]]);
  expect(bfx.get("BTC")).toEqual({ price: 84636, change: 0.760741 });
  expect(parseQuotes.nasdaq({ data: { primaryData: { lastSalePrice: "$341.07", percentageChange: "+1.53%" } } })).toEqual({ price: 341.07, change: 1.53 });
  expect(parseQuotes.cboe({ success: true, details: { current_price: 224.9975, price_change_percent: 0.2177 } })).toEqual({ price: 224.9975, change: 0.2177 });
  const y = parseQuotes.yahoo({ chart: { result: [{ meta: { regularMarketPrice: 341.07 }, indicators: { quote: [{ close: [338.98, null, 335.92, 341.07] }] } }] } });
  expect(y.price).toBe(341.07);
  expect(y.change!).toBeCloseTo(1.5331, 3); // 341,07 / 335,92 − 1
});

test("consensus : médiane, source divergente écartée, source en panne signalée", () => {
  const btc = ASSETS.filter((a) => a.symbol === "BTC");
  const q = (price: number, change?: number) => new Map([["BTC", { price, change }]]);
  const [r] = combine(btc, [
    { name: "A", kind: "crypto", quotes: q(100, 1) },
    { name: "B", kind: "crypto", quotes: q(100.1, 2) },
    { name: "C", kind: "crypto", quotes: q(99.95) },
    { name: "Faux", kind: "crypto", quotes: q(110, 9) },
    { name: "Panne", kind: "crypto", error: "HTTP 451" },
    { name: "Actions", kind: "stock", quotes: q(1) },
  ]);
  expect(r!.agreeing).toBe(3);
  expect(r!.total).toBe(5);
  expect(r!.price).toBe(100);
  expect(r!.change).toBe(1.5);
  expect(r!.sources.find((s) => s.name === "Faux")!.ok).toBe(false);
  expect(r!.sources.find((s) => s.name === "Panne")!.error).toBe("HTTP 451");
});

test("aucun cours : actif omis", () => {
  expect(combine(ASSETS.slice(0, 1), [{ name: "A", kind: "crypto", error: "x" }])).toEqual([]);
});

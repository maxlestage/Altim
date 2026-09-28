/**
 * Live comparison of the asset universe: bun parity/universe.ts <dir>
 * 1. Fetches the raw upstream responses once into <dir>/raw/*.txt (same URLs as web/server/universe.ts), builds the
 *    lists with the TypeScript parsers from them and writes <dir>/ts_from_raw.json.
 * 2. Runs the real cryptoUniverse() / stockUniverse() and writes <dir>/ts_live.json (lists + searches).
 * Then `ALTIM_UNIVERSE_DIR=<dir> cargo test --test universe_live -- --ignored --nocapture` does the same in Rust
 * and reports the differences.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildCrypto, buildStocks, cryptoUniverse, parseUniverse, searchAll, stockUniverse } from "../../web/server/universe";

const dir = process.argv[2] ?? "/tmp/altim-universe";
mkdirSync(join(dir, "raw"), { recursive: true });
const UA = { "User-Agent": "Mozilla/5.0 Altim/1.0", Accept: "application/json, text/plain" };
const SOURCES: Record<string, string> = {
  okx: "https://www.okx.com/api/v5/public/instruments?instType=SPOT",
  coinbase: "https://api.exchange.coinbase.com/products",
  kraken: "https://api.kraken.com/0/public/AssetPairs",
  kucoin: "https://api.kucoin.com/api/v2/symbols",
  gate: "https://api.gateio.ws/api/v4/spot/currency_pairs",
  coinbaseNames: "https://api.exchange.coinbase.com/currencies",
  kucoinNames: "https://api.kucoin.com/api/v3/currencies",
  gecko1: "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=1",
  gecko2: "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=2",
  gecko3: "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=3",
  gecko4: "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=4",
  geckoNames: "https://api.coingecko.com/api/v3/coins/list",
  nasdaq: "https://www.nasdaqtrader.com/dynamic/SymDir/nasdaqlisted.txt",
  other: "https://www.nasdaqtrader.com/dynamic/SymDir/otherlisted.txt",
  screener: "https://api.nasdaq.com/api/screener/stocks?tableonly=true&download=true",
};
const raw: Record<string, string> = {};
for (const [name, url] of Object.entries(SOURCES)) {
  try {
    const res = await fetch(url, { headers: UA, signal: AbortSignal.timeout(30_000) });
    raw[name] = res.ok ? await res.text() : "";
    console.log(name, res.status, raw[name]!.length);
  } catch (e) {
    raw[name] = "";
    console.log(name, "échec", (e as Error).message);
  }
  writeFileSync(join(dir, "raw", `${name}.txt`), raw[name]!);
  if (name.startsWith("gecko")) await new Promise((r) => setTimeout(r, 2500));
}
const json = (name: string) => {
  try {
    return JSON.parse(raw[name]!);
  } catch {
    return null;
  }
};
const safe = <T>(f: () => T, fallback: T): T => {
  try {
    return f();
  } catch {
    return fallback;
  }
};
const gecko = ["gecko1", "gecko2", "gecko3", "gecko4"].flatMap((p) => safe(() => parseUniverse.gecko(json(p)), []));
const crypto = buildCrypto(
  ["okx", "coinbase", "kraken", "kucoin", "gate"].map((n) => safe(() => (parseUniverse as any)[n](json(n)) as string[], [])),
  gecko,
  [...safe(() => parseUniverse.coinbaseNames(json("coinbaseNames")), []), ...safe(() => parseUniverse.kucoinNames(json("kucoinNames")), []), ...safe(() => parseUniverse.geckoNames(json("geckoNames")), [])],
);
const stock = buildStocks(
  [safe(() => parseUniverse.nasdaqDirectory(raw.nasdaq!), []), safe(() => parseUniverse.nasdaqDirectory(raw.other!), [])],
  safe(() => parseUniverse.screener(json("screener")), new Map()),
);
const QUERIES = ["bit", "apple", "nvd"];
const searches = (c: typeof crypto, s: typeof stock) =>
  Object.fromEntries(QUERIES.map((q) => [q, searchAll(c, s, q, 20).map(({ e, kind }) => [kind, ...e])]));
writeFileSync(join(dir, "ts_from_raw.json"), JSON.stringify({ crypto, stock, searches: searches(crypto, stock) }));
console.log("depuis les réponses brutes :", crypto.length, "cryptos,", stock.length, "actions");

const [lc, ls] = await Promise.all([cryptoUniverse().catch(() => []), stockUniverse().catch(() => [])]);
writeFileSync(join(dir, "ts_live.json"), JSON.stringify({ crypto: lc, stock: ls, searches: searches(lc, ls) }));
console.log("en direct :", lc.length, "cryptos,", ls.length, "actions");
process.exit(0);

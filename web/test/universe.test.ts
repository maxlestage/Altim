import { describe, expect, test } from "bun:test";
import { buildCrypto, buildStocks, cleanStockName, parseUniverse, searchUniverse } from "../server/universe";

// Extracts of real responses (formats checked on 27/09/2026).
const NASDAQ_LISTED = `Symbol|Security Name|Market Category|Test Issue|Financial Status|Round Lot Size|ETF|NextShares
AAPL|Apple Inc. - Common Stock|Q|N|N|100|N|N
QQQ|Invesco QQQ Trust, Series 1|G|N|N|100|Y|N
GOOG|Alphabet Inc. - Class C Capital Stock|Q|N|N|100|N|N
ZXZZT|NASDAQ TEST STOCK|G|Y|N|100|N|N
ABCDW|Some Acquisition Corp - Warrants|S|N|N|100|N|N
File Creation Time: 0925202621:31|||||||`;

const OTHER_LISTED = `ACT Symbol|Security Name|Exchange|CQS Symbol|ETF|Round Lot Size|Test Issue|NASDAQ Symbol
BRK.B|Berkshire Hathaway Inc. New Common Stock|N|BRK.B|N|100|N|BRK.B
BFH$A|Bread Financial Holdings, Inc. Depositary Shares 8.625% Preferred Stock|N|BFHpA|N|100|N|BFH-A
SPY|State Street SPDR S&P 500 ETF Trust|P|SPY|Y|100|N|SPY
TNL|Travel   Leisure Co. Common  Stock|N|TNL|N|100|N|TNL
File Creation Time: 0925202621:31||||||`;

describe("stock universe", () => {
  const nasdaq = parseUniverse.nasdaqDirectory(NASDAQ_LISTED);
  const other = parseUniverse.nasdaqDirectory(OTHER_LISTED);

  test("keeps stocks and ETF, drops test issues, warrants and preferred shares", () => {
    expect(nasdaq.map((s) => s.symbol)).toEqual(["AAPL", "QQQ", "GOOG"]);
    expect(other.map((s) => s.symbol)).toEqual(["BRK-B", "SPY", "TNL"]);
    expect(nasdaq.find((s) => s.symbol === "QQQ")!.etf).toBe(true);
  });

  test("class shares use the Yahoo format and names are cleaned", () => {
    expect(other[0]!.symbol).toBe("BRK-B");
    expect(cleanStockName("Apple Inc. - Common Stock")).toBe("Apple Inc.");
    expect(cleanStockName("Alphabet Inc. - Class A Common Stock")).toBe("Alphabet Inc. Class A");
    expect(nasdaq.find((s) => s.symbol === "GOOG")!.name).toBe("Alphabet Inc. Class C");
    expect(other.find((s) => s.symbol === "TNL")!.name).toBe("Travel Leisure Co.");
    expect(cleanStockName("Invesco QQQ Trust, Series 1")).toBe("Invesco QQQ Trust, Series 1");
  });

  test("ranked by market capitalisation (screener), then alphabetical", () => {
    const caps = parseUniverse.screener({ data: { rows: [
      { symbol: "AAPL", marketCap: "3,500,000,000,000" },
      { symbol: "BRK/B", marketCap: "1000000000000.00" },
      { symbol: "GOOG", marketCap: "2000000000000" },
    ] } });
    const list = buildStocks([nasdaq, other], caps);
    expect(list.slice(0, 3).map((e) => [e[0], e[2]])).toEqual([["AAPL", 1], ["GOOG", 2], ["BRK-B", 3]]);
    // Unranked: the largest ETFs first (SPY before QQQ by assets), then alphabetical.
    expect(list.slice(3).map((e) => e[0])).toEqual(["SPY", "QQQ", "TNL"]);
    expect(list.find((e) => e[0] === "SPY")![3]).toBe(1);
  });
});

describe("crypto universe", () => {
  const okx = parseUniverse.okx({ code: "0", data: [
    { instId: "BTC-USDT", baseCcy: "BTC", quoteCcy: "USDT", state: "live" },
    { instId: "ETH-USDC", baseCcy: "ETH", quoteCcy: "USDC", state: "live" },
    { instId: "PEPE-USDT", baseCcy: "PEPE", quoteCcy: "USDT", state: "live" },
    { instId: "OLD-USDT", baseCcy: "OLD", quoteCcy: "USDT", state: "suspend" },
  ] });
  const coinbase = parseUniverse.coinbase([
    { base_currency: "BTC", quote_currency: "USD", status: "online", trading_disabled: false },
    { base_currency: "ETH", quote_currency: "USD", status: "online", trading_disabled: false },
    { base_currency: "USDT", quote_currency: "USD", status: "online", trading_disabled: false },
    { base_currency: "DEAD", quote_currency: "USD", status: "delisted", trading_disabled: true },
  ]);
  const kraken = parseUniverse.kraken({ error: [], result: {
    XXBTZUSD: { wsname: "XBT/USD" }, XDGUSD: { wsname: "XDG/USD" }, ETHEUR: { wsname: "ETH/EUR" },
  } });
  const gate = parseUniverse.gate([
    { base: "BTC3L", quote: "USDT", trade_status: "tradable" },
    { base: "PEPE", quote: "USDT", trade_status: "tradable" },
    { base: "NEWCOIN", quote: "USDT", trade_status: "tradable" },
  ]);

  test("parsers keep only active pairs against USD / USDT", () => {
    expect(okx).toEqual(["BTC", "PEPE"]);
    expect(coinbase).toEqual(["BTC", "ETH", "USDT"]);
    expect(kraken).toEqual(["BTC", "DOGE"]);
  });

  test("union of exchanges, without stablecoins nor leveraged tokens, ranked by market cap", () => {
    const gecko = parseUniverse.gecko([
      { symbol: "btc", name: "Bitcoin", market_cap_rank: 1 },
      { symbol: "eth", name: "Ethereum", market_cap_rank: 2 },
      { symbol: "doge", name: "Dogecoin", market_cap_rank: 9 },
      { symbol: "pepe", name: "Pepe", market_cap_rank: 30 },
      { symbol: "pepe", name: "Pepe copycat", market_cap_rank: 900 },
    ]);
    const list = buildCrypto([okx, coinbase, kraken, gate], gecko, [["NEWCOIN", "New Coin"]]);
    expect(list.map((e) => e[0])).toEqual(["BTC", "ETH", "DOGE", "PEPE", "NEWCOIN"]);
    expect(list[0]).toEqual(["BTC", "Bitcoin", 1, 3]);
    expect(list.find((e) => e[0] === "PEPE")![1]).toBe("Pepe");
    expect(list.find((e) => e[0] === "NEWCOIN")).toEqual(["NEWCOIN", "New Coin", 0, 1]);
  });
});

describe("search", () => {
  const list = buildStocks(
    [parseUniverse.nasdaqDirectory(NASDAQ_LISTED), parseUniverse.nasdaqDirectory(OTHER_LISTED)],
    new Map([["AAPL", 3e12], ["SPY", 6e11]]),
  );

  test("exact symbol, then symbol prefix, then name; accents ignored", () => {
    expect(searchUniverse(list, "spy").map((e) => e[0])).toEqual(["SPY"]);
    expect(searchUniverse(list, "apple")[0]![0]).toBe("AAPL");
    expect(searchUniverse(list, "berkshire")[0]![0]).toBe("BRK-B");
    expect(searchUniverse(list, "BRK")[0]![0]).toBe("BRK-B");
    expect(searchUniverse(list, "tràvel")[0]![0]).toBe("TNL");
  });

  test("empty query returns the whole ranked list; special characters are safe", () => {
    expect(searchUniverse(list, "", 100).length).toBe(list.length);
    expect(searchUniverse(list, "S&P (500")).toEqual([]);
    expect(searchUniverse(list, "S&P 500")[0]![0]).toBe("SPY");
  });
});

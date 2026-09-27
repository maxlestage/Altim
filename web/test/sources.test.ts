import { expect, test } from "bun:test";
import { crossCheck, nyOpen, parse, parseStock, SOURCES, STOCK_SOURCES } from "../server/market";
import { parseQuotes } from "../server/quotes";

// Real responses (27/09/2026), same samples as MoreSourcesTests.swift.
const bitstamp = {"data": {"pair": "BTC/USD", "ohlc": [{"timestamp": "1790503200", "open": "84918.88", "high": "85089.89", "low": "84791.70", "close": "84865.17", "volume": "26.56202532"}, {"timestamp": "1790506800", "open": "84858.10", "high": "84934.09", "low": "84819.11", "close": "84842.79", "volume": "7.22360728"}]}};
const gemini = [[1790503200000, 84912.04, 85085.62, 84791.47, 84871.45, 0.85068258], [1790499600000, 84778.14, 84934.12, 84590.71, 84912.04, 0.28386706]];
const cryptocom = {"id": -1, "method": "public/get-candlestick", "code": 0, "result": {"interval": "1h", "data": [{"o": "84954.30", "h": "85129.60", "l": "84794.51", "c": "84884.22", "v": "30.51772", "t": 1790503200000}, {"o": "84876.74", "h": "84952.93", "l": "84837.01", "c": "84863.74", "v": "11.38151", "t": 1790506800000}], "instrument_name": "BTC_USDT"}};
const bitget = {"code":"00000","msg":"success","requestTime":1790508916710,"data":[["1790503200000","84955.72","85120","84800","84880.21","92.958017","7897444.53188482","7897444.53188482"],["1790506800000","84880.21","84946.41","84838.01","84860.67","17.608917","1494815.2912817","1494815.2912817"]]};
const mexc = [[1790503200000,"84936.99","85080","84789.64","84867.09","276.41754731",1790506800000,"23478691.55"],[1790506800000,"84867.09","84939.97","84833.96","84849.99","88.52018559",1790510400000,"7513489.77"]];
const htx = {"ch":"market.btcusdt.kline.60min","status":"ok","ts":1790508917481,"data":[{"id":1790506800,"open":84860.22,"close":84857.99,"low":84832.26,"high":84937.44,"amount":156.82517159741286,"vol":1.330886168618912E7,"count":231},{"id":1790503200,"open":84890.81,"close":84860.22,"low":84777.56,"high":85065.26,"amount":306.5102188604617,"vol":2.6017566588014267E7,"count":763}]};
const rh_ = {"quote": "https://api.robinhood.com/quotes/450dfc6d-5510-4d40-abfb-f633b7d9be3e/", "symbol": "AAPL", "interval": "day", "span": "week", "bounds": "regular", "instrument": "https://api.robinhood.com/instruments/450dfc6d-5510-4d40-abfb-f633b7d9be3e/", "historicals": [{"begins_at": "2026-09-24T00:00:00Z", "open_price": "336.720000", "close_price": "335.920000", "high_price": "338.910000", "low_price": "334.300000", "volume": 24733098, "session": "reg", "interpolated": false}, {"begins_at": "2026-09-25T00:00:00Z", "open_price": "336.040000", "close_price": "341.070000", "high_price": "341.670000", "low_price": "334.530000", "volume": 30002507, "session": "reg", "interpolated": false}], "InstrumentID": "450dfc6d-5510-4d40-abfb-f633b7d9be3e"};
const cboe = {"timestamp": "11:00:23", "data": [{"date": "2026-09-23", "volume": 31658823, "open": 340.93, "high": 341.8, "low": 335.5, "close": 337.02}, {"date": "2026-09-24", "volume": 24733098, "open": 336.65, "high": 338.91, "low": 334.3, "close": 335.92}], "symbol": "AAPL"};
const rhq = {"results": [{"symbol": "AAPL", "last_trade_price": "341.040000", "previous_close": "335.920000", "adjusted_previous_close": "335.920000"}, {"symbol": "BRK.B", "last_trade_price": "505.430000", "previous_close": "505.180000", "adjusted_previous_close": "505.180000"}, null]};

const t0 = 1_790_503_200_000, t1 = 1_790_506_800_000;

test("additional crypto exchanges: parsed oldest first, same timestamps", () => {
  expect(parse.bitstamp(bitstamp).map((c) => c.time)).toEqual([t0, t1]);
  expect(parse.bitstamp(bitstamp)[0]!.close).toBe(84865.17);
  expect(parse.gemini(gemini).map((c) => c.time)).toEqual([1_790_499_600_000, t0]);
  expect(parse.gemini(gemini)[1]!.close).toBe(84871.45);
  expect(parse.cryptocom(cryptocom).map((c) => c.time)).toEqual([t0, t1]);
  expect(parse.cryptocom(cryptocom)[0]!.close).toBe(84884.22);
  expect(parse.bitget(bitget).map((c) => c.time)).toEqual([t0, t1]);
  expect(parse.bitget(bitget)[0]).toMatchObject({ open: 84955.72, close: 84880.21 });
  expect(parse.binance(mexc).map((c) => c.time)).toEqual([t0, t1]);
  expect(parse.binance(mexc)[1]!.close).toBe(84849.99);
  expect(parse.htx(htx).map((c) => c.time)).toEqual([t0, t1]);
  expect(parse.htx(htx)[0]).toMatchObject({ close: 84860.22, high: 85065.26 });
  expect(() => parse.bitget({ code: "40034", data: null })).toThrow();
  expect(() => parse.htx({ status: "error" })).toThrow();
});

test("every crypto exchange is queried; HTX is intraday only", () => {
  expect(SOURCES.length).toBeGreaterThanOrEqual(14);
  expect(new Set(SOURCES.map((s) => s.name)).size).toBe(SOURCES.length);
  expect(SOURCES.find((s) => s.name === "HTX")!.supports!("1d")).toBe(false);
});

test("stock sources: Robinhood and Cboe daily candles on the New York session", () => {
  const rh = parseStock.robinhood(rh_);
  expect(rh.map((c) => c.time)).toEqual([nyOpen(2026, 9, 24), nyOpen(2026, 9, 25)]);
  expect(rh[1]!.close).toBe(341.07);
  const cb = parseStock.cboe(cboe);
  expect(cb[cb.length - 1]).toMatchObject({ time: nyOpen(2026, 9, 24), close: 335.92 });
  expect(rh[0]!.close).toBe(cb[cb.length - 1]!.close);
  expect(STOCK_SOURCES.map((s) => s.name)).toEqual(["Yahoo Finance", "Nasdaq", "Robinhood", "Cboe"]);
});

test("stock quotes: Robinhood (null for unknown symbols) and TradingView", () => {
  const q = parseQuotes.robinhood(rhq);
  expect(q.get("AAPL")!.price).toBe(341.04);
  expect(q.get("BRK-B")!.price).toBe(505.43);
  expect(q.size).toBe(2);
  const tv = parseQuotes.tradingview({ data: [{ s: "NYSE:BRK.B", d: ["BRK.B", "NYSE", 505.48, 0.0594] }, { s: "CBOE:BRK.B", d: ["BRK.B", "CBOE", 505.5, 0.06] }] });
  expect(tv.get("BRK-B")).toEqual({ price: 505.48, change: 0.0594 });
});

test("intraday cross-check with live prices: a provider already counted is not added twice", () => {
  const checks = crossCheck(100, [
    { name: "Yahoo Finance", ok: true, price: 100.1 },
    { name: "Nasdaq", ok: true, price: 101 },
    { name: "Cboe", ok: true, price: 105 },
    { name: "TradingView", ok: false, error: "non coté" },
  ], ["Yahoo Finance"], 2);
  expect(checks.map((c) => [c.name, c.ok])).toEqual([["Nasdaq (cours)", true], ["Cboe (cours)", false], ["TradingView (cours)", false]]);
});

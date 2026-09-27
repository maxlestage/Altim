import { expect, test } from "bun:test";
import { consensus, crossCheck, nyOpen, parse, parseStock, SOURCES, STOCK_SOURCES } from "../server/market";
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

// More sources (27/09/2026): same samples as MoreSourcesTests.swift.
const s_poloniex = [["84613.15","85142.85","84792.69","85022.91","18080857.16","213.12316","9299681.9","109.620007",13568,1790514000000,"84837.77","HOUR_1",1790514000000,1790517599999],["84637.88","85059.57","85020.91","84748.61","7709537.27","90.873482","3915747.99","46.158834",6379,1790517600000,"84838.3","HOUR_1",1790517600000,1790521199999]];
const s_hitbtc = [{"timestamp":"2026-09-27T14:00:00.000Z","open":"85037.17","close":"84784.83","min":"84675.11","max":"85037.17","volume":"8.82692","volume_quote":"748673.9565532"},{"timestamp":"2026-09-27T13:00:00.000Z","open":"84760.80","close":"84981.72","min":"84661.02","max":"85080.12","volume":"20.86364","volume_quote":"1771753.1072592"}];
const s_whitebit = {"success":true,"message":null,"result":[[1790510400,"84892.49","84804.13","85034.95","84800","7.470855","634848.86257627"],[1790514000,"84804.13","85026.13","85145.5","84636.72","23.682812","2009009.15098621"]]};
const s_coinex = {"code":0,"data":[{"close":"84899","created_at":1790514000000,"high":"85120","low":"84630","market":"BTCUSDT","open":"84770","value":"118935.71389611","volume":"1.40216354"},{"close":"84713","created_at":1790517600000,"high":"84900","low":"84703","market":"BTCUSDT","open":"84899","value":"7723.23455732","volume":"0.09116916"}],"message":"OK"};
const s_xt = {"rc":0,"mc":"SUCCESS","ma":[],"result":[{"t":1790517600000,"o":"85030.41","c":"84762.00","h":"85048.14","l":"84674.00","q":"97.46926","v":"8268632.5688551"},{"t":1790514000000,"o":"84802.01","c":"85030.41","h":"85159.02","l":"84637.73","q":"163.52720","v":"13876531.0387037"}]};
const s_woox = {"success":true,"rows":[{"open":85034.01,"close":84764.17,"low":84674.00,"high":85048.14,"volume":390.685282,"amount":33157857.12302202,"symbol":"SPOT_BTC_USDT","type":"1h","start_timestamp":1790517600000,"end_timestamp":1790521200000},{"open":84802.02,"close":85030.42,"low":84637.73,"high":85154.01,"volume":533.718607,"amount":45287344.72708746,"symbol":"SPOT_BTC_USDT","type":"1h","start_timestamp":1790514000000,"end_timestamp":1790517600000}]};
const s_bingx = {"code":0,"timestamp":1790520282511,"data":[[1790517600000,85030.44,85048.13,84682.42,84765.97,52.282931,1790521199999,4436187.75],[1790514000000,84802.02,85150.83,84644.26,85030.43,82.368202,1790517599999,6989644.06]]};
const s_lbank = {"msg":"Success","result":"true","data":[[1790506800,84874.41,84942.35,84836.42,84891.72,65.15268],[1790510400,84891.72,85031.12,84796.32,84796.62,81.55706]],"error_code":0,"ts":1790520283501};
const s_sa = {"status": 200, "data": [{"t": "2026-09-25", "o": 336.04, "h": 341.67, "l": 334.53, "c": 341.07, "a": 341.07, "v": 29407559, "ch": 1.53}, {"t": "2026-09-24", "o": 336.72, "h": 338.91, "l": 334.3, "c": 335.92, "a": 335.92, "v": 24236309, "ch": -0.33}]};
const s_wbchart = [{"tickerId": 913256135, "data": ["1790308800,336.04,341.07,341.67,334.53,335.92,30002507,339.30", "1790222400,336.72,335.92,338.91,334.30,337.02,24733098,336.74"]}];
const s_wbquote = {"tickerId": 913256135, "symbol": "AAPL", "close": "341.07", "preClose": "335.92", "changeRatio": "0.0153"};
const s_wbsearch = {"data": [{"tickerId": 916040668, "symbol": "BRK B", "disSymbol": "BRK-B", "regionCode": "US"}]};
const s_zacks = {"AAPL": {"last": "341.07", "percent_net_change": "1.53310311979042629197427959037866158609", "previous_close": "341.07"}};

test("8 plateformes crypto supplémentaires : même bougie 13:00 UTC, du plus ancien au plus récent", () => {
  const T0 = 1_790_514_000_000, T1 = 1_790_517_600_000;
  const close = (c: { time: number; close: number }[], t: number) => c.find((x) => x.time === t)!.close;
  expect(parse.poloniex(s_poloniex).map((c) => c.time)).toEqual([T0, T1]);
  expect(parse.poloniex(s_poloniex)[0]).toMatchObject({ open: 84792.69, high: 85142.85, low: 84613.15, close: 85022.91 });
  expect(parse.hitbtc(s_hitbtc).map((c) => c.time)).toEqual([T0, T1]);
  expect(parse.hitbtc(s_hitbtc)[1]).toMatchObject({ high: 85037.17, low: 84675.11, close: 84784.83 });
  expect(close(parse.whitebit(s_whitebit), T0)).toBe(85026.13);
  expect(parse.coinex(s_coinex)[0]).toMatchObject({ time: T0, open: 84770, high: 85120, low: 84630, close: 84899 });
  expect(parse.xt(s_xt).map((c) => c.time)).toEqual([T0, T1]);
  expect(parse.xt(s_xt)[0]).toMatchObject({ open: 84802.01, high: 85159.02, low: 84637.73, close: 85030.41 });
  expect(parse.woox(s_woox).map((c) => c.time)).toEqual([T0, T1]);
  expect(close(parse.woox(s_woox), T0)).toBe(85030.42);
  expect(parse.bingx(s_bingx).map((c) => c.time)).toEqual([T0, T1]);
  expect(parse.bingx(s_bingx)[1]).toMatchObject({ low: 84682.42, close: 84765.97 });
  expect(parse.lbank(s_lbank).map((c) => c.close)).toEqual([84891.72, 84796.62]);
  // Same 13:00 candle, all within 0.02 % of each other.
  const closes = [parse.poloniex(s_poloniex), parse.coinex(s_coinex), parse.xt(s_xt), parse.woox(s_woox), parse.bingx(s_bingx), parse.whitebit(s_whitebit)].map((c) => close(c, T0));
  expect(Math.max(...closes) / Math.min(...closes) - 1).toBeLessThan(0.002);
  expect(() => parse.coinex({ code: 3008, data: [] })).toThrow();
  expect(() => parse.xt({ rc: 1, result: [] })).toThrow();
  expect(SOURCES.length).toBe(22);
  expect(SOURCES.filter((s) => s.supports && !s.supports("1d")).map((s) => s.name)).toEqual(["HTX", "BingX", "LBank"]);
});

test("actions : StockAnalysis et Webull (journalier), Webull et Zacks (cours)", () => {
  const sa = parseStock.stockanalysis(s_sa), wb = parseStock.webull(s_wbchart);
  expect(sa.map((c) => c.time)).toEqual([nyOpen(2026, 9, 24), nyOpen(2026, 9, 25)]);
  expect(wb.map((c) => c.time)).toEqual([nyOpen(2026, 9, 24), nyOpen(2026, 9, 25)]);
  expect(sa[1]).toMatchObject({ open: 336.04, high: 341.67, low: 334.53, close: 341.07 });
  expect(wb[1]).toMatchObject({ open: 336.04, high: 341.67, low: 334.53, close: 341.07 });
  expect(parseQuotes.webull(s_wbquote)).toEqual({ price: 341.07, change: 1.53 });
  expect(parseQuotes.webullTicker(s_wbsearch, "BRK-B")).toBe(916040668);
  expect(parseQuotes.webullTicker(s_wbsearch, "BRK-A")).toBeNull();
  expect(parseQuotes.zacks(s_zacks).get("AAPL")!.price).toBe(341.07);
  expect(STOCK_SOURCES.map((s) => s.name)).toEqual(["Yahoo Finance", "Nasdaq", "Robinhood", "StockAnalysis", "Webull", "Cboe"]);
});

test("une source en retard (paire inactive) est écartée même si ses vieilles bougies concordent", async () => {
  const H = 3_600_000;
  const serie = (n: number, f = 1) => Array.from({ length: n }, (_, i) => ({ time: i * H, open: 100 * f, high: 101 * f, low: 99 * f, close: (100 + (i % 7)) * f, volume: 1 }));
  const src = (name: string, c: ReturnType<typeof serie>) => ({ name, fetch: async () => c });
  const r = await consensus("PEPE", "1h", [src("A", serie(200)), src("B", serie(200, 1.0001)), src("Lente", serie(150))]);
  const late = r.sources.find((s) => s.name === "Lente")!;
  expect(late.ok).toBe(false);
  expect(late.error).toContain("en retard");
  expect(r.agreeing).toBe(2);
});

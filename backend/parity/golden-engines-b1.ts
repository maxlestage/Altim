/**
 * Golden outputs of fibonacci.ts, alerts.ts (+ formatPrice / formatPercent) and screener.ts, checked by
 * tests/parity_b1.rs. Imported by golden.ts.
 */
import { inputs, find, write } from "./golden";
import { EXTENSIONS, HORIZONS, RATIOS, fibZone, fibZones, level, swingAt, weekly, zoneEvidence, zoneState, type Horizon, type Swing } from "../../web/src/engine/fibonacci";
import { buyAlert, type AlertInput } from "../../web/src/engine/alerts";
import { formatPercent, formatPrice } from "../../web/src/market";
import {
  alignSeries, explain, factorsAt, isPegged, pick, ranks, roles, scoreUniverse, span, toHorizon, validate,
  CRITERIA, HORIZON_LABEL, HORIZON_LIST, RANKED_CRITERION, SPECS, type Market, type RankRule, type Spec,
} from "../../web/src/engine/screener";
import { atr } from "../../web/src/engine/signal";

const SYMBOLS = [...new Set(inputs.map((i) => i.symbol))];
const HZ: Horizon[] = ["short", "medium", "long"];

// ---------- fibonacci ----------
{
  write("fib-consts", [{ args: {}, output: { RATIOS, EXTENSIONS, HORIZONS } }]);

  const weeks = [];
  for (const i of inputs.filter((x) => x.interval === "1d" || x.interval === "long")) {
    weeks.push({ args: { symbol: i.symbol, interval: i.interval }, output: weekly(i.candles) });
  }
  weeks.push({ args: { symbol: null, interval: null }, output: weekly([]) });
  write("fib-weekly", weeks);

  // swingAt on every 7th candle, with each horizon's settings.
  const swings = [];
  for (const i of inputs) {
    const a = atr(i.candles);
    for (const h of HZ) {
      const { window, minAtr } = HORIZONS[h];
      const out = [];
      for (let u = 0; u < i.candles.length; u += 7) out.push(swingAt(i.candles, u, window, minAtr, a));
      swings.push({ args: { symbol: i.symbol, interval: i.interval, horizon: h }, output: out });
    }
  }
  write("fib-swing", swings);

  const evid = [];
  for (const i of inputs) for (const h of HZ) evid.push({ args: { symbol: i.symbol, interval: i.interval, horizon: h }, output: zoneEvidence(i.candles, h) });
  write("fib-evidence", evid);

  // zoneState over a grid of prices, on a large, a small and a tiny move (prices < 1 → significant digits).
  const states = [];
  const moves: Swing[] = [
    { trend: "up", low: 100, high: 200, lowIndex: 0, highIndex: 10, lowTime: 0, highTime: 1 },
    { trend: "up", low: 58_123.456, high: 71_987.1, lowIndex: 3, highIndex: 40, lowTime: 0, highTime: 1 },
    { trend: "up", low: 0.0712345, high: 0.1298765, lowIndex: 3, highIndex: 40, lowTime: 0, highTime: 1 },
  ];
  for (const s of moves) {
    for (let k = -0.2; k <= 1.3; k += 0.0371) {
      const p = s.low + k * (s.high - s.low);
      states.push({ args: { swing: s, price: p }, output: { state: zoneState(s, p), levels: RATIOS.map((r) => level(s, r)) } });
    }
  }
  write("fib-state", states);

  // Zones of each input as one horizon, with cuts (different statuses), and the three horizons of each symbol.
  const zones = [];
  for (const i of inputs) {
    for (const h of HZ) {
      for (const cut of [0, 30, 90]) {
        const candles = i.candles.slice(0, Math.max(0, i.candles.length - cut));
        zones.push({ args: { symbol: i.symbol, interval: i.interval, horizon: h, cut, price: null, evidence: true }, output: fibZone(candles, h, undefined, true) });
      }
      const last = i.candles.at(-1)!.close;
      for (const f of [0.8, 0.93, 1.1]) {
        zones.push({ args: { symbol: i.symbol, interval: i.interval, horizon: h, cut: 0, price: last * f, evidence: false }, output: fibZone(i.candles, h, last * f, false) });
      }
    }
  }
  zones.push({ args: { symbol: null, interval: null, horizon: "short", cut: 0, price: null, evidence: true }, output: fibZone([], "short") });
  write("fib-zone", zones);

  const all = [];
  for (const s of SYMBOLS) {
    const h4 = find(s, "4h")!.candles, d = find(s, "1d")!.candles, l = find(s, "long")!.candles;
    all.push({ args: { symbol: s, price: null, evidence: true, long: true }, output: fibZones(h4, d, l, undefined, true) });
    all.push({ args: { symbol: s, price: d.at(-1)!.close * 0.95, evidence: false, long: true }, output: fibZones(h4, d, l, d.at(-1)!.close * 0.95, false) });
    all.push({ args: { symbol: s, price: null, evidence: true, long: false }, output: fibZones(h4, d, [], null, true) });
  }
  write("fib-zones", all);
}

// ---------- alerts ----------
{
  const values = [0, 0.000012345, 0.0012345, 0.0099999, 0.01, 0.0543219, 0.99999, 1, 1.005, 12.345, 999.995, 72_000, 1_234_567.891, -3.14159, -0.004];
  write("format", values.map((v) => ({ args: { v }, output: { price: formatPrice(v), percent: formatPercent(v) } })));

  const cases = [];
  // Zones from the real data of each symbol, then every combination of the other inputs (deterministic subset).
  const real = SYMBOLS.map((s) => ({ s, z: fibZones(find(s, "4h")!.candles, find(s, "1d")!.candles, find(s, "long")!.candles, undefined, false) }));
  const synthetic = (status: string, horizon: Horizon, label: string, px: number) => ({
    horizon, label, status: status as never, zone: { from: px * 0.9, to: px * 1.05 }, golden: { from: px * 0.9, to: px * 0.92 }, invalidation: px * 0.8,
  });
  const zoneSets: AlertInput["zones"][] = [
    ...real.map((r) => r.z.map(({ horizon, label, status, zone, golden, invalidation }) => ({ horizon, label, status, zone, golden, invalidation }))),
    [],
    [synthetic("inZone", "short", "Court terme", 72_000), synthetic("golden", "medium", "Moyen terme", 72_000)],
    [synthetic("golden", "long", "Long terme", 0.05), synthetic("inZone", "medium", "Moyen terme", 0.05)],
    [{ ...synthetic("golden", "medium", "Moyen terme", 3), golden: null }],
    [{ ...synthetic("inZone", "short", "Court terme", 3), invalidation: null }],
    [{ ...synthetic("inZone", "short", "Court terme", 3), zone: null }, synthetic("deep", "long", "Long terme", 3)],
  ];
  const signals: AlertInput["signal"][] = [null, { action: "hold", confidence: 40 }, { action: "buy", confidence: 54.5 }, { action: "strongBuy", confidence: 70.49 }, { action: "sell", confidence: 60 }];
  const rels: AlertInput["reliability"][] = [null, "high", "medium", "low"];
  const shocks: AlertInput["shock"][] = [null, "calm", "agitated", "shock"];
  const trends: AlertInput["trend"][] = [null, "up", "down", "range"];
  const macros: AlertInput["macro"][] = [null, "calm", "tense", "high"];
  let k = 0;
  for (const [zi, zones] of zoneSets.entries()) {
    const ref = zones.find((z) => z.invalidation != null)?.invalidation ?? 72_000;
    for (const price of [null, ref * 1.3, ref * 0.99, 0.004321]) {
      for (const signal of signals) {
        const input: AlertInput = {
          symbol: zi < real.length ? real[zi]!.s : "TEST", name: "Actif", price, signal, zones,
          reliability: rels[k % 4]!, shock: shocks[(k >> 1) % 4]!, trend: trends[(k >> 2) % 4]!, macro: macros[(k * 3 + 1) % 4]!,
        };
        k++;
        cases.push({ args: input, output: buyAlert(input) });
      }
    }
  }
  write("alerts", cases);
}

// ---------- screener ----------
{
  const MARKETS: Market[] = ["stock", "crypto"];
  write("screener-consts", [{
    args: {},
    output: {
      HORIZON_LIST, HORIZON_LABEL, CRITERIA, RANKED_CRITERION, SPECS,
      roles: MARKETS.map((m) => HORIZON_LIST.map((h) => roles(SPECS[m][h], m))),
      toHorizon: MARKETS.map((m) => ["30m", "1h", "5h", "7d", "14d", "1m", "3m", "6m", "short", "medium", "long", "2y", ""].map((h) => toHorizon(h, m))),
      span: MARKETS.map((m) => (["5m", "15m", "30m", "1d"] as const).map((iv) => [1, 4, 6, 7, 10, 11, 12, 20, 21, 29, 30, 45, 63, 78, 90, 126, 180].map((n) => span(n, iv, m)))),
      isPegged: (["5m", "15m", "30m", "1d"] as const).map((iv) => [null, 0, 0.01, 0.05, 0.1, 0.3, 0.49, 0.5, 2.5].map((a) => isPegged(a, iv))),
      ranks: [
        ranks([10, 30, 20, null], true), ranks([10, 30, 20], false), ranks([5, 5, 5], true), ranks([null, 3], true), ranks([], true),
        ranks([1, 2, 2, 3, NaN, Infinity, null, -4], true), ranks([NaN, 1], false),
      ],
    },
  }]);

  // factorsAt on each input at several dates, with each spec shape.
  const specs: [string, Spec][] = [
    ["stock/3m", SPECS.stock["3m"]], ["stock/6m", SPECS.stock["6m"]], ["crypto/7d", SPECS.crypto["7d"]],
    ["stock/30m", SPECS.stock["30m"]], ["crypto/30m", SPECS.crypto["30m"]], ["crypto/6m", SPECS.crypto["6m"]],
  ];
  const facts = [];
  for (const i of inputs) {
    for (const [name, spec] of specs) {
      for (const back of [0, 5, 60, 200]) {
        const at = i.candles.length - 1 - back;
        facts.push({ args: { symbol: i.symbol, interval: i.interval, spec: name, i: at }, output: factorsAt(i.candles, at, spec) });
      }
    }
  }
  write("screener-factors", facts);

  // scoreUniverse + explain, one universe per interval (7 assets, one missing), every spec of both markets.
  const universes = [];
  for (const interval of ["1h", "4h", "1d", "long"]) {
    for (const m of MARKETS) {
      for (const h of HORIZON_LIST) {
        const spec = SPECS[m][h];
        const list = SYMBOLS.map((s) => {
          const c = find(s, interval)!.candles;
          return s === "DOGE" && interval === "1d" ? null : factorsAt(c, c.length - 1, spec);
        });
        const sc = scoreUniverse(list, spec);
        const why = list.map((f, k) => (f && sc[k] ? explain(f, sc[k]!, spec, m) : null));
        universes.push({ args: { interval, market: m, horizon: h }, output: { list, sc, why } });
      }
    }
  }
  // A rule on an asset whose momentum is unknown (too short for the window).
  {
    const spec = { ...SPECS.stock["3m"], momLen: 1000 };
    const list = SYMBOLS.map((s) => factorsAt(find(s, "long")!.candles, find(s, "long")!.candles.length - 1, spec));
    universes.push({ args: { interval: "long", market: "stock", horizon: "momLen1000" }, output: { list, sc: scoreUniverse(list, spec), why: null } });
  }
  write("screener-universe", universes);

  const items = [
    { s: "A", v: 99, sec: "Tech" }, { s: "B", v: 98, sec: "Tech" }, { s: "C", v: 97, sec: "Tech" }, { s: "D", v: 96, sec: "Tech" },
    { s: "E", v: 95, sec: "Santé" }, { s: "F", v: null, sec: "Santé" }, { s: "G", v: 10, sec: "Énergie" }, { s: "H", v: 95, sec: "" }, { s: "I", v: 97, sec: "Tech" },
  ];
  write("screener-pick", [1, 2, 3, 5, 7, 20].flatMap((n) => [
    { args: { n, sectors: true }, output: pick(items, (x) => x.v, (x) => x.sec, n).map((x) => x.s) },
    { args: { n, sectors: false }, output: pick(items, (x) => x.v, () => undefined, n).map((x) => x.s) },
  ]));

  // alignSeries and validate (replay) on the real long daily histories, and on the hourly crypto series.
  const align = [];
  const longs = SYMBOLS.map((s) => find(s, "long")!.candles);
  const aligned = alignSeries(longs);
  align.push({ args: { interval: "long", intraday: false }, output: aligned.map((s) => s.map((c) => c.time)) });
  const cryptoH = ["BTC", "ETH", "SOL", "DOGE"].map((s) => find(s, "1h")!.candles);
  const alignedH = alignSeries(cryptoH, true);
  align.push({ args: { interval: "1h", intraday: true }, output: alignedH.map((s) => s.map((c) => c.time)) });
  align.push({ args: { interval: "1h+1d", intraday: false }, output: alignSeries([find("BTC", "1h")!.candles, find("AAPL", "1h")!.candles]).map((s) => s.map((c) => c.time)) });
  write("screener-align", align);

  const sectors = ["A", "A", "B", "B", "B", "C", "C"];
  const val = [];
  const runs: { name: string; spec: Spec; h: string; topN: number; sectors: boolean; bench: number | null }[] = [
    { name: "stock/1m", spec: SPECS.stock["1m"], h: "1m", topN: 3, sectors: false, bench: null },
    { name: "stock/1m", spec: SPECS.stock["1m"], h: "1m", topN: 3, sectors: true, bench: 0 },
    { name: "crypto/7d", spec: SPECS.crypto["7d"], h: "7d", topN: 2, sectors: false, bench: 0 },
    { name: "crypto/1m", spec: SPECS.crypto["1m"], h: "1m", topN: 3, sectors: false, bench: 0 },
    { name: "crypto/6m", spec: SPECS.crypto["6m"], h: "6m", topN: 3, sectors: false, bench: null },
    { name: "stock/6m", spec: SPECS.stock["6m"], h: "6m", topN: 4, sectors: true, bench: null },
    { name: "stock/1m-reversal", spec: { ...SPECS.stock["1m"], rank: "reversal" as RankRule }, h: "1m", topN: 3, sectors: false, bench: null },
    { name: "stock/1m-cost", spec: { ...SPECS.stock["1m"], cost: 0.5 }, h: "1m", topN: 3, sectors: false, bench: null },
    { name: "stock/1m-top10", spec: SPECS.stock["1m"], h: "1m", topN: 10, sectors: false, bench: null },
  ];
  for (const r of runs) {
    val.push({ args: { ...r, series: "long" }, output: validate(aligned, r.spec, r.h as never, r.topN, r.sectors ? sectors : undefined, r.bench ?? undefined) });
  }
  val.push({ args: { name: "crypto/1h", spec: SPECS.crypto["1h"], h: "1h", topN: 2, sectors: false, bench: 0, series: "1h" }, output: validate(alignedH, SPECS.crypto["1h"], "1h", 2, undefined, 0) });
  write("screener-validate", val);
}

/**
 * Golden outputs of backtest.ts, history.ts, brief.ts and news.ts (imported by golden.ts), compared by tests/parity_a.rs.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { inputs, find, NOW, write } from "./golden";
import { backtest, trackRecord } from "../../web/src/engine/backtest";
import { portfolioHistory, type Close } from "../../web/src/engine/history";
import { headline, movers, type BriefBuy, type MarketLevel, type Mover } from "../../web/src/engine/brief";
import { aggregate, decodeText, mentions, newsDigest, parseFeed, safeLink, sameStory, topStories, type NewsCategory, type RawNews } from "../../web/src/engine/news";

const attempt = <T>(f: () => T) => {
  try {
    return { ok: f() };
  } catch (e) {
    return { error: (e as Error).message };
  }
};

// ---------- backtest ----------
{
  const cases = [];
  for (const i of inputs.filter((x) => x.interval !== "long" || x.symbol === "BTC" || x.symbol === "AAPL")) {
    const r = backtest(i.candles);
    cases.push({ args: { symbol: i.symbol, interval: i.interval }, output: { r, track: trackRecord(r) } });
  }
  for (const [symbol, interval] of [["ETH", "4h"], ["NVDA", "1d"], ["DOGE", "1h"], ["SPY", "4h"]]) {
    const i = find(symbol!, interval!)!;
    for (const [feeRate, lookback, rewardRisk, warmup] of [[0.002, 120, 1.5, 100], [0, 250, 3, 60], [0.001, 80, 1, 200], [0.001, 250, 2, i.candles.length - 1], [0.001, 250, 2, i.candles.length - 2]]) {
      const r = backtest(i.candles, feeRate, lookback, rewardRisk, warmup);
      cases.push({ args: { symbol, interval, feeRate, lookback, rewardRisk, warmup }, output: { r, track: trackRecord(r) } });
    }
  }
  write("backtest", cases);
}

// ---------- history ----------
{
  const series: Record<string, Close[]> = {};
  for (const i of inputs.filter((x) => x.interval === "long")) series[`${i.kind}:${i.symbol}`] = i.candles.map((c) => [c.time, c.close]);
  // A few doubtful points (zero, negative, NaN is not in JSON: skipped) and an unsorted series.
  series["crypto:ODD"] = [...series["crypto:SOL"]!.slice(-60)].reverse().map(([t, c], k) => [t, k % 17 === 0 ? 0 : k % 23 === 0 ? -c : c]);
  series["stock:LATE"] = series["stock:NVDA"]!.slice(-20);
  series["crypto:FUTURE"] = series["crypto:ETH"]!.map(([t, c]) => [t + 5 * 86_400_000, c]);
  const sample = JSON.parse(readFileSync(join(import.meta.dir, "../../web/test/history-sample.json"), "utf8")) as { asOf: number; series: { symbol: string; kind: string; closes: Close[] }[] };
  const real: Record<string, Close[]> = Object.fromEntries(sample.series.map((s) => [`${s.kind}:${s.symbol}`, s.closes]));
  const DAY = 86_400_000;
  const d = (iso: string) => Date.parse(`${iso}T00:00:00Z`);
  const cases: { args: unknown; output: unknown }[] = [];
  const add = (lines: { id: string; quantity: number }[], s: Record<string, Close[]>, days: number, now: number, name: string) =>
    cases.push({ args: { lines, series: name === "inline" ? s : name, days, now }, output: portfolioHistory(lines, s, days, now) });
  const combos = [
    [{ id: "crypto:ETH", quantity: 2 }, { id: "stock:AAPL", quantity: 10 }],
    [{ id: "crypto:BTC", quantity: 0.5 }],
    [{ id: "stock:NVDA", quantity: 3 }, { id: "stock:SPY", quantity: 1 }, { id: "crypto:DOGE", quantity: 10000 }, { id: "crypto:GONE", quantity: 1 }],
    [{ id: "crypto:SOL", quantity: 4 }, { id: "crypto:ODD", quantity: 1 }],
    [{ id: "stock:AAPL", quantity: 5 }, { id: "stock:LATE", quantity: 2 }],
    [{ id: "crypto:FUTURE", quantity: 1 }, { id: "stock:AAPL", quantity: 0 }],
    [{ id: "crypto:ETH", quantity: -1 }],
  ];
  for (const lines of combos) for (const days of [1, 7, 30, 90, 365]) add(lines, series, days, NOW, "inputs");
  add(combos[0]!, series, 90, NOW - 40 * DAY, "inputs");
  add(combos[0]!, real, 90, sample.asOf, "sample");
  add([{ id: "crypto:BTC", quantity: 1 }, { id: "stock:SPY", quantity: 2 }], real, 30, sample.asOf, "sample");
  add([{ id: "stock:X", quantity: 1 }], { "stock:X": [[d("2026-09-18"), 100], [d("2026-09-21"), 110]] }, 3, d("2026-09-21") + 3_600_000, "inline");
  add(
    [{ id: "crypto:A", quantity: 1 }, { id: "crypto:NEW", quantity: 2 }],
    {
      "crypto:A": Array.from({ length: 11 }, (_, i) => [d("2026-09-01") + i * DAY, 10] as Close),
      "crypto:NEW": Array.from({ length: 4 }, (_, i) => [d("2026-09-08") + i * DAY, 5] as Close),
    },
    10,
    d("2026-09-11") + 1,
    "inline",
  );
  const closes = [100, 120, 90, 108, 60, 80].map((c, i) => [d("2026-09-01") + i * DAY, c] as Close);
  add([{ id: "crypto:A", quantity: 1 }, { id: "crypto:GONE", quantity: 3 }], { "crypto:A": closes }, 5, d("2026-09-06") + 1, "inline");
  add([{ id: "crypto:GONE", quantity: 1 }], {}, 30, NOW, "inline");
  add([], {}, 30, NOW, "inline");
  add([{ id: "crypto:A", quantity: 1 }], { "crypto:A": [[d("2026-09-01"), 5]] }, 0, d("2026-09-01") + 5, "inline");
  add([{ id: "crypto:A", quantity: 1 }], { "crypto:A": [[d("2026-09-01"), 5], [d("2026-09-02"), 5]] }, 1, d("2026-09-02") + 5, "inline");
  // Flat then rising: no worst day, no best below 0.
  add([{ id: "crypto:A", quantity: 1 }], { "crypto:A": [100, 100, 100, 101].map((c, i) => [d("2026-09-01") + i * DAY, c] as Close) }, 3, d("2026-09-04") + 5, "inline");
  write("history", cases);
}

// ---------- brief ----------
{
  const cases: { args: unknown; output: unknown }[] = [];
  const prices: { symbol: string; kind: "crypto" | "stock"; price: number | null }[] = [];
  const prev: Record<string, number | null> = {};
  for (const i of inputs.filter((x) => x.interval === "1h")) {
    prices.push({ symbol: i.symbol, kind: i.kind, price: i.candles.at(-1)!.close });
    const daily = find(i.symbol, "1d")!.candles;
    prev[`${i.kind}:${i.symbol}`] = daily.at(-2)!.close;
  }
  prices.push({ symbol: "X", kind: "crypto", price: null }, { symbol: "Y", kind: "stock", price: 0 }, { symbol: "Z", kind: "stock", price: 12 }, { symbol: "W", kind: "crypto", price: 3 });
  Object.assign(prev, { "crypto:X": 10, "stock:Y": 10, "stock:Z": null, "crypto:W": -1, "stock:BTC": 1 });
  prices.push({ symbol: "T1", kind: "crypto", price: 101 }, { symbol: "T2", kind: "crypto", price: 99 }, { symbol: "T3", kind: "stock", price: 101 });
  Object.assign(prev, { "crypto:T1": 100, "crypto:T2": 100, "stock:T3": 100 });
  const m = movers(prices, prev);
  cases.push({ args: { fn: "movers", prices, prev }, output: m });
  const buy = (symbol: string, strong = false): BriefBuy => ({ symbol, kind: "crypto", strong, title: "" });
  const mv = (symbol: string, change: number): Mover => ({ symbol, kind: "stock", price: 1, change });
  const moveSets: Mover[][] = [
    m,
    [],
    [mv("NVDA", -3.4), mv("SOL", 2.8), mv("BTC", 0.4)],
    [mv("A", 1), mv("B", -1), mv("C", 5)],
    [mv("A", 0.999), mv("B", -0.95), mv("C", -1.04999)],
    [mv("A", 1234.56), mv("B", -12345.678)],
    [mv("A", 1.05), mv("B", 1.25), mv("C", 1.35)],
    [mv("A", -0), mv("B", 10.96)],
  ];
  const buySets: BriefBuy[][] = [[], [buy("BTC")], [buy("BTC", true), buy("ETH"), buy("AAPL")], [buy("A"), buy("B"), buy("C"), buy("D")], [buy("é")]];
  for (const level of [null, "calm", "tense", "high"] as (MarketLevel | null)[])
    for (const buyable of buySets) for (const moves of moveSets) cases.push({ args: { fn: "headline", level, buyable, moves }, output: headline(level, buyable, moves) });
  write("brief", cases);
}

// ---------- news ----------
{
  const SAMPLES = ["bfm", "cointelegraph", "decrypt", "google-fr", "yahoo-aapl"];
  const xml = (name: string) => readFileSync(join(import.meta.dir, "../../web/test/news-samples", `${name}.xml`), "utf8");
  const cases: { args: unknown; output: unknown }[] = [];
  // parseFeed on the real feeds, with several fallback names (Google ones cut the " - Source" suffix).
  const parsed: Record<string, RawNews[]> = {};
  for (const s of SAMPLES)
    for (const source of ["BFM Économie", "Google Actualités", "Google News", "Yahoo Finance", "X"]) {
      const out = parseFeed(xml(s), source);
      if (source === "X") parsed[s] = out;
      cases.push({ args: { fn: "parseFeed", sample: s, source }, output: attempt(() => out) });
    }
  const long = "Lorem ipsum dolor sit amet, ".repeat(12);
  const feeds = [
    `<rss><channel><item><title>Titre piégé</title><link>javascript:alert(1)</link><pubDate>Mon, 28 Sep 2026 06:00:00 GMT</pubDate></item></channel></rss>`,
    `<feed><entry><title>Fed holds rates</title><link href="https://ex.com/1"/><updated>2026-09-28T06:00:00Z</updated></entry></feed>`,
    `<feed><entry xml:lang="en"><title type="html">&lt;b&gt;Fed&lt;/b&gt; &amp;amp; ECB</title><link rel="alternate" href="HTTPS://Ex.COM:443/a b?q=é"/><published>2026-09-28T06:00:00+02:00</published><summary>${long}</summary></entry></feed>`,
    `<rss><item><title><![CDATA[L&#8217;or &amp; le <b>dollar</b> grimpent après la décision de la Fed - Les Echos]]></title><link> https://ex.com/or </link><dc:date>2026-09-27T22:00:00.123Z</dc:date><source url="x">Les Echos</source><description><![CDATA[<p>${long}😀😀 fin</p>]]></description></item></rss>`,
    `<rss><item><title>Un titre assez long pour être coupé - Le Figaro</title><link>https://ex.com/2</link><pubDate>Mon, 28 Sep 2026 06:00:00</pubDate><description>Un titre assez long pour être coupé - Le Figaro</description></item><item><title>Court - AFP</title><link>https://ex.com/3</link><pubDate>Mon, 28 Sep 2026 06:00:00 EDT</pubDate></item><item><title>Pas de date</title><link>https://ex.com/4</link></item><item><title>Date vide</title><link>https://ex.com/5</link><pubDate></pubDate></item><item><title></title><link>https://ex.com/6</link><pubDate>2026-09-28</pubDate></item></rss>`,
    `<rss><item><title>Emoji &#x1F600; &#128512; &#X41; &#0065; &#xD83D;&#xDE00;</title><link>http://ex.com/?a=1&amp;b=2</link><pubDate>Sun, 27 Sep 2026 23:58:00 +0200</pubDate><description>&lt;a href=&quot;x&quot;&gt;Titre&lt;/a&gt;&nbsp;Source &#39;q&#39; &apos;r&apos;</description></item><items>not an item</items></rss>`,
    `<rss><item><title>Bad &#x110000; entity</title><link>https://ex.com/7</link><pubDate>Mon, 28 Sep 2026 06:00:00 GMT</pubDate></item></rss>`,
    `<rss><item><title>${"x".repeat(25)} - Reuters</title><link>https://ex.com/8</link><pubDate>Mon, 28 Sep 2026 06:00:00 +0000</pubDate><description>${"y".repeat(400)}</description></item></rss>`,
  ];
  for (const [k, f] of feeds.entries()) for (const source of ["X", "Google News"]) cases.push({ args: { fn: "parseFeed", xml: f, source, k }, output: attempt(() => parseFeed(f, source)) });
  for (const s of [
    "<![CDATA[L&#8217;or &amp; le <b>dollar</b>]]>",
    "&lt;a href=&quot;x&quot;&gt;Titre&lt;/a&gt;&nbsp;Source",
    "  a\t\n b  c﻿ ",
    "&amp;lt;b&amp;gt;double&amp;lt;/b&amp;gt;",
    "x\u0085y",
    "",
    "<![CDATA[a]]> <![CDATA[b]]>",
    "&#x0;&#00000000065;",
  ])
    cases.push({ args: { fn: "decodeText", s }, output: attempt(() => decodeText(s)) });
  for (const s of ["javascript:alert(1)", "data:text/html,x", "https://example.com/a?b=1", " http://EX.com ", "ftp://x.com/", "not a url", "https://ex.com/a b#c d", "https://bücher.de/x", "http://ex.com:80/", "https://user:pw@ex.com/p/../q"])
    cases.push({ args: { fn: "safeLink", s }, output: safeLink(s) });
  const now = Date.parse("2026-09-28T08:00:00Z");
  const item = (title: string, source: string, hoursAgo = 1, link = `https://ex.com/${encodeURIComponent(title)}/${source}`): RawNews => ({ title, link, source, time: now - hoursAgo * 3_600_000 });
  const pairs: [RawNews, RawNews][] = [
    [item("Fed holds interest rates steady as inflation cools", "A", 3), item("Fed holds interest rates steady while inflation cools", "B", 2)],
    [item("Fed holds interest rates steady as inflation cools", "A", 3), item("Bitcoin ETF inflows reach record high", "B", 1)],
    [item("La Bourse de Paris grimpe après la décision", "A", 1), item("La bourse de PARIS grimpe apres la decision", "B", 40)],
    [item("La Bourse de Paris grimpe après la décision", "A", 1), item("La bourse de PARIS grimpe apres la decision", "B", 30)],
    [item("Ça ÉCHOUE à Zürich — Straße ﬁnale İstanbul", "A"), item("ca echoue a zurich strasse finale istanbul", "B")],
    [item("ab cd ef", "A"), item("ab cd ef", "B")],
  ];
  for (const [a, b] of pairs) cases.push({ args: { fn: "sameStory", a, b }, output: sameStory(a, b) });
  const assets = [
    { id: "crypto:BTC", symbol: "BTC", name: "Bitcoin" },
    { id: "stock:AAPL", symbol: "AAPL", name: "Apple Inc." },
    { id: "crypto:SOL", symbol: "SOL", name: "Solana" },
    { id: "stock:NVDA", symbol: "NVDA", name: "NVIDIA Corporation" },
    { id: "stock:BRK-B", symbol: "BRK-B", name: "Berkshire Hathaway Holdings" },
    { id: "crypto:ETH", symbol: "ETH", name: "Ethereum" },
    { id: "stock:MC.PA", symbol: "MC.PA", name: "LVMH, SA" },
    { id: "crypto:XRP", symbol: "XRP", name: "XRP" },
    { id: "stock:SAN", symbol: "SAN", name: "Société Générale" },
    { id: "stock:FOO", symbol: "FOO", name: "Foo Bar Holdingſ" },
    { id: "stock:KEL", symbol: "KEL", name: "\u212Aelvin Corp." },
  ];
  const titles = [
    "Bitcoin tops $90,000", "BTC dominance rises", "Apple unveils new iPhone", "Pineapple prices rise", "Le sol est gelé", "$SOL rallies", "SOLANA surges",
    "nvidia beats estimates", "Berkshire Hathaway buys more", "BRK-B hits record", "Ethereum's upgrade", "Ethereumclassic", "LVMH recule", "MC.PA en baisse",
    "XRP lawsuit ends", "xrp", "SOCIÉTÉ GÉNÉRALE publie", "Société Généraleé", "l'ETH monte", "BTCUSD", "1BTC", "$$BTC", "Foo Bar news", "Foo Bar Holdingſ news", "kelvin up", "\u212Aelvin up", "KELVIN",
  ];
  for (const t of titles) cases.push({ args: { fn: "mentions", title: t }, output: assets.map((a) => mentions(t, a)) });

  const agg = (feeds: { category: NewsCategory; fallback?: NewsCategory; items: RawNews[] }[], assetsIn: typeof assets, at: number, maxAgeMs?: number) => {
    const out = aggregate(feeds, assetsIn, at, maxAgeMs);
    cases.push({ args: { fn: "aggregate", feeds, assets: assetsIn, now: at, maxAgeMs }, output: { out, top: topStories(out), top2: topStories(out, 2), digest: newsDigest(out, at) } });
  };
  const sampleNow = Date.parse("2026-09-28T08:00:00Z");
  const real = [
    { category: "monde" as const, items: parsed["bfm"]! },
    { category: "crypto" as const, items: parsed["cointelegraph"]! },
    { category: "crypto" as const, items: parsed["decrypt"]! },
    { category: "monde" as const, items: parseFeed(xml("google-fr"), "Google Actualités") },
    { category: "actifs" as const, fallback: "marches" as const, items: parsed["yahoo-aapl"]! },
  ];
  agg(real, assets, sampleNow);
  agg(real, [], sampleNow, 12 * 3_600_000);
  agg([...real, ...real.map((f) => ({ ...f, items: f.items.map((i) => ({ ...i, source: i.source + " bis", time: i.time + 60_000 })) }))], assets.slice(0, 3), sampleNow);
  // The unit tests' cases.
  agg([{ category: "monde", items: pairs[0]!.concat(item("Fed holds interest rates steady, inflation cools", "MarketWatch", 1)) }, { category: "crypto", items: [item("Bitcoin ETF inflows reach record high", "CoinDesk", 1)] }], [], now);
  agg([{ category: "actifs", fallback: "crypto", items: [item("Bitcoin miners expand in Texas", "Google News"), item("Crypto market wobbles on macro fears", "Google News")] }], [assets[0]!], now);
  agg([{ category: "actifs", items: [item("Crypto market wobbles on macro fears", "Google News")] }], [], now);
  agg(
    [
      {
        category: "monde",
        items: [
          item("Stocks plunge as bank collapse sparks contagion fears", "A"),
          item("La Bourse de Paris grimpe après la décision de la BCE", "B"),
          item("Russia declares war on neighbour, markets slide", "C"),
          item("Old story from last week", "D", 24 * 7),
          item("Future story", "E", -0.5),
          item("Far future story", "F", -1),
          item("Armée et récession : la guerre commerciale fait rage", "G"),
          item("SEC sues exchange over fraud; ETF approval delayed", "H"),
          item("Nvidia earnings beat, revenue soars on guidance", "I"),
          item("Les résultats trimestriels de LVMH et le chiffre d'affaires", "J"),
          item("Taux d'intérêt : la banque centrale et la hausse des taux", "K"),
          item("Tariffs and export controls: embargo looms", "L"),
          item("Martial law declared as nuclear test shocks markets", "M"),
          item("Bank runs might spread, analysts fear", "N"),
          item("Stocks: rally and plunge in one day", "O"),
          item("Hostages freed after ceasefire", "P", 30),
          item("Des marchés sur la défensive", "Q"),
          item("WAR IN EUROPE", "R"),
          item("Rate cuts ahead? The Fed's Powell speaks", "S"),
          item("Ban\u212A ſell-off: hac\u212Aed exchange ſues, KRACH à Paris", "T"),
          item("Martial law in \u212Aorea; bank run feared", "U"),
        ],
      },
    ],
    [],
    now,
  );
  agg([{ category: "monde", items: [item("Les agents IA pourraient déclencher un bank run, selon cet économiste", "X"), item("AI agents could trigger a bank run", "Y")] }], [], now);
  agg([{ category: "monde", items: [item("Russia declares war on neighbour, markets slide", "A"), item("Russia declares war on its neighbour as markets slide", "B"), item("Fed holds interest rates steady as inflation cools", "C"), item("Fed holds interest rates steady while inflation cools", "D")] }], [], now);
  const w = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet"];
  const distinct = (i: number) => `${w[i % 10]} ${w[Math.floor(i / 10)]}token ${i}x${w[(i * 3) % 10]}chain news`;
  agg([{ category: "crypto", items: Array.from({ length: 100 }, (_, i) => item(distinct(i), `S${i}`, i / 10)) }], [], now);
  // Same link from two sources, same source twice, more than 8 other sources.
  agg([{ category: "marches", items: Array.from({ length: 12 }, (_, i) => item("Wall Street closes higher on tech rally", `Src${i % 10}`, 5 - i * 0.1, "https://ex.com/same")) }], [], now);
  write("news", cases);
}

// ---------- server/news.ts: feed list ----------
{
  const { FEEDS, assetFeeds } = await import("../../web/server/news");
  const assets = [
    { symbol: "BTC", name: "Bitcoin", kind: "crypto" as const, gecko: "bitcoin" },
    { symbol: "AAPL", name: "Apple Inc.", kind: "stock" as const },
    { symbol: "BRK-B", name: "Berkshire Hathaway Inc", kind: "stock" as const },
    { symbol: "MC.PA", name: "LVMH Moët Hennessy, plc.", kind: "stock" as const },
    { symbol: "X&Y", name: "Café « Test » & Co/Corp", kind: "stock" as const },
  ];
  write("news-feeds", [{ args: { assets }, output: { feeds: FEEDS, assetFeeds: assetFeeds(assets) } }]);
}

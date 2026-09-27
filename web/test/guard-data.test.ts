import { expect, test } from "bun:test";
import { parseGuard } from "../server/guard";

// Real responses (27/09/2026), same samples as GuardDataTests.swift.
const funding = {"code": "0", "data": [{"instId": "BTC-USDT-SWAP", "fundingRate": "-0.0000093022018830", "fundingTime": "1790524800000"}]};
const ls = {"code": "0", "data": [["1790517600000", "1.2"], ["1790514000000", "1.27"], ["1790510400000", "1.29"]], "msg": ""};
const oi = {"code": "0", "data": [["1790521200000", "3096187639.1516", "373921486.7371"], ["1790517600000", "3151244635.3541", "379272299.6517"], ["1790514000000", "3131575585.0841", "119727471.7871"]], "msg": ""};
const fng = {"data": [{"value": "70", "value_classification": "Greed", "timestamp": "1790467200"}, {"value": "74", "value_classification": "Greed", "timestamp": "1790380800"}, {"value": "71", "value_classification": "Greed", "timestamp": "1790294400"}]};
const vix = {"data": [{"date": "2026-09-23", "volume": "0.0", "open": "15.10", "high": "15.80", "low": "14.90", "close": "15.42"}, {"date": "2026-09-24", "volume": "0.0", "open": "15.40", "high": "16.20", "low": "15.00", "close": "15.61"}, {"date": "2026-09-25", "volume": "0.0", "open": "15.60", "high": "15.90", "low": "14.70", "close": "14.87"}]};
const rss = '<rss><channel><title>Google News</title><item><title>Is It Too Late to Buy Bitcoin After a 32% Rally in Two Months? - 24/7 Wall St.</title><pubDate>Sun, 27 Sep 2026 14:25:00 GMT</pubDate><source url="https://247wallst.com">24/7 Wall St.</source></item><item><title>Bitcoin Holders Are Selling, But This Time It’s Different: What You Need to Know - Yahoo Finance</title><pubDate>Sun, 27 Sep 2026 11:00:58 GMT</pubDate><source url="https://finance.yahoo.com">Yahoo Finance</source></item><item><title><![CDATA[S&amp;P 500 &amp; Bitcoin: “risk-on” returns]]></title><pubDate>Sat, 26 Sep 2026 22:15:03 GMT</pubDate></item><item><title>No date here</title></item></channel></rss>';

test("positionnement OKX : financement, ratio acheteurs/vendeurs et positions ouvertes (plus ancien d'abord)", () => {
  expect(parseGuard.funding(funding)).toBeCloseTo(-0.000009302201883, 15);
  expect(parseGuard.funding({ code: "51001", data: [] })).toBeNull();
  expect(parseGuard.rubik(ls)).toEqual([1.29, 1.27, 1.2]);
  expect(parseGuard.rubik(oi)).toEqual([3131575585.0841, 3151244635.3541, 3096187639.1516]);
  expect(parseGuard.rubik({ code: "50011", data: null })).toEqual([]);
});

test("Fear & Greed, VIX et flux RSS d'actualités", () => {
  expect(parseGuard.fearGreed(fng)).toEqual([71, 74, 70]);
  expect(parseGuard.vix(vix)).toEqual([15.42, 15.61, 14.87]);
  const items = parseGuard.rss(rss);
  expect(items.length).toBe(3); // the item without a date is ignored
  expect(items[0]!.title).toContain("Is It Too Late to Buy Bitcoin");
  expect(items[0]!.time).toBe(Date.UTC(2026, 8, 27, 14, 25, 0));
  expect(items[0]!.source).toBe("24/7 Wall St.");
  expect(items[2]!.title).toBe("S&P 500 & Bitcoin: “risk-on” returns");
});

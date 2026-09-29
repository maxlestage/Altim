import { afterEach, describe, expect, test } from "bun:test";
import { evaluateTargets, isReached, journalSummary, newBuyAlerts, newNews, parseAlerts, rearm, type AlertEntry, type PriceTarget } from "../src/webapp/alerts-store";
import { setMoneyDisplay, type FxRate } from "../src/money";
import type { BuyAlert } from "../src/webapp/api";
import type { NewsItem } from "../src/engine/news";

const FX: FxRate = { rate: 0.9, usdPerEur: 1 / 0.9, time: 0, source: "BCE", fetchedAt: 0, stale: false };
const target = (p: Partial<PriceTarget>): PriceTarget => ({
  id: "t", symbol: "BTC", kind: "crypto", name: "Bitcoin", above: false, price: 80_000, currency: "USD", created: 0, triggered: null, move: null, ...p,
});
afterEach(() => setMoneyDisplay("USD", null));

describe("price alerts", () => {
  test("a dollar threshold compares the dollar price", () => {
    expect(isReached(target({}), 79_999)).toBe(true);
    expect(isReached(target({}), 80_001)).toBe(false);
    expect(isReached(target({ above: true }), 80_000)).toBe(true);
  });

  test("a euro threshold compares the price converted at the current rate, and waits without a rate", () => {
    const t = target({ currency: "EUR", price: 72_000 });
    expect(isReached(t, 79_000)).toBe(false);
    setMoneyDisplay("EUR", FX);
    expect(isReached(t, 79_000)).toBe(true); // 71 100 €
    expect(isReached(t, 81_000)).toBe(false); // 72 900 €
  });

  test("move alerts and re-arming from the current price", () => {
    const m = target({ move: 5, price: 100, symbol: "AAPL", kind: "stock" });
    expect(isReached(m, 104.9)).toBe(false);
    expect(isReached(m, 94.9)).toBe(true);
    const r = evaluateTargets([m], { "stock:AAPL": 106 }, 42);
    expect(r.fired).toHaveLength(1);
    expect(r.targets[0]!.triggered).toBe(42);
    // Already triggered: not fired twice.
    expect(evaluateTargets(r.targets, { "stock:AAPL": 110 }, 43).fired).toHaveLength(0);
    const again = rearm(r.targets[0]!, 106);
    expect([again.triggered, again.price]).toEqual([null, 106]);
  });
});

describe("buy alerts notified once per reason", () => {
  const alert = (p: Partial<BuyAlert>): BuyAlert => ({ symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 1, buy: true, strong: false, key: "zone:medium", ...p });
  test("new reason, same reason, lost then back after the cooldown", () => {
    let r = newBuyAlerts({ notified: {}, lost: {} }, [alert({})], false, 0);
    expect(r.fresh).toHaveLength(1);
    r = newBuyAlerts(r.tracker, [alert({})], false, 1);
    expect(r.fresh).toHaveLength(0);
    r = newBuyAlerts(r.tracker, [alert({ key: "signal+zone:medium", strong: true })], false, 2);
    expect(r.fresh).toHaveLength(1);
    r = newBuyAlerts(r.tracker, [alert({ buy: false })], false, 10);
    r = newBuyAlerts(r.tracker, [alert({ buy: false })], false, 10 + 6 * 3_600_000);
    expect(r.tracker.notified).toEqual({});
    expect(newBuyAlerts({ notified: {}, lost: {} }, [alert({})], true, 0).fresh).toHaveLength(0);
  });
});

describe("news and journal", () => {
  const news = (p: Partial<NewsItem>): NewsItem => ({
    id: "n1", title: "Invasion confirmed by officials overnight", link: "", time: 1_000, source: "Reuters", summary: null, lang: "en", category: "monde",
    themes: [], tone: "negative", assets: [], alsoIn: ["AP"], alert: true, ...p,
  });
  test("grave escalation told twice, or an owned asset told three times; the same story once", () => {
    const r = newNews([], [news({}), news({ id: "n2", title: "Invasion confirmed by officials overnight, markets" })], new Set(), 2_000);
    expect(r.fresh.map((n) => n.id)).toEqual(["n1"]);
    expect(newNews([], [news({ alert: false, assets: ["crypto:BTC"], alsoIn: ["A", "B"] })], new Set(["crypto:BTC"]), 2_000).fresh).toHaveLength(1);
    expect(newNews([], [news({ alert: false, assets: ["crypto:BTC"], alsoIn: ["A"] })], new Set(["crypto:BTC"]), 2_000).fresh).toHaveLength(0);
    expect(newNews([], [news({ time: 0 })], new Set(), 7 * 3_600_000).fresh).toHaveLength(0);
  });

  test("summary of the buy alerts older than an hour", () => {
    const e = (p: Partial<AlertEntry>): AlertEntry => ({ id: "e", symbol: "BTC", kind: "crypto", name: "", source: "buy", title: "", price: 100, date: 0, ...p });
    const s = journalSummary([e({}), e({ symbol: "ETH", price: 50 }), e({ source: "target" }), e({ date: 3_000_000 })], { "crypto:BTC": 110, "crypto:ETH": 45 }, 3_600_000);
    expect([s!.count, s!.up, s!.upShare]).toEqual([2, 1, 50]);
    expect(s!.average).toBeCloseTo(0, 9);
  });

  test("unreadable saved data is dropped, old targets without a currency are dollars", () => {
    expect(parseAlerts("{").targets).toEqual([]);
    const p = parseAlerts(JSON.stringify({ version: 1, targets: [{ id: "a", symbol: "BTC", kind: "crypto", price: 80_000 }, { id: "b", symbol: "X", kind: "bond", price: 1 }] }));
    expect(p.targets).toHaveLength(1);
    expect(p.targets[0]!.currency).toBe("USD");
  });
});

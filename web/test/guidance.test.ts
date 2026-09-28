import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import raw from "../../backend/tests/samples/decision-guidance.json";
import btcRaw from "../../backend/tests/samples/decision-btc.json";
import { NNBSP, parseDecision, type Decision } from "../src/webapp/decision";
import { ActionLadder, ChangeBlock, DecisionView } from "../src/webapp/DecisionCard";
import { applyDecision, type ConfigState } from "../src/webapp/config-changes";

const clone = <T>(x: T): T => JSON.parse(JSON.stringify(x));
/** A real AAPL decision of the server (backend/tests/decision.rs print_real), with the guidance fields. */
const aapl = (): Decision => parseDecision(clone(raw));
const html = (d: Decision, change?: Parameters<typeof DecisionView>[0]["change"]) => renderToStaticMarkup(createElement(DecisionView, { d, change }));

describe("guidance fields of the contract", () => {
  test("the server's sample decodes with every new field", () => {
    const d = aapl();
    expect(d.noTrade?.active).toBe(false);
    expect(d.noTrade?.unchecked.length).toBeGreaterThan(0);
    expect(d.actionZones?.zones.map((z) => z.kind)).toEqual(["exit", "invalidation", "buy", "wait", "profit"]);
    expect(d.actionZones?.here).toBe("wait");
    expect(d.scenarios.map((s) => s.conditions?.length)).toEqual([3, 2, 3]);
    expect(d.scenarios.filter((s) => s.unfolding).map((s) => s.kind)).toEqual([d.unfolding!.kind]);
    expect(d.counterArgument?.text).toMatch(/^🟢 Raisons favorables : \d+ \/ 🔴 Raisons défavorables : \d+$/);
    expect(d.snapshot?.families.length).toBe(d.families.length);
  });
  test("bad guidance fields are refused, older answers still pass", () => {
    expect(() => parseDecision({ ...clone(raw), noTrade: { active: true } })).toThrow("noTrade");
    expect(() => parseDecision({ ...clone(raw), actionZones: { zones: [] } })).toThrow("actionZones");
    expect(() => parseDecision({ ...clone(raw), counterArgument: {} })).toThrow("counterArgument");
    expect(() => parseDecision({ ...clone(raw), snapshot: {} })).toThrow("snapshot");
    const old = parseDecision(clone(btcRaw));
    expect(old.noTrade).toBeUndefined();
    const out = html(old);
    expect(out).not.toContain("Pas le moment de trader");
    expect(out).not.toContain("Zones d'action");
    expect(out).not.toContain("Contre-argument");
  });
});

describe("decision card", () => {
  test("no banner when nothing is wrong, the section says so", () => {
    const out = html(aapl());
    expect(out).not.toContain("dec-notrade");
    expect(out).toContain("Quand ne pas trader");
    expect(out).toContain("rien à signaler");
    expect(out).toContain("Non vérifié faute de données");
  });
  test("the banner lists the reasons near the top", () => {
    const d = aapl();
    d.noTrade = {
      active: true, headline: "🕰️ Pas le moment de trader : marché sans direction, marché fermé.", unchecked: [],
      reasons: [
        { code: "trendless", label: "Marché sans direction", detail: "ADX 14 (sous 20) et sommets et creux sans direction" },
        { code: "marketClosed", label: "Marché fermé", detail: "Hors séance régulière de Wall Street" },
      ],
    };
    const out = html(d);
    const banner = out.indexOf("dec-notrade");
    expect(banner).toBeGreaterThan(-1);
    expect(banner).toBeLessThan(out.indexOf("dec-headline"));
    expect(out).toContain("🕰️ Pas le moment de trader : marché sans direction, marché fermé.");
    expect(out).toContain("2 raisons");
    expect(out).toContain("<b>Marché sans direction</b> — ADX 14");
  });
  test("the ladder reads from the highest price down, with the price marked", () => {
    const d = aapl();
    const out = renderToStaticMarkup(createElement(ActionLadder, { z: d.actionZones! }));
    const order = ["az-profit", "az-wait", "az-buy", "az-invalidation", "az-exit"].map((k) => out.indexOf(k));
    expect(order.every((i, n) => i > -1 && (n === 0 || i > order[n - 1]!))).toBe(true);
    expect(out).toContain('class="az az-wait here"');
    expect(out).toContain(`◀ vous êtes ici · 341,04${NNBSP}$`);
    // Price between two zones: a marker of its own, placed above the first zone under it.
    const z = { ...d.actionZones!, price: 275, here: null, hereText: "Vous êtes ici : entre la zone de sortie et la zone d'achat (275 $)" };
    const between = renderToStaticMarkup(createElement(ActionLadder, { z }));
    expect(between.indexOf("az-marker")).toBeGreaterThan(between.indexOf("az-buy"));
    expect(between.indexOf("az-marker")).toBeLessThan(between.indexOf("az-invalidation"));
  });
  test("scenarios show their checklist, counts and the one unfolding", () => {
    const d = aapl();
    const out = html(d);
    expect(out).toContain(`en cours : ${d.unfolding!.kind === "neutral" ? "neutre" : d.unfolding!.kind === "bull" ? "haussier" : "baissier"}`);
    expect(out).toContain("sc-unfolding");
    expect(out).toContain(d.unfolding!.text);
    expect(out).toContain("1/2 conditions · en cours");
    expect(out).toContain("Clôture au-dessus de 345,34");
  });
  test("counter-argument and why the signal changed", () => {
    const d = aapl();
    const out = html(d);
    expect(out).toContain("Contre-argument");
    expect(out).toContain("Points qui pourraient invalider le scénario");
    expect(out).toContain("Cassure de la résistance 344,95");
    const s0: ConfigState = { version: 2, last: {}, transitions: [] };
    const before = { ...aapl(), verdict: "buyZone", label: "ZONE D'ACHAT", level: "moderate", levelLabel: "Signal modéré" } as Decision;
    before.snapshot = { ...before.snapshot!, composite: 30, relativeVolume: 1.4 };
    const s1 = applyDecision(s0, before, false, Date.UTC(2026, 8, 28, 12, 2)).state;
    const t = applyDecision(s1, d, false, Date.UTC(2026, 8, 28, 16)).transition!;
    const block = renderToStaticMarkup(createElement(ChangeBlock, { t }));
    expect(block).toContain("Pourquoi le signal a changé depuis le 28/09 à 14:02");
    expect(block).toContain("ZONE D&#x27;ACHAT → ATTENDRE");
    expect(block).toContain("Score composite +30 → +41");
    expect(block).toContain("Volume en baisse (1,4× → 0,7× la moyenne)");
    expect(html(d, t)).toContain("dec-change");
  });
});

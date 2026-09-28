import { describe, expect, test } from "bun:test";
import sample from "./news-summary-sample.json";
import { IMPACT_LABEL, assetLink, basisLabel, consensusText, moveText, summaryHeading, type StorySummary } from "../src/webapp/news-summary";

// A real /api/news `summary` (28/09/2026, radar AAPL, BTC, NVDA; guard and hourly candles cached for them).
const list = sample.summary as StorySummary[];

describe("résumé des actualités", () => {
  test("échantillon réel : au plus 5 événements, chacun complet", () => {
    expect(list.length).toBeGreaterThan(0);
    expect(list.length).toBeLessThanOrEqual(5);
    for (const s of list) {
      expect(["low", "medium", "high"]).toContain(s.impact);
      expect(s.links[0]!.link).toBe(s.link);
      expect(s.independentSources).toBeLessThanOrEqual(s.sources);
      // Measured only with a move, and every move's asset is one of the story's.
      expect(s.impactBasis === "measured").toBe(s.moves.length > 0);
      for (const m of s.moves) expect(s.assets).toContain(m.asset);
      for (const t of s.technical) expect(s.assets).toContain(t.asset);
    }
  });

  test("libellés", () => {
    const at = (impact: StorySummary["impact"]) => ({ ...list[0]!, impact });
    expect(summaryHeading([])).toEqual({ title: "Aucun événement important aujourd'hui", others: null });
    expect(summaryHeading([at("low"), at("low")])).toEqual({ title: "Aucun événement important aujourd'hui", others: "2 sujets repris par plusieurs sources, à impact faible." });
    expect(summaryHeading([at("high"), at("low")])).toEqual({ title: "1 événement important aujourd'hui", others: "Et 1 sujet repris par plusieurs sources, à impact faible." });
    expect(summaryHeading([at("high"), at("medium")]).title).toBe("2 événements importants aujourd'hui");
    expect(IMPACT_LABEL.high).toBe("important");
    const nvda = list.find((s) => s.assets.includes("stock:NVDA"))!;
    expect(basisLabel(nvda)).toBe("impact mesuré");
    expect(consensusText(nvda)).toBe("Convergent · 2 sources · ton des titres : 2 positifs");
    expect(moveText({ ...nvda.moves[0]!, changePct: -0.4220148770699983 })).toBe("NVDA −0,4 % depuis la publication");
    expect(assetLink("stock:BRK-B")).toEqual({ kind: "stock", symbol: "BRK-B", href: "/app/actif/stock/BRK-B" });
  });

  test("consensus : divergent, source unique, titre repris", () => {
    const base = list[0]!;
    const divergent = { ...base, sources: 3, consensus: { agreement: "divergent", tone: "negative", negative: 2, positive: 1, neutral: 0 } } as StorySummary;
    expect(consensusText(divergent)).toBe("Divergent · 3 sources · ton des titres : 2 négatifs, 1 positif");
    const single = { ...base, sources: 1, consensus: { ...base.consensus, agreement: "single" } } as StorySummary;
    expect(consensusText(single)).toBe("Une seule source : pas de consensus mesurable");
    expect(consensusText({ ...single, sources: 4 })).toBe("Même titre repris par 4 sources : pas de consensus mesurable");
    const partly = { ...divergent, sources: 4 } as StorySummary;
    expect(consensusText(partly)).toBe("Divergent · 4 sources (3 titres distincts) · ton des titres : 2 négatifs, 1 positif");
  });
});

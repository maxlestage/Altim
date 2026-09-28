import { describe, expect, test } from "bun:test";
import { headline, movers } from "../src/engine/brief";

describe("point du jour", () => {
  test("variations depuis la dernière clôture, les plus fortes d'abord, sans prix manquant", () => {
    const m = movers(
      [
        { symbol: "BTC", kind: "crypto", price: 102 },
        { symbol: "NVDA", kind: "stock", price: 96.6 },
        { symbol: "ETH", kind: "crypto", price: null },
        { symbol: "SOL", kind: "crypto", price: 50 },
      ],
      { "crypto:BTC": 100, "stock:NVDA": 100, "crypto:ETH": 2000, "crypto:SOL": null },
    );
    expect(m.map((x) => x.symbol)).toEqual(["NVDA", "BTC"]);
    expect(m[0]!.change).toBeCloseTo(-3.4, 9);
    expect(m[1]!.change).toBeCloseTo(2, 9);
  });

  test("une phrase qui résume la journée", () => {
    const buy = (symbol: string, strong = false) => ({ symbol, kind: "crypto" as const, strong, title: "" });
    const moves = [
      { symbol: "NVDA", kind: "stock" as const, price: 1, change: -3.4 },
      { symbol: "SOL", kind: "crypto" as const, price: 1, change: 2.8 },
      { symbol: "BTC", kind: "crypto" as const, price: 1, change: 0.4 },
    ];
    expect(headline("calm", [buy("BTC", true), buy("ETH"), buy("AAPL")], moves)).toBe("Contexte calme · 3 achetables (BTC, ETH, AAPL) · NVDA −3,4\u00a0%, SOL +2,8\u00a0%");
    expect(headline("high", [], moves.slice(2))).toBe("Tension élevée · rien d'achetable pour l'instant");
    expect(headline(null, [buy("A"), buy("B"), buy("C"), buy("D")], [])).toBe("4 achetables (A, B, C…)");
    expect(headline(null, [buy("BTC")], [])).toBe("1 achetable (BTC)");
  });
});

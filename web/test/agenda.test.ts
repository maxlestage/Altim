import { describe, expect, test } from "bun:test";
import sample from "./calendar-sample.json";
import { calendarUrl, dayLabel, filterEvents, groupByDay, stockSymbols, type CalendarReport } from "../src/webapp/calendar";

// A real /api/calendar answer (28/09/2026, 14 days, trimmed to a few events of each kind).
const report = sample as CalendarReport;

describe("agenda", () => {
  test("libellés des jours", () => {
    const now = new Date(2026, 8, 28, 10, 0);
    expect(dayLabel("2026-09-28", now)).toBe("Aujourd'hui");
    expect(dayLabel("2026-09-29", now)).toBe("Demain");
    expect(dayLabel("2026-09-30", now)).toBe("mer. 30 sept.");
    // Month end: tomorrow is the 1st of the next month.
    expect(dayLabel("2026-10-01", new Date(2026, 8, 30, 23, 0))).toBe("Demain");
  });

  test("regroupement par jour, dans l'ordre", () => {
    const groups = groupByDay(report.events);
    expect(groups.map((g) => g.day)).toEqual([...new Set(report.events.map((e) => e.day))].sort());
    expect(groups.reduce((n, g) => n + g.events.length, 0)).toBe(report.events.length);
  });

  test("filtres par type et « Mes actifs »", () => {
    expect(filterEvents(report.events, "ipo", null).every((e) => e.kind === "ipo")).toBe(true);
    expect(filterEvents(report.events, "centralBank", null).length).toBeGreaterThan(0);
    // Mine: MU's earnings stay, other companies and IPOs go, the economy and central banks stay.
    const mine = filterEvents(report.events, "all", ["MU"]);
    expect(mine.filter((e) => e.kind === "earnings").map((e) => e.symbol)).toEqual(["MU"]);
    expect(mine.some((e) => e.kind === "ipo" || e.kind === "dividend")).toBe(false);
    expect(mine.filter((e) => e.kind === "macro").length).toBe(report.events.filter((e) => e.kind === "macro").length);
    // No stock at all: only the economy and central banks.
    expect(filterEvents(report.events, "all", []).every((e) => e.kind === "macro" || e.kind === "centralBank")).toBe(true);
  });

  test("chaque événement a sa source et un lien", () => {
    for (const e of report.events) {
      expect(e.source.length).toBeGreaterThan(3);
      expect(e.url).toMatch(/^https:\/\//);
      expect(["high", "medium"]).toContain(e.importance);
    }
    expect(report.notCovered.length).toBeGreaterThan(0);
  });

  test("symboles envoyés et adresse", () => {
    expect(stockSymbols([{ symbol: "aapl", kind: "stock" }, { symbol: "BTC", kind: "crypto" }, { symbol: "AAPL", kind: "stock" }])).toEqual(["AAPL"]);
    expect(calendarUrl(14, null)).toBe("/api/calendar?days=14");
    expect(calendarUrl(14, [])).toBe("/api/calendar?days=14");
    expect(calendarUrl(30, ["AAPL", "BRK-B"])).toBe("/api/calendar?days=30&symbols=AAPL%2CBRK-B");
  });
});

import { describe, expect, test } from "bun:test";
import sample from "./calendar-sample.json";
import {
  calendarUrl, dayLabel, eventRisk, failedDays, filterEvents, groupByDay, riskDays, shortTitle, stockSymbols, type CalendarEvent, type CalendarReport,
} from "../src/webapp/calendar";

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
    // Risk view: top=1 only with symbols (without, the calendar already keeps the largest companies).
    expect(calendarUrl(7, ["AAPL"], true)).toBe("/api/calendar?days=7&symbols=AAPL&top=1");
    expect(calendarUrl(7, null, true)).toBe("/api/calendar?days=7");
  });

  test("calendrier de risque sur 7 jours, week-end compris", () => {
    // KDP held (its dividend counts), MU watched (its earnings count as high).
    const days = riskDays(report.events, "2026-09-28", { held: ["KDP"], watched: ["MU"] }, 7, ["2026-10-03"]);
    expect(days.map((d) => d.label)).toEqual(["Lun. 28 sept.", "Mar. 29 sept.", "Mer. 30 sept.", "Jeu. 1 oct.", "Ven. 2 oct.", "Sam. 3 oct.", "Dim. 4 oct."]);
    expect(days.map((d) => d.level)).toEqual(["medium", "medium", "high", "high", "medium", "low", "low"]);
    expect(days.map((d) => d.weekend)).toEqual([false, false, false, false, false, true, true]);
    expect(days[0]!.main).toEqual(["Dividende KDP"]);
    // Other companies' earnings (large caps) and a central bank speech: 🟠; the others' dividends do not count.
    expect(days[1]!.main).toEqual(["Résultats CCL", "Prise de parole de la présidence de la BCE"]);
    expect(days[1]!.events.some((x) => x.event.symbol === "ERIC")).toBe(false);
    expect(days[2]!.main).toEqual(["Résultats MU"]);
    // US GDP and PCE (high); the UK's GDP (medium) is not among the main events.
    expect(days[3]!.main).toEqual(["PIB", "Inflation PCE"]);
    expect(days[5]!.main).toEqual([]);
    expect(days[5]!.incomplete).toBe(true);
    expect(days[6]!.incomplete).toBe(false);
  });

  test("règle de risque par événement", () => {
    const ev = (p: Partial<CalendarEvent>): CalendarEvent =>
      ({ date: 0, day: "2026-10-06", kind: "macro", category: "inflation", importance: "high", title: "Inflation (CPI)", source: "s", url: "https://x", ...p });
    expect(eventRisk(ev({}), [], [])).toBe("high");
    expect(eventRisk(ev({ kind: "centralBank", category: "tauxDirecteurs" }), [], [])).toBe("high");
    expect(eventRisk(ev({ category: "activite", importance: "medium" }), [], [])).toBe("medium");
    expect(eventRisk(ev({ kind: "earnings", category: "resultats", symbol: "BRK.B" }), ["BRK-B"], [])).toBe("high");
    expect(eventRisk(ev({ kind: "earnings", category: "resultats", symbol: "NVDA" }), [], [])).toBe("medium");
    expect(eventRisk(ev({ kind: "dividend", category: "dividende", symbol: "KO" }), [], ["KO"])).toBeNull();
    expect(eventRisk(ev({ kind: "split", category: "split", symbol: "KO" }), ["KO"], [])).toBe("medium");
    expect(eventRisk(ev({ kind: "ipo", category: "ipo", symbol: "OURA" }), ["OURA"], [])).toBeNull();
    expect(shortTitle(ev({}))).toBe("CPI");
    expect(shortTitle(ev({ country: "Allemagne" }))).toBe("CPI (Allemagne)");
    expect(shortTitle(ev({ title: "Confiance des consommateurs (Conference Board)" }))).toBe("Confiance des consommateurs (Conference Board)");
    expect(failedDays({ ...report, sources: [{ name: "a", ok: false, failed: ["2026-10-01", "2026-10"] }] })).toEqual(["2026-10-01"]);
  });
});

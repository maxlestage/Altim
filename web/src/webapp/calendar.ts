/** Agenda (/api/calendar): types and the pure helpers of the view (day labels, grouping, filters). */

export type CalendarKind = "macro" | "earnings" | "dividend" | "split" | "ipo" | "centralBank";
export type CalendarCategory =
  | "tauxDirecteurs" | "inflation" | "emploi" | "pib" | "activite" | "discours" | "resultats" | "dividende" | "split" | "ipo";

export type CalendarEvent = {
  /** Exact instant (ms) when the source gives a time, otherwise 00:00 UTC of `day`. */
  date: number;
  /** "YYYY-MM-DD" (Paris time when the time is known). */
  day: string;
  /** "14:30" (Paris time) or, for earnings, "avant l'ouverture" / "après la clôture". */
  time?: string;
  kind: CalendarKind;
  category: CalendarCategory;
  importance: "high" | "medium";
  title: string;
  originalName?: string;
  country?: string;
  symbol?: string;
  actual?: string;
  consensus?: string;
  previous?: string;
  detail?: string;
  note?: string;
  source: string;
  url: string;
};

export type CalendarReport = {
  asOf: number;
  days: number;
  from: string;
  to: string;
  events: CalendarEvent[];
  sources: { name: string; ok: boolean; failed?: string[]; error?: string }[];
  notCovered: string[];
};

export type AgendaFilter = "all" | "macro" | "centralBank" | "earnings" | "dividend" | "split" | "ipo";
export const AGENDA_FILTERS: [AgendaFilter, string][] = [
  ["all", "Tout"],
  ["macro", "Macro"],
  ["centralBank", "Banques centrales"],
  ["earnings", "Résultats"],
  ["dividend", "Dividendes"],
  ["split", "Splits"],
  ["ipo", "IPO"],
];

export const CATEGORY_LABEL: Record<CalendarCategory, string> = {
  tauxDirecteurs: "Taux directeurs",
  inflation: "Inflation",
  emploi: "Emploi",
  pib: "PIB",
  activite: "Activité",
  discours: "Discours",
  resultats: "Résultats",
  dividende: "Dividende",
  split: "Split",
  ipo: "IPO",
};

function localDay(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** "Aujourd'hui", "Demain", else "mer. 30 sept." (`now`: the viewer's clock). */
export function dayLabel(day: string, now: Date = new Date()): string {
  const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  if (day === localDay(now)) return "Aujourd'hui";
  if (day === localDay(tomorrow)) return "Demain";
  const [y = 1970, m = 1, d = 1] = day.split("-").map(Number);
  return new Intl.DateTimeFormat("fr-FR", { weekday: "short", day: "numeric", month: "short" }).format(new Date(y, m - 1, d));
}

const COMPANY: CalendarKind[] = ["earnings", "dividend", "split"];
const norm = (s: string) => s.toUpperCase().replace(/[./]/g, "-");

/** Events of the chosen filter. `mine` (the user's stock symbols) keeps the company events of these stocks only and
 * leaves the IPOs out; the economy and central banks concern every asset and always stay. */
export function filterEvents(events: CalendarEvent[], filter: AgendaFilter, mine: string[] | null): CalendarEvent[] {
  const own = mine && new Set(mine.map(norm));
  return events.filter((e) => {
    if (filter !== "all" && e.kind !== filter) return false;
    if (!own) return true;
    if (e.kind === "ipo") return false;
    return !COMPANY.includes(e.kind) || (!!e.symbol && own.has(norm(e.symbol)));
  });
}

/** Days in order, each with its events in the server's order. */
export function groupByDay(events: CalendarEvent[]): { day: string; events: CalendarEvent[] }[] {
  const days = new Map<string, CalendarEvent[]>();
  for (const e of [...events].sort((a, b) => a.day.localeCompare(b.day))) {
    const list = days.get(e.day);
    if (list) list.push(e);
    else days.set(e.day, [e]);
  }
  return [...days].map(([day, list]) => ({ day, events: list }));
}

/** The stock symbols of the user's radar and holdings (company events exist for stocks only), 50 at most. */
export function stockSymbols(items: { symbol: string; kind: string }[]): string[] {
  return [...new Set(items.filter((i) => i.kind === "stock").map((i) => i.symbol.toUpperCase()))].slice(0, 50);
}

/** No symbol (or none given): the whole calendar, filtered on the device when "Mes actifs" is on. */
export function calendarUrl(days: number, symbols: string[] | null): string {
  return `/api/calendar?days=${days}${symbols?.length ? `&symbols=${encodeURIComponent(symbols.join(","))}` : ""}`;
}

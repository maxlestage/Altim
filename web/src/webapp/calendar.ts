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

/** No symbol (or none given): the whole calendar, filtered on the device when "Mes actifs" is on. `top`: the company
 * events of `symbols` and of the largest companies together (the risk view). */
export function calendarUrl(days: number, symbols: string[] | null, top = false): string {
  return `/api/calendar?days=${days}${symbols?.length ? `&symbols=${encodeURIComponent(symbols.join(","))}` : ""}${top && symbols?.length ? "&top=1" : ""}`;
}

// ---------- Risk by day ----------

/** 🔴 high, 🟠 medium, 🟢 low. */
export type RiskLevel = "high" | "medium" | "low";
export const RISK_ICON: Record<RiskLevel, string> = { high: "🔴", medium: "🟠", low: "🟢" };
export const RISK_LABEL: Record<RiskLevel, string> = { high: "Risque élevé", medium: "Risque modéré", low: "Risque faible" };

/** The categories whose high-importance releases make a 🔴 day. */
const MAJOR: CalendarCategory[] = ["tauxDirecteurs", "inflation", "emploi", "pib"];

/** Risk of one event, null when it does not count (IPOs, other companies' dividends and splits).
 * - 🔴: a high-importance central bank decision / inflation (CPI) / jobs / GDP release, or the earnings of a stock the
 *   user holds or watches;
 * - 🟠: the other macro and central bank events, the earnings of other companies (the calendar keeps the largest US
 *   ones only), a dividend or split of a held stock. */
export function eventRisk(e: CalendarEvent, held: string[], watched: string[]): RiskLevel | null {
  const sym = e.symbol ? norm(e.symbol) : "";
  const isHeld = !!sym && held.some((s) => norm(s) === sym);
  const isMine = isHeld || (!!sym && watched.some((s) => norm(s) === sym));
  switch (e.kind) {
    case "macro":
    case "centralBank":
      return e.importance === "high" && MAJOR.includes(e.category) ? "high" : "medium";
    case "earnings":
      return isMine ? "high" : "medium";
    case "dividend":
    case "split":
      return isHeld ? "medium" : null;
    default:
      return null;
  }
}

/** Short name of an event for the risk row: "CPI", "Décision de taux de la Fed", "Résultats AAPL"… */
export function shortTitle(e: CalendarEvent): string {
  if (e.kind === "earnings" && e.symbol) return `Résultats ${e.symbol}`;
  if (e.kind === "dividend" && e.symbol) return `Dividende ${e.symbol}`;
  if (e.kind === "split" && e.symbol) return `Split ${e.symbol}`;
  // The acronym in brackets when there is one ("Inflation (CPI)" → "CPI"), else the whole title.
  const paren = e.kind === "macro" ? /\(([^)]*[A-Z]{2,}[^)]*)\)\s*$/.exec(e.title)?.[1] : undefined;
  const name = paren ?? e.title;
  return e.country && e.country !== "États-Unis" && e.kind === "macro" ? `${name} (${e.country})` : name;
}

export type RiskDay = {
  /** "YYYY-MM-DD". */
  day: string;
  /** "Lun. 28 sept." */
  label: string;
  weekend: boolean;
  level: RiskLevel;
  /** Short names of the events at the day's level, in the calendar's order, without repeats. */
  main: string[];
  /** Every event that counts, with its risk. */
  events: { event: CalendarEvent; risk: RiskLevel }[];
  /** A source could not be read for this day: 🟢 is then not asserted. */
  incomplete: boolean;
};

const RANK: Record<RiskLevel, number> = { low: 0, medium: 1, high: 2 };

function addDays(day: string, n: number): string {
  const [y = 1970, m = 1, d = 1] = day.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d + n)).toISOString().slice(0, 10);
}

/** "Lun. 28 sept." (a calendar day, read without time zone). */
export function riskDayLabel(day: string): string {
  const [y = 1970, m = 1, d = 1] = day.split("-").map(Number);
  const s = new Intl.DateTimeFormat("fr-FR", { weekday: "short", day: "numeric", month: "short", timeZone: "UTC" }).format(new Date(Date.UTC(y, m - 1, d)));
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** The `n` days from `from` ("YYYY-MM-DD", the report's first day, Paris time), weekends included, each with its
 * risk: the highest of its events' (`eventRisk`), 🟢 when none counts. `failed`: days a source could not read. */
export function riskDays(
  events: CalendarEvent[],
  from: string,
  mine: { held: string[]; watched: string[] },
  n = 7,
  failed: string[] = [],
): RiskDay[] {
  return Array.from({ length: n }, (_, i) => {
    const day = addDays(from, i);
    const dow = new Date(`${day}T12:00:00Z`).getUTCDay();
    const counted = events
      .filter((e) => e.day === day)
      .map((event) => ({ event, risk: eventRisk(event, mine.held, mine.watched) }))
      .filter((x): x is { event: CalendarEvent; risk: RiskLevel } => x.risk !== null);
    const level = counted.reduce<RiskLevel>((l, x) => (RANK[x.risk] > RANK[l] ? x.risk : l), "low");
    const main = [...new Set(counted.filter((x) => x.risk === level).map((x) => shortTitle(x.event)))];
    return { day, label: riskDayLabel(day), weekend: dow === 0 || dow === 6, level, main, events: counted, incomplete: failed.includes(day) };
  });
}

/** Days a source could not read ("YYYY-MM-DD" only: the IPO months do not change the risk). */
export function failedDays(report: CalendarReport): string[] {
  return [...new Set(report.sources.flatMap((s) => (s.failed ?? []).filter((d) => d.length === 10)))];
}

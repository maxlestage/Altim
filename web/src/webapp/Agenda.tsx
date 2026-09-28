import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { Segmented } from "./ui";
import { useAppState, useHoldings } from "./store";
import {
  AGENDA_FILTERS, CATEGORY_LABEL, dayLabel, filterEvents, groupByDay, stockSymbols,
  type AgendaFilter, type CalendarEvent, type CalendarReport,
} from "./calendar";

const FILTER_KEY = "altim.agenda.filter";
const MINE_KEY = "altim.agenda.mine";
type Days = "7" | "14" | "30";
const DAYS: [Days, string][] = [["7", "7 jours"], ["14", "14 jours"], ["30", "30 jours"]];

function remembered<T extends string>(key: string, fallback: T): T {
  try {
    return (localStorage.getItem(key) as T) || fallback;
  } catch {
    return fallback;
  }
}

function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* private browsing: not remembered */
  }
}

function Figures({ e }: { e: CalendarEvent }) {
  if (!e.actual && !e.consensus && !e.previous) return null;
  const earnings = e.kind === "earnings";
  return (
    <div className="agenda-figures small">
      {e.actual && <span>Publié <b>{e.actual}</b></span>}
      {e.consensus && <span>{earnings ? "Consensus" : "Attendu"} <b>{e.consensus}</b></span>}
      {e.previous && (earnings ? <span className="muted">{e.previous}</span> : <span>Précédent <b>{e.previous}</b></span>)}
    </div>
  );
}

export function AgendaEvent({ e }: { e: CalendarEvent }) {
  const high = e.importance === "high";
  return (
    <li className={`agenda-item ${high ? "high" : ""}`}>
      <div className="agenda-head">
        <i className={`agenda-dot ${high ? "high" : "medium"}`} role="img" aria-label={high ? "Importance haute" : "Importance moyenne"} title={high ? "Importance haute" : "Importance moyenne"} />
        <span className="agenda-time small">{e.time ?? "Journée"}</span>
        <b className="agenda-title">{e.title}</b>
      </div>
      <div className="news-tags">
        <span className="chip muted">{CATEGORY_LABEL[e.category]}</span>
        {e.country && <span className="chip muted">{e.country}</span>}
        {e.symbol && <span className="chip">{e.symbol}</span>}
        {e.originalName && e.originalName !== e.title && <span className="chip muted" title="Nom publié par la source">{e.originalName}</span>}
      </div>
      <Figures e={e} />
      {e.detail && <p className="small muted">{e.detail}</p>}
      {e.note && <p className="small agenda-note">{e.note}</p>}
      <a className="small link agenda-source" href={e.url} target="_blank" rel="noopener noreferrer nofollow">
        Source : {e.source}
      </a>
    </li>
  );
}

/** Agenda: the coming days' economic releases, central bank decisions, earnings, dividends, splits and IPOs, each
 * with its source; what no free source covers is listed at the bottom. */
export function Agenda() {
  const { watchlist } = useAppState();
  const { holdings } = useHoldings();
  const [report, setReport] = useState<CalendarReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<AgendaFilter>(() => remembered(FILTER_KEY, "all"));
  const [mine, setMine] = useState(() => remembered<string>(MINE_KEY, "0") === "1");
  const [days, setDays] = useState<Days>("14");

  const stocks = useMemo(() => stockSymbols([...watchlist, ...holdings]), [watchlist, holdings]);
  // "Mes actifs": the server gives these stocks' events (even small companies); without any stock, the whole
  // calendar is filtered on the device.
  const asked = mine && stocks.length > 0 ? stocks : null;
  const askedKey = asked?.join(",") ?? "";

  useEffect(() => {
    let alive = true;
    const load = () => {
      setLoading(true);
      api.calendar(Number(days), askedKey ? askedKey.split(",") : null).then((r) => {
        if (!alive) return;
        setReport(r);
        setError(null);
      }).catch((e) => alive && setError(e instanceof Error ? e.message : "Agenda indisponible"))
        .finally(() => alive && setLoading(false));
    };
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 900_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [days, askedKey]);

  const choose = (f: AgendaFilter) => {
    setFilter(f);
    remember(FILTER_KEY, f);
  };
  const toggleMine = () => {
    setMine(!mine);
    remember(MINE_KEY, mine ? "0" : "1");
  };

  const shown = report ? filterEvents(report.events, filter, mine ? stocks : null) : [];
  const groups = groupByDay(shown);
  const failed = report?.sources.filter((s) => !s.ok) ?? [];

  return (
    <div className="agenda">
      <div className="card">
        <h2 className="card-title">Agenda</h2>
        <p className="muted small">
          Publications économiques majeures, décisions des banques centrales, résultats, dividendes, splits et introductions en bourse. Heures de Paris ; chiffres tels que publiés par la source.
        </p>
        <Segmented label="Période" value={days} options={DAYS} onChange={setDays} />
        <div className="agenda-chips" role="radiogroup" aria-label="Type d'événement">
          {AGENDA_FILTERS.map(([v, l]) => (
            <button key={v} role="radio" aria-checked={filter === v} className={`chip pick ${filter === v ? "on" : ""}`} onClick={() => choose(v)}>
              {l}
            </button>
          ))}
          <button className={`chip pick mine ${mine ? "on" : ""}`} aria-pressed={mine} onClick={toggleMine}>
            {mine ? "✓ " : ""}Mes actifs
          </button>
        </div>
        {mine && (
          <p className="muted small">
            {stocks.length > 0
              ? `Résultats, dividendes et splits de vos ${stocks.length} action${stocks.length > 1 ? "s" : ""} (radar et avoirs), plus l'économie et les banques centrales, qui concernent tous les actifs, cryptos compris.`
              : "Aucune action dans votre radar ni vos avoirs : seules l'économie et les banques centrales, qui concernent aussi les cryptos, sont affichées."}
          </p>
        )}
      </div>

      {error && <p className="notice warn">⚠ {error}</p>}
      {!report && !error && <p className="muted">Chargement de l'agenda…</p>}
      {report && loading && <p className="muted small">Mise à jour…</p>}
      {failed.length > 0 && (
        <p className="notice warn small">
          ⚠ Sources incomplètes : {failed.map((s) => `${s.name}${s.failed?.length ? ` (${s.failed.length} jour${s.failed.length > 1 ? "s" : ""} manquant${s.failed.length > 1 ? "s" : ""})` : ""}`).join(", ")}.
        </p>
      )}
      {report && groups.length === 0 && <p className="muted">Aucun événement de ce type sur la période.</p>}

      {groups.map((g) => (
        <section key={g.day} className="agenda-day">
          <h3 className="section-label">{dayLabel(g.day)}</h3>
          <ul className="news-list">{g.events.map((e, i) => <AgendaEvent key={`${e.kind}:${e.title}:${e.time ?? ""}:${e.symbol ?? ""}:${i}`} e={e} />)}</ul>
        </section>
      ))}

      {report && (
        <div className="card small">
          <h2 className="card-title">Non couvert</h2>
          <ul className="agenda-gaps">{report.notCovered.map((t) => <li key={t}>{t}</li>)}</ul>
          <p className="muted">
            « · » sépare plusieurs séries publiées sous le même nom par la source (souvent la variation sur un mois et sur un an), dans l'ordre de la source.
          </p>
          <details>
            <summary>
              Sources · {report.sources.filter((s) => s.ok).length}/{report.sources.length} en ligne
            </summary>
            <ul className="news-sources">
              {report.sources.map((s) => (
                <li key={s.name} className={s.ok ? "" : "muted"}>
                  {s.ok ? "✔" : "✕"} {s.name}
                  {!s.ok && ` · ${s.error ?? "indisponible"}${s.failed?.length ? ` (${s.failed.join(", ")})` : ""}`}
                </li>
              ))}
            </ul>
          </details>
        </div>
      )}
    </div>
  );
}

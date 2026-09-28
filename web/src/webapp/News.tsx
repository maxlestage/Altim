import { useEffect, useMemo, useState } from "react";
import { api, type NewsReport } from "./api";
import { Segmented } from "./ui";
import { assetKey, useAppState, useHoldings } from "./store";
import type { NewsCategory, NewsItem } from "../engine/news";
import { THEME_LABEL } from "../engine/news";
import { Agenda } from "./Agenda";

type View = "articles" | "agenda";
const VIEWS: [View, string][] = [["articles", "Articles"], ["agenda", "Agenda"]];
const VIEW_KEY = "altim.news.view";

type Filter = "all" | NewsCategory;
const FILTERS: [Filter, string][] = [
  ["all", "Tout"],
  ["actifs", "Mes actifs"],
  ["monde", "Monde"],
  ["marches", "Marchés"],
  ["crypto", "Crypto"],
];
const FILTER_KEY = "altim.news.filter";

function ago(ms: number): string {
  const m = Math.max(0, Math.round((Date.now() - ms) / 60_000));
  if (m < 1) return "à l'instant";
  if (m < 60) return `il y a ${m} min`;
  const h = Math.round(m / 60);
  return h < 24 ? `il y a ${h} h` : `il y a ${Math.round(h / 24)} j`;
}

function Story({ n, featured }: { n: NewsItem; featured?: boolean }) {
  return (
    <li className={`news-item ${featured ? "featured" : ""} ${n.alert ? "alert" : ""}`}>
      <a href={n.link} target="_blank" rel="noopener noreferrer nofollow">
        <b>{n.title}</b>
      </a>
      {featured && n.summary && <p className="small muted">{n.summary}</p>}
      <div className="news-meta small">
        <span>{n.source}</span>
        <span className="muted">{ago(n.time)}</span>
        {n.alsoIn.length > 0 && <span className="muted" title={n.alsoIn.join(", ")}>+{n.alsoIn.length} source{n.alsoIn.length > 1 ? "s" : ""}</span>}
        {n.lang === "en" && <span className="muted">EN</span>}
        {n.tone !== "neutral" && <span className={n.tone === "negative" ? "down" : "up"}>{n.tone === "negative" ? "▼ négatif" : "▲ positif"}</span>}
      </div>
      {(n.assets.length > 0 || n.themes.length > 0 || n.alert) && (
        <div className="news-tags">
          {n.alert && <span className="badge sell">ALERTE</span>}
          {n.assets.map((a) => <span key={a} className="chip">{a.split(":")[1]}</span>)}
          {n.themes.map((t) => <span key={t} className="chip muted">{THEME_LABEL[t]}</span>)}
        </div>
      )}
    </li>
  );
}

/** News section: every feed in one place, stories told by several sources merged, the user's assets first. */
export function News() {
  const { watchlist } = useAppState();
  const { holdings } = useHoldings();
  const [report, setReport] = useState<NewsReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>(() => {
    try {
      return (localStorage.getItem(FILTER_KEY) as Filter) || "all";
    } catch {
      return "all";
    }
  });
  const [frenchOnly, setFrenchOnly] = useState(false);
  const [view, setView] = useState<View>(() => {
    try {
      return localStorage.getItem(VIEW_KEY) === "agenda" ? "agenda" : "articles";
    } catch {
      return "articles";
    }
  });
  const chooseView = (v: View) => {
    setView(v);
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      /* private browsing: not remembered */
    }
  };

  const assets = useMemo(() => {
    const all = [...watchlist, ...holdings.map((h) => ({ symbol: h.symbol, kind: h.kind }))];
    return [...new Map(all.map((a) => [assetKey(a), { symbol: a.symbol, kind: a.kind }])).values()].slice(0, 20);
  }, [watchlist, holdings]);

  useEffect(() => {
    if (view !== "articles") return;
    let alive = true;
    const load = () =>
      api.news(assets).then((r) => {
        if (!alive) return;
        setReport(r);
        setError(null);
      }).catch((e) => alive && setError(e instanceof Error ? e.message : "Actualités indisponibles"));
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 300_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [assets, view]);

  const choose = (f: Filter) => {
    setFilter(f);
    try {
      localStorage.setItem(FILTER_KEY, f);
    } catch {
      /* private browsing: not remembered */
    }
  };

  const shown = (report?.items ?? []).filter((n) => (filter === "all" || n.category === filter) && (!frenchOnly || n.lang === "fr"));
  const top = report ? report.top.map((id) => report.items.find((n) => n.id === id)).filter((n): n is NewsItem => !!n) : [];
  const upSources = report?.sources.filter((s) => s.ok).length ?? 0;

  return (
    <section className="app-screen news">
      <header className="screen-top">
        <div>
          <h1>Actualités</h1>
          <p className="muted small">
            {view === "agenda"
              ? "Les événements à venir, chacun avec sa source."
              : report ? `${report.items.length} articles de ${upSources} sources sur 48 h, mis à jour ${ago(report.asOf)}.` : "Chargement des sources…"}
          </p>
        </div>
      </header>

      <Segmented label="Vue" value={view} options={VIEWS} onChange={chooseView} />

      {view === "agenda" ? <Agenda /> : <>
      {error && <p className="notice warn">⚠ {error}</p>}

      {top.length > 0 && filter === "all" && (
        <div className="card">
          <h2 className="card-title">À la une</h2>
          <p className="muted small">Les sujets repris par plusieurs sources, et toute escalade grave (guerre, panique bancaire…).</p>
          <ul className="news-list">{top.map((n) => <Story key={n.id} n={n} featured />)}</ul>
        </div>
      )}

      {report && report.digest.total > 0 && filter === "all" && (
        <div className="card">
          <h2 className="card-title">Ce qui domine (24 h)</h2>
          <div className="news-tags">
            {report.digest.themes.map((t) => <span key={t.theme} className="chip">{t.label} · {t.count}</span>)}
          </div>
          <div className="tone-bar" role="img" aria-label={`Ton des titres : ${report.digest.tone.negative} négatifs, ${report.digest.tone.neutral} neutres, ${report.digest.tone.positive} positifs`}>
            <i className="neg" style={{ flex: report.digest.tone.negative || 0.0001 }} />
            <i className="neu" style={{ flex: report.digest.tone.neutral || 0.0001 }} />
            <i className="pos" style={{ flex: report.digest.tone.positive || 0.0001 }} />
          </div>
          <p className="muted small">
            Ton des titres : {report.digest.tone.negative} négatifs, {report.digest.tone.neutral} neutres, {report.digest.tone.positive} positifs (repérage par mots-clés, indicatif).
          </p>
        </div>
      )}

      <Segmented label="Rubrique" value={filter} options={FILTERS} onChange={choose} />
      <label className="check small">
        <input type="checkbox" checked={frenchOnly} onChange={(e) => setFrenchOnly(e.target.checked)} /> Articles en français seulement
      </label>

      {report && shown.length === 0 && <p className="muted">Aucun article dans cette rubrique pour le moment.</p>}
      <ul className="news-list">{shown.map((n) => <Story key={n.id} n={n} />)}</ul>

      {report && (
        <details className="card small">
          <summary>
            Sources · {upSources}/{report.sources.length} en ligne
          </summary>
          <ul className="news-sources">
            {report.sources.map((s) => (
              <li key={s.name} className={s.ok ? "" : "muted"}>
                {s.ok ? "✔" : "✕"} {s.name} {s.ok ? `· ${s.count}` : `· ${s.error ?? "indisponible"}`}
              </li>
            ))}
          </ul>
          <p className="muted">Les titres sont affichés tels que publiés (non traduits) ; les liens ouvrent l'article chez sa source.</p>
        </details>
      )}
      </>}
    </section>
  );
}

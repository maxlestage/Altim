import { useEffect, useMemo, useState } from "react";
import { api, type NewsReport } from "./api";
import { Segmented } from "./ui";
import { assetKey, useAppState, useHoldings } from "./store";
import type { NewsCategory, NewsItem } from "../engine/news";
import { THEME_LABEL } from "../engine/news";
import { Agenda } from "./Agenda";
import { onLink } from "./router";
import { IMPACT_LABEL, assetLink, basisLabel, consensusText, moveText, summaryHeading, type StorySummary } from "./news-summary";

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

const TONE_MARK: Record<string, string> = { negative: "▼", positive: "▲", neutral: "·" };

function SummaryEvent({ s }: { s: StorySummary }) {
  return (
    <li className={`news-item summary-item ${s.alert ? "alert" : ""}`}>
      <a href={s.link} target="_blank" rel="noopener noreferrer nofollow"><b>{s.title}</b></a>
      <div className="news-tags">
        <span className={`chip impact ${s.impact}`}>Impact potentiel {IMPACT_LABEL[s.impact]}</span>
        <span className="chip muted">{basisLabel(s)}</span>
      </div>
      {s.assets.length > 0 && (
        <div className="news-tags small">
          <span className="muted">Actifs concernés</span>
          {s.assets.map((id) => {
            const a = assetLink(id);
            return <a key={id} href={a.href} onClick={onLink} className="chip link">{a.symbol}</a>;
          })}
        </div>
      )}
      <p className="small"><span className="muted">Sources </span>{consensusText(s)}</p>
      {s.moves.map((m) => (
        <p key={m.asset} className="small">
          <span className={m.changePct >= 0 ? "up" : "down"}>{moveText(m)}</span>{" "}
          <span className="muted">(clôtures horaires, pas forcément dues à cette actualité)</span>
        </p>
      ))}
      {s.technical.map((t) => (
        <p key={t.asset} className="small summary-tech">
          <span className="muted">Impact sur le signal technique{s.technical.length > 1 ? ` (${assetLink(t.asset).symbol})` : ""} : </span>{t.text}
        </p>
      ))}
      <details className="small">
        <summary>Sources et calcul</summary>
        <ul className="news-sources">
          {s.links.map((l) => (
            <li key={l.link}>
              <span className={l.tone === "negative" ? "down" : l.tone === "positive" ? "up" : "muted"} aria-label={l.tone}>{TONE_MARK[l.tone]}</span>{" "}
              <a className="link" href={l.link} target="_blank" rel="noopener noreferrer nofollow">{l.source}</a> <span className="muted">· {l.title}</span>
            </li>
          ))}
        </ul>
        <p className="muted">
          Règle : {s.impactReasons.filter((r) => r.includes("(+")).join(" ; ")} → {s.impactPoints} point{s.impactPoints > 1 ? "s" : ""}, impact {IMPACT_LABEL[s.ruleImpact]} par règle
          {s.impactBasis === "measured" ? ` ; retenu : ${IMPACT_LABEL[s.impact]}, d'après la variation mesurée (${s.impactReasons.filter((r) => !r.includes("(+")).join(" ; ")}).` : "."}
        </p>
      </details>
    </li>
  );
}

function Summary({ list }: { list: StorySummary[] }) {
  const head = summaryHeading(list);
  return (
    <div className="card news-summary">
      <h2 className="card-title">{head.title}</h2>
      {head.others && <p className="small">{head.others}</p>}
      <p className="muted small">
        Sujets des dernières 24 h repris par plusieurs sources indépendantes, escalades graves, ou articles citant vos actifs sur un sujet sensible. Impact potentiel indicatif, pas un signal.
      </p>
      {list.length > 0 && <ol className="news-list">{list.map((s) => <SummaryEvent key={s.id} s={s} />)}</ol>}
      <details className="small">
        <summary>Comment l'impact est estimé</summary>
        <p className="muted">
          Par règle : sources indépendantes, un même titre repris mot pour mot comptant pour une (2–3 : +1, 4 et plus : +2), thème (escalade grave +2 ; banques centrales, régulation ou piratage +1),
          cite un de vos actifs (+1). 0–1 point : faible, 2–3 : moyen, 4 et plus : important.
        </p>
        <p className="muted">
          Mesuré : si les bougies horaires d'un actif cité sont déjà en mémoire sur le serveur, la plus forte variation depuis la clôture précédant la
          publication (action : 1 % moyen, 3 % important ; crypto : 2 % et 5 %). Une variation après un article ne prouve pas qu'il en est la cause.
        </p>
        <p className="muted">
          Consensus : ton des titres de chaque source (repérage par mots-clés) ; divergent dès qu'un titre est négatif et un autre positif.
        </p>
      </details>
    </div>
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

      {report?.summary && <Summary list={report.summary} />}

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

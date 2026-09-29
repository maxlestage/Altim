import { useEffect, useMemo, useState } from "react";
import { api, type BriefReport } from "./api";
import { assetKey, useAppState, useHoldings } from "./store";
import { onLink } from "./router";

const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;

/** "Point du jour": the day in a few lines for the radar and the holdings, refreshed every 5 minutes. */
export function BriefCard() {
  const { watchlist } = useAppState();
  const { holdings } = useHoldings();
  const [brief, setBrief] = useState<BriefReport | null>(null);
  const assets = useMemo(() => {
    const all = [...watchlist, ...holdings.map((h) => ({ symbol: h.symbol, kind: h.kind }))];
    return [...new Map(all.map((a) => [assetKey(a), { symbol: a.symbol, kind: a.kind }])).values()].slice(0, 20);
  }, [watchlist, holdings]);

  useEffect(() => {
    let alive = true;
    const load = () => api.brief(assets).then((b) => alive && setBrief(b)).catch(() => {});
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 300_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [assets]);

  if (!brief) return <div className="skeleton" aria-label="Chargement du point du jour" />;
  const moves = brief.movers.filter((m) => Math.abs(m.change) >= 0.05).slice(0, 4);
  return (
    <div className="card brief-card">
      <h2 className="card-title">Point du jour</h2>
      <p className="brief-headline">{brief.headline}</p>
      {brief.market && brief.market.themes.length > 0 && (
        <p className="muted small">Sujets du moment : {brief.market.themes.join(", ").toLowerCase()}.</p>
      )}
      {moves.length > 0 && (
        <div className="news-tags">
          {moves.map((m) => (
            <a key={`${m.kind}:${m.symbol}`} className="chip" href={`/app/actif/${m.kind}/${m.symbol}`} onClick={onLink}>
              {m.symbol} <b className={m.change >= 0 ? "up" : "down"}>{pct(m.change)}</b>
            </a>
          ))}
        </div>
      )}
      {brief.news.length > 0 && (
        <ul className="brief-news small">
          {brief.news.map((n) => (
            <li key={n.id}>
              {n.alert && <span className="badge sell">ALERTE</span>}{" "}
              <a href={n.link} target="_blank" rel="noopener noreferrer nofollow">{n.title}</a> <span className="muted">· {n.source}{n.alsoIn.length ? ` +${n.alsoIn.length}` : ""}</span>
            </li>
          ))}
        </ul>
      )}
      <p className="muted small">Achetable = la décision complète de l'actif dit ACHETER ou ZONE D'ACHAT (la règle des notifications) ; tant qu'elle n'est pas calculée, rien n'est annoncé. Variations depuis la dernière clôture journalière. Conseil indicatif : Altim ne passe aucun ordre.</p>
    </div>
  );
}

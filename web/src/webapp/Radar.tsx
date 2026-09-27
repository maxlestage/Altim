import { useCallback, useEffect, useRef, useState } from "react";
import { api, INTERVAL_LABEL, type RadarRow, type Sentiment } from "./api";
import { onLink } from "./router";
import { assetKey, setState, useAppState, type Interval } from "./store";
import { ActionBadge, Change, Price, ReliabilityBadge, Segmented, Sparkline } from "./ui";

export function Radar() {
  const { watchlist, interval } = useAppState();
  const [rows, setRows] = useState<Record<string, RadarRow>>({});
  const [loading, setLoading] = useState(false);
  const [updated, setUpdated] = useState<Date | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fg, setFg] = useState<Sentiment["fearGreed"]>();
  const busy = useRef(false);

  const refresh = useCallback(async () => {
    if (busy.current || !watchlist.length) return;
    busy.current = true;
    setLoading(true);
    try {
      const data = await api.radar(watchlist, interval);
      // An asset in error keeps its last valid data.
      setRows((prev) => {
        const next = { ...prev };
        for (const r of data) {
          const k = assetKey(r);
          next[k] = r.error && prev[k]?.signal ? { ...prev[k]!, error: r.error } : r;
        }
        return next;
      });
      setError(null);
      setUpdated(new Date());
    } catch (e) {
      setError(e instanceof Error ? e.message : "Réseau indisponible");
    } finally {
      busy.current = false;
      setLoading(false);
    }
  }, [watchlist, interval]);

  useEffect(() => {
    setRows({});
    refresh();
    const id = setInterval(() => document.visibilityState === "visible" && refresh(), 60_000);
    return () => clearInterval(id);
  }, [refresh]);

  useEffect(() => {
    api.sentiment("BTC", "crypto").then((s) => setFg(s.fearGreed)).catch(() => {});
  }, []);

  const opportunities = watchlist
    .map((w) => rows[assetKey(w)])
    .filter((r): r is RadarRow => !!r?.signal && r.signal.action !== "hold" && r.signal.confidence >= 40)
    .sort((a, b) => b.signal!.confidence - a.signal!.confidence)
    .slice(0, 3);

  return (
    <section className="app-screen">
      <div className="screen-top">
        <div>
          <h1>Radar</h1>
          <p className="muted small">
            {updated ? `Mis à jour à ${updated.toLocaleTimeString("fr-FR")}` : "Interrogation des sources…"}
            {loading && updated ? " · actualisation…" : ""}
          </p>
        </div>
        {fg && (
          <div className={`fg ${fg.value < 45 ? "down" : fg.value > 55 ? "up" : ""}`} title="Indice Fear & Greed crypto (contexte, hors score)">
            <b>{fg.value}</b>
            <small>{fg.label}</small>
          </div>
        )}
      </div>

      <Segmented<Interval>
        label="Unité de temps"
        value={interval}
        onChange={(v) => setState({ interval: v })}
        options={(["1h", "4h", "1d"] as Interval[]).map((i) => [i, INTERVAL_LABEL[i]])}
      />

      {error && <p className="notice warn">⚠ {error} — nouvelle tentative automatique.</p>}

      {opportunities.length > 0 && (
        <div className="card opportunities">
          <h2 className="card-title">Opportunités détectées</h2>
          {opportunities.map((r) => (
            <a key={assetKey(r)} href={`/app/actif/${r.kind}/${r.symbol}`} onClick={onLink} className="opp-row">
              <b>{r.name}</b>
              <span className="muted">{Math.round(r.signal!.confidence)} %</span>
              <ActionBadge action={r.signal!.action} />
            </a>
          ))}
        </div>
      )}

      <ul className="asset-list">
        {watchlist.map((w) => {
          const r = rows[assetKey(w)];
          return (
            <li key={assetKey(w)}>
              <a href={`/app/actif/${w.kind}/${w.symbol}`} onClick={onLink} className={`asset-card ${r?.signal ? r.signal.action : ""}`}>
                <div className="asset-id">
                  <b>{w.name}</b>
                  <small>{w.symbol} · {w.kind === "crypto" ? "Crypto" : "Action"}</small>
                </div>
                <Sparkline values={r?.sparkline ?? []} />
                <div className="asset-price">
                  <b><Price value={r?.price} /></b>
                  <Change value={r?.change} />
                </div>
                <div className="asset-meta">
                  {r?.signal ? <ActionBadge action={r.signal.action} /> : r?.error ? <span className="badge hold">INDISPONIBLE</span> : <span className="skeleton-line" />}
                  {r?.reliability && <ReliabilityBadge rel={r.reliability} />}
                  {r?.priceSources && <small className="muted">prix {r.priceSources} sources</small>}
                </div>
              </a>
            </li>
          );
        })}
      </ul>
      {!watchlist.length && (
        <p className="muted">
          Votre radar est vide. <a href="/app/reglages" onClick={onLink} className="link">Ajouter des actifs</a>
        </p>
      )}
    </section>
  );
}

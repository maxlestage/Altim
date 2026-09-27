import { useCallback, useEffect, useRef, useState } from "react";
import { api, INTERVAL_LABEL, type MacroInfo, type RadarRow, type Sentiment } from "./api";
import { onLink } from "./router";
import { assetKey, setState, useAppState, type Interval } from "./store";
import { ActionBadge, Change, ReliabilityBadge, Segmented, Sparkline } from "./ui";
import { LiveBadge, LivePrice, useLive } from "./live";
import { formatPrice } from "../market";

export function Radar() {
  const { watchlist, interval } = useAppState();
  const [rows, setRows] = useState<Record<string, RadarRow>>({});
  const [loading, setLoading] = useState(false);
  const [updated, setUpdated] = useState<Date | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fg, setFg] = useState<Sentiment["fearGreed"]>();
  const [macro, setMacro] = useState<MacroInfo | null>(null);
  const busy = useRef(false);
  const live = useLive(watchlist);

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
    const loadMacro = () => api.macro().then(setMacro).catch(() => {});
    loadMacro();
    const id = setInterval(() => document.visibilityState === "visible" && loadMacro(), 300_000);
    return () => clearInterval(id);
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
          <LiveBadge status={live.status} last={live.last} />
          <p className="muted small">
            {updated ? `Signaux recalculés à ${updated.toLocaleTimeString("fr-FR")}` : "Interrogation des sources…"}
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

      {macro && macro.level !== "calm" && (
        <div className={`notice ${macro.level === "high" ? "danger" : "warn"}`} role="status">
          <b>Contexte macro {macro.level === "high" ? "très tendu" : "tendu"} ({macro.score}/100)</b>
          <ul className="small">{macro.factors.slice(0, 3).map((f) => <li key={f.code}>{f.text}</li>)}</ul>
          <small>Les zones d'achat techniques résistent mal aux crises : tailles réduites, achats échelonnés.</small>
        </div>
      )}

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
          const t = live.ticks[assetKey(w)];
          // The sparkline ends on the live price: the chart moves with the market.
          const spark = r?.sparkline?.length && t ? [...r.sparkline, t.price] : (r?.sparkline ?? []);
          return (
            <li key={assetKey(w)}>
              <a href={`/app/actif/${w.kind}/${w.symbol}`} onClick={onLink} className={`asset-card ${r?.signal ? r.signal.action : ""}`}>
                <div className="asset-id">
                  <b>{w.name}</b>
                  <small>{w.symbol} · {w.kind === "crypto" ? "Crypto" : "Action"}</small>
                </div>
                <Sparkline values={spark} />
                <div className="asset-price">
                  <b><LivePrice tick={t} fallback={r?.price} format={(v) => `${formatPrice(v)} $`} /></b>
                  <Change value={t?.change ?? r?.change} />
                  {t?.market === "closed" && <small className="market-closed" title="Bourse de New York fermée : dernier cours connu">Bourse fermée</small>}
                </div>
                <div className="asset-meta">
                  {r?.signal ? <ActionBadge action={r.signal.action} /> : r?.error ? <span className="badge hold">INDISPONIBLE</span> : <span className="skeleton-line" />}
                  {r?.reliability && <ReliabilityBadge rel={r.reliability} />}
                  {t ? <small className="muted">prix {t.agreeing}/{t.total} sources</small> : r?.priceSources && <small className="muted">prix {r.priceSources} sources</small>}
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

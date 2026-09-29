import { useCallback, useEffect, useRef, useState } from "react";
import { api, INTERVAL_LABEL, type BuyAlert, type MacroInfo, type RadarRow, type Sentiment } from "./api";
import { onLink } from "./router";
import { assetKey, setState, useAppState, type Interval, type WatchItem } from "./store";
import { BriefCard } from "./BriefCard";
import { CompareCard } from "./ToolCards";
import { Change, ReliabilityBadge, Segmented, Sparkline, technicalText } from "./ui";
import { LiveBadge, LivePrice, useLive } from "./live";
import { DecisionBadge, RATING_RANK, ratingTone } from "./DecisionCard";
import { formatPrice } from "../market";
import { cacheDecision, cachedDecision, shortDateTime, type Decision } from "./decision";
import { clearTransitions, recordConfiguration, transitionTitle, useTransitions } from "./config-changes";

/** The cached full decision when under 12 h old (like the badge): an older one never sorts nor lists an asset. */
function freshDecision(w: WatchItem): Decision | undefined {
  const c = cachedDecision(w.kind, w.symbol);
  return c && Date.now() - c.at <= 12 * 3_600_000 ? c.decision : undefined;
}
import { readDangers } from "./danger-store";

/** The decisions of the radar are re-read at most this often (they are heavier than the signals). */
const DECISION_EVERY = 15 * 60_000;

export function Radar() {
  const { watchlist, interval } = useAppState();
  const [rows, setRows] = useState<Record<string, RadarRow>>({});
  const [loading, setLoading] = useState(false);
  const [updated, setUpdated] = useState<Date | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fg, setFg] = useState<Sentiment["fearGreed"]>();
  const [macro, setMacro] = useState<MacroInfo | null>(null);
  const [buyable, setBuyable] = useState<BuyAlert[] | null>(null);
  const busy = useRef(false);
  const live = useLive(watchlist);
  // Order of the list: the user's own, the largest moves first, or the strongest buy signals first (remembered).
  const [sort, setSort] = useState<"mine" | "change" | "signal">(() => {
    try {
      const s = localStorage.getItem("altim.radar.sort");
      return s === "change" || s === "signal" ? s : "mine";
    } catch {
      return "mine";
    }
  });
  const chooseSort = (s: "mine" | "change" | "signal") => {
    setSort(s);
    try {
      localStorage.setItem("altim.radar.sort", s);
    } catch {
      /* private browsing: not remembered */
    }
  };
  const sorted = sort === "mine"
    ? watchlist
    : [...watchlist].sort((a, b) => {
        const ra = rows[assetKey(a)];
        const rb = rows[assetKey(b)];
        if (sort === "change") {
          const ca = live.ticks[assetKey(a)]?.change ?? ra?.change ?? null;
          const cb = live.ticks[assetKey(b)]?.change ?? rb?.change ?? null;
          return Math.abs(cb ?? -1) - Math.abs(ca ?? -1);
        }
        // "Décision": the full decision's rating (buy side first), then its confidence.
        const da = freshDecision(a);
        const db = freshDecision(b);
        const rank = (d: typeof da) => (d?.rating ? RATING_RANK[d.rating] : 9);
        return rank(da) - rank(db) || (db?.confidence ?? 0) - (da?.confidence ?? 0);
      });

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

  // Same rule as the notifications of the iPhone, Apple Watch and Android apps, refreshed every 5 minutes.
  useEffect(() => {
    if (!watchlist.length) return setBuyable([]);
    const load = () => api.alerts(watchlist).then((a) => setBuyable(a.filter((x) => x.buy))).catch(() => setBuyable(null));
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 300_000);
    return () => clearInterval(id);
  }, [watchlist]);

  // Decisions of the watched assets (market data only), re-read every 15 minutes: each one goes through the
  // configuration diff (config-changes.ts). A fresher personal decision seen on the asset page stays in the cache.
  const transitions = useTransitions();
  // The badges read the decisions from the browser's cache: a new one re-renders the list.
  const [, setDecisionsSeen] = useState(0);
  const [showAll, setShowAll] = useState(false);
  useEffect(() => {
    let alive = true;
    const load = async () => {
      const due = watchlist.slice(0, 20).filter((w) => {
        const c = cachedDecision(w.kind, w.symbol);
        return !c || Date.now() - c.at >= DECISION_EVERY;
      });
      // Two at a time: the server fetches fundamentals and order books for each one.
      for (let i = 0; i < due.length && alive; i += 2) {
        await Promise.all(due.slice(i, i + 2).map(async (w) => {
          try {
            const d = await api.decision(w.symbol, w.kind);
            if (!alive) return;
            if (cachedDecision(w.kind, w.symbol)?.personal) recordConfiguration(d, false);
            else cacheDecision(d, false);
            setDecisionsSeen((n) => n + 1);
          } catch {
            /* unavailable: compared at the next refresh */
          }
        }));
      }
    };
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), DECISION_EVERY);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [watchlist]);
  const dangers = readDangers();

  const cardTone = (w: WatchItem) => {
    const d = freshDecision(w);
    return d ? ratingTone(d.rating, d.verdict) : "";
  };

  // Assets whose full decision is ACHETER or ZONE D'ACHAT (not the 4 h technical signal alone).
  const opportunities = watchlist
    .map((w) => ({ w, d: freshDecision(w) }))
    .filter((x): x is { w: WatchItem; d: Decision } => !!x.d && (x.d.verdict === "buy" || x.d.verdict === "buyZone"))
    .sort((a, b) => b.d.confidence - a.d.confidence)
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

      <BriefCard />

      {error && <p className="notice warn">⚠ {error} — nouvelle tentative automatique.</p>}

      {macro && macro.level !== "calm" && (
        <div className={`notice ${macro.level === "high" ? "danger" : "warn"}`} role="status">
          <b>Contexte macro {macro.level === "high" ? "très tendu" : "tendu"} ({macro.score}/100)</b>
          <ul className="small">{macro.factors.slice(0, 3).map((f) => <li key={f.code}>{f.text}</li>)}</ul>
          <small>Les zones d'achat techniques résistent mal aux crises : tailles réduites, achats échelonnés.</small>
        </div>
      )}

      {dangers && dangers.items.length > 0 && (
        <div className="notice danger" role="status">
          <b>⚠ {dangers.items.length > 1 ? "Positions devenues dangereuses" : "Position devenue dangereuse"} dans vos avoirs</b>
          <ul className="small">
            {dangers.items.map((d) => <li key={d.id}><b>{d.symbol}</b> : {d.reasons.map((r) => r.text).join(" ")}</li>)}
          </ul>
          <small>
            Mesuré le {shortDateTime(dangers.at)} sur <a href="/app/avoirs" onClick={onLink} className="link">Mes avoirs</a> (ouvrez-le pour actualiser).
          </small>
        </div>
      )}

      {transitions.length > 0 && (
        <div className="card">
          <h2 className="card-title">Changements de configuration · {transitions.length}</h2>
          <ul className="insights">
            {(showAll ? transitions : transitions.slice(0, 5)).map((t) => (
              <li key={`${t.kind}:${t.symbol}:${t.personal}:${t.at}`} className={`insight ${t.to.verdict === "buy" || t.to.verdict === "buyZone" ? "good" : t.to.verdict === "sell" || t.to.verdict === "trim" ? "danger" : "info"}`}>
                <span aria-hidden>↻</span>
                <span>
                  <a href={`/app/actif/${t.kind}/${t.symbol}`} onClick={onLink} className="link"><b>{transitionTitle(t)}</b></a>
                  <br />
                  <small className="muted">
                    {shortDateTime(t.at)} · niveau {t.from.levelLabel} → {t.to.levelLabel} · configuration précédente vue le {shortDateTime(t.since)}
                    {t.personal ? " · mode personnel" : ""}
                  </small>
                  {t.changes.length > 0 && (
                    <>
                      <br />
                      <small>Pourquoi le signal a changé : {t.changes.join(" ; ")}.</small>
                    </>
                  )}
                  {t.missing.length > 0 && (
                    <>
                      <br />
                      <small>Conditions manquantes : {t.missing.join(" ; ")}.</small>
                    </>
                  )}
                  {t.triggers.length > 0 && (
                    <>
                      <br />
                      <small>Ce qui changerait la décision : {t.triggers.join(" ; ")}.</small>
                    </>
                  )}
                </span>
              </li>
            ))}
          </ul>
          <div className="row-actions">
            {transitions.length > 5 && <button className="link-btn" onClick={() => setShowAll((v) => !v)}>{showAll ? "Voir moins" : `Voir les ${transitions.length}`}</button>}
            <button className="link-btn" onClick={() => confirm("Effacer l'historique des changements ?") && clearTransitions()}>Effacer</button>
          </div>
          <small className="muted">
            Comparaison avec la dernière décision vue dans ce navigateur. Nouvelle analyse toutes les 15 minutes tant que le radar est ouvert, et à chaque ouverture d'une fiche. 50 derniers
            changements conservés ici uniquement.
          </small>
        </div>
      )}

      {buyable && buyable.length > 0 && (
        <div className="card buyable">
          <h2 className="card-title">Achetables maintenant · {buyable.length}</h2>
          <ul className="buyable-list">
            {buyable.map((a) => (
              <li key={`${a.kind}:${a.symbol}`}>
                <a href={`/app/actif/${a.kind}/${a.symbol}`} onClick={onLink} className="opp-row">
                  <b>{a.name}</b>
                  <span className="mono">{a.price != null ? `${formatPrice(a.price)} $` : "—"}</span>
                  <span className="badge buy">{a.strong ? "ACHETER" : "ZONE D'ACHAT"}</span>
                </a>
                <small className="muted">{[...(a.reasons ?? []), ...(a.cautions ?? [])].join(" ")}</small>
              </li>
            ))}
          </ul>
          <small className="muted">
            Listés seulement quand la décision complète de l'actif dit ACHETER ou ZONE D'ACHAT (signal 4 h ou zone Fibonacci, sans source en désaccord, choc ni zone cassée) : la même règle que les notifications des apps. Conseil indicatif.
          </small>
        </div>
      )}

      {opportunities.length > 0 && (
        <div className="card opportunities">
          <h2 className="card-title">Opportunités détectées</h2>
          {opportunities.map(({ w, d }) => (
            <a key={assetKey(w)} href={`/app/actif/${w.kind}/${w.symbol}`} onClick={onLink} className="opp-row">
              <b>{w.name}</b>
              <span className="muted">confiance {Math.round(d.confidence)}</span>
              <DecisionBadge kind={w.kind} symbol={w.symbol} />
            </a>
          ))}
        </div>
      )}

      {watchlist.length > 1 && (
        <Segmented label="Trier le radar" value={sort} options={[["mine", "Mon ordre"], ["change", "Variation"], ["signal", "Décision"]]} onChange={chooseSort} />
      )}
      <ul className="asset-list">
        {sorted.map((w) => {
          const r = rows[assetKey(w)];
          const t = live.ticks[assetKey(w)];
          // The sparkline ends on the live price: the chart moves with the market.
          const spark = r?.sparkline?.length && t ? [...r.sparkline, t.price] : (r?.sparkline ?? []);
          return (
            <li key={assetKey(w)}>
              <a href={`/app/actif/${w.kind}/${w.symbol}`} onClick={onLink} className={`asset-card ${cardTone(w)}`}>
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
                  <DecisionBadge kind={w.kind} symbol={w.symbol} />
                  {r?.signal ? (
                    <small className="muted" title="Signal technique sur bougies de 4 h : un indice parmi d'autres de la décision">technique {INTERVAL_LABEL[interval]} : {technicalText(r.signal.action)}</small>
                  ) : r?.error ? <small className="muted">signal technique indisponible</small> : <span className="skeleton-line" />}
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

      {watchlist.length >= 2 && <CompareCard assets={watchlist} />}
    </section>
  );
}

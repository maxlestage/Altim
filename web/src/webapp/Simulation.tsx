import { currencySymbol, displayCurrency, fromDisplay } from "../money";
import { FxNote } from "./FxNote";
import { useEffect, useMemo, useState } from "react";
import { checkExits, closePosition, DEFAULT_CAPITAL, paperStats, valuation, type DailyCandle, type OpenLine, type PaperTrade } from "../engine/paper";
import { api } from "./api";
import { onLink } from "./router";
import { LiveBadge, useLive } from "./live";
import { getPaper, resetPaper, setPaper, usePaper } from "./paper-store";
import {
  assetsToCheck, exitNotice, FEW_TRADES, FEW_TRADES_NOTE, frDate, journal, pct, price as fmtPrice, qty, REASON_LABEL, signedPct, signedUsd, startCapital, toDaily, usd, verdictRows,
} from "./paper-ui";

/** "Mes avoirs réels" / "Simulation" switch, at the top of both screens. */
export function PortfolioTabs({ active }: { active: "real" | "paper" | "journal" }) {
  return (
    <nav className="portfolio-tabs three" aria-label="Portefeuille">
      <a href="/app/avoirs" onClick={onLink} className={active === "real" ? "on" : ""} aria-current={active === "real" ? "page" : undefined}>Mes avoirs réels</a>
      <a href="/app/simulation" onClick={onLink} className={active === "paper" ? "on" : ""} aria-current={active === "paper" ? "page" : undefined}>
        Simulation <small>(sans argent réel)</small>
      </a>
      <a href="/app/journal" onClick={onLink} className={active === "journal" ? "on" : ""} aria-current={active === "journal" ? "page" : undefined}>Journal</a>
    </nav>
  );
}

const upDown = (v: number | null) => (v == null ? "" : v > 0 ? "up" : v < 0 ? "down" : "");
const CHECK_EVERY = 300_000;

/** Paper trading screen (/app/simulation): simulated portfolio, open positions, journal and statistics. */
export function Simulation() {
  const { state, error: loadError } = usePaper();
  const [quotes, setQuotes] = useState<Record<string, number>>({});
  const [notices, setNotices] = useState<string[]>([]);
  const [check, setCheck] = useState<{ running: boolean; at: number | null; error: string | null }>({ running: false, at: null, error: null });
  const [restarting, setRestarting] = useState(false);
  const [sellError, setSellError] = useState<string | null>(null);

  const positions = state?.positions ?? [];
  const assets = useMemo(() => [...new Map(positions.map((p) => [`${p.kind}:${p.symbol}`, { symbol: p.symbol, kind: p.kind }])).values()], [positions]);
  const assetsKey = assets.map((a) => `${a.kind}:${a.symbol}`).sort().join(",");

  // Live prices (same stream as the rest of the app) + consensus quotes as a fallback.
  const live = useLive(assets);
  useEffect(() => {
    if (!assets.length) return;
    let alive = true;
    const load = () => api.quotes(assets).then((q) => alive && setQuotes(Object.fromEntries(q.map((x) => [`${x.kind}:${x.symbol}`, x.price])))).catch(() => {});
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 120_000);
    return () => { alive = false; clearInterval(id); };
  }, [assetsKey]);
  const prices = useMemo(() => {
    const m: Record<string, number> = { ...quotes };
    for (const [k, t] of Object.entries(live.ticks)) m[k] = t.price;
    return m;
  }, [quotes, live.ticks]);

  // Automatic exits: daily candles of the positions with a stop or a target, on opening and every 5 min while visible.
  useEffect(() => {
    let alive = true;
    const run = async () => {
      const s = getPaper();
      const list = s ? assetsToCheck(s.positions) : [];
      if (!list.length) return;
      setCheck((c) => ({ ...c, running: true }));
      const candles: Record<string, DailyCandle[]> = {};
      let failed = 0;
      await Promise.all(list.map((a) => api.candles(a.symbol, a.kind, "1d")
        .then((snap) => { candles[`${a.kind}:${a.symbol}`] = toDaily(snap.candles); })
        .catch(() => { failed++; })));
      if (!alive) return;
      const latest = getPaper();
      if (latest) {
        const { state: next, closed } = checkExits(latest, candles);
        if (closed.length) {
          setPaper(next);
          setNotices((n) => [...closed.map(exitNotice), ...n]);
        }
      }
      setCheck({ running: false, at: Date.now(), error: failed ? `${failed} actif${failed > 1 ? "s" : ""} sans bougies (nouvel essai dans 5 min).` : null });
    };
    run();
    const id = setInterval(() => document.visibilityState === "visible" && run(), CHECK_EVERY);
    const onVisible = () => document.visibilityState === "visible" && run();
    document.addEventListener("visibilitychange", onVisible);
    return () => { alive = false; clearInterval(id); document.removeEventListener("visibilitychange", onVisible); };
  }, [assetsKey]);

  const val = useMemo(() => (state ? valuation(state, prices) : null), [state, prices]);
  const stats = useMemo(() => (state ? paperStats(state) : null), [state]);

  const sell = (l: OpenLine) => {
    setSellError(null);
    const p = prices[`${l.kind}:${l.symbol}`];
    if (!(p != null && p > 0)) return setSellError(`Cours de ${l.name} indisponible : vente simulée impossible pour l'instant.`);
    const estimate = l.value != null && l.pnl != null ? ` Résultat estimé : ${signedUsd(l.pnl)} (${signedPct(l.pnlPct ?? 0)}).` : "";
    if (!confirm(`Vendre (simulé) toute la position ${l.name} au cours de ${fmtPrice(p)} ?${estimate}\nAucun ordre réel n'est passé.`)) return;
    const s = getPaper();
    if (!s) return;
    const res = closePosition(s, l.id, p, Date.now());
    if (res.error) return setSellError(res.error);
    setPaper(res.state);
  };

  return (
    <section className="app-screen simulation">
      <PortfolioTabs active="paper" />
      <div className="screen-top">
        <h1>Simulation</h1>
        {state && <button className="btn btn-small btn-ghost" onClick={() => setRestarting(true)}>Recommencer</button>}
      </div>
      <p className="paper-banner"><span aria-hidden>🧪</span> Portefeuille simulé — aucun argent réel, aucun ordre passé</p>

      {loadError && <p className="notice warn" role="alert">⚠ {loadError}</p>}

      {notices.length > 0 && (
        <div className="paper-notices" role="status">
          {notices.map((n) => <p key={n} className="notice small">🔔 {n}</p>)}
          <button className="link-btn" onClick={() => setNotices([])}>Masquer ces avis</button>
        </div>
      )}

      {!state ? (
        <StartCard onStart={(c) => resetPaper(fromDisplay(c))} />
      ) : (
        <>
          <div className="card paper-summary">
            <div className="summary-top">
              <small className="muted">Valeur du portefeuille simulé</small>
              {positions.length > 0 && <LiveBadge status={live.status} last={live.last} />}
            </div>
            <b className="mono big">{val ? usd(val.equity) : "…"}</b>
            {val && (
              <p className={`paper-pnl mono ${upDown(val.pnl)}`}>
                {signedUsd(val.pnl)} ({signedPct(val.pnlPct)}) <span className="muted">depuis le début</span>
              </p>
            )}
            <dl className="paper-figures">
              <div><dt>Capital de départ</dt><dd className="mono">{usd(state.startCapital)}</dd></div>
              <div><dt>Liquidités</dt><dd className="mono">{usd(state.cash)}</dd></div>
              <div><dt>Positions ouvertes</dt><dd className="mono">{val ? usd(val.positionsValue) : "…"}</dd></div>
              <div><dt>Début</dt><dd>{frDate(state.startedAt)}</dd></div>
            </dl>
            {val && val.unpriced > 0 && <p className="muted small">{val.unpriced} position{val.unpriced > 1 ? "s" : ""} sans cours pour l'instant : comptée{val.unpriced > 1 ? "s" : ""} à son prix d'achat.</p>}
            <p className="muted small">La valeur compte les frais et le glissement d'une vente immédiate.</p>
            {displayCurrency() === "EUR" && <p className="muted small">Portefeuille simulé tenu en $ comme les cours ; montants saisis en € convertis au taux du jour de la saisie, affichés au taux du jour.</p>}
            <FxNote />
          </div>

          <div className="card">
            <h2 className="card-title">Positions ouvertes · {positions.length}</h2>
            {!positions.length && (
              <p className="muted small">
                Aucune position. Ouvrez la fiche d'un actif (depuis le <a href="/app" onClick={onLink} className="link">Radar</a>) et touchez « Simuler cet achat » dans la carte Décision.
              </p>
            )}
            {sellError && <p className="notice danger small" role="alert">✕ {sellError}</p>}
            {check.running && <p className="muted small" role="status">Vérification des stops et objectifs sur les bougies journalières…</p>}
            {!check.running && check.at && assetsToCheck(positions).length > 0 && (
              <p className="muted small">Stops et objectifs vérifiés à {new Date(check.at).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" })} (toutes les 5 min).</p>
            )}
            {check.error && <p className="notice warn small">⚠ {check.error}</p>}
            {val && val.lines.length > 0 && (
              <ul className="paper-positions">
                {val.lines.map((l) => <PositionCard key={l.id} l={l} onSell={() => sell(l)} />)}
              </ul>
            )}
          </div>

          {stats && <StatsCard stats={stats} />}

          <div className="card">
            <h2 className="card-title">Journal des trades clôturés · {state.trades.length}</h2>
            {state.trades.length === 0 ? (
              <p className="muted small">Aucun trade clôturé pour l'instant : les ventes (manuelles, au stop ou à l'objectif) apparaîtront ici, la plus récente en premier.</p>
            ) : (
              <TradesTable trades={journal(state.trades)} />
            )}
          </div>
        </>
      )}

      <Rules />

      {restarting && <RestartSheet onClose={() => setRestarting(false)} onConfirm={(c) => { resetPaper(fromDisplay(c)); setNotices([]); setRestarting(false); }} />}
    </section>
  );
}

function PositionCard({ l, onSell }: { l: OpenLine; onSell: () => void }) {
  const d = l.decision;
  return (
    <li className="paper-pos">
      <div className="holding-head">
        <a href={`/app/actif/${l.kind}/${l.symbol}`} onClick={onLink} className="holding-name">
          <b>{l.name}</b>
          <small className="muted mono">{qty(l.quantity)} {l.symbol} · investi {usd(l.invested)} · ouvert le {frDate(l.openedAt)}</small>
        </a>
        <b className={`mono paper-pos-pnl ${upDown(l.pnl)}`}>{l.pnl != null ? signedPct(l.pnlPct ?? 0) : "—"}</b>
      </div>
      <p className="paper-decision small">
        {d ? <>Décision à l'achat : <b>{d.label}</b> · confiance {Math.round(d.confidence)}/100 · du {frDate(d.asOf, true)}</> : <>Achat sans décision affichée</>}
      </p>
      <dl className="paper-figures six">
        <div><dt>Entrée</dt><dd className="mono">{fmtPrice(l.entry)}</dd></div>
        <div><dt>Cours</dt><dd className="mono">{l.price != null ? fmtPrice(l.price) : "indisponible"}</dd></div>
        <div><dt>Valeur</dt><dd className="mono">{l.value != null ? usd(l.value) : "—"}</dd></div>
        <div><dt>Gain / perte</dt><dd className={`mono ${upDown(l.pnl)}`}>{l.pnl != null ? signedUsd(l.pnl) : "—"}</dd></div>
        <div><dt>Stop</dt><dd className="mono">{l.stop != null ? fmtPrice(l.stop) : "aucun"}</dd></div>
        <div><dt>Objectif</dt><dd className="mono">{l.target != null ? fmtPrice(l.target) : "aucun"}</dd></div>
      </dl>
      <button className="btn btn-ghost btn-small sell-sim" onClick={onSell} disabled={l.price == null} aria-label={`Vendre (simulé) ${l.name}`}>
        Vendre (simulé)
      </button>
    </li>
  );
}

function TradesTable({ trades }: { trades: PaperTrade[] }) {
  return (
    <table className="paper-table">
      <caption className="sr-only">Trades clôturés, le plus récent en premier</caption>
      <thead>
        <tr><th scope="col">Actif</th><th scope="col">Achat</th><th scope="col">Vente</th><th scope="col">Entrée</th><th scope="col">Sortie</th><th scope="col">Motif</th><th scope="col">Résultat</th></tr>
      </thead>
      <tbody>
        {trades.map((t) => (
          <tr key={t.id}>
            <th scope="row" data-label="Actif">
              <b>{t.name}</b> <small className="muted mono">{t.symbol}</small>
              <small className="muted paper-trade-dec">{t.decision ? `${t.decision.label} · ${Math.round(t.decision.confidence)}/100` : "sans décision"}</small>
            </th>
            <td data-label="Achat">{frDate(t.openedAt)}</td>
            <td data-label="Vente">{frDate(t.closedAt)}</td>
            <td data-label="Entrée" className="mono">{fmtPrice(t.entry)}</td>
            <td data-label="Sortie" className="mono">{fmtPrice(t.exit)}</td>
            <td data-label="Motif">{REASON_LABEL[t.reason]}</td>
            <td data-label="Résultat" className={`mono ${upDown(t.pnl)}`}>{signedUsd(t.pnl)} ({signedPct(t.pnlPct)})</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function StatsCard({ stats }: { stats: ReturnType<typeof paperStats> }) {
  const rows = verdictRows(stats.byVerdict);
  return (
    <div className="card">
      <h2 className="card-title">Statistiques</h2>
      {stats.trades === 0 ? (
        <p className="muted small">Les statistiques se calculent sur les trades clôturés : aucun pour l'instant.</p>
      ) : (
        <>
          <dl className="paper-figures">
            <div><dt>Trades clôturés</dt><dd className="mono">{stats.trades}</dd></div>
            <div><dt>% gagnants</dt><dd className="mono">{pct(stats.winRate)} <small className="muted">({stats.wins}/{stats.trades})</small></dd></div>
            <div><dt>Gain moyen</dt><dd className="mono up">{stats.avgWinPct != null ? signedPct(stats.avgWinPct) : "aucun gain"}</dd></div>
            <div><dt>Perte moyenne</dt><dd className="mono down">{stats.avgLossPct != null ? signedPct(stats.avgLossPct) : "aucune perte"}</dd></div>
            <div><dt>Profit factor</dt><dd className="mono">{stats.profitFactor != null ? stats.profitFactor.toLocaleString("fr-FR", { maximumFractionDigits: 2 }) : "— (aucune perte)"}</dd></div>
            <div><dt>Pire recul</dt><dd className="mono">{signedPct(stats.maxDrawdownPct, 1)}</dd></div>
            <div><dt>Résultat réalisé</dt><dd className={`mono ${upDown(stats.realizedPnl)}`}>{signedUsd(stats.realizedPnl)}</dd></div>
            <div><dt>Sorties</dt><dd className="small">{stats.byReason.target} objectif · {stats.byReason.stop} stop · {stats.byReason.manual} manuelle</dd></div>
          </dl>
          <h3 className="paper-h3">Résultats par décision affichée à l'achat</h3>
          <table className="paper-table verdicts">
            <caption className="sr-only">Résultats par décision affichée à l'achat</caption>
            <thead><tr><th scope="col">Décision</th><th scope="col">Trades</th><th scope="col">% gagnants</th><th scope="col">Résultat moyen</th></tr></thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.verdict}>
                  <th scope="row" data-label="Décision">{r.label}</th>
                  <td data-label="Trades" className="mono">{r.trades}</td>
                  <td data-label="% gagnants" className="mono">{pct(r.winRate)}</td>
                  <td data-label="Résultat moyen" className={`mono ${upDown(r.avgPnlPct)}`}>{signedPct(r.avgPnlPct)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      {stats.trades < FEW_TRADES ? (
        <p className="notice warn small">⚠ {stats.trades} trade{stats.trades > 1 ? "s" : ""} clôturé{stats.trades > 1 ? "s" : ""} seulement. {FEW_TRADES_NOTE}</p>
      ) : (
        <p className="muted small">Résultats simulés du passé : ils ne garantissent rien pour la suite.</p>
      )}
    </div>
  );
}

function Rules() {
  return (
    <details className="card paper-rules">
      <summary>Comment c'est calculé</summary>
      <ul className="dec-list small">
        <li>Chaque achat et chaque vente paie <b>0,1 % de frais</b> et <b>0,05 % de glissement</b> (le prix obtenu est un peu moins bon que le cours affiché), comme l'historique du signal.</li>
        <li>Stop et objectif sont vérifiés sur les <b>bougies journalières</b> (à l'ouverture de cet écran puis toutes les 5 min). La bougie du jour de l'achat n'est pas utilisée : un niveau touché ce jour-là n'est pris en compte que <b>le lendemain</b> (son plus bas a pu avoir lieu avant l'achat).</li>
        <li>Si une même bougie touche le stop <b>et</b> l'objectif, on compte <b>le stop</b> : l'ordre des mouvements dans la journée n'est pas connu, on prend le pire cas.</li>
        <li>Si le cours ouvre déjà sous le stop (trou de cotation), la vente se fait <b>au cours d'ouverture</b>, plus bas que le stop. Un objectif est vendu à l'objectif, jamais mieux.</li>
        <li>« Vendre (simulé) » vend toute la position au cours en direct, glissement et frais déduits.</li>
        <li>Les résultats sont regroupés par décision affichée au moment de l'achat, pour voir quels verdicts ont vraiment marché.</li>
        <li>Tout reste dans ce navigateur : rien n'est envoyé au serveur, aucun ordre n'est passé. Mêmes règles sur iPhone et Android.</li>
      </ul>
    </details>
  );
}

function StartCard({ onStart }: { onStart: (capital: number) => void }) {
  const [text, setText] = useState("10 000");
  return (
    <div className="card empty-card">
      <h2>Testez les décisions d'Altim sans risquer d'argent</h2>
      <p className="muted">
        Un portefeuille virtuel : vous « achetez » depuis la carte Décision d'un actif, Altim suit le cours, vend au stop ou à l'objectif, et mesure quels verdicts ont vraiment marché.
      </p>
      <label className="field">
        <span>Capital de départ simulé ({currencySymbol()})</span>
        <input inputMode="decimal" value={text} onChange={(e) => setText(e.target.value)} />
      </label>
      <button className="btn" onClick={() => onStart(startCapital(text))}>Commencer la simulation</button>
    </div>
  );
}

function RestartSheet({ onClose, onConfirm }: { onClose: () => void; onConfirm: (capital: number) => void }) {
  const [text, setText] = useState(DEFAULT_CAPITAL.toLocaleString("fr-FR"));
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" role="dialog" aria-modal="true" aria-labelledby="paper-restart-title" onClick={(e) => e.stopPropagation()}>
        <div className="sheet-handle" />
        <div className="sheet-head"><h2 id="paper-restart-title">Recommencer la simulation</h2></div>
        <p className="notice warn small">⚠ Les positions ouvertes, le journal et les statistiques simulés seront effacés. Vos avoirs réels ne sont pas touchés.</p>
        <label className="field">
          <span>Capital de départ simulé ({currencySymbol()})</span>
          <input inputMode="decimal" autoFocus value={text} onChange={(e) => setText(e.target.value)} />
        </label>
        <small className="muted">Par défaut 10 000 {currencySymbol()}. Un montant illisible ou nul reprend 10 000 {currencySymbol()}.</small>
        <button className="btn" onClick={() => onConfirm(startCapital(text))}>Effacer et recommencer avec {usd(fromDisplay(startCapital(text)))}</button>
        <button className="btn btn-ghost" onClick={onClose}>Annuler</button>
      </div>
    </div>
  );
}

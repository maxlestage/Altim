import { useEffect, useMemo, useState } from "react";
import { api, type SelectionReport, type SelectionCandidate } from "./api";
import { onLink } from "./router";
import { assetKey, setState, useAppState, useHoldings, type HorizonPref } from "./store";
import { Segmented } from "./ui";
import { LiveBadge, LivePrice, useLive } from "./live";
import { HORIZONS } from "../engine/fibonacci";
import { formatPrice } from "../market";

const ORDER = ["momentum", "zone", "trend", "risk", "signal"] as const;
const usd = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: v >= 100 ? 0 : 2 })} $`;
const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
const BUDGET_KEY = "altim.selection.budget";

function readBudget(): number | null {
  try {
    const v = Number(localStorage.getItem(BUDGET_KEY));
    return Number.isFinite(v) && v > 0 ? v : null;
  } catch {
    return null;
  }
}

/**
 * Amount for each stock: the budget is split so that each line risks the same amount if its stop is hit
 * (a volatile stock gets less money), and no line exceeds the maximum size set in Réglages.
 */
function allocate(picks: SelectionCandidate[], budget: number, maxPct: number): Map<string, number> {
  const withPlan = picks.filter((p) => p.plan);
  const inv = withPlan.map((p) => 1 / Math.max(0.5, (1 - p.plan!.stop / (p.plan!.limit ?? p.plan!.entry)) * 100));
  const sum = inv.reduce((a, b) => a + b, 0);
  const cap = (budget * maxPct) / 100;
  return new Map(withPlan.map((p, i) => [p.symbol, Math.min(cap, (budget * inv[i]!) / sum)]));
}

function Bar({ value }: { value: number }) {
  const tone = value >= 70 ? "good" : value >= 40 ? "mid" : "low";
  return <div className="crit-bar" role="img" aria-label={`${Math.round(value)} sur 100`}><i className={tone} style={{ width: `${Math.max(3, value)}%` }} /></div>;
}

function PickCard({ c, report, live, amount }: { c: SelectionCandidate; report: SelectionReport; live?: import("./live").LiveTick; amount?: number }) {
  const [open, setOpen] = useState(c.rank <= 3);
  const { watchlist } = useAppState();
  const inRadar = watchlist.some((w) => assetKey(w) === `stock:${c.symbol}`);
  const price = live?.price ?? c.price;
  const plan = c.plan;
  const buyAt = plan?.limit ?? price;
  const qty = amount && buyAt > 0 ? Math.floor(amount / buyAt) : 0;
  const loss = plan && qty ? qty * (buyAt - plan.stop) : null;
  return (
    <li className="card pick">
      <div className="pick-head">
        <span className="pick-rank" aria-label={`Rang ${c.rank}`}>{c.rank}</span>
        <div className="pick-id">
          <a href={`/app/actif/stock/${c.symbol}`} onClick={onLink}><b>{c.name}</b></a>
          <small className="muted">{c.symbol} · {c.sector}</small>
        </div>
        <div className="pick-price mono">
          <b><LivePrice tick={live} fallback={c.price} format={(v) => `${formatPrice(v)} $`} /></b>
          <small className="muted">force {Math.round(c.scores.momentum)}/100</small>
        </div>
      </div>

      {plan && (
        <div className="pick-plan">
          <div><small>Entrée</small><b>{plan.limit ? `ordre limite ${usd(plan.limit)}` : `maintenant ≈ ${usd(price)}`}</b>{plan.limit && <small className="muted">ou maintenant ≈ {usd(price)}</small>}</div>
          <div><small>Stop</small><b className="sell">{usd(plan.stop)}</b><small className="muted">{pct((plan.stop / buyAt - 1) * 100)}</small></div>
          <div><small>Objectif</small><b className="buy">{usd(plan.target)}</b><small className="muted">{pct((plan.target / buyAt - 1) * 100)}</small></div>
          {amount != null && amount > 0 && (
            <div className="pick-amount">
              <small>Montant suggéré</small>
              <b>{usd(amount)}</b>
              <small className="muted">{qty > 0 ? `${qty} action${qty > 1 ? "s" : ""}${loss ? ` · perte max ≈ ${usd(loss)} au stop` : ""}` : "moins d'une action : fractionnée chez votre courtier"}</small>
            </div>
          )}
        </div>
      )}

      <button className="link-btn pick-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "Masquer le détail" : "Pourquoi celle-ci ? Le détail"}</button>
      {open && (
        <div className="pick-detail">
          <ul className="crit-list">
            {ORDER.map((k) => (
              <li key={k} className={k === "momentum" ? "main" : ""}>
                <div className="crit-head">
                  <span>{report.criteria[k]}</span>
                  <small className="crit-role">{report.roles[k]}</small>
                  <b className="mono">{Math.round(c.scores[k])}</b>
                </div>
                <Bar value={c.scores[k]} />
                <small className="muted">{c.why[k]}</small>
              </li>
            ))}
          </ul>
          <ul className="checks">
            {c.checks.map((x) => <li key={x.label} className={x.ok ? "ok" : "ko"}><span aria-hidden>{x.ok ? "✔" : "⚠"}</span> <b>{x.label}</b> : {x.detail}</li>)}
            {c.track && (
              <li className="info"><span aria-hidden>ℹ</span> <b>Signaux d'Altim sur ce titre</b> : {c.track.trades} achats passés, {Math.round(c.track.winRate)} % gagnants, {pct(c.track.avgReturn)} en moyenne</li>
            )}
          </ul>
          <div className="pick-actions">
            <a className="btn btn-small" href={`/app/actif/stock/${c.symbol}`} onClick={onLink}>Voir la fiche complète</a>
            {!inRadar && <button className="link-btn" onClick={() => setState((s) => ({ watchlist: [...s.watchlist, { symbol: c.symbol, kind: "stock", name: c.name }] }))}>+ Ajouter au radar</button>}
          </div>
        </div>
      )}
    </li>
  );
}

/** Which stocks to buy: ranked by relative strength, checked, with an entry plan and an amount. */
export function Selection() {
  const { horizon, risk } = useAppState();
  const { holdings, cash } = useHoldings();
  const [report, setReport] = useState<SelectionReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [budgetText, setBudgetText] = useState(() => String(readBudget() ?? (cash > 0 ? Math.round(cash) : "")));
  const budget = Number(budgetText.replace(/\s/g, "").replace(",", ".")) || 0;
  const wealth = cash + holdings.reduce((a, h) => a + h.quantity * h.averagePrice, 0);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setReport(null);
    setError(null);
    const load = () =>
      api.selection(horizon)
        .then((r) => {
          if (!alive) return;
          if ("pending" in r) {
            setPending(true);
            timer = setTimeout(load, 5_000);
          } else {
            setPending(false);
            setReport(r);
          }
        })
        .catch((e) => alive && setError(e instanceof Error ? e.message : "Indisponible"));
    load();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [horizon]);

  useEffect(() => {
    try {
      if (budget > 0) localStorage.setItem(BUDGET_KEY, String(budget));
    } catch {}
  }, [budget]);

  const live = useLive(useMemo(() => [...(report?.buy ?? []), ...(report?.watch ?? [])].map((c) => ({ symbol: c.symbol, kind: "stock" as const })), [report]));
  const amounts = useMemo(() => (report && budget > 0 ? allocate(report.buy, budget, risk.maxPositionPercent) : new Map<string, number>()), [report, budget, risk.maxPositionPercent]);
  const v = report?.validation;

  return (
    <section className="app-screen selection">
      <div className="screen-top">
        <div>
          <h1>Quelles actions acheter</h1>
          <p className="muted small">Les plus grandes sociétés américaines, classées et vérifiées pour votre horizon.</p>
        </div>
        <LiveBadge status={live.status} last={live.last} />
      </div>

      <Segmented<HorizonPref>
        label="Horizon"
        value={horizon}
        onChange={(h) => setState({ horizon: h })}
        options={(["short", "medium", "long"] as HorizonPref[]).map((h) => [h, HORIZONS[h].label])}
      />

      <div className="card method">
        <h2 className="card-title">Comment Altim choisit</h2>
        <ol>
          <li><b>{report?.scanned ?? 150} grandes actions</b> analysées chaque jour (capitalisation, secteur : Nasdaq).</li>
          <li><b>Classement par force relative</b> : la hausse des 6 derniers mois (hors dernier mois) comparée aux autres. C'est le seul critère qui a réellement fait mieux que la moyenne sur l'historique ; les autres ont été mesurés et ne classent pas.</li>
          <li><b>Au plus 3 actions par secteur</b>, pour ne pas tout miser sur un seul thème.</li>
          <li><b>Vérifications</b> de chaque finaliste : prix recoupés sur plusieurs sources, garde-fou marché, tendance de fond. Un titre qui échoue passe « à surveiller ».</li>
          <li><b>Plan</b> pour votre horizon ({HORIZONS[horizon].holding}) : prix d'entrée (zone d'achat Fibonacci), stop selon la volatilité, objectif à 2 fois le risque, montant.</li>
        </ol>
      </div>

      {v && (
        <div className={`card validation ${v.top > v.universe && v.beatRate >= 60 ? "good" : "weak"}`}>
          <h2 className="card-title">Ce que cette méthode aurait donné</h2>
          <p>
            Rejouée <b>{v.periods} fois</b> depuis {new Date(v.from!).toLocaleDateString("fr-FR", { month: "long", year: "numeric" })} (sélection de 10 actions, gardées {v.hold} séances) :
            <b> {pct(v.top)}</b> en moyenne pour la sélection contre <b>{pct(v.universe)}</b> pour l'ensemble des {report?.scanned} actions ; la sélection a fait mieux <b>{Math.round(v.beatRate)} % du temps</b>.
          </p>
          {horizon === "short" && <p className="notice warn small">À court terme, l'avance est quasi nulle : le classement ne prédit pas les mouvements de quelques jours. Préférez le moyen ou le long terme pour choisir des actions.</p>}
          <p className="muted small">
            Limite honnête : la liste est celle des plus grandes sociétés d'aujourd'hui, qui ont par définition réussi, ce qui gonfle ces chiffres. Entre fin 2021 et 2023, la force relative n'a presque rien apporté ; l'essentiel de l'avance vient de 2023–2026. Ce n'est pas une garantie.
          </p>
        </div>
      )}

      <div className="card budget">
        <label className="field">
          <span>Budget à investir (USD)</span>
          <input inputMode="decimal" value={budgetText} placeholder={wealth > 0 ? String(Math.round(cash)) : "10000"} onChange={(e) => setBudgetText(e.target.value)} />
        </label>
        <p className="muted small">
          Réparti pour que chaque ligne risque la même somme si son stop est touché (une action volatile reçoit moins), sans dépasser {risk.maxPositionPercent} % du budget par ligne (Réglages).
        </p>
      </div>

      {error && <p className="notice warn">⚠ {error}</p>}
      {!report && !error && (
        <div className="card">
          <p className="muted">{pending ? "Analyse des 150 actions en cours (environ 30 secondes la première fois)…" : "Chargement de la sélection…"}</p>
          <div className="skeleton" />
        </div>
      )}

      {report && (
        <>
          <h2 className="section-label">À acheter · {report.buy.length}</h2>
          <ol className="pick-list">
            {report.buy.map((c) => <PickCard key={c.symbol} c={c} report={report} live={live.ticks[`stock:${c.symbol}`]} amount={amounts.get(c.symbol)} />)}
          </ol>

          {report.watch.length > 0 && (
            <>
              <h2 className="section-label">À surveiller · {report.watch.length}</h2>
              <ul className="watch-list">
                {report.watch.map((c) => (
                  <li key={c.symbol} className="card">
                    <a href={`/app/actif/stock/${c.symbol}`} onClick={onLink}><b>{c.name}</b></a> <small className="muted">{c.symbol} · force {Math.round(c.scores.momentum)}/100</small>
                    <p className="small">⚠ {c.reason}</p>
                  </li>
                ))}
              </ul>
            </>
          )}
          {report.setAside.length > 0 && (
            <details className="card">
              <summary>Écartées faute de données fiables · {report.setAside.length}</summary>
              <ul className="small">{report.setAside.map((x) => <li key={x.symbol}><b>{x.symbol}</b> : {x.reason}</li>)}</ul>
            </details>
          )}
          <p className="muted small">
            Sélection calculée à {new Date(report.asOf).toLocaleTimeString("fr-FR")}, prix en direct. Conseil indicatif, pas une recommandation personnalisée : Altim ne passe aucun ordre.
          </p>
        </>
      )}
    </section>
  );
}

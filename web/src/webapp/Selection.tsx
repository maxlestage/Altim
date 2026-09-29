import { useEffect, useMemo, useState } from "react";
import { api, type SelectionReport, type SelectionCandidate } from "./api";
import { onLink } from "./router";
import { assetKey, setState, useAppState, useHoldings } from "./store";
import { Segmented } from "./ui";
import { LiveBadge, LivePrice, useLive } from "./live";
import { HORIZON_LABEL, HORIZON_LIST, type Horizon } from "../engine/screener";
import { convert, currencySymbol, displayCurrency, moneyFmt, moneyPrice, type Currency } from "../money";

const ORDER = ["momentum", "zone", "trend", "risk", "signal"] as const;
const usd = (v: number) => moneyFmt(v, (x) => x.toLocaleString("fr-FR", { maximumFractionDigits: x >= 100 ? 0 : 2 }));
const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
const BUDGET_KEY = "altim.selection.budget";
const MARKET_KEY = "altim.selection.market";
const HORIZON_KEY = "altim.selection.horizon";
const RANK_TEXT = {
  signal: "signal technique d'Altim",
  momentum: "force relative, les plus en hausse d'abord",
  reversal: "rebond, les plus en baisse d'abord",
  lowRisk: "les plus calmes d'abord",
} as const;

function readHorizon(): Horizon {
  try {
    const v = localStorage.getItem(HORIZON_KEY) as Horizon | null;
    return v && HORIZON_LIST.includes(v) ? v : "1m";
  } catch {
    return "1m";
  }
}
type Market = "stock" | "crypto";

function readMarket(): Market {
  try {
    return localStorage.getItem(MARKET_KEY) === "crypto" ? "crypto" : "stock";
  } catch {
    return "stock";
  }
}

/** Saved budget and the currency it was typed in: `{ amount, currency }`, or a bare number (older versions: dollars). */
export function parseBudget(raw: string | null): { amount: number; currency: Currency } | null {
  if (raw == null) return null;
  try {
    const v = JSON.parse(raw) as unknown;
    if (typeof v === "number") return Number.isFinite(v) && v > 0 ? { amount: v, currency: "USD" } : null;
    const o = v as { amount?: unknown; currency?: unknown } | null;
    if (o && typeof o.amount === "number" && Number.isFinite(o.amount) && o.amount > 0) return { amount: o.amount, currency: o.currency === "EUR" ? "EUR" : "USD" };
  } catch {}
  return null;
}

function readBudget(): { amount: number; currency: Currency } | null {
  try {
    return parseBudget(localStorage.getItem(BUDGET_KEY));
  } catch {
    return null;
  }
}

/** The budget field: the saved amount converted to the display currency (as typed when no rate allows it). */
function initialBudget(cashUsd: number): { text: string; currency: Currency } {
  const cur = displayCurrency();
  const saved = readBudget();
  if (saved) {
    const v = convert(saved.amount, saved.currency, cur);
    return Number.isFinite(v) ? { text: String(Math.round(v)), currency: cur } : { text: String(saved.amount), currency: saved.currency };
  }
  const cash = convert(cashUsd, "USD", cur);
  return { text: cashUsd > 0 && Number.isFinite(cash) ? String(Math.round(cash)) : "", currency: cur };
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
  const kind = report.market;
  const inRadar = watchlist.some((w) => assetKey(w) === `${kind}:${c.symbol}`);
  const rankBy = report.rankBy;
  const rankLabel = { signal: "signal", momentum: report.rankRule === "reversal" ? "rebond" : "force", risk: "calme", trend: "tendance", zone: "zone" }[rankBy];
  const price = live?.price ?? c.price;
  const plan = c.plan;
  const buyAt = plan?.limit ?? price;
  // Whole shares for a stock; cryptos are divisible.
  const qty = amount && buyAt > 0 ? (kind === "stock" ? Math.floor(amount / buyAt) : Number((amount / buyAt).toPrecision(6))) : 0;
  const loss = plan && qty ? qty * (buyAt - plan.stop) : null;
  return (
    <li className="card pick">
      <div className="pick-head">
        <span className="pick-rank" aria-label={`Rang ${c.rank}`}>{c.rank}</span>
        <div className="pick-id">
          <a href={`/app/actif/${kind}/${c.symbol}`} onClick={onLink}><b>{c.name}</b></a>
          <small className="muted">{c.symbol} · {c.sector}</small>
        </div>
        <div className="pick-price mono">
          <b><LivePrice tick={live} fallback={c.price} format={(v) => moneyPrice(v)} /></b>
          <small className="muted">{rankLabel} {Math.round(report.rankRule === "reversal" ? 100 - c.scores.momentum : c.scores[rankBy])}/100</small>
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
              <small className="muted">
                {qty > 0
                  ? `${kind === "stock" ? `${qty} action${qty > 1 ? "s" : ""}` : `${qty.toLocaleString("fr-FR", { maximumSignificantDigits: 6 })} ${c.symbol}`}${loss ? ` · perte max ≈ ${usd(loss)} au stop` : ""}`
                  : "moins d'une action : fractionnée chez votre courtier"}
              </small>
            </div>
          )}
        </div>
      )}

      <button className="link-btn pick-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "Masquer le détail" : "Pourquoi celle-ci ? Le détail"}</button>
      {open && (
        <div className="pick-detail">
          <ul className="crit-list">
            {ORDER.map((k) => (
              <li key={k} className={k === rankBy ? "main" : ""}>
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
            <a className="btn btn-small" href={`/app/actif/${kind}/${c.symbol}`} onClick={onLink}>Voir la fiche complète</a>
            {!inRadar && <button className="link-btn" onClick={() => setState((s) => ({ watchlist: [...s.watchlist, { symbol: c.symbol, kind, name: c.name }] }))}>+ Ajouter au radar</button>}
          </div>
        </div>
      )}
    </li>
  );
}

/** Which stocks or cryptos to buy: ranked by what was measured to work, checked, with an entry plan and an amount. */
export function Selection() {
  const { risk } = useAppState();
  const [horizon, setHorizon] = useState<Horizon>(readHorizon);
  const [market, setMarket] = useState<Market>(readMarket);
  const { holdings, cash } = useHoldings();
  const [report, setReport] = useState<SelectionReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  // The typed budget keeps the currency it was typed in (a rate arriving later never re-reads it in another one).
  const [budgetInput, setBudgetInput] = useState(() => initialBudget(cash));
  const budgetText = budgetInput.text;
  const setBudgetText = (text: string) => setBudgetInput({ text, currency: displayCurrency() });
  const typed = Number(budgetText.replace(/\s/g, "").replace(",", ".")) || 0;
  const budget = convert(typed, budgetInput.currency, "USD") || 0;
  const wealth = cash + holdings.reduce((a, h) => a + h.quantity * h.averagePrice, 0);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setReport(null);
    setError(null);
    const load = () =>
      api.selection(horizon, market)
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
  }, [horizon, market]);

  useEffect(() => {
    try {
      localStorage.setItem(MARKET_KEY, market);
      localStorage.setItem(HORIZON_KEY, horizon);
    } catch {}
  }, [market, horizon]);

  useEffect(() => {
    try {
      if (typed > 0) localStorage.setItem(BUDGET_KEY, JSON.stringify({ amount: typed, currency: budgetInput.currency }));
    } catch {}
  }, [typed, budgetInput.currency]);

  const live = useLive(useMemo(() => [...(report?.buy ?? []), ...(report?.watch ?? [])].map((c) => ({ symbol: c.symbol, kind: report!.market })), [report]));
  const amounts = useMemo(() => (report && budget > 0 ? allocate(report.buy, budget, risk.maxPositionPercent) : new Map<string, number>()), [report, budget, risk.maxPositionPercent]);
  const v = report?.validation;

  return (
    <section className="app-screen selection">
      <div className="screen-top">
        <div>
          <h1>{market === "crypto" ? "Quelles cryptos acheter" : "Quelles actions acheter"}</h1>
          <p className="muted small">{market === "crypto" ? "Les 120 plus grandes cryptos (hors stablecoins et jetons adossés)" : "Les plus grandes sociétés américaines"}, classées et vérifiées pour votre horizon.</p>
        </div>
        <LiveBadge status={live.status} last={live.last} />
      </div>

      <a className="notice opp-link" href="/app/opportunites" onClick={onLink}><b>Opportunités du moment →</b> retournements, cassures, volumes anormaux, survendus, fondamentaux qui évoluent.</a>

      <Segmented<Market>
        label="Marché"
        value={market}
        onChange={setMarket}
        options={[["stock", "Actions"], ["crypto", "Cryptos"]]}
      />

      <div className="horizon-grid" role="radiogroup" aria-label="Durée de détention">
        {HORIZON_LIST.map((h) => (
          <button key={h} role="radio" aria-checked={h === horizon} className={h === horizon ? "on" : ""} onClick={() => setHorizon(h)}>
            {HORIZON_LABEL[h]}
          </button>
        ))}
      </div>

      <div className="card method">
        <h2 className="card-title">Comment Altim choisit</h2>
        <ol>
          {market === "crypto" ? (
            <>
              <li><b>{report?.scanned ?? 110} cryptos</b> parmi les 120 plus grandes (classement CoinGecko), sans stablecoins ni jetons adossés (WBTC, stETH, or…).</li>
              <li><b>Classement : {report ? RANK_TEXT[report.rankRule] : "…"}</b>, le critère qui a le mieux marché sur le passé pour cette durée. {report?.evidence}</li>
              <li><b>Pas de limite par secteur</b>, mais les cryptos bougent souvent ensemble : le montant par ligne reste plafonné.</li>
            </>
          ) : (
            <>
              <li><b>{report?.scanned ?? 150} grandes actions</b> analysées chaque jour (capitalisation, secteur : Nasdaq).</li>
              <li><b>Classement : {report ? RANK_TEXT[report.rankRule] : "…"}</b>, le critère qui a le mieux marché sur le passé pour cette durée. {report?.evidence}</li>
              <li><b>Au plus 3 actions par secteur</b>, pour ne pas tout miser sur un seul thème.</li>
            </>
          )}
          <li><b>Vérifications</b> de chaque finaliste : prix recoupés sur plusieurs sources, garde-fou marché, tendance de fond. Un titre qui échoue passe « à surveiller ».</li>
          <li><b>Plan</b> pour une détention de {HORIZON_LABEL[horizon]} : prix d'entrée (zone d'achat Fibonacci), stop selon la volatilité de la période, objectif à 2 fois le risque, montant.</li>
        </ol>
      </div>

      {report?.marketClosed && <p className="notice warn small">Bourse de New York fermée : ce classement vient de la dernière séance ; il changera à la réouverture.</p>}

      {v && (
        <div className={`card validation ${v.edge === "clear" ? "good" : "weak"}`}>
          <h2 className="card-title">Ce que cette méthode aurait donné</h2>
          {v.edge === "none" && (
            <p className="notice danger small">
              <b>Pas d'avance mesurée pour {HORIZON_LABEL[horizon]}.</b> Une fois les frais payés ({v.cost.toLocaleString("fr-FR", { maximumFractionDigits: 2 })} % l'aller-retour), ce classement n'a pas fait mieux que de choisir au hasard. Il est affiché à titre indicatif : ne misez pas dessus.
            </p>
          )}
          {v.edge === "weak" && (
            <p className="notice warn small">
              <b>Avance faible et irrégulière.</b> En moyenne la sélection a fait mieux, mais seulement environ une fois sur deux : quelques très bons choix tirent la moyenne. Une durée plus longue est plus fiable.
            </p>
          )}
          <p>
            Rejouée <b>{v.periods} fois</b> depuis {new Date(v.from!).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" })} (sélection de 10, gardée {report?.holdText}) :
            <b> {pct(v.top)}</b> en moyenne pour la sélection contre <b>{pct(v.universe)}</b> pour l'ensemble des {report?.scanned} {market === "crypto" ? "cryptos" : "actions"}
            {v.benchmark != null && <> et <b>{pct(v.benchmark)}</b> pour le simple achat de Bitcoin</>} ; la sélection a fait mieux que l'ensemble <b>{Math.round(v.beatRate)} % du temps</b>.
          </p>
          {market === "crypto" && v.top < 0 && <p className="notice warn small">Sur cette période, la sélection a perdu moins que les autres cryptos, mais elle a quand même perdu : quand presque toutes les cryptos baissent, bien choisir limite la casse sans l'éviter.</p>}
          <p className="muted small">
            {["30m", "1h", "5h"].includes(horizon)
              ? `Durées courtes : rejouées sur ${market === "crypto" ? "quelques jours à 3 semaines" : "60 jours"} seulement ; à ces échelles les prix sont surtout du bruit et les frais pèsent lourd. Ce n'est pas une garantie.`
              : market === "crypto"
                ? "Limites honnêtes : l'historique ne couvre qu'environ 2 ans et demi (les plateformes gardent 1 000 jours), et la liste est celle des cryptos qui existent encore aujourd'hui, ce qui embellit les chiffres. Les cryptos restent très risquées. Ce n'est pas une garantie."
                : "Limite honnête : la liste est celle des plus grandes sociétés d'aujourd'hui, qui ont par définition réussi, ce qui gonfle ces chiffres. Entre fin 2021 et 2023, la force relative n'a presque rien apporté ; l'essentiel de l'avance vient de 2023–2026. Ce n'est pas une garantie."}
          </p>
        </div>
      )}

      <div className="card budget">
        <label className="field">
          <span>Budget à investir ({currencySymbol(budgetInput.currency)})</span>
          <input inputMode="decimal" value={budgetText} placeholder={wealth > 0 ? String(Math.round(convert(cash, "USD", budgetInput.currency) || 0)) : "10000"} onChange={(e) => setBudgetText(e.target.value)} />
        </label>
        <p className="muted small">
          Réparti pour que chaque ligne risque la même somme si son stop est touché (une action volatile reçoit moins), sans dépasser {risk.maxPositionPercent} % du budget par ligne (Réglages).
        </p>
      </div>

      {error && <p className="notice warn">⚠ {error}</p>}
      {!report && !error && (
        <div className="card">
          <p className="muted">{pending ? `Analyse des ${market === "crypto" ? "120 cryptos" : "150 actions"} en cours (environ 30 secondes la première fois)…` : "Chargement de la sélection…"}</p>
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
                    <a href={`/app/actif/${report.market}/${c.symbol}`} onClick={onLink}><b>{c.name}</b></a> <small className="muted">{c.symbol}</small>
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

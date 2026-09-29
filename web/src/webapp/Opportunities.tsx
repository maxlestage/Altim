import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { onLink } from "./router";
import { Segmented } from "./ui";
import { moneyCompact, moneyPrice } from "../money";
import {
  DEFAULT_FILTERS, OPP_CATEGORIES, OPP_SHORT, compactUsd, countByCategory, filterItems,
  type OppCategory, type OppFilters, type OpportunityReport,
} from "../engine/opportunities";

type Market = "stock" | "crypto";
const KEY = "altim.opportunities.v1";

type Saved = { market: Market; filters: OppFilters };

function readSaved(): Saved {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null") as Saved | null;
    if (v && (v.market === "stock" || v.market === "crypto") && v.filters && Array.isArray(v.filters.categories)) {
      return { market: v.market, filters: { ...DEFAULT_FILTERS, ...v.filters, categories: v.filters.categories.filter((c) => OPP_CATEGORIES.includes(c)) } };
    }
  } catch {}
  return { market: "stock", filters: DEFAULT_FILTERS };
}

// Thresholds in dollars (the sources' currency), labelled in the display currency at render time.
const CAPS_USD = [null, 1e10, 5e10, 2e11];
const caps = (): [string, number | null][] => CAPS_USD.map((v) => [v == null ? "Toutes" : `≥ ${moneyCompact(v)}`, v]);
const RANKS: [string, number | null][] = [["Tous", null], ["Top 20", 20], ["Top 50", 50], ["Top 100", 100]];
const LIQ_USD = [null, 1e6, 1e7, 1e8];
const liq = (): [string, number | null][] => LIQ_USD.map((v) => [v == null ? "Toutes" : `≥ ${moneyCompact(v)} / jour`, v]);
const VOL: [string, number | null][] = [["Toutes", null], ["≤ 2 % / jour", 2], ["≤ 4 % / jour", 4], ["≤ 8 % / jour", 8]];

function Select({ label, value, options, onChange }: { label: string; value: number | null; options: [string, number | null][]; onChange: (v: number | null) => void }) {
  return (
    <label className="field opp-field">
      <span>{label}</span>
      <select value={value == null ? "" : String(value)} onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}>
        {options.map(([l, v]) => <option key={l} value={v == null ? "" : String(v)}>{l}</option>)}
      </select>
    </label>
  );
}

const fr1 = (v: number) => v.toLocaleString("fr-FR", { maximumFractionDigits: 1 });
const signed = (v: number) => `${v >= 0 ? "+" : "−"}${fr1(Math.abs(v))} %`;

/** Opportunities of the moment: the selection's universe scanned by category, with the reason for each asset. */
export function Opportunities() {
  const [saved, setSaved] = useState<Saved>(readSaved);
  const { market, filters } = saved;
  const [report, setReport] = useState<OpportunityReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    try {
      localStorage.setItem(KEY, JSON.stringify(saved));
    } catch {}
  }, [saved]);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setReport(null);
    setError(null);
    const load = () =>
      api.opportunities(market)
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
  }, [market]);

  const setFilters = (f: Partial<OppFilters>) => setSaved((s) => ({ ...s, filters: { ...s.filters, ...f } }));
  const toggle = (c: OppCategory) =>
    setFilters({ categories: filters.categories.includes(c) ? filters.categories.filter((x) => x !== c) : [...filters.categories, c] });
  const shown = useMemo(() => (report ? filterItems(report.items, filters) : []), [report, filters]);
  const counts = useMemo(() => (report ? countByCategory(report.items, filters) : null), [report, filters]);
  const info = (c: OppCategory) => report?.categories.find((x) => x.id === c);
  const limited = report?.categories.filter((c) => filters.categories.includes(c.id) && (c.note || c.error)) ?? [];

  return (
    <section className="app-screen opportunities">
      <div className="screen-top">
        <div>
          <h1>Opportunités du moment</h1>
          <p className="muted small">Ce qui bouge de façon notable aujourd'hui, catégorie par catégorie, avec la raison mesurée. Des pistes à examiner, pas des ordres d'achat.</p>
        </div>
      </div>

      <Segmented<Market> label="Marché" value={market} onChange={(m) => setSaved((s) => ({ ...s, market: m }))} options={[["stock", "Actions"], ["crypto", "Cryptos"]]} />

      <div className="card opp-filters">
        <h2 className="card-title">Catégories</h2>
        <div className="opp-chips" role="group" aria-label="Catégories">
          {OPP_CATEGORIES.map((c) => {
            const on = filters.categories.includes(c);
            return (
              <button key={c} className={`chip pick ${on ? "on" : ""}`} aria-pressed={on} onClick={() => toggle(c)} title={info(c)?.rule}>
                {OPP_SHORT[c]}{counts ? ` · ${counts[c]}` : ""}
              </button>
            );
          })}
        </div>
        <div className="opp-selects">
          {market === "stock"
            ? <Select label="Capitalisation" value={filters.minCap} options={caps()} onChange={(v) => setFilters({ minCap: v })} />
            : <Select label="Rang (capitalisation)" value={filters.maxRank} options={RANKS} onChange={(v) => setFilters({ maxRank: v })} />}
          <Select label="Liquidité (volume échangé)" value={filters.minLiquidity} options={liq()} onChange={(v) => setFilters({ minLiquidity: v })} />
          <Select label="Volatilité max (ATR)" value={filters.maxVolatility} options={VOL} onChange={(v) => setFilters({ maxVolatility: v })} />
        </div>
        {limited.map((c) => (
          <p key={c.id} className={`small ${c.error ? "notice warn" : "muted"}`}>
            <b>{OPP_SHORT[c.id]}</b> : {c.error ?? c.note}
          </p>
        ))}
      </div>

      {error && <p className="notice warn">⚠ {error}</p>}
      {!report && !error && (
        <div className="card">
          <p className="muted">{pending ? `Analyse des ${market === "crypto" ? "120 cryptos" : "150 actions"} en cours (environ 30 secondes la première fois)…` : "Chargement…"}</p>
          <div className="skeleton" />
        </div>
      )}

      {report && (
        <>
          <h2 className="section-label">Résultats · {shown.length} sur {report.scanned} analysé{report.scanned > 1 ? "s" : ""}</h2>
          {shown.length === 0 && <p className="muted">Aucun actif ne remplit ces critères en ce moment.</p>}
          <ul className="opp-list">
            {shown.map((i) => (
              <li key={i.symbol} className="card opp-card">
                <div className="opp-head">
                  <div className="opp-id">
                    <a href={`/app/actif/${market}/${i.symbol}`} onClick={onLink}><b>{i.name}</b></a>
                    <small className="muted">{i.symbol} · {market === "crypto" ? (i.rank ? `rang ${i.rank}` : "crypto") : i.sector}</small>
                  </div>
                  <div className="opp-price mono">
                    <b>{moneyPrice(i.price)}</b>
                    {i.change1d != null && <small className={i.change1d >= 0 ? "buy" : "sell"}>{signed(i.change1d)}</small>}
                  </div>
                </div>
                <ul className="opp-hits">
                  {i.hits.map((h) => (
                    <li key={h.category}><span className="chip">{OPP_SHORT[h.category]}</span> <span className="small">{h.reason}</span></li>
                  ))}
                </ul>
                <p className="muted small opp-metrics">
                  {i.rsi14 != null && <span>RSI 14 : {Math.round(i.rsi14)}</span>}
                  {i.volatility != null && <span>volatilité {fr1(i.volatility)} %/j</span>}
                  {i.liquidity != null && <span>échangé {compactUsd(i.liquidity)}/j</span>}
                  {i.marketCap != null && <span>capitalisation {compactUsd(i.marketCap)}</span>}
                </p>
                <a className="btn btn-small" href={`/app/actif/${market}/${i.symbol}`} onClick={onLink}>Voir la fiche</a>
              </li>
            ))}
          </ul>

          <details className="card">
            <summary>Règles, sources et limites</summary>
            <ul className="small opp-rules">
              {report.categories.map((c) => (
                <li key={c.id}>
                  <b>{c.label}</b> ({c.analyzed} analysé{c.analyzed > 1 ? "s" : ""}) : {c.rule}
                  {c.note && <span className="muted"> {c.note}</span>}
                  {c.error && <span className="sell"> {c.error}</span>}
                </li>
              ))}
              {report.notCovered.map((n) => <li key={n.label}><b>Non couvert — {n.label}</b> : {n.reason}.</li>)}
            </ul>
            <p className="muted small">{report.universe}. Sources : {report.source}. Séances closes uniquement.</p>
          </details>
          <p className="muted small">
            Scan calculé à {new Date(report.asOf).toLocaleTimeString("fr-FR")}. Conseil indicatif, pas une recommandation personnalisée : Altim ne passe aucun ordre.
          </p>
        </>
      )}
    </section>
  );
}

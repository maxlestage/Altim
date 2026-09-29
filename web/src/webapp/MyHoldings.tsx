import { useEffect, useMemo, useRef, useState } from "react";
import { analyzePortfolio, insightText, REASON_TEXT, RECOMMENDATION_LABEL, type Holding, type MarketInput, type PortfolioAnalysis, type Recommendation } from "../engine/holdings";
import { currencySymbol, displayCurrency, fromDisplay, money, moneyPrice, storedCurrency, toDisplay } from "../money";
import { api } from "./api";
import { onLink } from "./router";
import { AddHoldings } from "./AddHoldings";
import { KIND_LABEL } from "./AssetPicker";
import { exportHoldings, importHoldings, setHoldings, shown, upsertHolding, useAppState, useHoldings, useStoredHoldings, type StoredHolding } from "./store";
import { FxNote } from "./FxNote";
import {
  BENCHMARK, checkLimits, correlatedClusters, dailyChange, dangerousPositions, estimateBeta, stressTest, type BetaEstimate, type Danger,
} from "../engine/portfolio-risk";
import type { Candle } from "../engine/signal";
import { LimitsCard, StressCard } from "./RiskCards";
import { WhatIfCard } from "./WhatIfCard";
import { SectorCard } from "./SectorCard";
import { recordRealTrade } from "./journal-store";
import { JournalToggle } from "./Journal";
import { holdingChange } from "../engine/journal";
import { saveDangers } from "./danger-store";
import { Change } from "./ui";
import { LiveBadge, LivePrice, useLive } from "./live";
import { HistoryCard } from "./HistoryCard";
import { PortfolioTabs } from "./Simulation";
import { ProjectionCard, RebalanceCard, SaleCard } from "./ToolCards";

const usd = (v: number) => money(v, 2, 2);
/** A saved amount put back in an input, in the display currency ("" when no rate allows it). */
const inputText = (v: number) => (Number.isFinite(v) ? String(Math.round(v * 1e8) / 1e8).replace(".", ",") : "");
const REC_CLASS: Record<Recommendation, string> = { sell: "sell", protect: "sell", lighten: "hold", strengthen: "buy", hold: "hold", unknown: "unknown" };
// CSV columns: quantities and prices keep their precision (small cryptos), the rest 2 decimals.
const CSV_DIGITS = [0, 0, 0, 8, 6, 6, 2, 2, 2, 2, 0];
const LEVEL_ICON ={ danger: "⛔", warning: "⚠", info: "ℹ", good: "✔" } as const;

/** Real portfolio entered by the user (localStorage) and full analysis. */
export function MyHoldings() {
  const { holdings, cash, updatedAt, unconverted, cashUnconverted } = useHoldings();
  const stored = useStoredHoldings();
  const cur = displayCurrency();
  const { risk } = useAppState();
  const [market, setMarket] = useState<Record<string, MarketInput>>({});
  // Daily candles of the benchmarks (Bitcoin, S&P 500 via SPY) when they are not held: betas of the stress tests.
  const [bench, setBench] = useState<Record<string, Candle[]>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<StoredHolding | null>(null);
  const [adding, setAdding] = useState(false);
  const cashShown = shown(stored.cash, stored.cashCurrency);
  const cashInitial = stored.cash ? inputText(cashShown) : "";
  const [cashText, setCashText] = useState(cashInitial);
  // The display currency or the rate changed: the field follows (unless being typed in).
  useEffect(() => setCashText(cashInitial), [cashInitial]);
  const fileInput = useRef<HTMLInputElement>(null);
  const symbolsKey = holdings.map((h) => `${h.kind}:${h.symbol}`).sort().join(",");

  useEffect(() => {
    if (!holdings.length) return setMarket({});
    let alive = true;
    const load = async () => {
      setLoading(true);
      try {
        const assets = holdings.map((h) => ({ symbol: h.symbol, kind: h.kind, name: h.name }));
        const unique = [...new Map(assets.map((a) => [`${a.kind}:${a.symbol}`, a])).values()];
        const benchmarks = [...new Set(unique.map((a) => a.kind))]
          .map((k) => BENCHMARK[k])
          .filter((b) => !unique.some((a) => a.symbol === b.symbol && a.kind === b.kind));
        const [day, short, candles, benchCandles] = await Promise.all([
          api.radar(unique, "1d"),
          api.radar(unique, "4h").catch(() => []),
          Promise.all(unique.map((a) => api.candles(a.symbol, a.kind, "1d").catch(() => null))),
          Promise.all(benchmarks.map((b) => api.candles(b.symbol, b.kind, "1d").catch(() => null))),
        ]);
        if (!alive) return;
        setBench(Object.fromEntries(benchmarks.flatMap((b, i) => (benchCandles[i]?.candles.length ? [[`${b.kind}:${b.symbol}`, benchCandles[i]!.candles]] : []))));
        const m: Record<string, MarketInput> = {};
        unique.forEach((a, i) => {
          const k = `${a.kind}:${a.symbol}`;
          const d = day.find((r) => r.symbol === a.symbol && r.kind === a.kind);
          const s = short.find((r) => r.symbol === a.symbol && r.kind === a.kind);
          m[k] = {
            price: d?.price ?? s?.price ?? 0,
            daily: candles[i]?.candles ?? [],
            daySignal: d?.signal ?? null,
            shortSignal: s?.signal ?? null,
            // The least reliable of the two timeframes wins (caution).
            reliability: [d?.reliability?.level, s?.reliability?.level].includes("low") ? "low" : d?.reliability?.level ?? s?.reliability?.level ?? null,
          };
        });
        setMarket(m);
        setError(null);
      } catch (e) {
        if (alive) setError(e instanceof Error ? e.message : "Données indisponibles");
      } finally {
        if (alive) setLoading(false);
      }
    };
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 120_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [symbolsKey]);

  // Live prices: the value, gains and suggested amounts follow the market tick by tick.
  const live = useLive(holdings);
  const liveMarket = useMemo(() => {
    const m: Record<string, MarketInput> = {};
    for (const [k, v] of Object.entries(market)) m[k] = live.ticks[k] ? { ...v, price: live.ticks[k]!.price } : v;
    return m;
  }, [market, live.ticks]);
  const analysis = useMemo<PortfolioAnalysis>(() => analyzePortfolio(holdings, cash, liveMarket), [holdings, cash, liveMarket]);
  const ready = holdings.length === 0 || Object.keys(market).length > 0;

  // Risk beyond the analysis: betas, stress scenarios, limits of the settings, dangerous positions.
  const daily = useMemo(() => {
    const d: Record<string, Candle[]> = { ...bench };
    for (const [k, v] of Object.entries(market)) if (v.daily.length) d[k] = v.daily;
    return d;
  }, [market, bench]);
  const stops = useMemo(() => Object.fromEntries(holdings.map((h) => [h.id, h.stop])), [holdings]);
  const betas = useMemo(() => {
    const b: Record<string, BetaEstimate> = {};
    for (const h of holdings) {
      const k = `${h.kind}:${h.symbol}`;
      const ref = BENCHMARK[h.kind];
      // The benchmark itself: beta 1 by definition.
      b[k] = h.symbol === ref.symbol && h.kind === ref.kind ? { beta: 1, days: 0, estimated: false, reference: true } : estimateBeta(daily[k] ?? [], daily[`${ref.kind}:${ref.symbol}`] ?? []);
    }
    return b;
  }, [holdings, daily]);
  const riskView = useMemo(() => {
    if (!ready || !holdings.length) return null;
    const weights = new Map<string, { symbol: string; weight: number; daily: Candle[] }>();
    for (const l of analysis.lines) {
      const k = `${l.kind}:${l.symbol}`;
      weights.set(k, { symbol: l.symbol, weight: (weights.get(k)?.weight ?? 0) + l.weight, daily: daily[k] ?? [] });
    }
    const clusters = correlatedClusters([...weights.values()]);
    const today = dailyChange(analysis, daily, Date.now());
    return {
      stress: stressTest(analysis, betas),
      limits: checkLimits(analysis, risk, { clusters, daily: today, stops }),
      dangers: dangerousPositions(analysis, risk, daily, stops),
    };
  }, [ready, holdings.length, analysis, daily, betas, risk, stops]);
  const dangerById = useMemo(() => new Map<string, Danger>((riskView?.dangers ?? []).map((d) => [d.id, d])), [riskView]);
  // Kept for the Radar (the web app has no notifications); only once the market data is loaded.
  const dangerKey = JSON.stringify(riskView?.dangers.map((d) => [d.id, d.reasons.map((r) => r.code)]) ?? null);
  useEffect(() => {
    if (!holdings.length) saveDangers([]);
    else if (riskView && Object.keys(market).length) saveDangers(riskView.dangers);
  }, [dangerKey]);

  // Typed in the display currency and saved with it (an untouched field keeps its saved currency).
  const saveCash = () => {
    if (cashText === cashInitial) return;
    const v = Number(cashText.replace(/\s/g, "").replace(",", "."));
    if (Number.isFinite(v) && v >= 0) setHoldings({ cash: v, cashCurrency: cur });
    else setCashText(cashInitial);
  };

  /** Spreadsheet export (French Excel: ";" separator, decimal comma); texts starting like a formula are neutralised. */
  const downloadCsv = () => {
    const sym = currencySymbol();
    const cell = (v: string | number | null, digits = 2) => {
      if (v == null) return "";
      if (typeof v === "number") return Number.isFinite(v) ? String(Math.round(v * 10 ** digits) / 10 ** digits).replace(".", ",") : "";
      const s = /^[=+\-@\t\r]/.test(v) ? `'${v}` : v;
      return /[;"\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    };
    const rows = [
      ["Symbole", "Nom", "Type", "Quantité", `Prix moyen (${sym})`, `Cours (${sym})`, `Valeur (${sym})`, `Plus-value (${sym})`, "Plus-value (%)", "Poids (%)", "Conseil Altim"],
      ...analysis.lines.map((l) => [
        l.symbol, l.name, l.kind === "crypto" ? "Crypto" : "Action", l.quantity, toDisplay(l.averagePrice), l.price == null ? null : toDisplay(l.price), toDisplay(l.value),
        unconverted.includes(l.symbol) ? null : toDisplay(l.pnl), unconverted.includes(l.symbol) ? null : l.pnlPercent, l.weight, RECOMMENDATION_LABEL[l.recommendation],
      ]),
      ["Liquidités", "", "", "", "", "", toDisplay(analysis.cash), "", "", "", ""],
    ];
    const blob = new Blob(["﻿" + rows.map((r) => r.map((v, i) => cell(v, CSV_DIGITS[i])).join(";")).join("\r\n")], { type: "text/csv;charset=utf-8" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `altim-avoirs-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  const download = () => {
    const blob = new Blob([exportHoldings()], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `altim-avoirs-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  return (
    <section className="app-screen">
      <PortfolioTabs active="real" />
      <div className="screen-top">
        <div>
          <h1>Mes avoirs</h1>
          <p className="muted small">Enregistrés uniquement dans ce navigateur{updatedAt ? ` · modifiés le ${new Date(updatedAt).toLocaleDateString("fr-FR")}` : ""}</p>
        </div>
        <button className="btn btn-small" onClick={() => setAdding(true)}>+ Ajouter</button>
      </div>

      {!holdings.length && (
        <div className="card empty-card">
          <h2>Renseignez ce que vous possédez déjà</h2>
          <p className="muted">
            Ajoutez en une fois toutes vos cryptos et toutes vos actions, avec leur quantité et votre prix d'achat moyen. Altim calcule alors votre patrimoine, vos gains, vos risques et
            vous dit, ligne par ligne, quoi faire. Vos données restent dans ce navigateur.
          </p>
          <button className="btn" onClick={() => setAdding(true)}>Ajouter mes cryptos et actions</button>
        </div>
      )}

      <div className="card">
        <label className="field">
          <span>Liquidités disponibles ({cur === "EUR" ? "€" : "$"})</span>
          <input inputMode="decimal" value={cashText} placeholder="0" onChange={(e) => setCashText(e.target.value)} onBlur={saveCash} onKeyDown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()} />
        </label>
        {stored.cash > 0 && storedCurrency(stored.cashCurrency) !== cur && !cashUnconverted && (
          <p className="muted small">Montant saisi en {storedCurrency(stored.cashCurrency) === "USD" ? "$" : "€"}, converti au taux du jour.</p>
        )}
      </div>

      {error && <p className="notice warn">⚠ {error} — nouvelle tentative automatique.</p>}
      {(unconverted.length > 0 || cashUnconverted) && (
        <p className="notice warn" role="status">
          ⚠ Taux EUR/USD indisponible : {[...unconverted, ...(cashUnconverted ? ["liquidités"] : [])].join(", ")} saisi(es) en € ne peuvent pas être converti(es) ;
          plus-values de ces lignes non calculées, liquidités comptées à 0 jusqu'au retour du taux.
        </p>
      )}

      {holdings.length > 0 && (
        <>
          <div className="card summary-card">
            <div className="summary-top">
              <small className="muted">Patrimoine total</small>
              <LiveBadge status={live.status} last={live.last} />
            </div>
            <b className="mono big">{ready ? usd(analysis.total) : "…"}</b>
            {ready && !unconverted.length && (
              <p className="kv"><span>Plus-value latente</span><b className={analysis.pnl >= 0 ? "up" : "down"}>{analysis.pnl >= 0 ? "+" : "−"}{usd(Math.abs(analysis.pnl))} (<Change value={analysis.pnlPercent} />)</b></p>
            )}
            <p className="kv small"><span>Investi (prix d'achat)</span><b>{unconverted.length ? "non calculé" : usd(analysis.invested)}</b></p>
            {ready && <AllocationBar a={analysis.allocation} />}
            {loading && <p className="muted small">Actualisation des cours et des signaux…</p>}
            <FxNote />
          </div>

          <HistoryCard holdings={holdings} />

          {ready && <RebalanceCard analysis={analysis} />}

          {ready && <SaleCard analysis={analysis} />}

          {ready && <ProjectionCard start={analysis.total} />}

          {riskView?.limits.some((c) => c.code === "daily_loss" && c.level === "danger") && (
            <p className="notice danger" role="alert">⛔ Limite de perte du jour atteinte : n'ouvrez plus de position aujourd'hui.</p>
          )}

          {riskView && riskView.dangers.length > 0 && (
            <div className="notice danger" role="status">
              <b>{riskView.dangers.length > 1 ? "Positions devenues dangereuses" : "Position devenue dangereuse"} : {riskView.dangers.map((d) => d.symbol).join(", ")}</b>
              <br />
              <small>Détail sur chaque ligne ci-dessous. Altim ne passe aucun ordre : à vous de décider (réduire, sortir ou accepter le risque).</small>
            </div>
          )}

          {ready && (
            <div className="card insights-card">
              <h2 className="card-title">Ce qu'Altim vous dit</h2>
              <ul className="insights">
                {analysis.insights.map((i, n) => (
                  <li key={n} className={`insight ${i.level}`}>
                    <span aria-hidden>{LEVEL_ICON[i.level]}</span>
                    <span>{insightText(i)}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {(["crypto", "stock"] as const).map((kind) => {
            const lines = analysis.lines.filter((l) => l.kind === kind);
            if (!lines.length) return null;
            const value = lines.reduce((a, l) => a + l.value, 0);
            return (
              <div key={kind} className="holding-group">
                <h2 className="section-label group-head">
                  <span>{KIND_LABEL[kind]} · {lines.length}</span>
                  {ready && <span className="mono">{usd(value)}</span>}
                </h2>
                <ul className="holding-list">
                  {lines.map((l) => (
                    <li key={l.id} className={`card holding ${dangerById.has(l.id) ? "sell" : REC_CLASS[l.recommendation]}`}>
                      <div className="holding-head">
                        <a href={`/app/actif/${l.kind}/${l.symbol}`} onClick={onLink} className="holding-name">
                          <b>{l.name}</b>
                          <small className="muted mono">
                            {l.quantity.toLocaleString("fr-FR", { maximumFractionDigits: 8 })} {l.symbol} · PRU {unconverted.includes(l.symbol) ? "non converti" : moneyPrice(l.averagePrice)}
                          </small>
                          {costNote(stored.holdings.find((h) => h.id === l.id), cur) && <small className="muted">{costNote(stored.holdings.find((h) => h.id === l.id), cur)}</small>}
                        </a>
                        <span className={`rec rec-${REC_CLASS[l.recommendation]}`}>{RECOMMENDATION_LABEL[l.recommendation]}</span>
                      </div>
                      <div className="holding-figures">
                        <div><small>Valeur</small><b>{usd(l.value)}</b></div>
                        {unconverted.includes(l.symbol)
                          ? <div><small>Gain / perte</small><b className="muted">non calculé</b></div>
                          : <div><small>Gain / perte</small><b className={l.pnl >= 0 ? "up" : "down"}>{l.pnl >= 0 ? "+" : "−"}{usd(Math.abs(l.pnl))}</b><Change value={l.pnlPercent} /></div>}
                        <div><small>Cours{live.ticks[`${l.kind}:${l.symbol}`]?.market === "closed" ? " (fermé)" : ""}</small><b><LivePrice tick={live.ticks[`${l.kind}:${l.symbol}`]} fallback={l.price || null} format={(v) => moneyPrice(v)} /></b></div>
                      </div>
                      <div className="weight" role="img" aria-label={`Poids ${l.weight.toFixed(1)} % du patrimoine`}>
                        <div className="weight-track"><i style={{ width: `${Math.min(100, l.weight)}%` }} /></div>
                        <small>{l.weight.toFixed(1)} % du patrimoine</small>
                      </div>
                      {dangerById.get(l.id) && (
                        <div className="notice danger small" role="status">
                          <b>⚠ Position devenue dangereuse</b>
                          <ul className="reasons">{dangerById.get(l.id)!.reasons.map((r) => <li key={r.code}>{r.text}</li>)}</ul>
                        </div>
                      )}
                      <ul className="reasons">
                        {l.reasons.map((r) => <li key={r}>{REASON_TEXT[r] ?? r}</li>)}
                        {stops[l.id] && (
                          <li>
                            Votre stop : <b>{moneyPrice(stops[l.id]!)}</b>
                            {l.price ? ` (${(((l.price - stops[l.id]!) / l.price) * 100).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} % sous le cours)` : ""}.
                          </li>
                        )}
                        {l.recommendation === "lighten" && l.trimValue > 0 && l.price && (
                          <li>
                            Suggestion : vendre environ {usd(l.trimValue)} (≈ {(l.trimValue / l.price).toLocaleString("fr-FR", { maximumFractionDigits: 6 })} {l.symbol}).
                          </li>
                        )}
                        {l.stop && l.lossAtStop !== null && (
                          <li>
                            Stop de protection conseillé : <b>{moneyPrice(l.stop)}</b> (2 × la volatilité journalière) — perte limitée à ≈ {usd(l.lossAtStop)} depuis le cours actuel.
                          </li>
                        )}
                      </ul>
                      <div className="holding-actions">
                        <button className="link-btn" onClick={() => setEditing(stored.holdings.find((h) => h.id === l.id)!)}>Modifier</button>
                        <button className="link-btn danger" onClick={() => confirm(`Supprimer ${l.name} de vos avoirs ?`) && setHoldings((s) => ({ holdings: s.holdings.filter((h) => h.id !== l.id) }))}>Supprimer</button>
                      </div>
                    </li>
                  ))}
                </ul>
              </div>
            );
          })}

          {ready && (
            <div className="card">
              <h2 className="card-title">Risque du portefeuille</h2>
              <p className="kv"><span>Volatilité annuelle</span><b>{analysis.risk.volatilityAnnual !== null ? `${analysis.risk.volatilityAnnual.toFixed(1)} %` : "—"}</b></p>
              <p className="kv"><span>Perte possible sur 1 jour (1 fois sur 20)</span><b>{analysis.risk.var95Day !== null ? `${usd(analysis.risk.var95Day)} (${analysis.risk.var95DayPercent!.toFixed(1)} %)` : "—"}</b></p>
              <p className="kv"><span>Perte si tous les stops sont touchés</span><b>{usd(analysis.risk.lossAtStops)}</b></p>
              <p className="kv"><span>Ligne la plus lourde</span><b>{analysis.risk.maxWeight.toFixed(1)} %</b></p>
              <p className="kv"><span>Diversification effective</span><b>{analysis.risk.effectiveAssets.toFixed(1)} actif(s)</b></p>
              <p className="kv"><span>Corrélation moyenne</span><b>{analysis.risk.averageCorrelation !== null ? analysis.risk.averageCorrelation.toFixed(2) : "—"}</b></p>
              <p className="muted small">Calculé sur les 90 derniers jours de cours journaliers, recoupés entre plusieurs sources. Les performances passées ne préjugent pas des performances futures.</p>
            </div>
          )}

          {ready && <SectorCard analysis={analysis} />}

          {riskView && <LimitsCard checks={riskView.limits} />}

          {riskView && <StressCard analysis={analysis} results={riskView.stress} betas={betas} />}

          {riskView && <WhatIfCard analysis={analysis} daily={daily} />}
        </>
      )}

      <div className="card">
        <h2 className="card-title">Sauvegarde</h2>
        <p className="muted small">Vos avoirs sont conservés dans ce navigateur (localStorage). Exportez-les pour les garder en sécurité ou les transférer.</p>
        <div className="row-actions">
          <button className="btn btn-ghost" onClick={download} disabled={!holdings.length && !cash}>Exporter (JSON)</button>
          <button className="btn btn-ghost" onClick={downloadCsv} disabled={!holdings.length || !ready}>Tableur (CSV)</button>
          <button className="btn btn-ghost" onClick={() => fileInput.current?.click()}>Importer</button>
          <input
            ref={fileInput}
            type="file"
            accept="application/json,.json"
            hidden
            onChange={async (e) => {
              const f = e.target.files?.[0];
              if (!f) return;
              const err = importHoldings(await f.text());
              alert(err ?? "Avoirs importés.");
              e.target.value = "";
            }}
          />
        </div>
      </div>

      {editing && <HoldingForm stored={editing} initial={holdings.find((h) => h.id === editing.id) ?? editing} lastPrice={analysis.lines.find((l) => l.id === editing.id)?.price ?? null} onClose={() => setEditing(null)} />}
      {adding && <AddHoldings onClose={() => setAdding(false)} />}
    </section>
  );
}

/** Allocation by class: stacked bar + legend + labels (color never the only cue). */
function AllocationBar({ a }: { a: PortfolioAnalysis["allocation"] }) {
  const parts = [
    { key: "crypto", label: "Crypto", value: a.crypto },
    { key: "stock", label: "Actions", value: a.stock },
    { key: "cash", label: "Liquidités", value: a.cash },
  ].filter((p) => p.value > 0.05);
  return (
    <figure className="alloc">
      <figcaption className="muted small">Répartition</figcaption>
      <div className="alloc-bar" role="img" aria-label={parts.map((p) => `${p.label} ${p.value.toFixed(1)} %`).join(", ")}>
        {parts.map((p) => (
          <span key={p.key} className={`alloc-${p.key}`} style={{ flexGrow: p.value }} title={`${p.label} : ${p.value.toFixed(1)} %`} />
        ))}
      </div>
      <ul className="alloc-legend">
        {parts.map((p) => (
          <li key={p.key}><i className={`alloc-${p.key}`} />{p.label} <b>{p.value.toFixed(1)} %</b></li>
        ))}
      </ul>
    </figure>
  );
}

/** « PRU saisi en $, converti au taux du jour » when the cost was typed in the other currency. */
function costNote(h: StoredHolding | undefined, cur: "EUR" | "USD"): string | null {
  if (!h || storedCurrency(h.costCurrency) === cur || !Number.isFinite(shown(h.averagePrice, h.costCurrency))) return null;
  return `Prix de revient saisi en ${storedCurrency(h.costCurrency) === "USD" ? "$" : "€"}, converti au taux du jour.`;
}

/**
 * Editing one existing line (new lines are added with AddHoldings). Amounts are typed in the display currency; a
 * field left untouched keeps its saved value and currency (a dollar cost basis is not silently re-based in euros).
 */
function HoldingForm({ stored, initial, lastPrice, onClose }: { stored: StoredHolding; initial: Holding; lastPrice: number | null; onClose: () => void }) {
  const cur = displayCurrency();
  const sym = cur === "EUR" ? "€" : "$";
  const pruInitial = inputText(shown(stored.averagePrice, stored.costCurrency));
  const stopInitial = stored.stop ? inputText(shown(stored.stop, stored.stopCurrency)) : "";
  const [qty, setQty] = useState(String(initial.quantity));
  const [pru, setPru] = useState(pruInitial);
  const [stopText, setStopText] = useState(stopInitial);
  const [price, setPrice] = useState<number | null>(null);
  const [journal, setJournal] = useState(true);
  const [note, setNote] = useState("");

  useEffect(() => {
    api.quotes([initial]).then((r) => setPrice(r[0]?.price ?? null)).catch(() => setPrice(null));
  }, [initial.symbol, initial.kind]);

  const n = (s: string) => Number(s.replace(/\s/g, "").replace(",", "."));
  const quantity = n(qty);
  const keepCost = pru === pruInitial;
  const keepStop = stopText === stopInitial;
  // What is saved (typed currency), and the same in dollars for the journal and the engines.
  const averagePrice = keepCost ? stored.averagePrice : n(pru);
  const costCurrency = keepCost ? stored.costCurrency : cur;
  const stop = keepStop ? stored.stop : stopText.trim() ? n(stopText) : undefined;
  const stopCurrency = keepStop ? stored.stopCurrency : cur;
  const valid = quantity > 0 && Number.isFinite(averagePrice) && averagePrice >= 0 && (stop === undefined || (Number.isFinite(stop) && stop > 0));
  const costUsd = keepCost ? initial.averagePrice : fromDisplay(averagePrice);
  const stopUsd = stop === undefined ? undefined : keepStop ? initial.stop : fromDisplay(stop);

  const change = valid && Number.isFinite(costUsd) ? holdingChange(initial, { quantity, averagePrice: costUsd }, price ?? (lastPrice || null)) : null;
  const save = () => {
    if (!valid) return;
    upsertHolding({ id: initial.id, symbol: initial.symbol, kind: initial.kind, name: initial.name, quantity, averagePrice, costCurrency, stop, stopCurrency });
    if (journal && change) recordRealTrade({ ...change, symbol: initial.symbol, kind: initial.kind, name: initial.name, stop: change.side === "buy" ? stopUsd : null, note, refId: initial.id });
    onClose();
  };

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" role="dialog" aria-modal="true" aria-label={`Modifier ${initial.name}`} onClick={(e) => e.stopPropagation()}>
        <div className="sheet-handle" />
        <div className="sheet-head"><h2>Modifier {initial.name}</h2></div>
        <div className="chosen"><b>{initial.name}</b> <small className="muted mono">{initial.symbol}</small></div>
        {price && <p className="muted small">Cours actuel (consensus) : {moneyPrice(price)}</p>}
        <label className="field">
          <span>Quantité détenue</span>
          <input inputMode="decimal" autoFocus value={qty} onChange={(e) => setQty(e.target.value)} />
        </label>
        <label className="field">
          <span>Prix d'achat moyen ({sym}, PRU)</span>
          <input inputMode="decimal" value={pru} onChange={(e) => setPru(e.target.value)} />
        </label>
        {keepCost && costNote(stored, cur) && <p className="muted small">{costNote(stored, cur)} Le modifier l'enregistre en {sym}.</p>}
        <label className="field">
          <span>Mon stop ({sym}, facultatif)</span>
          <input inputMode="decimal" value={stopText} placeholder="aucun" onChange={(e) => setStopText(e.target.value)} />
        </label>
        <p className="muted small">Prix auquel vous comptez vendre pour limiter la perte. Altim vous alerte quand le cours s'en approche (moins d'une volatilité journalière) ou le casse. Aucun ordre n'est passé.</p>
        {quantity > 0 && costUsd > 0 && <p className="kv small"><span>Montant investi</span><b>{usd(quantity * costUsd)}</b></p>}
        {change && (
          <JournalToggle checked={journal} onChange={setJournal} note={note} onNote={setNote}
            text={`${change.side === "buy" ? "Achat" : "Vente"} de ${change.quantity.toLocaleString("fr-FR", { maximumFractionDigits: 8 })} ${initial.symbol} à ${moneyPrice(change.price)}${change.implied ? " (déduit du nouveau PRU)" : " (cours actuel)"} : l'inscrire au journal`} />
        )}
        <button className="btn" disabled={!valid} onClick={save}>Enregistrer</button>
        <button className="btn btn-ghost" onClick={onClose}>Annuler</button>
      </div>
    </div>
  );
}

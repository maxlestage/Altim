/**
 * "Comparer les stratégies" (asset screen): textbook strategies with fixed parameters on this asset's daily history
 * (/api/strategies). Chips to pick them, one single-axis chart (value of 100 invested) or its list view, then one
 * stacked card of metrics per strategy and how the test avoids flattering itself.
 */
import { useEffect, useState } from "react";
import { api } from "./api";
import type { Kind } from "../engine/reliability";
import {
  checkpoints, COLORS, DEFAULT_SELECTION, drawable, geometry, indexAt, lowSampleText, ORDER, placeLabels, plainPct, ratio, REFERENCE, SHORT, shortDate,
  signedPct, NNBSP, type StrategiesReport, type StrategyId, type StrategyResult,
} from "./strategies";

const BOX = { w: 320, h: 180, l: 34, r: 6, t: 10, b: 18 };
const LABELS_W = 62;
/** Direct labels up to 4 coloured curves (plus the reference). */
const MAX_LABELED = 4;
const value100 = (v: number) => v.toLocaleString("fr-FR", { maximumFractionDigits: 0 });

export function StrategiesCard({ symbol, kind }: { symbol: string; kind: Kind }) {
  const [report, setReport] = useState<StrategiesReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<StrategyId[]>(DEFAULT_SELECTION);
  const [asList, setAsList] = useState(false);

  useEffect(() => {
    let alive = true;
    setReport(null);
    setError(null);
    api.strategies(symbol, kind)
      .then((r) => alive && setReport(r))
      .catch((e) => alive && setError(e instanceof Error ? e.message : "Comparaison indisponible"));
    return () => {
      alive = false;
    };
  }, [symbol, kind]);

  const toggle = (id: StrategyId) => setSelected((s) => (s.includes(id) ? s.filter((x) => x !== id) : ORDER.filter((x) => x === id || s.includes(x))));
  const series = report ? drawable(report, selected) : [];
  const chosen = report ? ORDER.filter((id) => selected.includes(id)).map((id) => report.strategies.find((s) => s.id === id)).filter((s): s is StrategyResult => !!s) : [];

  return (
    <div className="card strat-card">
      <h2 className="card-title">Comparer les stratégies</h2>
      <p className="muted small">Comment des stratégies classiques, avec leurs réglages de manuel, se seraient comportées sur l'historique de {symbol}.</p>
      {error && <p className="notice warn">⚠ {error}</p>}
      {!report && !error && <div className="skeleton" aria-label="Chargement de la comparaison" />}
      {report && (
        <>
          <div className="strat-chips" role="group" aria-label="Stratégies à comparer">
            {ORDER.map((id) => {
              const s = report.strategies.find((x) => x.id === id);
              if (!s) return null;
              const on = selected.includes(id);
              return (
                <button key={id} type="button" className={`chip pick${on ? " on" : ""}`} aria-pressed={on} aria-label={s.name} title={s.name} onClick={() => toggle(id)} style={on ? { borderColor: COLORS[id] } : undefined}>
                  <Swatch id={id} />
                  {SHORT[id]}
                </button>
              );
            })}
          </div>
          <p className="small strat-period">
            <b>Période testée</b> : {report.period} · source {report.source}
          </p>

          {series.length === 0 ? (
            <p className="muted small">Choisissez au moins une stratégie disponible.</p>
          ) : asList ? (
            <ListView series={series} />
          ) : (
            <Chart series={series} />
          )}
          {series.length > 0 && (
            <button type="button" className="link-btn" onClick={() => setAsList((v) => !v)}>
              {asList ? "Voir le graphique" : "Voir en liste"}
            </button>
          )}

          <div className="strat-list">
            {chosen.map((s) => <StrategyBlock key={s.id} s={s} />)}
          </div>

          <p className="small"><b>Comment ce test évite de se flatter</b></p>
          <ul className="reasons">{report.notes.map((n) => <li key={n}>{n}</li>)}</ul>
        </>
      )}
    </div>
  );
}

function Swatch({ id }: { id: StrategyId }) {
  return <i className={`strat-swatch${id === REFERENCE ? " ref" : ""}`} style={{ color: COLORS[id] }} aria-hidden />;
}

export function Chart({ series }: { series: StrategyResult[] }) {
  const [hover, setHover] = useState<number | null>(null);
  const coloured = series.filter((s) => s.id !== REFERENCE).length;
  const labeled = coloured <= MAX_LABELED;
  const box = { ...BOX, r: labeled ? LABELS_W : BOX.r };
  const g = geometry(series, box);
  if (!g) return null;
  const path = (s: StrategyResult) => s.equity.map((p, i) => `${i ? "L" : "M"}${g.x(i).toFixed(1)},${g.y(p[1]).toFixed(1)}`).join("");
  const at = hover ?? g.n - 1;
  const time = series[0]!.equity[Math.min(at, series[0]!.equity.length - 1)]![0];
  const ends = series.map((s) => g.y(s.equity[s.equity.length - 1]![1]));
  const labelY = labeled ? placeLabels(ends, 11, box.t + 4, box.h - box.b) : [];
  const pick = (e: React.PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    setHover(indexAt(((e.clientX - r.left) / r.width) * box.w, g, box));
  };
  const summary = series.map((s) => `${s.name} ${value100(s.equity[s.equity.length - 1]![1])}`).join(", ");

  return (
    <div className="history-chart strat-chart">
      <p className="history-read small" aria-live="polite">
        <b>{shortDate(time)}</b> · valeur de 100 investis{NNBSP}:
        {series.map((s) => (
          <span key={s.id} className="strat-read"> <Swatch id={s.id} />{SHORT[s.id]} <b>{value100(s.equity[Math.min(at, s.equity.length - 1)]![1])}</b></span>
        ))}
      </p>
      <svg
        viewBox={`0 0 ${box.w} ${box.h}`}
        role="img"
        aria-label={`Valeur de 100 investis à la fin de la période : ${summary}`}
        onPointerMove={pick}
        onPointerDown={pick}
        onPointerLeave={() => setHover(null)}
      >
        <line x1={box.l} x2={box.w - box.r} y1={g.y(100)} y2={g.y(100)} className="zero" />
        <text x={box.l - 4} y={g.y(100) + 3} className="axis" textAnchor="end">100</text>
        <text x={box.l - 4} y={box.t + 6} className="axis" textAnchor="end">{value100(g.hi)}</text>
        <text x={box.l - 4} y={box.h - box.b} className="axis" textAnchor="end">{value100(g.lo)}</text>
        <text x={box.l} y={box.h - 4} className="axis">{shortDate(series[0]!.equity[0]![0])}</text>
        <text x={box.w - box.r} y={box.h - 4} className="axis" textAnchor="end">{shortDate(series[0]!.equity[series[0]!.equity.length - 1]![0])}</text>
        {series.map((s) => (
          <path key={s.id} d={path(s)} stroke={COLORS[s.id]} className={s.id === REFERENCE ? "bench strat-ref" : "mine"} />
        ))}
        {labeled && series.map((s, k) => (
          <g key={s.id}>
            <line x1={box.w - box.r + 3} x2={box.w - box.r + 9} y1={labelY[k]!} y2={labelY[k]!} stroke={COLORS[s.id]} className={s.id === REFERENCE ? "strat-tick ref" : "strat-tick"} />
            <text x={box.w - box.r + 12} y={labelY[k]! + 3} className="strat-label">{SHORT[s.id]}</text>
          </g>
        ))}
        {hover != null && <line x1={g.x(at)} x2={g.x(at)} y1={box.t} y2={box.h - box.b} className="cross" />}
        {hover != null && series.map((s) => (
          <circle key={s.id} cx={g.x(at)} cy={g.y(s.equity[Math.min(at, s.equity.length - 1)]![1])} r={4} fill={COLORS[s.id]} className="dot" />
        ))}
      </svg>
      <ul className="history-legend">
        {series.map((s) => (
          <li key={s.id}><Swatch id={s.id} />{s.name} <b>{signedPct(s.metrics?.totalReturn)}</b></li>
        ))}
      </ul>
      {!labeled && <p className="muted small">Plus de {MAX_LABELED} courbes : touchez le graphique pour lire les valeurs, ou passez en liste.</p>}
    </div>
  );
}

/** Stacked rows: the value of 100 invested at five dates of the period, per strategy. */
export function ListView({ series }: { series: StrategyResult[] }) {
  return (
    <ul className="strat-rows">
      {series.map((s) => (
        <li key={s.id}>
          <p className="small"><Swatch id={s.id} /><b>{s.name}</b> · {signedPct(s.metrics?.totalReturn)}</p>
          <ul className="strat-steps small">
            {checkpoints(s).map((c) => <li key={c.t}><span className="muted">{shortDate(c.t)}</span> <b>{value100(c.v)}</b></li>)}
          </ul>
        </li>
      ))}
    </ul>
  );
}

export function StrategyBlock({ s }: { s: StrategyResult }) {
  const m = s.metrics;
  const dca = s.id === "dca";
  const low = lowSampleText(s);
  const kv = (label: string, value: string, tone?: "up" | "down" | null) => (
    <p className="kv small"><span>{label}</span><b className={tone ?? undefined}>{value}</b></p>
  );
  const tone = (v: number | null | undefined) => (v == null ? null : v >= 0 ? "up" : "down");
  return (
    <section className="strat-block" aria-label={s.name}>
      <p className="strat-head"><Swatch id={s.id} /><b>{s.name}</b>{low && <span className="chip muted">{low}</span>}</p>
      <p className="small">{s.rule}</p>
      <p className="muted small">Paramètres fixes : {s.params}</p>
      {!s.available || !m ? (
        <p className="notice small">{s.unavailable ?? "Non couvert."}</p>
      ) : (
        <>
          <div className="strat-metrics">
            {kv(dca ? "Gain sur les sommes versées" : "Rendement total", signedPct(m.totalReturn), tone(m.totalReturn))}
            {kv(dca ? "Rendement annuel (TRI)" : "Rendement annuel", m.cagr == null ? "— (< 1 an)" : signedPct(m.cagr), tone(m.cagr))}
            {kv("Pire recul", signedPct(m.maxDrawdown), "down")}
            {kv("Sharpe / Sortino", dca ? "non pertinent" : `${ratio(m.sharpe)} / ${ratio(m.sortino)}`)}
            {kv(dca ? "Achats" : "Trades", String(m.trades))}
            {kv("Temps investi", plainPct(m.exposure))}
            {!dca && kv("Taux de réussite", plainPct(m.winRate))}
            {!dca && kv("Profit factor", ratio(m.profitFactor))}
            {!dca && kv("Espérance par trade", signedPct(m.expectancy, 2), tone(m.expectancy))}
            {m.avgR != null && kv("Multiple de R moyen", `${ratio(m.avgR)} R`)}
          </div>
          {s.regimes.length > 0 && (
            <ul className="strat-regimes small">
              {s.regimes.map((g) => (
                <li key={g.regime}>
                  <span>{g.label.split(" (")[0]}</span>
                  <b>
                    {g.trades} {dca ? "achat" : "trade"}{g.trades > 1 ? "s" : ""}
                    {g.trades > 0 && <> · moy. {signedPct(g.avgReturn)}</>}
                  </b>
                  {g.trades > 0 && g.lowSample && <span className="chip muted">échantillon trop faible</span>}
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      {s.note && <p className="muted small">{s.note}</p>}
    </section>
  );
}

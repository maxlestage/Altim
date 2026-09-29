import { useEffect, useMemo, useState } from "react";
import type { PortfolioAnalysis } from "../engine/holdings";
import { CLASSIFICATION_SOURCE, sectorExposure, sectorInsightText, type SectorClassification, type SectorItem } from "../engine/sectors";
import { api } from "./api";

const pc = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;

/** "Exposition sectorielle": stocks by sector (Nasdaq / SEC), cryptos and cash as their own blocks. */
export function SectorCard({ analysis }: { analysis: PortfolioAnalysis }) {
  const stocks = useMemo(() => [...new Set(analysis.lines.filter((l) => l.kind === "stock").map((l) => l.symbol))].sort(), [analysis.lines]);
  const key = stocks.join(",");
  const [state, setState] = useState<{ key: string; items: Record<string, SectorItem> | null; error: string | null } | null>(null);

  useEffect(() => {
    if (!stocks.length) return setState({ key, items: {}, error: null });
    let alive = true;
    api
      .sectors(stocks)
      .then((r) => alive && setState({ key, items: Object.fromEntries(r.items.map((i) => [i.symbol, i])), error: null }))
      .catch((e) => alive && setState({ key, items: null, error: e instanceof Error ? e.message : "serveur injoignable" }));
    return () => {
      alive = false;
    };
  }, [key]);

  const loaded = state?.key === key;
  const exposure = useMemo(
    () => (loaded ? sectorExposure(analysis.lines, analysis.cash, state!.items, state!.error ? `classement indisponible (${state!.error})` : undefined) : null),
    [loaded, state, analysis.lines, analysis.cash],
  );

  return (
    <div className="card sector-card">
      <h2 className="card-title">Exposition sectorielle</h2>
      {!exposure ? (
        <p className="muted small">Lecture des secteurs de vos actions…</p>
      ) : (
        <>
          <ul className="sector-bars" aria-label="Poids de chaque secteur dans le patrimoine">
            {exposure.blocks.map((b) => (
              <li key={b.key} className={`sector-row sector-${b.kind}`}>
                <div className="sector-head">
                  <span className="sector-label">{b.label}</span>
                  <b className="mono">{pc(b.weight)}</b>
                </div>
                <div className="sector-track" aria-hidden><i style={{ width: `${Math.min(100, b.weight)}%` }} /></div>
                {(b.symbols.length > 0 || b.stockWeight !== null) && (
                  <small className="muted">
                    {b.symbols.join(", ")}
                    {b.stockWeight !== null && exposure.stockValue < exposure.total ? ` · ${pc(b.stockWeight)} des actions` : ""}
                  </small>
                )}
              </li>
            ))}
          </ul>
          {/* The effective number of sectors is shown as a figure below. */}
          {exposure.insights.some((i) => i.code !== "sector_effective") && (
            <ul className="insights">
              {exposure.insights.filter((i) => i.code !== "sector_effective").map((i) => (
                <li key={i.code} className={`insight ${i.level}`}>
                  <span aria-hidden>{i.level === "warning" ? "⚠" : "ℹ"}</span>
                  <span>{sectorInsightText(i)}</span>
                </li>
              ))}
            </ul>
          )}
          {exposure.effectiveSectors !== null && (
            <p className="kv small"><span>Secteurs effectifs (actions classées)</span><b>{exposure.effectiveSectors.toLocaleString("fr-FR", { maximumFractionDigits: 1 })}</b></p>
          )}
          {exposure.unknown.length > 0 && (
            <ul className="reasons small">
              {exposure.unknown.map((u) => <li key={u.symbol}><b>{u.symbol}</b> : {u.reason}</li>)}
            </ul>
          )}
          <p className="muted small">
            Sources : {sourceLine(exposure.bySource) || "aucune action à classer"}. Crypto et liquidités : vos avoirs. Les secteurs SEC (codes SIC) sont de grandes divisions, affichés à part de ceux du Nasdaq.
          </p>
        </>
      )}
    </div>
  );
}

function sourceLine(by: Record<SectorClassification, number>): string {
  return (Object.keys(by) as SectorClassification[])
    .filter((k) => by[k] > 0)
    .map((k) => `${CLASSIFICATION_SOURCE[k]} · ${by[k]} action${by[k] > 1 ? "s" : ""}`)
    .join(" ; ");
}

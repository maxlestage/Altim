import { useEffect, useState } from "react";
import { api } from "./api";
import { compactUsd, longShare, type Anomaly, type AnomalyReport, type Derivatives } from "../engine/opportunities";

const fr = (v: number, d = 1) => v.toLocaleString("fr-FR", { maximumFractionDigits: d });
const signed = (v: number, d = 1) => `${v >= 0 ? "+" : "−"}${fr(Math.abs(v), d)} %`;
const time = (t: number) => new Date(t).toLocaleString("fr-FR", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

function Alert({ a }: { a: Anomaly }) {
  return (
    <li className={`anom anom-${a.severity}`}>
      <b>⚠️ {a.title}</b>
      <small className="mono">{a.measured}</small>
      <p className="small">{a.meaning}</p>
      <small className="muted">Source : {a.source}</small>
    </li>
  );
}

function DerivativesBlock({ d }: { d: Derivatives }) {
  const l = d.liquidations;
  const share = l ? longShare(l) : null;
  return (
    <div className="anom-deriv">
      <h3 className="section-label">Dérivés et gros mouvements</h3>
      {d.errors.map((e) => <p key={e} className="notice warn small">{e}</p>)}
      {l && (
        <div className="anom-block">
          <p className="kv"><span>Liquidations {l.complete ? "24 h" : `${fr(l.hours)} h (lecture partielle)`}</span><b>{compactUsd(l.longUsd + l.shortUsd)}</b></p>
          <p className="kv small"><span>Acheteurs liquidés ({l.longCount})</span><b className="sell">{compactUsd(l.longUsd)}{share != null && ` · ${Math.round(share)} %`}</b></p>
          <p className="kv small"><span>Vendeurs liquidés ({l.shortCount})</span><b className="buy">{compactUsd(l.shortUsd)}</b></p>
          {l.largest && (
            <p className="kv small"><span>Plus grosse</span><b>{compactUsd(l.largest.usd)} · {l.largest.long ? "acheteur" : "vendeur"} à {fr(l.largest.price, 2)} $ · {time(l.largest.time)}</b></p>
          )}
          <p className="muted small">{l.scope}</p>
        </div>
      )}
      {d.openInterest && (
        <p className="kv"><span>Open interest</span><b>{compactUsd(d.openInterest.usd)}{d.openInterest.change24h != null && ` · ${signed(d.openInterest.change24h)} 24 h`}{d.openInterest.change7d != null && ` · ${signed(d.openInterest.change7d)} 7 j`}</b></p>
      )}
      {d.funding && (
        <p className="kv"><span>Funding (dernier règlement{d.funding.periodHours ? `, toutes les ${fr(d.funding.periodHours, 0)} h` : ""})</span><b>{signed(d.funding.rate, 4)} · habituel {signed(d.funding.p5, 4)} à {signed(d.funding.p95, 4)}</b></p>
      )}
      {d.longShort && (
        <p className="kv"><span>Ratio comptes acheteurs / vendeurs</span><b>{fr(d.longShort.ratio, 2)} · habituel {fr(d.longShort.p5, 2)} à {fr(d.longShort.p95, 2)}</b></p>
      )}
      <p className="muted small">Source : {d.source} ; plages habituelles = 5 à 95 % des valeurs récentes (funding : ≈ 100 derniers règlements ; ratio : 30 j).</p>
      <ul className="small anom-nc">
        {d.notCovered.map((n) => <li key={n.label}><b>Non couvert — {n.label}</b> : {n.reason}.</li>)}
      </ul>
    </div>
  );
}

/** Unusual readings on this asset (volume, price/volume, z-score; derivatives for cryptos), with what they may mean. */
export function AnomaliesCard({ symbol, kind }: { symbol: string; kind: "crypto" | "stock" }) {
  const [r, setR] = useState<AnomalyReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setR(null);
    setError(null);
    api.anomalies(symbol, kind).then((x) => alive && setR(x)).catch((e) => alive && setError(e instanceof Error ? e.message : "Indisponible"));
    return () => {
      alive = false;
    };
  }, [symbol, kind]);

  return (
    <div className="card anomalies">
      <h2 className="card-title">Détection d'anomalies</h2>
      {error && <p className="notice warn small">⚠ {error}</p>}
      {!r && !error && <div className="skeleton" aria-label="Chargement des anomalies" />}
      {r && (
        <>
          {r.anomalies.length === 0
            ? <p className="small">Rien d'inhabituel sur les mesures ci-dessous{r.session ? ` (séance du ${new Date(r.session).toLocaleDateString("fr-FR")})` : ""}.</p>
            : <ul className="anom-list">{r.anomalies.map((a) => <Alert key={a.code} a={a} />)}</ul>}
          {r.errors.map((e) => <p key={e} className="notice warn small">{e}</p>)}
          {r.normal.length > 0 && (
            <details>
              <summary className="small">Mesures dans la normale · {r.normal.length}</summary>
              <ul className="anom-normal small">
                {r.normal.map((a) => <li key={a.code}><span>{a.title}</span> <small className="muted mono">{a.measured}</small></li>)}
              </ul>
            </details>
          )}
          {r.derivatives && <DerivativesBlock d={r.derivatives} />}
          <p className="muted small">Une anomalie est un écart mesuré, pas une prévision : son sens reste à confirmer. {r.source && `${r.source}.`}</p>
        </>
      )}
    </div>
  );
}

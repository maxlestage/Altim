import { useState } from "react";
import { weigh } from "../engine/guard";
import { zoneState, type FibZone, type ZoneStatus } from "../engine/fibonacci";
import type { MacroInfo, ZonesReport } from "./api";
import type { HorizonPref } from "./store";
import { formatPrice } from "../market";

const STATUS: Record<ZoneStatus, { label: string; tone: "buy" | "hold" | "sell" | "unknown" }> = {
  above: { label: "Attendre le repli", tone: "hold" },
  inZone: { label: "Dans la zone", tone: "buy" },
  golden: { label: "Zone d'or", tone: "buy" },
  deep: { label: "Repli profond", tone: "hold" },
  broken: { label: "Zone invalidée", tone: "sell" },
  downtrend: { label: "Tendance baissière", tone: "sell" },
  none: { label: "Pas de niveau net", tone: "unknown" },
};
const MACRO_LABEL = { calm: "Calme", tense: "Tendu", high: "Très tendu" } as const;
const usd = (v: number) => `${formatPrice(v)} $`;
const pct = (v: number) => `${Math.round(v)} %`;

function Evidence({ z }: { z: FibZone }) {
  const e = z.evidence;
  if (!e) return <p className="muted small">Historique : aucun repli comparable sur cet actif pour cet horizon, zone non vérifiée.</p>;
  const { status } = weigh(e, true);
  const verdict = {
    verified: "ces zones ont mieux tenu qu'une entrée au hasard sur cet actif",
    unproven: "trop peu de cas pour conclure",
    rejected: "ces zones n'ont pas fait mieux qu'une entrée au hasard sur cet actif",
    unverifiable: "",
  }[status];
  return (
    <p className={`small ${status === "verified" ? "up" : "muted"}`}>
      Historique : sur {e.samples} repli{e.samples > 1 ? "s" : ""} dans la zone, {pct(e.rate)} sont remontés au plus haut avant de casser le plus bas, contre {pct(e.base)} pour une
      entrée au hasard : {verdict}.
    </p>
  );
}

/** Horizontal ladder: low of the move → high, buy zone, golden pocket and current price. */
function Ladder({ z, price }: { z: FibZone; price: number }) {
  if (!z.swing || !z.zone || !z.golden) return null;
  const lo = Math.min(z.swing.low, price), hi = Math.max(z.swing.high, price);
  const x = (v: number) => `${((v - lo) / (hi - lo || 1)) * 100}%`;
  const w = (a: number, b: number) => `${(Math.abs(b - a) / (hi - lo || 1)) * 100}%`;
  return (
    <div className="fib-ladder" role="img" aria-label={`Prix ${usd(price)}, zone d'achat ${usd(z.zone.from)} – ${usd(z.zone.to)}`}>
      <div className="fib-track">
        <i className="fib-zone" style={{ left: x(z.zone.from), width: w(z.zone.from, z.zone.to) }} />
        <i className="fib-golden" style={{ left: x(z.golden.from), width: w(z.golden.from, z.golden.to) }} />
        <i className="fib-price" style={{ left: x(price) }} />
      </div>
      <div className="fib-ends mono small"><span>{usd(z.swing.low)}</span><span>{usd(z.swing.high)}</span></div>
    </div>
  );
}

function ZoneDetail({ z, price }: { z: FibZone & { macroNote: string | null }; price: number | null }) {
  return (
    <div className="fib-detail">
      <p className="small">{z.text}</p>
      {price && z.swing && z.zone && <Ladder z={z} price={price} />}
      {z.zone && z.golden && (
        <dl className="fib-levels">
          <div><dt>Zone d'achat (38,2 – 61,8 %)</dt><dd className="mono">{usd(z.levels.find((l) => l.ratio === 0.618)!.price)} – {usd(z.zone.to)}</dd></div>
          <div><dt>Zone d'or (61,8 – 65 %)</dt><dd className="mono">{usd(z.golden.from)} – {usd(z.golden.to)}</dd></div>
          <div><dt>Invalidation (plus bas)</dt><dd className="mono sell">{usd(z.invalidation!)}</dd></div>
          <div><dt>Objectifs</dt><dd className="mono buy">{z.targets.map(usd).join(" · ")}</dd></div>
        </dl>
      )}
      {z.status !== "none" && z.status !== "downtrend" && <Evidence z={z} />}
      {z.macroNote && <p className="notice warn small">⚠ {z.macroNote}</p>}
    </div>
  );
}

export function MacroBlock({ m }: { m: MacroInfo }) {
  const e = m.evidence;
  return (
    <div className={`macro macro-${m.level}`}>
      <p className="kv"><span>Contexte macro et géopolitique</span><b>{MACRO_LABEL[m.level]} · {m.score}/100</b></p>
      {m.factors.length ? (
        <ul className="guard-factors">{m.factors.map((f) => <li key={f.code}>{f.text} <small className="muted">+{f.points}</small></li>)}</ul>
      ) : (
        <p className="muted small">Aucun signe de stress sur la peur (VIX), le S&P 500, le pétrole, l'or, le dollar ni les taux.</p>
      )}
      {e && e.samples >= 20 && (
        <p className="muted small">
          Sur cet actif, les jours de stress macro ont été suivis d'une forte baisse dans {pct(e.rate)} des cas en 5 jours, contre {pct(e.base)} d'habitude
          {e.lift >= 1.1 ? " : à prendre au sérieux." : " : pas d'effet mesurable ici."}
        </p>
      )}
      {m.themes.length > 0 && (
        <details>
          <summary className="small">Sujets du jour ({m.themes.map((t) => `${t.label} ${t.count}`).join(" · ")})</summary>
          <ul className="small">{m.themes.flatMap((t) => t.examples.slice(0, 2).map((x) => <li key={x}>{x}</li>))}</ul>
        </details>
      )}
      <p className="muted small">
        Personne ne peut prévoir une guerre ou une crise. Altim mesure le stress qu'elle crée dès qu'elle commence et vous dit d'être plus prudent.
      </p>
    </div>
  );
}

/** Buy zones by horizon (Fibonacci): the user's horizon first and open, the others one tap away. */
export function ZonesCard({ report, price, horizon }: { report: ZonesReport; price: number | null; horizon: HorizonPref }) {
  const [open, setOpen] = useState<string>(horizon);
  const p = price ?? report.price;
  // Status and distance follow the live price.
  const zones = report.zones.map((z) => (p && z.swing && z.swing.trend === "up" ? { ...z, ...zoneState(z.swing, p) } : z));
  const ordered = [...zones].sort((a, b) => (a.horizon === horizon ? -1 : b.horizon === horizon ? 1 : 0));
  return (
    <div className="card zones">
      <h2 className="card-title">Zones d'achat par horizon</h2>
      <p className="muted small">Retracements de Fibonacci du dernier mouvement haussier : là où les traders attendent un repli pour acheter.</p>
      <ul className="zone-list">
        {ordered.map((z) => {
          const s = STATUS[z.status];
          const isOpen = open === z.horizon;
          return (
            <li key={z.horizon} className={`zone-row ${z.horizon === horizon ? "mine" : ""}`}>
              <button className="zone-head" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? "" : z.horizon)}>
                <span>
                  <b>{z.label}</b>
                  <small className="muted"> · {z.unit}{z.horizon === horizon ? " · votre horizon" : ""}</small>
                </span>
                <span className={`badge ${s.tone}`}>{s.label}{z.distance != null ? ` −${z.distance.toFixed(1).replace(".", ",")} %` : ""}</span>
              </button>
              {isOpen && <ZoneDetail z={z} price={p} />}
            </li>
          );
        })}
      </ul>
      {report.macro && <MacroBlock m={report.macro} />}
    </div>
  );
}

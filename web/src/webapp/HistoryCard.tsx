import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { Segmented } from "./ui";
import type { Holding } from "../engine/holdings";
import { portfolioHistory, type Close } from "../engine/history";

type Days = 30 | 90 | 365;
const PERIODS: [string, string][] = [["30", "30 j"], ["90", "90 j"], ["365", "1 an"]];
// Categorical palette validated for the allocation (dark surface, colour-blind readers): same entity, same colour.
const COLORS = { portfolio: "#3987e5", "crypto:BTC": "#d95926", "stock:SPY": "#199e70" } as Record<string, string>;
const usd = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: 0 })} $`;
const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
const date = (t: number, year = false) => new Date(t).toLocaleDateString("fr-FR", { day: "numeric", month: "short", year: year ? "numeric" : undefined, timeZone: "UTC" });

/** "How did what I own now behave": value of today's lines over the period, against Bitcoin and the S&P 500. */
export function HistoryCard({ holdings }: { holdings: Holding[] }) {
  const [days, setDays] = useState<Days>(90);
  const [series, setSeries] = useState<Record<string, Close[]> | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<number | null>(null);
  const key = holdings.map((h) => `${h.kind}:${h.symbol}`).sort().join(",");

  useEffect(() => {
    let alive = true;
    setSeries(null);
    const unique = [...new Map(holdings.map((h) => [`${h.kind}:${h.symbol}`, { symbol: h.symbol, kind: h.kind }])).values()];
    api.history(unique, days)
      .then((r) => alive && (setSeries(Object.fromEntries(r.series.map((s) => [`${s.kind}:${s.symbol}`, s.closes]))), setError(null)))
      .catch((e) => alive && setError(e instanceof Error ? e.message : "Historique indisponible"));
    return () => {
      alive = false;
    };
  }, [key, days]);

  const lines = useMemo(() => {
    const byId = new Map<string, number>();
    for (const h of holdings) byId.set(`${h.kind}:${h.symbol}`, (byId.get(`${h.kind}:${h.symbol}`) ?? 0) + h.quantity);
    return [...byId].map(([id, quantity]) => ({ id, quantity }));
  }, [holdings]);
  const h = useMemo(() => (series ? portfolioHistory(lines, series, days) : null), [lines, series, days]);

  return (
    <div className="card history-card">
      <h2 className="card-title">Évolution de mes lignes</h2>
      <Segmented label="Période" value={String(days)} options={PERIODS} onChange={(v) => setDays(Number(v) as Days)} />
      {error && <p className="notice warn">⚠ {error}</p>}
      {!series && !error && <p className="muted small">Chargement de l'historique…</p>}
      {series && !h && <p className="muted small">Pas assez d'historique pour vos lignes sur cette période.</p>}
      {h && (
        <>
          <p className="kv">
            <span>{usd(h.points[0]!.value)} → {usd(h.points.at(-1)!.value)}</span>
            <b className={h.change >= 0 ? "up" : "down"}>{pct(h.change)}</b>
          </p>
          <Chart h={h} hover={hover} setHover={setHover} />
          <ul className="history-legend">
            <li><i style={{ background: COLORS.portfolio }} />Mes lignes <b>{pct(h.change)}</b></li>
            {h.benchmarks.map((b) => <li key={b.id}><i style={{ background: COLORS[b.id] }} />{b.label} <b>{pct(b.change)}</b></li>)}
          </ul>
          <p className="kv small"><span>Pire recul depuis un sommet</span><b className="down">{pct(h.maxDrawdown)}</b></p>
          {h.best && <p className="kv small"><span>Meilleure journée ({date(h.best.t)})</span><b className="up">{pct(h.best.change)}</b></p>}
          {h.worst && <p className="kv small"><span>Pire journée ({date(h.worst.t)})</span><b className="down">{pct(h.worst.change)}</b></p>}
          <p className="muted small">
            Valeur chaque jour des quantités que vous détenez aujourd'hui (cours de clôture, liquidités non comprises) : vos achats et ventes passés ne sont pas connus, ce n'est donc pas la
            performance de votre compte.{h.shortened ? " La courbe commence plus tard : une de vos lignes a un historique plus court." : ""}
            {h.missing.length ? ` Sans historique : ${h.missing.map((m) => m.split(":")[1]).join(", ")}.` : ""}
          </p>
        </>
      )}
    </div>
  );
}

function Chart({ h, hover, setHover }: { h: NonNullable<ReturnType<typeof portfolioHistory>>; hover: number | null; setHover: (i: number | null) => void }) {
  const W = 320;
  const H = 150;
  const PAD = { l: 38, r: 6, t: 8, b: 18 };
  const base = h.points[0]!.value;
  const mine = h.points.map((p) => (p.value / base - 1) * 100);
  const all = [...mine, ...h.benchmarks.flatMap((b) => b.points.map((p) => p.pct)), 0];
  const lo = Math.min(...all);
  const hi = Math.max(...all);
  const span = hi - lo || 1;
  const x = (i: number) => PAD.l + (i / (mine.length - 1)) * (W - PAD.l - PAD.r);
  const y = (v: number) => PAD.t + (1 - (v - lo) / span) * (H - PAD.t - PAD.b);
  const path = (vals: number[]) => vals.map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join("");
  const at = hover ?? mine.length - 1;

  const pick = (e: React.PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const px = ((e.clientX - r.left) / r.width) * W;
    setHover(Math.max(0, Math.min(mine.length - 1, Math.round(((px - PAD.l) / (W - PAD.l - PAD.r)) * (mine.length - 1)))));
  };

  return (
    <div className="history-chart">
      <p className="history-read small" aria-live="polite">
        <b>{date(h.points[at]!.t)}</b> · Mes lignes {usd(h.points[at]!.value)} ({pct(mine[at]!)})
        {h.benchmarks.map((b) => <span key={b.id}> · {b.label.split(" (")[0]} {pct(b.points[at]!.pct)}</span>)}
      </p>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        role="img"
        aria-label={`Mes lignes ${pct(h.change)} sur la période${h.benchmarks.map((b) => `, ${b.label} ${pct(b.change)}`).join("")}`}
        onPointerMove={pick}
        onPointerDown={pick}
        onPointerLeave={() => setHover(null)}
      >
        <line x1={PAD.l} x2={W - PAD.r} y1={y(0)} y2={y(0)} className="zero" />
        <text x={PAD.l - 4} y={y(hi) + 4} className="axis" textAnchor="end">{pct(hi)}</text>
        <text x={PAD.l - 4} y={y(lo)} className="axis" textAnchor="end">{pct(lo)}</text>
        <text x={PAD.l} y={H - 4} className="axis">{date(h.points[0]!.t, h.points.length > 200)}</text>
        <text x={W - PAD.r} y={H - 4} className="axis" textAnchor="end">{date(h.points.at(-1)!.t, h.points.length > 200)}</text>
        {h.benchmarks.map((b) => <path key={b.id} d={path(b.points.map((p) => p.pct))} stroke={COLORS[b.id]} className="bench" />)}
        <path d={path(mine)} stroke={COLORS.portfolio} className="mine" />
        {hover != null && <line x1={x(at)} x2={x(at)} y1={PAD.t} y2={H - PAD.b} className="cross" />}
        {hover != null && <circle cx={x(at)} cy={y(mine[at]!)} r={4} fill={COLORS.portfolio} className="dot" />}
      </svg>
    </div>
  );
}

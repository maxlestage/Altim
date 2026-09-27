import { useMemo } from "react";
import { ACTION_LABEL, ema, type Action, type Candle } from "../engine/signal";
import { LEVEL_LABEL, type Reliability } from "../engine/reliability";
import { formatPercent, formatPrice } from "../market";
import type { BacktestTrade } from "../engine/backtest";

export const actionKind = (a: Action) => (a === "buy" || a === "strongBuy" ? "buy" : a === "sell" || a === "strongSell" ? "sell" : "hold");

export function ActionBadge({ action, big }: { action: Action; big?: boolean }) {
  return <span className={`badge ${actionKind(action)}${big ? " big" : ""}`}>{ACTION_LABEL[action]}</span>;
}

export function ReliabilityBadge({ rel }: { rel: Reliability }) {
  const icon = rel.level === "high" ? "✔" : rel.level === "medium" ? "!" : "✕";
  return (
    <span className={`rel-badge ${rel.level}`} title={`${LEVEL_LABEL[rel.level]} — ${Math.round(rel.score)}/100`}>
      {icon} {LEVEL_LABEL[rel.level]}
    </span>
  );
}

export function Change({ value }: { value: number | null | undefined }) {
  if (value == null || !Number.isFinite(value)) return <span className="muted">—</span>;
  return <span className={value >= 0 ? "up" : "down"}>{formatPercent(value)}</span>;
}

export function Price({ value }: { value: number | null | undefined }) {
  return <>{value != null && Number.isFinite(value) ? `${formatPrice(value)} $` : "—"}</>;
}

export function Gauge({ score, size = 180 }: { score: number; size?: number }) {
  const angle = -90 + ((score + 100) / 200) * 180;
  return (
    <div className="gauge-wrap" style={{ width: size }}>
      <svg viewBox="0 0 200 115" className="gauge" role="img" aria-label={`Score ${score.toFixed(0)} sur 100`}>
        <defs>
          <linearGradient id="app-g" x1="0" x2="1">
            <stop offset="0" stopColor="#ff3b5c" />
            <stop offset="0.45" stopColor="#ffc733" />
            <stop offset="0.7" stopColor="#00f0ff" />
            <stop offset="1" stopColor="#39ff88" />
          </linearGradient>
        </defs>
        <path d="M20 100 A80 80 0 0 1 180 100" stroke="rgba(255,255,255,.08)" strokeWidth="14" fill="none" strokeLinecap="round" />
        <path d="M20 100 A80 80 0 0 1 180 100" stroke="url(#app-g)" strokeWidth="14" fill="none" strokeLinecap="round" />
        <g style={{ transform: `rotate(${angle}deg)`, transformOrigin: "100px 100px", transition: "transform 1s cubic-bezier(.3,1.6,.5,1)" }}>
          <line x1="100" y1="100" x2="100" y2="32" stroke="#fff" strokeWidth="4" strokeLinecap="round" />
        </g>
        <circle cx="100" cy="100" r="7" fill="#fff" />
      </svg>
      <b className="gauge-score">{score >= 0 ? "+" : ""}{score.toFixed(0)}</b>
    </div>
  );
}

export function Sparkline({ values }: { values: number[] }) {
  if (values.length < 2) return <svg className="spark" />;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const pts = values.map((v, i) => `${((i / (values.length - 1)) * 100).toFixed(2)},${(30 - ((v - min) / (max - min || 1)) * 28 - 1).toFixed(2)}`).join(" ");
  const up = values[values.length - 1]! >= values[0]!;
  return (
    <svg className="spark" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden>
      <polyline points={pts} fill="none" stroke={up ? "#39ff88" : "#ff3b5c"} strokeWidth="1.6" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/** Price chart: close, EMA 20/50, stop/target lines and backtest entries. */
export function PriceChart({ candles, stop, target, trades = [] }: { candles: Candle[]; stop?: number; target?: number; trades?: BacktestTrade[] }) {
  const data = useMemo(() => {
    const closes = candles.map((c) => c.close);
    const e20 = ema(closes, 20);
    const e50 = ema(closes, 50);
    const start = Math.max(0, closes.length - 150);
    return { closes: closes.slice(start), times: candles.slice(start).map((c) => c.time), e20: e20.slice(start), e50: e50.slice(start) };
  }, [candles]);
  const W = 600;
  const H = 280;
  const values = [...data.closes, ...(stop ? [stop] : []), ...(target ? [target] : [])];
  const min = Math.min(...values) * 0.998;
  const max = Math.max(...values) * 1.002;
  const x = (i: number) => (i / Math.max(1, data.closes.length - 1)) * W;
  const y = (v: number) => H - ((v - min) / (max - min || 1)) * H;
  const path = (s: (number | null)[]) => s.reduce<string>((d, v, i) => (v == null ? d : `${d}${d ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`), "");
  const line = path(data.closes);
  const t0 = data.times[0] ?? 0;
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="price-chart" preserveAspectRatio="none" role="img" aria-label="Graphique des prix">
      <defs>
        <linearGradient id="app-area" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="rgba(0,240,255,.32)" />
          <stop offset="1" stopColor="rgba(0,240,255,0)" />
        </linearGradient>
      </defs>
      {[0.25, 0.5, 0.75].map((p) => <line key={p} x1="0" x2={W} y1={H * p} y2={H * p} stroke="rgba(255,255,255,.06)" />)}
      <path d={`${line}L${W},${H}L0,${H}Z`} fill="url(#app-area)" />
      <path d={path(data.e50)} stroke="#7d4dff" strokeWidth="1.5" fill="none" vectorEffect="non-scaling-stroke" />
      <path d={path(data.e20)} stroke="#ff2bd6" strokeWidth="1.5" fill="none" vectorEffect="non-scaling-stroke" />
      <path d={line} stroke="#00f0ff" strokeWidth="2.5" fill="none" className="glow-line" vectorEffect="non-scaling-stroke" />
      {stop && <line x1="0" x2={W} y1={y(stop)} y2={y(stop)} stroke="#ff3b5c" strokeDasharray="6 6" vectorEffect="non-scaling-stroke" />}
      {target && <line x1="0" x2={W} y1={y(target)} y2={y(target)} stroke="#39ff88" strokeDasharray="6 6" vectorEffect="non-scaling-stroke" />}
      {trades.filter((t) => t.entryTime >= t0).map((t) => {
        const i = data.times.indexOf(t.entryTime);
        return i < 0 ? null : <circle key={t.entryTime} cx={x(i)} cy={y(t.entryPrice)} r="4" fill="#39ff88" vectorEffect="non-scaling-stroke" />;
      })}
    </svg>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: [T, string][]; onChange: (v: T) => void; label: string }) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map(([v, l]) => (
        <button key={v} role="radio" aria-checked={v === value} className={v === value ? "on" : ""} onClick={() => onChange(v)}>
          {l}
        </button>
      ))}
    </div>
  );
}

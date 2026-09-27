import { useEffect, useMemo, useState } from "react";
import { ACTION_LABEL, analyze, ema, type Candle, type Signal } from "../engine/signal";
import { COINS, fetchCandles, formatPrice, type Interval, type SourceStatus } from "../market";
import { useReveal } from "../hooks";

const INTERVALS: [Interval, string][] = [
  ["1h", "1 h"],
  ["4h", "4 h"],
  ["1d", "1 j"],
];

const kind = (s: Signal) => (s.action.includes("uy") ? "buy" : s.action.includes("ell") ? "sell" : "hold");

export function LiveSignal() {
  const ref = useReveal<HTMLElement>();
  const [coin, setCoin] = useState(COINS[0]!);
  const [interval, setTimeframe] = useState<Interval>("4h");
  const [state, setState] = useState<{
    candles: Candle[];
    signal: Signal | null;
    source: string;
    sources: SourceStatus[];
    agreeing: number;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setError(null);
    fetchCandles(coin, interval)
      .then(({ candles, source, sources, agreeing }) => {
        if (alive) setState({ candles, signal: analyze(candles), source, sources, agreeing });
      })
      .catch(() => alive && setError("Marchés injoignables depuis votre réseau. Réessayez dans un instant."));
    return () => {
      alive = false;
    };
  }, [coin, interval]);

  const signal = state?.signal ?? null;

  return (
    <section className="section live reveal" id="live" ref={ref}>
      <div className="section-head">
        <p className="eyebrow">Démo en direct</p>
        <h2>
          Le moteur Altim tourne <span className="gradient">dans votre navigateur</span>
        </h2>
        <p className="muted">
          Même algorithme que l'app iOS (vérifié par des tests automatiques), appliqué aux dernières bougies clôturées.
        </p>
      </div>

      <div className="live-controls">
        <div className="pills" role="tablist" aria-label="Actif">
          {COINS.slice(0, 4).map((c) => (
            <button key={c.symbol} className={c === coin ? "pill on" : "pill"} onClick={() => setCoin(c)}>
              {c.symbol.replace("USDT", "")}
            </button>
          ))}
        </div>
        <div className="pills" role="tablist" aria-label="Unité de temps">
          {INTERVALS.map(([v, l]) => (
            <button key={v} className={v === interval ? "pill on" : "pill"} onClick={() => setTimeframe(v)}>
              {l}
            </button>
          ))}
        </div>
      </div>

      {error && <p className="warn">{error}</p>}

      <div className="live-grid">
        <div className="card chart-card">
          {state ? <MiniChart candles={state.candles} signal={signal} /> : <div className={error ? "skeleton idle" : "skeleton"} />}
          {state && (
            <div className="sources">
              <span className={state.agreeing >= 2 ? "rel high" : "rel low"}>
                {state.agreeing >= 2 ? "✔" : "!"} {state.agreeing} source{state.agreeing > 1 ? "s" : ""} concordante
                {state.agreeing > 1 ? "s" : ""}
              </span>
              {state.sources.map((s) => (
                <span key={s.name} className={s.ok ? "src ok" : "src ko"} title={s.error ?? ""}>
                  {s.ok ? "●" : "○"} {s.name}
                  {s.ok && s.deviation !== undefined && Number.isFinite(s.deviation) ? ` ${s.deviation.toFixed(3)} %` : ""}
                </span>
              ))}
            </div>
          )}
        </div>

        <div className={`card signal-card ${signal ? kind(signal) : ""}`}>
          {signal ? (
            <>
              <div className="signal-top">
                <Gauge score={signal.score} />
                <div className="signal-meta">
                  <span className={`badge big ${kind(signal)}`}>{ACTION_LABEL[signal.action]}</span>
                  <span className="mono">{formatPrice(signal.price)} $</span>
                  <small className="muted">confiance {Math.round(signal.confidence)} %</small>
                </div>
              </div>
              <ul className="factors">
                {signal.factors.map((f) => (
                  <li key={f.name}>
                    <div className="factor-head">
                      <span>{f.name}</span>
                      <small className="muted">{f.detail}</small>
                    </div>
                    <div className="bar">
                      <i
                        className={f.score >= 0 ? "pos" : "neg"}
                        style={{
                          width: `${Math.abs(f.score) * 50}%`,
                          left: f.score >= 0 ? "50%" : `${50 - Math.abs(f.score) * 50}%`,
                        }}
                      />
                    </div>
                  </li>
                ))}
              </ul>
              {signal.action !== "hold" && signal.hasPlan && (
                <div className="plan">
                  <div>
                    <small>STOP</small>
                    <b className="sell">{formatPrice(signal.stopLoss)}</b>
                  </div>
                  <div>
                    <small>ENTRÉE</small>
                    <b>{formatPrice(signal.price)}</b>
                  </div>
                  <div>
                    <small>OBJECTIF</small>
                    <b className="buy">{formatPrice(signal.takeProfit)}</b>
                  </div>
                </div>
              )}
            </>
          ) : (
            <div className={error ? "skeleton tall idle" : "skeleton tall"} />
          )}
        </div>
      </div>
      <p className="disclaimer">
        Démonstration pédagogique, pas un conseil en investissement. Dans l'app, ce signal est complété par la
        confirmation de l'unité de temps supérieure et par un backtest sur l'actif.
      </p>
    </section>
  );
}

function Gauge({ score }: { score: number }) {
  const angle = -90 + ((score + 100) / 200) * 180;
  return (
    <div className="gauge-wrap">
      <svg viewBox="0 0 200 115" className="gauge">
        <defs>
          <linearGradient id="lg" x1="0" x2="1">
            <stop offset="0" stopColor="#ff3b5c" />
            <stop offset="0.45" stopColor="#ffc733" />
            <stop offset="0.7" stopColor="#00f0ff" />
            <stop offset="1" stopColor="#39ff88" />
          </linearGradient>
        </defs>
        <path d="M20 100 A80 80 0 0 1 180 100" stroke="rgba(255,255,255,.08)" strokeWidth="14" fill="none" strokeLinecap="round" />
        <path d="M20 100 A80 80 0 0 1 180 100" stroke="url(#lg)" strokeWidth="14" fill="none" strokeLinecap="round" />
        <g style={{ transform: `rotate(${angle}deg)`, transformOrigin: "100px 100px", transition: "transform 1s cubic-bezier(.3,1.6,.5,1)" }}>
          <line x1="100" y1="100" x2="100" y2="32" stroke="#fff" strokeWidth="4" strokeLinecap="round" />
        </g>
        <circle cx="100" cy="100" r="7" fill="#fff" />
      </svg>
      <b className="gauge-score">{score >= 0 ? "+" : ""}{score.toFixed(0)}</b>
    </div>
  );
}

function MiniChart({ candles, signal }: { candles: Candle[]; signal: Signal | null }) {
  const data = useMemo(() => {
    const closes = candles.map((c) => c.close);
    const e20 = ema(closes, 20);
    const e50 = ema(closes, 50);
    const start = Math.max(0, closes.length - 140);
    return { closes: closes.slice(start), e20: e20.slice(start), e50: e50.slice(start) };
  }, [candles]);

  const W = 600;
  const H = 300;
  const values = [...data.closes, ...(signal && signal.action !== "hold" ? [signal.stopLoss, signal.takeProfit] : [])];
  const min = Math.min(...values) * 0.998;
  const max = Math.max(...values) * 1.002;
  const x = (i: number) => (i / Math.max(1, data.closes.length - 1)) * W;
  const y = (v: number) => H - ((v - min) / (max - min || 1)) * H;
  const path = (series: (number | null)[]) =>
    series.reduce<string>((d, v, i) => (v == null ? d : `${d}${d ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`), "");
  const line = path(data.closes);

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="mini-chart" preserveAspectRatio="none" role="img" aria-label="Graphique des prix">
      <defs>
        <linearGradient id="area" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="rgba(0,240,255,.35)" />
          <stop offset="1" stopColor="rgba(0,240,255,0)" />
        </linearGradient>
      </defs>
      {[0.25, 0.5, 0.75].map((p) => (
        <line key={p} x1="0" x2={W} y1={H * p} y2={H * p} stroke="rgba(255,255,255,.06)" />
      ))}
      <path d={`${line}L${W},${H}L0,${H}Z`} fill="url(#area)" />
      <path d={path(data.e50)} stroke="#7d4dff" strokeWidth="1.5" fill="none" vectorEffect="non-scaling-stroke" />
      <path d={path(data.e20)} stroke="#ff2bd6" strokeWidth="1.5" fill="none" vectorEffect="non-scaling-stroke" />
      <path d={line} stroke="#00f0ff" strokeWidth="2.5" fill="none" className="glow-line" vectorEffect="non-scaling-stroke" />
      {signal && signal.action !== "hold" && (
        <>
          <line x1="0" x2={W} y1={y(signal.stopLoss)} y2={y(signal.stopLoss)} stroke="#ff3b5c" strokeDasharray="6 6" vectorEffect="non-scaling-stroke" />
          <line x1="0" x2={W} y1={y(signal.takeProfit)} y2={y(signal.takeProfit)} stroke="#39ff88" strokeDasharray="6 6" vectorEffect="non-scaling-stroke" />
        </>
      )}
    </svg>
  );
}

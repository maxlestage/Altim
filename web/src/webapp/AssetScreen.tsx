import { useEffect, useState } from "react";
import { analyze, type Signal } from "../engine/signal";
import { gate } from "../engine/reliability";
import { backtest, type BacktestResult } from "../engine/backtest";
import { formatPrice } from "../market";
import { api, HIGHER, INTERVAL_LABEL, STEP_MS, type Quote, type Sentiment, type Snapshot } from "./api";
import { onLink } from "./router";
import { assetKey, setState, useAppState, useHoldings, type Interval } from "./store";
import { ActionBadge, Change, Gauge, Price, PriceChart, ReliabilityBadge, Segmented } from "./ui";
import { adviseAsset } from "../engine/advice";
import { analyzePortfolio, type MarketInput } from "../engine/holdings";

type Loaded = { snap: Snapshot; signal: Signal | null; quote: Quote | null };

export function AssetScreen({ kind, symbol }: { kind: "crypto" | "stock"; symbol: string }) {
  const { interval, watchlist, risk } = useAppState();
  const holdingsState = useHoldings();
  const held = holdingsState.holdings.find((h) => h.symbol === symbol && h.kind === kind) ?? null;
  const [heldMarket, setHeldMarket] = useState<MarketInput | null>(null);
  const [holdingPrices, setHoldingPrices] = useState<Record<string, number>>({});
  const holdingsKey = holdingsState.holdings.map((h) => `${h.kind}:${h.symbol}`).sort().join(",");

  // Current value of every line (same valuation as "Mes avoirs").
  useEffect(() => {
    if (!holdingsState.holdings.length) return setHoldingPrices({});
    let alive = true;
    api.quotes(holdingsState.holdings)
      .then((q) => alive && setHoldingPrices(Object.fromEntries(q.map((x) => [`${x.kind}:${x.symbol}`, x.price]))))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [holdingsKey]);
  const [data, setData] = useState<Loaded | null>(null);
  const [bt, setBt] = useState<BacktestResult | null>(null);
  const [sent, setSent] = useState<Sentiment | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showSources, setShowSources] = useState(false);
  const inWatchlist = watchlist.some((w) => assetKey(w) === `${kind}:${symbol}`);
  const name = watchlist.find((w) => assetKey(w) === `${kind}:${symbol}`)?.name ?? data?.quote?.name ?? symbol;

  useEffect(() => {
    let alive = true;
    setError(null);
    setBt(null);
    const hi = HIGHER[interval];
    Promise.all([
      api.candles(symbol, kind, interval),
      hi ? api.candles(symbol, kind, hi).catch(() => null) : Promise.resolve(null),
      api.quotes([{ symbol, kind }]).then((q) => q[0] ?? null).catch(() => null),
    ])
      .then(([snap, higher, quote]) => {
        if (!alive) return;
        const raw = analyze(snap.candles, { higher: higher?.candles, intervalMs: STEP_MS[interval] });
        setData({ snap, signal: raw ? gate(raw, snap.reliability, snap.quality.issues) : null, quote });
        // Backtest off the first render (heavier computation).
        setTimeout(() => alive && setBt(backtest(snap.candles)), 30);
      })
      .catch((e) => alive && setError(e instanceof Error ? e.message : "Données indisponibles"));
    api.sentiment(symbol, kind).then((s) => alive && setSent(s)).catch(() => {});
    // Asset held: same inputs as "Mes avoirs" (1 d and 4 h signals, daily candles).
    if (held) {
      Promise.all([api.radar([{ symbol, kind, name: held.name }], "1d"), api.radar([{ symbol, kind, name: held.name }], "4h").catch(() => []), api.candles(symbol, kind, "1d")])
        .then(([d, s4, daily]) => {
          if (!alive) return;
          const dr = d[0];
          const sr = s4[0];
          setHeldMarket({
            price: dr?.price ?? 0,
            daily: daily.candles,
            daySignal: dr?.signal ?? null,
            shortSignal: sr?.signal ?? null,
            reliability: [dr?.reliability?.level, sr?.reliability?.level].includes("low") ? "low" : dr?.reliability?.level ?? null,
          });
        })
        .catch(() => {});
    }
    return () => {
      alive = false;
    };
  }, [symbol, kind, interval, !!held]);

  const signal = data?.signal ?? null;
  const price = data?.quote?.price ?? signal?.price ?? null;
  const rel = data?.snap.reliability;
  // Wealth (other lines valued at their purchase price) and line of this asset if held.
  const valuation: Record<string, MarketInput> = Object.fromEntries(
    Object.entries(holdingPrices).map(([k, price]) => [k, { price, daily: [], daySignal: null, shortSignal: null, reliability: null }]),
  );
  if (held && heldMarket) valuation[`${kind}:${symbol}`] = heldMarket;
  const portfolio = analyzePortfolio(holdingsState.holdings, holdingsState.cash, valuation);
  const line = held ? portfolio.lines.find((l) => l.id === held.id) ?? null : null;
  const advice = data ? adviseAsset({ signal, reliability: rel?.level ?? null, price, line: held && heldMarket ? line : null, capital: portfolio.total, risk }) : null;

  return (
    <section className="app-screen asset-screen">
      <a href="/app" onClick={onLink} className="back">← Radar</a>
      <div className="asset-head">
        <div>
          <h1>{name}</h1>
          <small className="muted mono">{symbol} · {kind === "crypto" ? "Crypto" : "Action"}</small>
        </div>
        <div className="asset-head-price">
          <b className="mono"><Price value={price} /></b>
          <Change value={data?.quote?.change} />
          {data?.quote && <small className="muted">prix : {data.quote.agreeing}/{data.quote.total} sources</small>}
        </div>
      </div>

      <Segmented<Interval>
        label="Unité de temps"
        value={interval}
        onChange={(v) => setState({ interval: v })}
        options={(["1h", "4h", "1d"] as Interval[]).map((i) => [i, INTERVAL_LABEL[i]])}
      />

      {error && <p className="notice warn">⚠ {error}</p>}
      {!data && !error && <div className="skeleton tall" />}

      {advice && (
        <div className={`card advice advice-${advice.tone}`}>
          <h2 className="card-title">Le conseil d'Altim</h2>
          <p className="advice-title">{advice.title}</p>
          <ul>{advice.points.map((p) => <li key={p}>{p}</li>)}</ul>
          <div className="advice-actions">
            {!held && <a href="/app/avoirs" onClick={onLink} className="link">J'en possède déjà</a>}
            {held && <a href="/app/avoirs" onClick={onLink} className="link">Voir mes avoirs</a>}
            {!inWatchlist && (
              <button className="link-btn" onClick={() => setState((s) => ({ watchlist: [...s.watchlist, { symbol, kind, name }] }))}>+ Ajouter au radar</button>
            )}
          </div>
          <p className="muted small">Conseil indicatif, pas une recommandation d'investissement personnalisée. Altim ne passe aucun ordre.</p>
        </div>
      )}

      {data && (
        <div className="asset-grid">
          <div className="card chart-box">
            <PriceChart candles={data.snap.candles} stop={signal && signal.action !== "hold" && signal.hasPlan ? signal.stopLoss : undefined} target={signal && signal.action !== "hold" && signal.hasPlan ? signal.takeProfit : undefined} trades={bt?.trades} />
            <div className="legend">
              <span><i className="l-price" /> Prix</span>
              <span><i className="l-e20" /> EMA 20</span>
              <span><i className="l-e50" /> EMA 50</span>
              {bt && <span><i className="l-trade" /> Entrées backtest</span>}
            </div>
          </div>

          {signal ? (
            <div className={`card signal-card ${signal.action.includes("uy") ? "buy" : signal.action.includes("ell") ? "sell" : ""}`}>
              <h2 className="card-title">Signal Altim · {INTERVAL_LABEL[interval]}</h2>
              <div className="signal-top">
                <Gauge score={signal.score} size={150} />
                <div className="signal-meta">
                  <ActionBadge action={signal.action} big />
                  <small className="muted">confiance {Math.round(signal.confidence)} %</small>
                  <small className="muted">bougie du {new Date(signal.time).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })}</small>
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
                      <i className={f.score >= 0 ? "pos" : "neg"} style={{ width: `${Math.abs(f.score) * 50}%`, left: f.score >= 0 ? "50%" : `${50 - Math.abs(f.score) * 50}%` }} />
                    </div>
                  </li>
                ))}
              </ul>
              {signal.action !== "hold" && signal.hasPlan && (
                <div className="plan">
                  <div><small>STOP</small><b className="sell">{formatPrice(signal.stopLoss)}</b></div>
                  <div><small>ENTRÉE</small><b>{formatPrice(signal.price)}</b></div>
                  <div><small>OBJECTIF</small><b className="buy">{formatPrice(signal.takeProfit)}</b></div>
                </div>
              )}
              {signal.warnings.map((w) => <p key={w} className="warn small">⚠ {w}</p>)}
            </div>
          ) : (
            <p className="notice warn">Historique insuffisant pour calculer un signal sur cette unité de temps.</p>
          )}

          {rel && (
            <div className={`card rel-card ${rel.level}`}>
              <h2 className="card-title">Fiabilité des données</h2>
              <div className="rel-top">
                <ReliabilityBadge rel={rel} />
                <b className="mono">{Math.round(rel.score)}/100</b>
              </div>
              <p className="muted small">
                Bougies de <b>{data.snap.source}</b>, recoupées : {data.snap.agreeing} source(s) concordante(s) sur {data.snap.sources.length}.
                {rel.level === "medium" && data.snap.kind === "stock" && " En intraday, une seule source indépendante est disponible pour les actions : les signaux forts sont ramenés à normaux."}
              </p>
              <button className="link-btn" onClick={() => setShowSources(!showSources)} aria-expanded={showSources}>
                {showSources ? "Masquer les sources" : "Voir les sources"}
              </button>
              {showSources && (
                <ul className="source-list">
                  {data.snap.sources.map((s) => (
                    <li key={s.name}>
                      <span className={s.ok ? "up" : "down"}>{s.ok ? "●" : "○"}</span> {s.name}
                      <small className="muted">{s.ok && s.deviation !== undefined && Number.isFinite(s.deviation) ? `écart ${s.deviation.toFixed(3)} %` : s.error ?? "écartée"}</small>
                    </li>
                  ))}
                  {data.quote?.sources.map((s) => (
                    <li key={`q-${s.name}`}>
                      <span className={s.ok ? "up" : "down"}>{s.ok ? "●" : "○"}</span> {s.name} <small className="muted">(cours) {s.price ? formatPrice(s.price) : s.error}</small>
                    </li>
                  ))}
                  {data.snap.quality.issues.map((i) => <li key={i} className="warn small">⚠ {i}</li>)}
                </ul>
              )}
            </div>
          )}

          <div className="card bt-card">
            <h2 className="card-title">Backtest · {INTERVAL_LABEL[interval]} · historique chargé</h2>
            {bt ? (
              <>
                <div className="metrics">
                  <div><small>Stratégie</small><b><Change value={bt.totalReturnPercent} /></b></div>
                  <div><small>Achat-conservation</small><b><Change value={bt.buyAndHoldPercent} /></b></div>
                  <div><small>Trades</small><b>{bt.trades.length}</b></div>
                  <div><small>Réussite</small><b>{bt.winRatePercent.toFixed(0)} %</b></div>
                  <div><small>Drawdown max</small><b className="down">−{bt.maxDrawdownPercent.toFixed(1)} %</b></div>
                </div>
                <p className="muted small">
                  {bt.trades.length < 5
                    ? "Trop peu de trades pour conclure : prudence."
                    : bt.totalReturnPercent > bt.buyAndHoldPercent
                      ? "La stratégie a fait mieux que l'achat-conservation sur cette période (frais de 0,1 % inclus). Les performances passées ne préjugent pas des performances futures."
                      : "Sur cette période, conserver l'actif a mieux rapporté que suivre les signaux. La stratégie sert surtout à limiter les pertes en marché baissier."}
                </p>
              </>
            ) : (
              <div className="skeleton" />
            )}
          </div>

          {(sent?.fearGreed || sent?.social?.bullishPercent != null) && (
            <div className="card">
              <h2 className="card-title">Sentiment du marché</h2>
              {sent.fearGreed && <p className="kv"><span>Fear & Greed crypto</span><b>{sent.fearGreed.value} · {sent.fearGreed.label}</b></p>}
              {sent.social?.bullishPercent != null && (
                <p className="kv"><span>StockTwits</span><b className={sent.social.bullishPercent >= 50 ? "up" : "down"}>{sent.social.bullishPercent.toFixed(0)} % haussier ({sent.social.sample} avis)</b></p>
              )}
              <p className="muted small">Contexte, non intégré au score : la foule se trompe souvent aux extrêmes.</p>
            </div>
          )}
        </div>
      )}

    </section>
  );
}

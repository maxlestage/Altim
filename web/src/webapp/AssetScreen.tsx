import { useEffect, useState } from "react";
import { analyze, type Signal } from "../engine/signal";
import { gate } from "../engine/reliability";
import { backtest, trackRecord, type BacktestResult } from "../engine/backtest";
import { formatPrice } from "../market";
import { api, HIGHER, INTERVAL_LABEL, STEP_MS, type GuardReport, type Quote, type Sentiment, type Snapshot, type ZonesReport } from "./api";
import { GuardCard } from "./GuardCard";
import { WhyCard } from "./WhyCard";
import { ZonesCard } from "./ZonesCard";
import { DcaCard } from "./DcaCard";
import { StrategiesCard } from "./StrategiesCard";
import { PositionCard } from "./ToolCards";
import { NoteCard } from "./NoteCard";
import { zoneState } from "../engine/fibonacci";
import { onLink } from "./router";
import { assetKey, setState, useAppState, useHoldings, type Interval } from "./store";
import { ActionBadge, Change, Gauge, PriceChart, ReliabilityBadge, Segmented } from "./ui";
import { LiveBadge, LivePrice, useLive } from "./live";
import { adviseAsset } from "../engine/advice";
import { analyzePortfolio, type MarketInput } from "../engine/holdings";
import { DecisionCard } from "./DecisionCard";
import { averageCost, portfolioWeights } from "./decision";

type Loaded = { snap: Snapshot; signal: Signal | null; quote: Quote | null };

export function AssetScreen({ kind, symbol }: { kind: "crypto" | "stock"; symbol: string }) {
  const { interval, watchlist, risk, horizon } = useAppState();
  const holdingsState = useHoldings();
  const held = holdingsState.holdings.find((h) => h.symbol === symbol && h.kind === kind) ?? null;
  const [heldMarket, setHeldMarket] = useState<MarketInput | null>(null);
  const [holdingPrices, setHoldingPrices] = useState<Record<string, number>>({});
  // false while the prices of the held lines load: the personal decision waits for its weights.
  const [pricesReady, setPricesReady] = useState(false);
  const holdingsKey = holdingsState.holdings.map((h) => `${h.kind}:${h.symbol}`).sort().join(",");

  // Current value of every line (same valuation as "Mes avoirs").
  useEffect(() => {
    if (!holdingsState.holdings.length) {
      setHoldingPrices({});
      setPricesReady(true);
      return;
    }
    let alive = true;
    setPricesReady(false);
    api.quotes(holdingsState.holdings)
      .then((q) => alive && setHoldingPrices(Object.fromEntries(q.map((x) => [`${x.kind}:${x.symbol}`, x.price]))))
      .catch(() => {})
      .finally(() => alive && setPricesReady(true));
    return () => {
      alive = false;
    };
  }, [holdingsKey]);
  const [data, setData] = useState<Loaded | null>(null);
  const [bt, setBt] = useState<BacktestResult | null>(null);
  const [sent, setSent] = useState<Sentiment | null>(null);
  const [guardReport, setGuardReport] = useState<GuardReport | null>(null);
  const [zonesReport, setZonesReport] = useState<ZonesReport | null>(null);

  // Buy zones by horizon and macro context: they change with the candles, refreshed every 2 minutes.
  useEffect(() => {
    let alive = true;
    setZonesReport(null);
    const load = () => api.zones(symbol, kind).then((z) => alive && setZonesReport(z)).catch(() => {});
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 120_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [symbol, kind]);

  // Market guard: independent of the timeframe, refreshed every 2 minutes.
  useEffect(() => {
    let alive = true;
    const load = () => api.guard(symbol, kind).then((g) => alive && setGuardReport(g)).catch(() => {});
    load();
    const id = setInterval(() => document.visibilityState === "visible" && load(), 120_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [symbol, kind]);
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

  // Live prices of this asset and of every held line: amounts and advice follow the market.
  const live = useLive([{ symbol, kind }, ...holdingsState.holdings]);
  const tick = live.ticks[`${kind}:${symbol}`];
  const signal = data?.signal ?? null;
  const price = tick?.price ?? data?.quote?.price ?? signal?.price ?? null;
  const rel = data?.snap.reliability;
  const valuation: Record<string, MarketInput> = Object.fromEntries(
    Object.entries(holdingPrices).map(([k, p]) => [k, { price: live.ticks[k]?.price ?? p, daily: [], daySignal: null, shortSignal: null, reliability: null }]),
  );
  if (held && heldMarket) valuation[`${kind}:${symbol}`] = tick ? { ...heldMarket, price: tick.price } : heldMarket;
  // The chart ends on the live price (candle being formed; the signal itself only uses closed candles).
  const candles = data?.snap.candles ?? [];
  const lastC = candles[candles.length - 1];
  const chartCandles = lastC && tick && tick.time > lastC.time
    ? [...candles, { time: lastC.time + STEP_MS[interval], open: lastC.close, high: Math.max(lastC.close, tick.price), low: Math.min(lastC.close, tick.price), close: tick.price, volume: 0 }]
    : candles;
  const portfolio = analyzePortfolio(holdingsState.holdings, holdingsState.cash, valuation);
  const line = held ? portfolio.lines.find((l) => l.id === held.id) ?? null : null;
  // Buy zone of the user's horizon, at the live price.
  const rawZone = zonesReport?.zones.find((z) => z.horizon === horizon);
  const myZone = rawZone && rawZone.status !== "none"
    ? { label: rawZone.label, macroNote: rawZone.macroNote, ...(price && rawZone.swing?.trend === "up" ? zoneState(rawZone.swing, price) : { status: rawZone.status, text: rawZone.text }) }
    : null;
  const advice = data
    ? adviseAsset({
        signal, reliability: rel?.level ?? null, price, line: held && heldMarket ? line : null, capital: portfolio.total, risk,
        track: bt ? trackRecord(bt) : null, symbol, kind,
        guard: guardReport ? { shock: guardReport.shock.level, reversalScore: guardReport.reversal.score, reversalDirection: guardReport.reversal.direction } : null,
        zone: myZone,
      })
    : null;

  // Personal decision: average cost and each line's share of the portfolio (never quantities nor amounts). Prices
  // of the first load, not the live ticks, so the request does not change at every tick.
  const personal = held
    ? { cost: averageCost(holdingsState.holdings, symbol, kind), weights: portfolioWeights(holdingsState.holdings, holdingPrices, holdingsState.cash) }
    : null;

  return (
    <section className="app-screen asset-screen">
      <a href="/app" onClick={onLink} className="back">← Radar</a>
      <div className="asset-head">
        <div>
          <h1>{name}</h1>
          <small className="muted mono">{symbol} · {kind === "crypto" ? "Crypto" : "Action"}</small>
        </div>
        <div className="asset-head-price">
          <b className="mono"><LivePrice tick={tick} fallback={price} format={(v) => `${formatPrice(v)} $`} /></b>
          <Change value={tick?.change ?? data?.quote?.change} />
          <LiveBadge status={live.status} last={live.last} />
          {tick?.market === "closed" && <small className="market-closed">Bourse fermée · dernier cours</small>}
          {tick ? <small className="muted">prix : {tick.agreeing}/{tick.total} sources en direct</small>
            : data?.quote && <small className="muted">prix : {data.quote.agreeing}/{data.quote.total} sources</small>}
        </div>
      </div>

      <DecisionCard symbol={symbol} kind={kind} personal={personal} ready={!held || pricesReady} livePrice={tick?.price ?? null} />

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
          <h2 className="card-title">Lecture du signal · {INTERVAL_LABEL[interval]}</h2>
          <p className="muted small">Une seule unité de temps et le signal technique : la décision en haut de page réunit toutes les familles, les interdictions d'achat et le rapport gain/risque, et c'est elle qui prime.</p>
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

      {zonesReport ? <ZonesCard report={zonesReport} price={price} horizon={horizon} /> : data && <div className="skeleton" aria-label="Chargement des zones d'achat" />}

      {guardReport ? <GuardCard g={guardReport} /> : data && <div className="skeleton" aria-label="Chargement du garde-fou" />}

      <WhyCard symbol={symbol} kind={kind} />

      <PositionCard
        symbol={symbol}
        price={price}
        {...(signal?.hasPlan && signal.stopLoss < (price ?? Infinity)
          ? { stop: signal.stopLoss, stopSource: "stop du plan (2 × ATR)", target: signal.takeProfit > (price ?? 0) ? signal.takeProfit : null }
          : rawZone?.invalidation && rawZone.invalidation < (price ?? 0)
            ? { stop: rawZone.invalidation, stopSource: "plus bas qui invalide la zone d'achat", target: rawZone.targets.find((t) => t > (price ?? 0)) ?? null }
            : { stop: price ? price * 0.95 : null, stopSource: "5 % sous le prix (à ajuster)", target: null })}
        capital={portfolio.total}
        riskPct={risk.riskPerTradePercent}
      />

      <NoteCard id={`${kind}:${symbol}`} symbol={symbol} />

      <DcaCard symbol={symbol} kind={kind} />
      <StrategiesCard symbol={symbol} kind={kind} />

      {data && (
        <div className="asset-grid">
          <div className="card chart-box">
            <PriceChart candles={chartCandles} stop={signal && signal.action !== "hold" && signal.hasPlan ? signal.stopLoss : undefined} target={signal && signal.action !== "hold" && signal.hasPlan ? signal.takeProfit : undefined} trades={bt?.trades} />
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

import { useEffect, useState } from "react";
import { formatPrice } from "../market";
import { api, type Quote } from "./api";
import { onLink } from "./router";
import { assetKey, resetDemo, STARTING_CASH, useAppState } from "./store";
import { Change } from "./ui";

export function Portfolio() {
  const { cash, positions, journal } = useAppState();
  const [quotes, setQuotes] = useState<Record<string, Quote>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!positions.length) return;
    let alive = true;
    const load = () =>
      api.quotes(positions)
        .then((q) => alive && (setQuotes(Object.fromEntries(q.map((x) => [assetKey(x), x]))), setError(null)))
        .catch((e) => alive && setError(e instanceof Error ? e.message : "Cours indisponibles"));
    load();
    const id = setInterval(load, 30_000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [positions.map(assetKey).join(",")]);

  const valued = positions.map((p) => {
    const q = quotes[assetKey(p)];
    const price = q?.price ?? p.averagePrice;
    return { ...p, price, value: p.quantity * price, pnl: (price / p.averagePrice - 1) * 100, live: !!q };
  });
  const equity = cash + valued.reduce((a, p) => a + p.value, 0);
  const perf = (equity / STARTING_CASH - 1) * 100;

  return (
    <section className="app-screen">
      <div className="screen-top">
        <div>
          <h1>Portefeuille</h1>
          <p className="muted small">Compte de démonstration · conservé dans ce navigateur</p>
        </div>
      </div>

      <div className="card equity">
        <small className="muted">Valeur totale</small>
        <b className="mono big">{formatPrice(equity)} $</b>
        <Change value={perf} />
        <div className="kv small"><span>Liquidités</span><b>{formatPrice(cash)} USDT</b></div>
      </div>

      {error && <p className="notice warn">⚠ {error} — valorisation au prix d'achat.</p>}

      <h2 className="section-label">Positions</h2>
      {valued.length ? (
        <ul className="asset-list">
          {valued.map((p) => (
            <li key={assetKey(p)}>
              <a href={`/app/actif/${p.kind}/${p.symbol}`} onClick={onLink} className="asset-card">
                <div className="asset-id">
                  <b>{p.name}</b>
                  <small>{p.quantity.toPrecision(6).replace(/\.?0+$/, "")} {p.symbol} · PRU {formatPrice(p.averagePrice)}</small>
                </div>
                <div className="asset-price">
                  <b>{formatPrice(p.value)} $</b>
                  <Change value={p.pnl} />
                </div>
                {(p.stopLoss || p.takeProfit) && (
                  <div className="asset-meta">
                    {p.stopLoss && <small className="down">stop {formatPrice(p.stopLoss)}</small>}
                    {p.takeProfit && <small className="up">objectif {formatPrice(p.takeProfit)}</small>}
                  </div>
                )}
              </a>
            </li>
          ))}
        </ul>
      ) : (
        <p className="muted">Aucune position. Ouvrez un actif depuis le <a href="/app" onClick={onLink} className="link">Radar</a> pour passer un ordre de démonstration.</p>
      )}

      <h2 className="section-label">Journal des ordres</h2>
      {journal.length ? (
        <ul className="journal">
          {journal.slice(0, 50).map((e) => (
            <li key={e.id}>
              <span className={e.side === "buy" ? "up" : "down"}>{e.side === "buy" ? "ACHAT" : "VENTE"}</span>
              <span><b>{e.symbol}</b><small className="muted">{new Date(e.time).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })}</small></span>
              <span className="mono">{e.quantity.toPrecision(5).replace(/\.?0+$/, "")} @ {formatPrice(e.price)}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="muted">Aucun ordre pour l'instant.</p>
      )}

      <button className="btn btn-ghost" onClick={() => confirm("Réinitialiser le compte démo à 10 000 USDT ?") && resetDemo()}>
        Réinitialiser le compte démo
      </button>
    </section>
  );
}

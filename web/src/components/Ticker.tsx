import { formatPercent, formatPrice, type Tick } from "../market";

/** Markets panel: prices validated by consensus, fixed grid (no horizontal scrolling). */
export function Ticker({ ticks }: { ticks: Tick[] }) {
  return (
    <section className="markets" aria-labelledby="markets-title">
      <div className="markets-head">
        <h2 id="markets-title">Marchés en direct</h2>
        <small className="muted">Chaque cours = médiane des sources concordantes</small>
      </div>
      {ticks.length ? (
        <ul className="markets-grid">
          {ticks.map((t) => (
            <li key={t.symbol} className="market">
              <div className="market-top">
                <b>{t.symbol}</b>
                <span className={t.agreeing >= 2 ? "agree ok" : "agree weak"} title={`${t.agreeing} source(s) concordante(s) sur ${t.total}`}>
                  {t.agreeing >= 2 ? "✔" : "!"} {t.agreeing}/{t.total}
                </span>
              </div>
              <span className="market-price">{formatPrice(t.price)} $</span>
              {t.change !== null && <span className={t.change >= 0 ? "up" : "down"}>{formatPercent(t.change)}</span>}
            </li>
          ))}
        </ul>
      ) : (
        <p className="muted markets-loading">Interrogation des sources…</p>
      )}
    </section>
  );
}

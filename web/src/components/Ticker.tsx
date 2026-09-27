import { formatPercent, formatPrice, type Tick } from "../market";

export function Ticker({ ticks }: { ticks: Tick[] }) {
  if (!ticks.length) return <div className="ticker placeholder">Connexion aux marchés…</div>;
  const items = [...ticks, ...ticks];
  return (
    <div className="ticker" aria-label="Cours en direct">
      <div className="track">
        {items.map((t, i) => (
          <span key={i} className="tick">
            <b>{t.symbol.replace("USDT", "")}</b> {formatPrice(t.price)} $
            <em className={t.change >= 0 ? "up" : "down"}>{formatPercent(t.change)}</em>
          </span>
        ))}
      </div>
    </div>
  );
}

import { formatPercent, formatPrice, type Tick } from "../market";

const FALLBACK: Tick[] = [
  { symbol: "BTCUSDT", name: "Bitcoin", price: 0, change: 0 },
  { symbol: "ETHUSDT", name: "Ethereum", price: 0, change: 0 },
  { symbol: "SOLUSDT", name: "Solana", price: 0, change: 0 },
  { symbol: "BNBUSDT", name: "BNB", price: 0, change: 0 },
];

const BADGES = [
  ["ACHAT", "buy"],
  ["ATTENDRE", "hold"],
  ["ACHAT FORT", "buy"],
  ["VENTE", "sell"],
] as const;

/** Maquette d'iPhone reproduisant l'écran Radar de l'app. */
export function PhoneMockup({ ticks }: { ticks: Tick[] }) {
  const rows = (ticks.length ? ticks : FALLBACK).slice(0, 4);
  return (
    <div className="phone-wrap" aria-label="Aperçu de l'application Altim">
      <div className="phone">
        <div className="notch" />
        <div className="screen">
          <div className="screen-head">
            <span className="screen-title">Radar</span>
            <span className="env">DÉMO</span>
          </div>
          <div className="seg">
            <span>15 min</span>
            <span>1 h</span>
            <span className="on">4 h</span>
            <span>1 j</span>
          </div>
          <div className="gauge-card">
            <svg viewBox="0 0 200 110" className="gauge">
              <defs>
                <linearGradient id="g" x1="0" x2="1">
                  <stop offset="0" stopColor="#ff3b5c" />
                  <stop offset="0.45" stopColor="#ffc733" />
                  <stop offset="0.7" stopColor="#00f0ff" />
                  <stop offset="1" stopColor="#39ff88" />
                </linearGradient>
              </defs>
              <path d="M20 100 A80 80 0 0 1 180 100" stroke="rgba(255,255,255,.08)" strokeWidth="14" fill="none" strokeLinecap="round" />
              <path d="M20 100 A80 80 0 0 1 180 100" stroke="url(#g)" strokeWidth="14" fill="none" strokeLinecap="round" />
              <line x1="100" y1="100" x2="100" y2="34" stroke="#fff" strokeWidth="4" strokeLinecap="round" className="needle" />
            </svg>
            <div className="gauge-label">
              <b>+41</b>
              <small>confiance 46 %</small>
            </div>
          </div>
          {rows.map((t, i) => {
            const [label, kind] = BADGES[i % BADGES.length]!;
            return (
              <div className={`row row-${kind}`} key={t.symbol} style={{ animationDelay: `${i * 120}ms` }}>
                <div>
                  <b>{t.name}</b>
                  <small>{t.symbol}</small>
                  <span className={`badge ${kind}`}>{label}</span>
                </div>
                <div className="num">
                  <b>{t.price ? formatPrice(t.price) : "—"}</b>
                  <small className={t.change >= 0 ? "up" : "down"}>{t.price ? formatPercent(t.change) : ""}</small>
                </div>
              </div>
            );
          })}
        </div>
      </div>
      <div className="phone-glow" />
    </div>
  );
}

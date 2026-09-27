import { Section } from "./Section";

const CRYPTO = ["Binance", "OKX", "Coinbase", "Kraken", "KuCoin", "Gate.io", "Bitfinex", "Binance.US", "CoinGecko", "Yahoo Finance"];
const STOCKS = ["Yahoo Finance", "Nasdaq", "Cboe", "Alpaca", "Twelve Data*", "Polygon*", "Finnhub*"];
const CONTEXT = ["Fear & Greed (alternative.me)", "StockTwits"];

export function Sources() {
  return (
    <Section
      id="sources"
      eyebrow="Fiabilité"
      title={
        <>
          <span className="gradient">16 sources de prix</span> recoupées en permanence
        </>
      }
      intro="Chaque prix est vérifié auprès de plusieurs places de marché indépendantes. Une source qui diverge est écartée ; si les données ne sont pas fiables, Altim suspend le signal au lieu de deviner. Ce site applique le même consensus à chaque cours affiché."
    >
      <div className="sources-grid">
        <div className="card">
          <h3>Crypto</h3>
          <ul className="chips">{CRYPTO.map((s) => <li key={s}>{s}</li>)}</ul>
        </div>
        <div className="card">
          <h3>Actions & ETF</h3>
          <ul className="chips">{STOCKS.map((s) => <li key={s}>{s}</li>)}</ul>
          <small className="muted">* avec une clé gratuite</small>
        </div>
        <div className="card">
          <h3>Contexte</h3>
          <ul className="chips">{CONTEXT.map((s) => <li key={s}>{s}</li>)}</ul>
        </div>
      </div>
      <ol className="pipeline">
        <li><b>Consensus</b> médiane bougie par bougie, écart toléré 0,5 % (crypto) / 1 % (actions)</li>
        <li><b>Contrôle qualité</b> trous, pics aberrants, données périmées, volume absent</li>
        <li><b>Réseau résilient</b> nouvelles tentatives, disjoncteur par source, bascule automatique</li>
        <li><b>Garde-fou</b> données douteuses ou sources en désaccord : aucun conseil plutôt qu'un mauvais conseil</li>
      </ol>
    </Section>
  );
}

import { Section } from "./Section";

const QA = [
  [
    "Altim garantit-il des gains ?",
    "Non. Altim est un outil d'aide à la décision. Les signaux sont des probabilités basées sur l'historique. Investissez uniquement ce que vous pouvez vous permettre de perdre.",
  ],
  [
    "Quels marchés sont couverts ?",
    "Les cryptos cotées en USDT sur Binance (BTC, ETH, SOL…) et les actions/ETF via Yahoo Finance. L'exécution d'actions passe par Alpaca (actions US).",
  ],
  [
    "Utilisez-vous Bloomberg ?",
    "Les données Bloomberg (Terminal, B-PIPE) nécessitent une licence professionnelle. Altim recoupe à la place jusqu'à 16 sources de prix (Binance, OKX, Coinbase, Kraken, Nasdaq, Cboe…) et son architecture permet de brancher un flux Bloomberg si vous disposez d'une licence.",
  ],
  [
    "Que se passe-t-il si une source donne un mauvais prix ?",
    "Elle est comparée aux autres bougie par bougie et écartée si elle s'éloigne de la médiane. Si les sources sont en désaccord ou trop peu nombreuses, le signal est suspendu et les achats sont bloqués. Avant chaque ordre, le prix du courtier est aussi comparé au consensus.",
  ],
  [
    "Puis-je essayer sans risque ?",
    "Oui : l'app démarre en mode démo avec 10 000 USDT fictifs. Vous pouvez ensuite utiliser le testnet Binance ou le paper trading Alpaca avant le réel.",
  ],
  [
    "Mes clés API sont-elles en sécurité ?",
    "Elles sont stockées chiffrées dans le trousseau de votre iPhone (accessible uniquement appareil déverrouillé) et ne quittent jamais l'appareil, sauf pour signer vos ordres chez le courtier.",
  ],
];

export function FAQ() {
  return (
    <Section id="faq" eyebrow="FAQ" title="Questions fréquentes">
      <div className="faq">
        {QA.map(([q, a]) => (
          <details key={q} className="card">
            <summary>{q}</summary>
            <p>{a}</p>
          </details>
        ))}
      </div>
    </Section>
  );
}

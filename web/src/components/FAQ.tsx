import { Section } from "./Section";

const QA = [
  [
    "Altim garantit-il des gains ?",
    "Non. Altim est un outil d'aide à la décision. Les signaux sont des probabilités basées sur l'historique. Investissez uniquement ce que vous pouvez vous permettre de perdre.",
  ],
  [
    "Quels marchés sont couverts ?",
    "Toutes les cryptos cotées en dollar sur Binance, OKX, Coinbase, Kraken, KuCoin et Gate (environ 2 000), et toutes les actions et ETF cotés aux États-Unis (plus de 11 000, y compris de nombreuses sociétés européennes via leur cotation américaine). Les actions cotées en euros ne sont pas encore prises en charge.",
  ],
  [
    "Utilisez-vous Bloomberg ?",
    "Les données Bloomberg (Terminal, B-PIPE) nécessitent une licence professionnelle. Altim recoupe à la place 40 sources de prix : jusqu'à 23 pour une crypto (Binance, OKX, Coinbase, Kraken, Bitstamp, Gemini…) et 17 pour une action (Nasdaq, Cboe, WSJ / MarketWatch, Financial Times, Fidelity, Robinhood…), et son architecture permet de brancher un flux Bloomberg si vous disposez d'une licence.",
  ],
  [
    "Que se passe-t-il si une source donne un mauvais prix ?",
    "Elle est comparée aux autres bougie par bougie et écartée si elle s'éloigne de la médiane. Si les sources sont en désaccord ou trop peu nombreuses, le signal est suspendu et Altim ne donne aucun conseil sur cet actif.",
  ],
  [
    "Altim achète-t-il ou vend-il à ma place ?",
    "Non. Altim est un conseiller : il ne passe aucun ordre et ne demande aucun accès à vos comptes. Vous suivez ou non ses conseils chez votre courtier habituel.",
  ],
  [
    "Comment Altim tient-il compte de ce que je possède ?",
    "Renseignez vos avoirs (actif, quantité, prix d'achat moyen, liquidités) : Altim calcule votre patrimoine, vos plus-values, vos risques, et adapte chaque conseil — par exemple le montant prudent pour un nouvel achat, ou s'il faut alléger une ligne trop lourde.",
  ],
  [
    "Altim peut-il protéger mon bot de trading des grands mouvements imprévus ?",
    "Oui, via le garde-fou marché : tendance de fond, risque de choc (volatilité anormale, sauts de prix, rafales d'actualités, VIX) et risque de retournement contre la tendance (excès techniques, foule surendettée, sentiment extrême, ton des actualités). Chaque signal est vérifié sur l'historique de l'actif avant de compter. Votre bot l'interroge via /api/guard et applique la consigne : continuer, réduire la taille ou suspendre. Aucun outil ne prévoit une vraie surprise, mais on peut éviter d'y être exposé à pleine taille.",
  ],
  [
    "Où sont stockés mes avoirs ?",
    "Sur votre appareil : dans le navigateur pour l'app web, dans un stockage protégé et exclu des sauvegardes pour les apps iPhone et Android. Jamais sur nos serveurs. Un export permet de les transférer d'un navigateur à l'autre.",
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

/** Words used in Altim, in plain French (same list on the iPhone and Android). */
export const GLOSSARY: [string, string][] = [
  ["Signal (ACHAT, ATTENDRE, VENTE)", "Vote pondéré de 7 indicateurs techniques sur les bougies clôturées : ACHAT au-dessus de +25, VENTE sous −25. Une probabilité mesurée sur l'historique, jamais une certitude."],
  ["Confiance", "Force du score multipliée par l'accord entre indicateurs et unités de temps. 60 % ne veut pas dire « 60 % de chances de gagner »."],
  ["Zone d'achat (Fibonacci)", "Entre 38,2 % et 65 % de repli du dernier mouvement haussier : là où un repli s'arrête souvent. La « zone d'or » va de 61,8 % à 65 %. Cassée si le prix passe sous le plus bas du mouvement."],
  ["Stop (stop-loss)", "Prix auquel on vend pour limiter la perte si l'idée ne marche pas. Altim le place à 2 × ATR sous le prix, ou sous le plus bas qui invalide la zone d'achat."],
  ["ATR", "Average True Range : l'amplitude moyenne d'une bougie. Un stop à 2 × ATR laisse respirer le prix sans être touché par le bruit ordinaire."],
  ["Rapport gain/risque (R)", "Ce que rapporte l'objectif divisé par ce que coûte le stop. 2 R : on vise deux fois ce qu'on risque. Sous 1,5, l'idée rapporte peu."],
  ["RSI", "Relative Strength Index (14 bougies) : sous 30, le prix a beaucoup baissé (survente) ; au-dessus de 70, beaucoup monté (surachat)."],
  ["MACD", "Écart entre deux moyennes mobiles exponentielles (12 et 26) et sa moyenne (9) : son croisement signale un changement d'élan."],
  ["EMA", "Moyenne mobile exponentielle : moyenne des prix qui donne plus de poids aux plus récents. Prix au-dessus de l'EMA 200 : tendance de fond haussière."],
  ["Volatilité", "Ampleur des variations. Annualisée : l'écart type des variations journalières ramené à un an. Bitcoin ≈ 40–60 %/an, une grande action ≈ 20–30 %/an."],
  ["Pire recul (drawdown)", "Plus forte baisse depuis un sommet sur la période. −50 % : il faut ensuite +100 % pour revenir au sommet."],
  ["Corrélation", "Entre −1 et 1 : proche de 1, deux actifs montent et baissent ensemble (peu de diversification) ; proche de 0, ils sont indépendants."],
  ["Consensus de prix", "Prix médian de plusieurs sources (bourses, courtiers) ; une source qui s'écarte est écartée. La fiabilité baisse quand les sources se contredisent."],
  ["Garde-fou marché", "Risque de choc (volatilité anormale, sauts de prix, VIX…) et de retournement : il réduit ou suspend les conseils d'achat quand le marché est dangereux."],
  ["Investissement programmé (DCA)", "Acheter le même montant à intervalle régulier : on achète plus quand c'est bas, moins quand c'est haut, sans chercher le bon moment."],
  ["PFU (flat tax)", "Prélèvement forfaitaire unique de 30 % sur les plus-values en France (12,8 % d'impôt + 17,2 % de prélèvements sociaux). Les pertes de l'année compensent les gains."],
  ["Risque/rendement (gain/risque)", "Dans la carte Décision : gain jusqu'à l'objectif 1 divisé par la perte jusqu'au stop, depuis le prix d'entrée indiqué. Altim n'entre pas sous 2 : même à moitié de réussite, l'idée doit rester gagnante."],
  ["Veto (interdiction d'achat)", "Une raison qui interdit d'acheter maintenant, quel que soit le reste : écart achat/vente trop grand, résultats imminents, chute brutale, tendance baissière… Toutes les vérifications sont listées, actives ou non, et celles sans source sont dites « non vérifiables »."],
  ["Invalidation", "Le niveau ou l'événement qui prouve que le scénario était faux (souvent une clôture sous le plus bas du mouvement). Atteint, on sort : on n'attend pas que ça remonte."],
  ["FDV", "Fully Diluted Valuation : prix × nombre maximal de jetons. Une capitalisation bien inférieure à la FDV veut dire que beaucoup de jetons restent à émettre et peuvent peser sur le prix."],
  ["Profit factor", "Somme des gains divisée par la somme des pertes du signal sur le passé. Au-dessus de 1, il a gagné plus qu'il n'a perdu ; sous 1,3, la marge est mince une fois les frais comptés."],
  ["Sharpe", "Rendement moyen divisé par la volatilité des rendements (annualisé). Au-dessus de 1 : correct ; il pénalise les hausses comme les baisses."],
  ["Sortino", "Comme le Sharpe, mais ne compte que la volatilité des baisses : il ne pénalise pas les fortes hausses."],
  ["Drawdown (pire recul)", "Plus forte baisse depuis un sommet, pour un actif ou pour le signal. Il dit ce qu'il aurait fallu supporter sans vendre."],
  ["PER et PEG", "PER : prix de l'action divisé par le bénéfice par action (25 : on paie 25 années de bénéfice actuel). PEG : le PER divisé par la croissance du bénéfice ; autour de 1, le prix suit la croissance, bien au-dessus, il l'anticipe."],
  ["EV/EBITDA", "Valeur de l'entreprise (capitalisation + dette nette) divisée par son résultat avant intérêts, impôts et amortissements. Compare des sociétés endettées ou non."],
  ["P/S (prix ÷ ventes)", "Capitalisation boursière divisée par le chiffre d'affaires des 12 derniers mois. Utile quand le bénéfice est faible ou négatif ; à comparer entre sociétés d'un même secteur, les marges variant beaucoup d'un secteur à l'autre."],
  ["P/B (prix ÷ fonds propres)", "Capitalisation divisée par les fonds propres inscrits au bilan. Sous 1, la Bourse valorise la société moins que sa valeur comptable ; très élevé chez les sociétés qui rachètent beaucoup leurs actions, sans être un signal en soi."],
  ["ROIC", "Return on Invested Capital : résultat opérationnel après impôt divisé par le capital investi (dette + fonds propres − trésorerie). Mesure ce que rapporte chaque dollar engagé dans l'activité ; durablement au-dessus de 10–15 %, l'entreprise crée de la valeur."],
  ["Centile de valorisation", "Place du ratio actuel (PER ou P/S) dans son propre historique jour par jour, sur 5 ans au plus. 90e centile : plus cher que 90 % des jours de la période. Ne dit pas si l'action va baisser, seulement si elle est chère par rapport à elle-même."],
  ["Flux de stablecoins", "Variation du montant total de stablecoins (USDT, USDC…) en circulation. Une hausse signale de l'argent frais disponible pour acheter des cryptos ; une baisse, de la liquidité qui sort du marché. Indicateur de fond, pas un signal d'achat."],
  ["Activité de développement", "Travail visible sur le code d'un projet : commits, lignes modifiées et pull requests sur son dépôt public (GitHub). Une activité quasi nulle sur une plateforme de contrats est un mauvais signe ; beaucoup de commits ne garantissent pas la valeur du jeton."],
  ["TVL","Total Value Locked : montant déposé dans les contrats d'un réseau ou d'un protocole (prêts, échanges). Mesure son usage, pas sa rentabilité."],
  ["Funding (taux de financement)", "Sur les contrats perpétuels, paiement périodique entre acheteurs et vendeurs à effet de levier. Positif et élevé : la foule est très acheteuse à crédit, les baisses brutales deviennent plus probables."],
  ["Simulation (paper trading)", "Portefeuille virtuel, sans argent réel ni ordre passé : on « achète » depuis la carte Décision, Altim suit le cours et vend au stop ou à l'objectif sur les bougies journalières, frais (0,1 %) et glissement (0,05 %) compris. Les résultats, regroupés par décision affichée à l'achat, montrent quels verdicts ont vraiment marché ; sous une vingtaine de trades, ils veulent dire peu."],
  ["Rééquilibrage", "Revenir à la répartition choisie (ex. 40 % crypto, 60 % actions) en vendant ce qui a trop monté et en achetant ce qui a pris du retard."],
];

/** Glossary screen (/app/lexique). */
export function Glossary() {
  return (
    <section className="app-screen">
      <div className="screen-top">
        <div>
          <h1>Lexique</h1>
          <p className="muted small">Les mots d'Altim, expliqués simplement.</p>
        </div>
      </div>
      <div className="card">
        <dl className="glossary">
          {GLOSSARY.map(([term, text]) => (
            <div key={term}>
              <dt>{term}</dt>
              <dd className="muted">{text}</dd>
            </div>
          ))}
        </dl>
      </div>
    </section>
  );
}

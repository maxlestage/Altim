/** Words used in Altim, in plain French (same list on the iPhone and Android). */
export const GLOSSARY: [string, string][] = [
  ["Signal (ACHAT, ATTENDRE, VENTE)", "Vote pondéré de 7 indicateurs techniques sur les bougies clôturées : ACHAT au-dessus de +25, VENTE sous −25. Une probabilité mesurée sur l'historique, jamais une certitude."],
  ["Confiance", "Force du score multipliée par l'accord entre indicateurs et unités de temps. 60 % ne veut pas dire « 60 % de chances de gagner »."],
  ["Preuve du modèle", "Ce que dit la validation du signal sur 34 actifs fixés d'avance (écran « Validation du modèle ») pour la classe de l'actif (actions, Bitcoin, Ethereum, altcoins) et le régime de marché où il se trouve : gain moyen par trade, statistique t, nombre d'actifs où le signal a battu la simple détention. Sans avantage démontré, la carte Décision le dit ; si la classe perd en moyenne ou si la détention a fait mieux sur plus des deux tiers de ses actifs, la confiance est plafonnée à 60 et ACHAT FORT devient ACHAT. La validation n'augmente jamais la confiance ; le passé ne garantit rien."],
  ["Bot Altim", "Un modèle appris (deux régressions logistiques) sur 34 actifs fixés d'avance : à chaque clôture, 16 mesures (variations, RSI, moyennes, MACD, volatilité, volume, régime…) donnent une probabilité de hausse et de baisse à 20 jours, coûts compris. ACHETER, VENDRE (sortir ou rester dehors, jamais de vente à découvert) ou ATTENDRE selon des seuils fixés sur l'entraînement. Jugé seulement sur des périodes qu'il n'avait pas vues (« walk-forward »), contre un jour pris au hasard. Il ne compte dans une décision que du côté où il a montré un avantage (t ≥ 2 sur au moins 30 signaux), et de 3 points de confiance au plus ; jamais contre un veto."],
  ["Hors échantillon", "Résultat mesuré sur des données que le modèle n'a pas vues pendant son apprentissage. C'est le seul qui compte : sur les données d'entraînement, un modèle paraît toujours meilleur qu'il n'est."],
  ["Calibration", "Accord entre les probabilités annoncées et ce qui arrive : un modèle bien calibré qui annonce 60 % voit l'événement se produire environ 6 fois sur 10."],
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
  ["Note (ACHAT FORT … VENTE FORTE)", "Résumé en 6 niveaux de la carte Décision, tiré du verdict, de son niveau et de la confiance : ACHAT FORT (achat avec signal fort et confiance d'au moins 70), ACHAT, ATTENDRE, ALLÉGER, VENDRE, VENTE FORTE (vente en tendance baissière, confiance d'au moins 70). Un signal dégradé n'a jamais de note d'achat."],
  ["Score composite", "Note de −100 à +100 qui pondère les familles d'indices : technique 32 %, momentum 18 %, fondamentaux 20 %, sentiment 10 %, actualités 10 %, macro 10 % (modifiables dans Réglages). Une famille sans données n'est pas estimée : son poids est réparti sur les autres."],
  ["Signal dégradé", "Les indices se contredisent (au moins 3 familles favorables et 3 défavorables, ou technique et fondamentaux opposés), les données sont peu fiables, ou le signal a perdu de l'argent sur cet actif par le passé. Aucune entrée n'est alors privilégiée."],
  ["Risk-on / risk-off", "Humeur des marchés. Risk-on : stress macro calme et indice de référence en hausse, les investisseurs prennent des risques. Risk-off : stress élevé, VIX au-dessus de 30, ou stress tendu avec un indice en baisse : ils se protègent. Sinon : neutre."],
  ["Horizon (scalping … long terme)", "Durée probable d'un plan, d'après ses bougies et la distance à l'objectif 1 mesurée en ATR : scalping et day trading (dans la journée), swing (quelques jours à semaines), moyen terme (semaines à mois), long terme (plusieurs mois et plus)."],
  ["Objectif 3", "Troisième objectif de prix : le prochain niveau touché plusieurs fois au-dessus de l'objectif 2, sans dépasser l'objectif 2 plus l'écart entre les objectifs 1 et 2. Le rapport gain/risque reste calculé sur l'objectif 1."],
  ["Ichimoku", "Indicateur japonais : Tenkan (milieu des 9 dernières bougies), Kijun (26) et un « nuage » tracé à partir de ces lignes et des 52 dernières bougies. Prix au-dessus du nuage : tendance haussière ; dedans : indécision ; dessous : baissière."],
  ["Supertrend", "Ligne qui suit le prix à 3 ATR (10 bougies) de distance et change de côté quand le prix la traverse. Sous le prix : tendance haussière ; au-dessus : baissière."],
  ["Canal de Donchian", "Plus haut et plus bas des 20 dernières bougies. Une clôture au-dessus du plus haut est une cassure haussière, sous le plus bas une cassure baissière."],
  ["VWAP", "Prix moyen pondéré par les volumes. Altim le calcule sur les 20 dernières bougies (glissant), pas depuis l'ouverture de la séance comme les salles de marché : c'est une approximation."],
  ["Profil de volume", "Volume échangé à chaque niveau de prix. Altim l'estime à partir des bougies en répartissant le volume de chacune entre son plus bas et son plus haut : une approximation. Le niveau le plus échangé (POC) et la zone de valeur (70 % du volume) servent souvent d'aimants ou d'appuis."],
  ["Points pivots", "Niveaux calculés sur la séance précédente : pivot P = (plus haut + plus bas + clôture) ÷ 3, puis résistances R1, R2 et supports S1, S2 autour. Très suivis en séance."],
  ["Support et résistance", "Prix où le marché a rebondi (support) ou buté (résistance) plusieurs fois. Altim regroupe les sommets et creux proches (à moins d'un demi-ATR) et ne garde que les niveaux touchés au moins deux fois : plus il y a de contacts, plus le niveau compte."],
  ["Cassure et fausse cassure", "Cassure confirmée : clôture au-delà d'un support ou d'une résistance avec un volume d'au moins 1,5 fois la moyenne de 20 bougies. Fausse cassure : le niveau est franchi, puis le prix revient de l'autre côté en moins de 3 bougies ; souvent un piège pour ceux qui ont suivi."],
  ["Structure de marché", "Suite des sommets et des creux : de plus en plus hauts, la tendance monte ; de plus en plus bas, elle baisse ; sinon, pas de direction."],
  ["Force relative", "Performance de l'actif moins celle de son indice de référence (bitcoin pour une crypto, S&P 500 et Nasdaq-100 pour une action) sur 1, 3 et 6 mois, en points. Positive : l'actif fait mieux que son marché."],
  ["Impact potentiel d'une actualité", "Faible, moyen ou important. « Estimé par règle » : nombre de sources indépendantes, thème (escalade grave, banques centrales, régulation, piratage) et mention de vos actifs. « Mesuré » : variation de l'actif cité depuis la publication, sur ses bougies horaires ; elle ne prouve pas que l'article en est la cause."],
  ["Consensus des sources", "Accord du ton des titres de chaque source sur un même sujet (repérage par mots-clés). Convergent : aucun titre positif face à un négatif ; divergent : les sources se contredisent."],
  ["Calendrier de risque", "Risque de chaque jour des 7 prochains d'après l'Agenda. 🔴 : décision de taux, inflation, emploi ou PIB majeurs, ou résultats d'une action que vous suivez ; 🟠 : autres annonces, résultats des grandes sociétés, dividende ou split d'une action détenue ; 🟢 : aucun événement majeur."],
  ["Quand ne pas trader", "Moments où même un bon signal a peu de chances d'être bien exécuté : volatilité extrême, liquidité insuffisante, écart achat/vente anormal, résultats dans les 5 jours ou publiés depuis 1 à 2 séances, annonce économique dans les 48 h, marché sans direction, signal trop faible (score composite entre −15 et +15 avec une confiance sous 50) ou dégradé, Bourse de New York fermée. Chaque raison vient d'une mesure ; ce qui n'a pas pu être vérifié est dit."],
  ["Marché sans direction", "ADX sous 20 et sommets et creux qui ne montent ni ne baissent : le prix oscille sans tendance. Les signaux de tendance y échouent plus souvent."],
  ["Zones d'action", "Le plan de la carte Décision sur une échelle de prix : niveau d'invalidation (le stop), zone de sortie (autour du stop, à ½ ATR près), zone d'achat, zone d'attente (au-dessus de la zone d'achat, jusqu'à l'objectif 1 : pas d'entrée) et zone de prise de bénéfices (objectifs 1 à 3). « Vous êtes ici » marque le prix actuel."],
  ["Scénario en cours", "Chaque scénario (haussier, neutre, baissier) a ses conditions vérifiées sur les dernières données : clôture au-delà d'un niveau, volume au-dessus de sa moyenne de 20 séances, RSI au-dessus ou sous 50, ADX. Le scénario « en cours » est celui dont la plus grande part des conditions est remplie ; en cas d'égalité, le neutre. Une lecture de l'instant, pas une prévision."],
  ["Contre-argument", "Le nombre de raisons favorables et défavorables de la carte Décision, et ce qui prouverait le scénario faux : cassure d'un support ou d'une résistance, volume qui retombe sous sa moyenne de 20 séances (ou qui la dépasse de 50 % en cas d'attente), événement à venir."],
  ["Pourquoi le signal a changé", "Quand le verdict, le niveau ou la note d'un actif change, Altim compare les mesures de la décision précédente, gardées dans ce navigateur, aux nouvelles : score composite, familles d'indices (écart d'au moins 10 points), volume relatif, RSI, support et résistance les plus proches, ton des actualités."],
  ["Open interest", "Valeur totale des contrats à terme encore ouverts sur une crypto. S'il monte vite : beaucoup de nouvelles positions à effet de levier, le prix peut ensuite bouger plus brutalement."],
  ["Liquidation", "Fermeture forcée par la plateforme d'une position à effet de levier dont la garantie ne suffit plus. Beaucoup de liquidations d'un même côté accélèrent souvent le mouvement, sans en prédire la suite."],
  ["Z-score (écart à la moyenne)", "Distance du prix à sa moyenne de 20 séances, en écarts types de ces 20 clôtures. Au-delà de ±2,5, l'écart est inhabituel ; il peut se résorber comme marquer le début d'une tendance."],
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

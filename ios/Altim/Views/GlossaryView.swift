import SwiftUI

/// Words used in Altim, in plain French (same list as the site and Android).
enum Glossary {
    static let terms: [(term: String, text: String)] = [
        ("Signal (ACHAT, ATTENDRE, VENTE)", "Vote pondéré de 7 indicateurs techniques sur les bougies clôturées : ACHAT au-dessus de +25, VENTE sous −25. Une probabilité mesurée sur l'historique, jamais une certitude."),
        ("Confiance", "Force du score multipliée par l'accord entre indicateurs et unités de temps. 60 % ne veut pas dire « 60 % de chances de gagner »."),
        ("Zone d'achat (Fibonacci)", "Entre 38,2 % et 65 % de repli du dernier mouvement haussier : là où un repli s'arrête souvent. La « zone d'or » va de 61,8 % à 65 %. Cassée si le prix passe sous le plus bas du mouvement."),
        ("Stop (stop-loss)", "Prix auquel on vend pour limiter la perte si l'idée ne marche pas. Altim le place à 2 × ATR sous le prix, ou sous le plus bas qui invalide la zone d'achat."),
        ("ATR", "Average True Range : l'amplitude moyenne d'une bougie. Un stop à 2 × ATR laisse respirer le prix sans être touché par le bruit ordinaire."),
        ("Rapport gain/risque (risque/rendement, R)", "Ce que rapporte l'objectif divisé par ce que coûte le stop. 2 R : on vise deux fois ce qu'on risque. Sous 1,5, l'idée rapporte peu ; la décision d'Altim n'entre pas sous 2."),
        ("Veto (interdiction d'achat)", "Raison qui interdit d'acheter maintenant, quel que soit le reste : écart achat/vente trop grand, résultats imminents, chute brutale, choc macro… Une vérification sans source gratuite est affichée « non vérifiable », jamais supposée bonne."),
        ("Invalidation", "Prix ou événement qui rend le scénario faux (par exemple une clôture sous le plus bas du mouvement) : on n'achète plus, et si on détient, on protège ou on sort."),
        ("RSI", "Relative Strength Index (14 bougies) : sous 30, le prix a beaucoup baissé (survente) ; au-dessus de 70, beaucoup monté (surachat)."),
        ("MACD", "Écart entre deux moyennes mobiles exponentielles (12 et 26) et sa moyenne (9) : son croisement signale un changement d'élan."),
        ("EMA", "Moyenne mobile exponentielle : moyenne des prix qui donne plus de poids aux plus récents. Prix au-dessus de l'EMA 200 : tendance de fond haussière."),
        ("Volatilité", "Ampleur des variations. Annualisée : l'écart type des variations journalières ramené à un an. Bitcoin ≈ 40–60 %/an, une grande action ≈ 20–30 %/an."),
        ("Pire recul (drawdown)", "Plus forte baisse depuis un sommet sur la période. −50 % : il faut ensuite +100 % pour revenir au sommet."),
        ("Simulation (paper trading)", "Portefeuille virtuel, sans argent réel ni ordre passé : on « achète » depuis la carte Décision, Altim suit le cours et vend au stop ou à l'objectif sur les bougies journalières, frais (0,1 %) et glissement (0,05 %) compris. Les résultats, regroupés par décision affichée à l'achat, montrent quels verdicts ont vraiment marché ; sous une vingtaine de trades, ils veulent dire peu."),
        ("Profit factor", "Somme des gains divisée par la somme des pertes des trades passés. Au-dessus de 1, les gains l'emportent ; 2 : deux fois plus gagné que perdu. Peu de trades : chiffre fragile."),
        ("Sharpe", "Rendement moyen divisé par sa volatilité : combien on a gagné par unité de risque. Au-dessus de 1, correct ; mesuré sur le passé, il ne garantit rien."),
        ("Sortino", "Comme le Sharpe, mais ne compte que les baisses comme risque (une hausse brutale n'est pas pénalisée)."),
        ("PER / PEG", "PER : prix de l'action divisé par le bénéfice par action (combien d'années de bénéfices on paie). PEG : PER divisé par la croissance du bénéfice ; autour de 1, prix en ligne avec la croissance, bien au-dessus, cher."),
        ("EV/EBITDA", "Valeur de l'entreprise (capitalisation + dette nette) divisée par son résultat avant intérêts, impôts et amortissements. Compare des sociétés endettées ou non ; plus il est haut, plus c'est cher."),
        ("P/S (prix ÷ ventes)", "Capitalisation boursière divisée par le chiffre d'affaires des 12 derniers mois. Utile quand le bénéfice est faible ou négatif ; à comparer entre sociétés d'un même secteur, les marges variant beaucoup d'un secteur à l'autre."),
        ("P/B (prix ÷ fonds propres)", "Capitalisation divisée par les fonds propres inscrits au bilan. Sous 1, la Bourse valorise la société moins que sa valeur comptable ; très élevé chez les sociétés qui rachètent beaucoup leurs actions, sans être un signal en soi."),
        ("ROIC", "Return on Invested Capital : résultat opérationnel après impôt divisé par le capital investi (dette + fonds propres − trésorerie). Mesure ce que rapporte chaque dollar engagé dans l'activité ; durablement au-dessus de 10–15 %, l'entreprise crée de la valeur."),
        ("Centile de valorisation", "Place du ratio actuel (PER ou P/S) dans son propre historique jour par jour, sur 5 ans au plus. 90e centile : plus cher que 90 % des jours de la période. Ne dit pas si l'action va baisser, seulement si elle est chère par rapport à elle-même."),
        ("FDV", "Fully diluted valuation : valeur de la crypto si toute l'offre maximale était déjà en circulation. Capitalisation / FDV loin de 1 : beaucoup de jetons restent à débloquer, ce qui peut peser sur le prix."),
        ("TVL", "Total value locked : montant déposé dans les applications d'un réseau (prêts, échanges…). Mesure l'usage réel, pas le prix."),
        ("Flux de stablecoins", "Variation du montant total de stablecoins (USDT, USDC…) en circulation. Une hausse signale de l'argent frais disponible pour acheter des cryptos ; une baisse, de la liquidité qui sort du marché. Indicateur de fond, pas un signal d'achat."),
        ("Activité de développement", "Travail visible sur le code d'un projet : commits, lignes modifiées et pull requests sur son dépôt public (GitHub). Une activité quasi nulle sur une plateforme de contrats est un mauvais signe ; beaucoup de commits ne garantissent pas la valeur du jeton."),
        ("Funding (financement)", "Taux payé toutes les 8 h entre acheteurs et vendeurs de contrats perpétuels. Très positif : beaucoup de paris à la hausse avec effet de levier, risque de purge ; négatif : l'inverse."),
        ("Corrélation", "Entre −1 et 1 : proche de 1, deux actifs montent et baissent ensemble (peu de diversification) ; proche de 0, ils sont indépendants."),
        ("Consensus de prix", "Prix médian de plusieurs sources (bourses, courtiers) ; une source qui s'écarte est écartée. La fiabilité baisse quand les sources se contredisent."),
        ("Garde-fou marché", "Risque de choc (volatilité anormale, sauts de prix, VIX…) et de retournement : il réduit ou suspend les conseils d'achat quand le marché est dangereux."),
        ("Investissement programmé (DCA)", "Acheter le même montant à intervalle régulier : on achète plus quand c'est bas, moins quand c'est haut, sans chercher le bon moment."),
        ("PFU (flat tax)", "Prélèvement forfaitaire unique de 30 % sur les plus-values en France (12,8 % d'impôt + 17,2 % de prélèvements sociaux). Les pertes de l'année compensent les gains."),
        ("Rééquilibrage", "Revenir à la répartition choisie (ex. 40 % crypto, 60 % actions) en vendant ce qui a trop monté et en achetant ce qui a pris du retard."),
        ("Note (ACHAT FORT … VENTE FORTE)", "Résumé en 6 niveaux de la carte Décision, tiré du verdict, de son niveau et de la confiance : ACHAT FORT (achat avec signal fort et confiance d'au moins 70), ACHAT, ATTENDRE, ALLÉGER, VENDRE, VENTE FORTE (vente en tendance baissière, confiance d'au moins 70). Un signal dégradé n'a jamais de note d'achat."),
        ("Score composite", "Note de −100 à +100 qui pondère les familles d'indices : technique 32 %, momentum 18 %, fondamentaux 20 %, sentiment 10 %, actualités 10 %, macro 10 % (modifiables dans Réglages). Une famille sans données n'est pas estimée : son poids est réparti sur les autres."),
        ("Signal dégradé", "Les indices se contredisent (au moins 3 familles favorables et 3 défavorables, ou technique et fondamentaux opposés), les données sont peu fiables, ou le signal a perdu de l'argent sur cet actif par le passé. Aucune entrée n'est alors privilégiée."),
        ("Risk-on / risk-off", "Humeur des marchés. Risk-on : stress macro calme et indice de référence en hausse, les investisseurs prennent des risques. Risk-off : stress élevé, VIX au-dessus de 30, ou stress tendu avec un indice en baisse : ils se protègent. Sinon : neutre."),
        ("Horizon (scalping … long terme)", "Durée probable d'un plan, d'après ses bougies et la distance à l'objectif 1 mesurée en ATR : scalping et day trading (dans la journée), swing (quelques jours à semaines), moyen terme (semaines à mois), long terme (plusieurs mois et plus)."),
        ("Objectif 3", "Troisième objectif de prix : le prochain niveau touché plusieurs fois au-dessus de l'objectif 2, sans dépasser l'objectif 2 plus l'écart entre les objectifs 1 et 2. Le rapport gain/risque reste calculé sur l'objectif 1."),
        ("Ichimoku", "Indicateur japonais : Tenkan (milieu des 9 dernières bougies), Kijun (26) et un « nuage » tracé à partir de ces lignes et des 52 dernières bougies. Prix au-dessus du nuage : tendance haussière ; dedans : indécision ; dessous : baissière."),
        ("Supertrend", "Ligne qui suit le prix à 3 ATR (10 bougies) de distance et change de côté quand le prix la traverse. Sous le prix : tendance haussière ; au-dessus : baissière."),
        ("Canal de Donchian", "Plus haut et plus bas des 20 dernières bougies. Une clôture au-dessus du plus haut est une cassure haussière, sous le plus bas une cassure baissière."),
        ("VWAP", "Prix moyen pondéré par les volumes. Altim le calcule sur les 20 dernières bougies (glissant), pas depuis l'ouverture de la séance comme les salles de marché : c'est une approximation."),
        ("Profil de volume", "Volume échangé à chaque niveau de prix. Altim l'estime à partir des bougies en répartissant le volume de chacune entre son plus bas et son plus haut : une approximation. Le niveau le plus échangé (POC) et la zone de valeur (70 % du volume) servent souvent d'aimants ou d'appuis."),
        ("Points pivots", "Niveaux calculés sur la séance précédente : pivot P = (plus haut + plus bas + clôture) ÷ 3, puis résistances R1, R2 et supports S1, S2 autour. Très suivis en séance."),
        ("Support et résistance", "Prix où le marché a rebondi (support) ou buté (résistance) plusieurs fois. Altim regroupe les sommets et creux proches (à moins d'un demi-ATR) et ne garde que les niveaux touchés au moins deux fois : plus il y a de contacts, plus le niveau compte."),
        ("Cassure et fausse cassure", "Cassure confirmée : clôture au-delà d'un support ou d'une résistance avec un volume d'au moins 1,5 fois la moyenne de 20 bougies. Fausse cassure : le niveau est franchi, puis le prix revient de l'autre côté en moins de 3 bougies ; souvent un piège pour ceux qui ont suivi."),
        ("Structure de marché", "Suite des sommets et des creux : de plus en plus hauts, la tendance monte ; de plus en plus bas, elle baisse ; sinon, pas de direction."),
        ("Force relative", "Performance de l'actif moins celle de son indice de référence (bitcoin pour une crypto, S&P 500 et Nasdaq-100 pour une action) sur 1, 3 et 6 mois, en points. Positive : l'actif fait mieux que son marché."),
    ]
}

struct GlossaryView: View {
    var body: some View {
        List {
            ForEach(Glossary.terms, id: \.term) { item in
                VStack(alignment: .leading, spacing: 4) {
                    Text(item.term).font(.subheadline.weight(.semibold))
                    Text(item.text).font(.footnote).foregroundStyle(Theme.textSecondary)
                }
                .padding(.vertical, 2)
                .accessibilityElement(children: .combine)
                .listRowBackground(Theme.surface.opacity(0.6))
            }
        }
        .listStyle(.insetGrouped)
        .altimScreen()
        .navigationTitle("Lexique")
    }
}

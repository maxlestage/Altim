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
        ("FDV", "Fully diluted valuation : valeur de la crypto si toute l'offre maximale était déjà en circulation. Capitalisation / FDV loin de 1 : beaucoup de jetons restent à débloquer, ce qui peut peser sur le prix."),
        ("TVL", "Total value locked : montant déposé dans les applications d'un réseau (prêts, échanges…). Mesure l'usage réel, pas le prix."),
        ("Funding (financement)", "Taux payé toutes les 8 h entre acheteurs et vendeurs de contrats perpétuels. Très positif : beaucoup de paris à la hausse avec effet de levier, risque de purge ; négatif : l'inverse."),
        ("Corrélation", "Entre −1 et 1 : proche de 1, deux actifs montent et baissent ensemble (peu de diversification) ; proche de 0, ils sont indépendants."),
        ("Consensus de prix", "Prix médian de plusieurs sources (bourses, courtiers) ; une source qui s'écarte est écartée. La fiabilité baisse quand les sources se contredisent."),
        ("Garde-fou marché", "Risque de choc (volatilité anormale, sauts de prix, VIX…) et de retournement : il réduit ou suspend les conseils d'achat quand le marché est dangereux."),
        ("Investissement programmé (DCA)", "Acheter le même montant à intervalle régulier : on achète plus quand c'est bas, moins quand c'est haut, sans chercher le bon moment."),
        ("PFU (flat tax)", "Prélèvement forfaitaire unique de 30 % sur les plus-values en France (12,8 % d'impôt + 17,2 % de prélèvements sociaux). Les pertes de l'année compensent les gains."),
        ("Rééquilibrage", "Revenir à la répartition choisie (ex. 40 % crypto, 60 % actions) en vendant ce qui a trop monté et en achetant ce qui a pris du retard."),
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

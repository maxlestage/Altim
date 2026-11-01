import SwiftUI

/// Words used in Altim, in plain French (same list as the site and Android).
enum Glossary {
    static let terms: [(term: String, text: String)] = [
        ("Verdict (ACHAT FORT, ACHAT, ATTENDRE, VENTE)", "Règle journalière choisie sur des résultats mesurés : ACHAT quand le prix est au-dessus de sa moyenne 200 jours et que le momentum (60 jours pour une crypto, 3 mois pour une action) est positif ; ACHAT FORT sur un repli de quelques jours dans cette tendance (RSI 2 sous 10) ; VENTE quand la tendance et le momentum sont baissiers ; ATTENDRE quand ils se contredisent. Filtre de marché : un actif n'est acheté ou gardé que si son marché est lui aussi en tendance haussière avec un momentum positif (le Bitcoin pour les cryptos, le S&P 500 via SPY pour les actions) ; au test, ce filtre a donné un rendement égal ou meilleur et une pire baisse plus faible dans les 8 cas (deux listes, deux moitiés, cryptos et actions). Mesuré sur les deux listes d'actifs du laboratoire : un repli a donné en moyenne une meilleure entrée sur la semaine suivante, sans avantage net à un mois. Une règle testée sur le passé, jamais une certitude."),
        ("Filtre de marché", "Un actif en tendance haussière n'est acheté ou gardé que si tout son marché l'est aussi : le Bitcoin pour les cryptos, le S&P 500 (via l'ETF SPY) pour les actions, au-dessus de sa moyenne 200 jours avec un momentum positif. Sinon le verdict dit SORTIR (MARCHÉ). Testé sur les deux listes d'actifs et les deux moitiés d'historique : rendement égal ou meilleur et pire baisse plus faible dans les 8 cas (ex. cryptos 1re moitié +55 % / pire baisse −37 % contre +46 % / −48 %). Attention, ce résultat est fragile : lu avec un momentum deux fois plus court ou plus long pour le marché, le filtre a fait moins bien que la règle seule ; il tient seulement avec le même momentum que celui de l'actif (60 jours, 3 mois), celui retenu d'avance. Appliqué partout, widget, montre et Siri compris ; la machine à remonter le temps le rejoue aussi (sans le veto des bots)."),
        ("Bots entraînés", "Modèles appris sur l'iPhone (arbres de décision boostés) à partir de 12 mesures de chaque journée — momentum sur 4 horizons, écarts aux moyennes 50 et 200 jours, RSI, volatilité, recul depuis le plus haut, volume — et de ce qui a suivi, pour tous les actifs d'un même marché. Bot Tendance : prévoit le rendement des 20 jours suivants ; il sert seulement de veto à l'achat (pas de nouvel achat sous −1 %, une position ouverte est conservée), ce qui a amélioré la règle au test ; il ne fait plus sortir, car avec le filtre de marché ses sorties faisaient rater des hausses, mais classer les actifs par sa prévision ne marche pas. Bot Risque : prévoit la pire baisse des 20 jours suivants ; fiable dans les 8 cas mesurés, il aide à régler la taille d'une position. Ses prévisions les plus calmes sous-estimaient la baisse réelle : il n'annonce donc jamais moins de 14 % en crypto ni 4 % en actions. Le veto du bot Tendance s'applique partout dans l'app (verdict, Radar, alertes, Sélection), sauf sur le widget, la montre et Siri. Testés « en marchant » (chaque prévision n'utilise que le passé) sur deux listes d'actifs et deux moitiés d'historique ; un bot linéaire et un bot à 5 jours, sans avantage démontré, ont été écartés."),
        ("Confiance", "Part des trois conditions qui vont dans le sens du verdict : prix au-dessus de la moyenne 200 jours, momentum positif, moyenne 20 jours au-dessus de la 50 jours (0, 33, 67 ou 100 %). Elle dit à quel point la tendance est nette, pas « les chances de gagner »."),
        ("Hors échantillon", "Résultat mesuré sur des données que le modèle n'a pas vues pendant son apprentissage. C'est le seul qui compte : sur les données d'entraînement, un modèle paraît toujours meilleur qu'il n'est."),
        ("Zone d'achat (Fibonacci)", "Entre 38,2 % et 65 % de repli du dernier mouvement haussier : là où un repli s'arrête souvent. La « zone d'or » va de 61,8 % à 65 %. Cassée si le prix passe sous le plus bas du mouvement."),
        ("Stop (stop-loss)", "Prix auquel on vend pour limiter la perte si l'idée ne marche pas. Altim le place à 2 × ATR sous le prix, ou sous le plus bas qui invalide la zone d'achat."),
        ("ATR", "Average True Range : l'amplitude moyenne d'une bougie. Un stop à 2 × ATR laisse respirer le prix sans être touché par le bruit ordinaire."),
        ("Rapport gain/risque (risque/rendement, R)", "Ce que rapporte l'objectif divisé par ce que coûte le stop. 2 R : on vise deux fois ce qu'on risque. Sous 1,5, l'idée rapporte peu."),
        ("Blocage (pas d'achat)", "Raison qui interdit d'acheter maintenant, quel que soit le reste : tendance de fond baissière ou signaux contraires, sources de prix en désaccord, risque de choc élevé (garde-fou), plus bas du mouvement cassé dans une zone d'achat. Des résultats trimestriels dans les 7 jours ou un marché agité ajoutent une mise en garde."),
        ("Invalidation", "Prix ou événement qui rend le scénario faux (par exemple une clôture sous le plus bas du mouvement) : on n'achète plus, et si on détient, on protège ou on sort."),
        ("RSI", "Relative Strength Index (14 bougies) : sous 30, le prix a beaucoup baissé (survente) ; au-dessus de 70, beaucoup monté (surachat)."),
        ("MACD", "Écart entre deux moyennes mobiles exponentielles (12 et 26) et sa moyenne (9) : son croisement signale un changement d'élan."),
        ("EMA", "Moyenne mobile exponentielle : moyenne des prix qui donne plus de poids aux plus récents. Prix au-dessus de l'EMA 200 : tendance de fond haussière."),
        ("Volatilité", "Ampleur des variations. Annualisée : l'écart type des variations journalières ramené à un an. Bitcoin ≈ 40–60 %/an, une grande action ≈ 20–30 %/an."),
        ("Pire recul (drawdown)", "Plus forte baisse depuis un sommet sur la période. −50 % : il faut ensuite +100 % pour revenir au sommet."),
        ("Simulation (paper trading)", "Portefeuille virtuel, sans argent réel ni ordre passé : Altim suit le cours et vend au stop ou à l'objectif sur les bougies journalières, frais (0,1 %) et glissement (0,05 %) compris. Sous une vingtaine de trades, les résultats veulent dire peu."),
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
        ("Montants en euros (taux EUR/USD)", "Les cours viennent en dollars des sources ; Altim les convertit au taux EUR/USD du moment (Yahoo Finance, sinon taux de référence de la BCE), relu toutes les 10 minutes, avec sa source et son heure. Pour une crypto, le prix médian en USD ou USDT est converti de même (1 USDT compté pour 1 $). Sans taux disponible, les montants restent en $ : jamais de taux inventé."),
        ("Plus-value en euros", "Votre prix de revient en € comparé au cours converti au taux du jour : c'est ce que vous gagneriez en vendant maintenant, effet de change compris (si le dollar baisse, une action américaine en hausse peut rapporter moins en euros). Un prix de revient saisi en $ est converti au taux du jour, pas au taux de votre achat : la plus-value en € de ces lignes est alors approchée."),
        ("Rééquilibrage", "Revenir à la répartition choisie (ex. 40 % crypto, 60 % actions) en vendant ce qui a trop monté et en achetant ce qui a pris du retard."),
        ("Risk-on / risk-off", "Humeur des marchés. Risk-on : stress macro calme et indice de référence en hausse, les investisseurs prennent des risques. Risk-off : stress élevé, VIX au-dessus de 30, ou stress tendu avec un indice en baisse : ils se protègent. Sinon : neutre."),
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
        ("Impact potentiel d'une actualité", "Faible, moyen ou important. « Estimé par règle » : nombre de sources indépendantes, thème (escalade grave, banques centrales, régulation, piratage) et mention de vos actifs. « Mesuré » : variation de l'actif cité depuis la publication, sur ses bougies horaires ; elle ne prouve pas que l'article en est la cause."),
        ("Consensus des sources", "Accord du ton des titres de chaque source sur un même sujet (repérage par mots-clés). Convergent : aucun titre positif face à un négatif ; divergent : les sources se contredisent."),
        ("Quand ne pas acheter", "Moments où même un verdict d'achat mérite d'attendre ou de réduire la taille : marché agité ou risque de choc (garde-fou), sources de prix en désaccord, résultats trimestriels dans les 7 jours, Bourse de New York fermée (le prix affiché est celui de la dernière séance)."),
        ("Marché sans direction", "ADX sous 20 et sommets et creux qui ne montent ni ne baissent : le prix oscille sans tendance. Les signaux de tendance y échouent plus souvent."),
        ("Changement de configuration", "Quand le verdict d'un actif change (tendance haussière, signaux contraires, tendance baissière), Altim le note avec le prix du moment, sur cet iPhone. Un simple repli dans la tendance n'en est pas un. Le bilan des verdicts (onglet Alertes) montre ce que le prix a fait ensuite."),
        ("Open interest", "Valeur totale des contrats à terme encore ouverts sur une crypto. S'il monte vite : beaucoup de nouvelles positions à effet de levier, le prix peut ensuite bouger plus brutalement."),
        ("Liquidation", "Fermeture forcée par la plateforme d'une position à effet de levier dont la garantie ne suffit plus. Beaucoup de liquidations d'un même côté accélèrent souvent le mouvement, sans en prédire la suite."),
        ("Z-score (écart à la moyenne)", "Distance du prix à sa moyenne de 20 séances, en écarts types de ces 20 clôtures. Au-delà de ±2,5, l'écart est inhabituel ; il peut se résorber comme marquer le début d'une tendance."),
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

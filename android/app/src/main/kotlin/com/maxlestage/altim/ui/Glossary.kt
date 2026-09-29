package com.maxlestage.altim.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Words used in Altim, in plain French (same list as the site and the iPhone). */
val GLOSSARY = listOf(
    "Signal (ACHAT, ATTENDRE, VENTE)" to "Vote pondéré de 7 indicateurs techniques sur les bougies clôturées : ACHAT au-dessus de +25, VENTE sous −25. Une probabilité mesurée sur l'historique, jamais une certitude.",
    "Confiance" to "Force du score multipliée par l'accord entre indicateurs et unités de temps. 60 % ne veut pas dire « 60 % de chances de gagner ».",
    "Preuve du modèle" to "Ce que dit la validation du signal sur 34 actifs fixés d'avance (écran « Validation du modèle ») pour la classe de l'actif (actions, Bitcoin, Ethereum, altcoins) et le régime de marché où il se trouve : gain moyen par trade, statistique t, nombre d'actifs où le signal a battu la simple détention. Sans avantage démontré, la carte Décision le dit ; si la classe perd en moyenne ou si la détention a fait mieux sur plus des deux tiers de ses actifs, la confiance est plafonnée à 60 et ACHAT FORT devient ACHAT. La validation n'augmente jamais la confiance ; le passé ne garantit rien.",
    "Zone d'achat (Fibonacci)" to "Entre 38,2 % et 65 % de repli du dernier mouvement haussier : là où un repli s'arrête souvent. La « zone d'or » va de 61,8 % à 65 %. Cassée si le prix passe sous le plus bas du mouvement.",
    "Stop (stop-loss)" to "Prix auquel on vend pour limiter la perte si l'idée ne marche pas. Altim le place à 2 × ATR sous le prix, ou sous le plus bas qui invalide la zone d'achat.",
    "ATR" to "Average True Range : l'amplitude moyenne d'une bougie. Un stop à 2 × ATR laisse respirer le prix sans être touché par le bruit ordinaire.",
    "Rapport gain/risque (R)" to "Ce que rapporte l'objectif divisé par ce que coûte le stop. 2 R : on vise deux fois ce qu'on risque. Sous 1,5, l'idée rapporte peu.",
    "RSI" to "Relative Strength Index (14 bougies) : sous 30, le prix a beaucoup baissé (survente) ; au-dessus de 70, beaucoup monté (surachat).",
    "MACD" to "Écart entre deux moyennes mobiles exponentielles (12 et 26) et sa moyenne (9) : son croisement signale un changement d'élan.",
    "EMA" to "Moyenne mobile exponentielle : moyenne des prix qui donne plus de poids aux plus récents. Prix au-dessus de l'EMA 200 : tendance de fond haussière.",
    "Volatilité" to "Ampleur des variations. Annualisée : l'écart type des variations journalières ramené à un an. Bitcoin ≈ 40–60 %/an, une grande action ≈ 20–30 %/an.",
    "Pire recul (drawdown)" to "Plus forte baisse depuis un sommet sur la période. −50 % : il faut ensuite +100 % pour revenir au sommet.",
    "Corrélation" to "Entre −1 et 1 : proche de 1, deux actifs montent et baissent ensemble (peu de diversification) ; proche de 0, ils sont indépendants.",
    "Consensus de prix" to "Prix médian de plusieurs sources (bourses, courtiers) ; une source qui s'écarte est écartée. La fiabilité baisse quand les sources se contredisent.",
    "Garde-fou marché" to "Risque de choc (volatilité anormale, sauts de prix, VIX…) et de retournement : il réduit ou suspend les conseils d'achat quand le marché est dangereux.",
    "Investissement programmé (DCA)" to "Acheter le même montant à intervalle régulier : on achète plus quand c'est bas, moins quand c'est haut, sans chercher le bon moment.",
    "PFU (flat tax)" to "Prélèvement forfaitaire unique de 30 % sur les plus-values en France (12,8 % d'impôt + 17,2 % de prélèvements sociaux). Les pertes de l'année compensent les gains.",
    "Risque/rendement" to "Ce qu'une position peut rapporter jusqu'à l'objectif comparé à ce qu'elle peut coûter jusqu'au stop. La décision d'Altim exige au moins 2 : viser deux fois ce qu'on risque.",
    "Veto (interdiction d'achat)" to "Condition qui interdit d'acheter même si le reste est favorable : écart achat/vente trop grand, résultats imminents, chute brutale, hausse anormale, gain/risque insuffisant… Toutes sont listées, actives ou non ; « non vérifiable » quand aucune source gratuite ne permet de la contrôler.",
    "Invalidation" to "Niveau ou événement qui rend le scénario faux (ex. clôture sous le plus bas du mouvement). Atteint, on ne garde pas l'idée « en espérant » : on sort ou on n'entre pas.",
    "FDV" to "Fully Diluted Valuation : prix × nombre maximal de jetons. Une capitalisation bien inférieure à la FDV annonce de nouveaux jetons en circulation, donc une pression vendeuse possible.",
    "Profit factor" to "Somme des gains divisée par la somme des pertes des trades passés. Au-dessus de 1, le signal a gagné plus qu'il n'a perdu ; au-dessus de 1,5, c'est solide.",
    "Sharpe" to "Rendement moyen divisé par la volatilité (annualisé) : le rendement obtenu par unité de risque. Au-dessus de 1, c'est bon.",
    "Sortino" to "Comme le Sharpe, mais ne compte que la volatilité des baisses : les fortes hausses ne sont pas pénalisées.",
    "Drawdown" to "Autre nom du pire recul : la plus forte baisse depuis un sommet. Pour un signal, la pire perte cumulée qu'il aurait fallu supporter.",
    "PER et PEG" to "PER : prix de l'action divisé par le bénéfice par action (combien d'années de bénéfices on paie). PEG : PER divisé par la croissance du bénéfice ; vers 1, prix en ligne avec la croissance, bien au-dessus, cher.",
    "EV/EBITDA" to "Valeur d'entreprise (capitalisation + dette nette) divisée par le résultat avant intérêts, impôts et amortissements. Compare des sociétés endettées différemment ; plus il est élevé, plus c'est cher.",
    "P/S (prix ÷ ventes)" to "Capitalisation boursière divisée par le chiffre d'affaires des 12 derniers mois. Utile quand le bénéfice est faible ou négatif ; à comparer entre sociétés d'un même secteur, les marges variant beaucoup d'un secteur à l'autre.",
    "P/B (prix ÷ fonds propres)" to "Capitalisation divisée par les fonds propres inscrits au bilan. Sous 1, la Bourse valorise la société moins que sa valeur comptable ; très élevé chez les sociétés qui rachètent beaucoup leurs actions, sans être un signal en soi.",
    "ROIC" to "Return on Invested Capital : résultat opérationnel après impôt divisé par le capital investi (dette + fonds propres − trésorerie). Mesure ce que rapporte chaque dollar engagé dans l'activité ; durablement au-dessus de 10–15 %, l'entreprise crée de la valeur.",
    "Centile de valorisation" to "Place du ratio actuel (PER ou P/S) dans son propre historique jour par jour, sur 5 ans au plus. 90e centile : plus cher que 90 % des jours de la période. Ne dit pas si l'action va baisser, seulement si elle est chère par rapport à elle-même.",
    "Flux de stablecoins" to "Variation du montant total de stablecoins (USDT, USDC…) en circulation. Une hausse signale de l'argent frais disponible pour acheter des cryptos ; une baisse, de la liquidité qui sort du marché. Indicateur de fond, pas un signal d'achat.",
    "Activité de développement" to "Travail visible sur le code d'un projet : commits, lignes modifiées et pull requests sur son dépôt public (GitHub). Une activité quasi nulle sur une plateforme de contrats est un mauvais signe ; beaucoup de commits ne garantissent pas la valeur du jeton.",
    "TVL" to "Total Value Locked : valeur des cryptos déposées dans un protocole (prêts, échanges). Mesure son usage réel, en dollars.",
    "Funding (financement)" to "Taux payé toutes les 8 h entre acheteurs et vendeurs de contrats perpétuels. Très positif : la foule parie à la hausse avec levier, un repli brutal devient plus probable.",
    "Rééquilibrage" to "Revenir à la répartition choisie (ex. 40 % crypto, 60 % actions) en vendant ce qui a trop monté et en achetant ce qui a pris du retard.",
    "Note (ACHAT FORT … VENTE FORTE)" to "Résumé en 6 niveaux de la carte Décision, tiré du verdict, de son niveau et de la confiance : ACHAT FORT (achat avec signal fort et confiance d'au moins 70), ACHAT, ATTENDRE, ALLÉGER, VENDRE, VENTE FORTE (vente en tendance baissière, confiance d'au moins 70). Un signal dégradé n'a jamais de note d'achat.",
    "Score composite" to "Note de −100 à +100 qui pondère les familles d'indices : technique 32 %, momentum 18 %, fondamentaux 20 %, sentiment 10 %, actualités 10 %, macro 10 % (modifiables dans Réglages). Une famille sans données n'est pas estimée : son poids est réparti sur les autres.",
    "Signal dégradé" to "Les indices se contredisent (au moins 3 familles favorables et 3 défavorables, ou technique et fondamentaux opposés), les données sont peu fiables, ou le signal a perdu de l'argent sur cet actif par le passé. Aucune entrée n'est alors privilégiée.",
    "Risk-on / risk-off" to "Humeur des marchés. Risk-on : stress macro calme et indice de référence en hausse, les investisseurs prennent des risques. Risk-off : stress élevé, VIX au-dessus de 30, ou stress tendu avec un indice en baisse : ils se protègent. Sinon : neutre.",
    "Horizon (scalping … long terme)" to "Durée probable d'un plan, d'après ses bougies et la distance à l'objectif 1 mesurée en ATR : scalping et day trading (dans la journée), swing (quelques jours à semaines), moyen terme (semaines à mois), long terme (plusieurs mois et plus).",
    "Objectif 3" to "Troisième objectif de prix : le prochain niveau touché plusieurs fois au-dessus de l'objectif 2, sans dépasser l'objectif 2 plus l'écart entre les objectifs 1 et 2. Le rapport gain/risque reste calculé sur l'objectif 1.",
    "Ichimoku" to "Indicateur japonais : Tenkan (milieu des 9 dernières bougies), Kijun (26) et un « nuage » tracé à partir de ces lignes et des 52 dernières bougies. Prix au-dessus du nuage : tendance haussière ; dedans : indécision ; dessous : baissière.",
    "Supertrend" to "Ligne qui suit le prix à 3 ATR (10 bougies) de distance et change de côté quand le prix la traverse. Sous le prix : tendance haussière ; au-dessus : baissière.",
    "Canal de Donchian" to "Plus haut et plus bas des 20 dernières bougies. Une clôture au-dessus du plus haut est une cassure haussière, sous le plus bas une cassure baissière.",
    "VWAP" to "Prix moyen pondéré par les volumes. Altim le calcule sur les 20 dernières bougies (glissant), pas depuis l'ouverture de la séance comme les salles de marché : c'est une approximation.",
    "Profil de volume" to "Volume échangé à chaque niveau de prix. Altim l'estime à partir des bougies en répartissant le volume de chacune entre son plus bas et son plus haut : une approximation. Le niveau le plus échangé (POC) et la zone de valeur (70 % du volume) servent souvent d'aimants ou d'appuis.",
    "Points pivots" to "Niveaux calculés sur la séance précédente : pivot P = (plus haut + plus bas + clôture) ÷ 3, puis résistances R1, R2 et supports S1, S2 autour. Très suivis en séance.",
    "Support et résistance" to "Prix où le marché a rebondi (support) ou buté (résistance) plusieurs fois. Altim regroupe les sommets et creux proches (à moins d'un demi-ATR) et ne garde que les niveaux touchés au moins deux fois : plus il y a de contacts, plus le niveau compte.",
    "Cassure et fausse cassure" to "Cassure confirmée : clôture au-delà d'un support ou d'une résistance avec un volume d'au moins 1,5 fois la moyenne de 20 bougies. Fausse cassure : le niveau est franchi, puis le prix revient de l'autre côté en moins de 3 bougies ; souvent un piège pour ceux qui ont suivi.",
    "Structure de marché" to "Suite des sommets et des creux : de plus en plus hauts, la tendance monte ; de plus en plus bas, elle baisse ; sinon, pas de direction.",
    "Force relative" to "Performance de l'actif moins celle de son indice de référence (bitcoin pour une crypto, S&P 500 et Nasdaq-100 pour une action) sur 1, 3 et 6 mois, en points. Positive : l'actif fait mieux que son marché.",
    "Impact potentiel d'une actualité" to "Faible, moyen ou important. « Estimé par règle » : nombre de sources indépendantes, thème (escalade grave, banques centrales, régulation, piratage) et mention de vos actifs. « Mesuré » : variation de l'actif cité depuis la publication, sur ses bougies horaires ; elle ne prouve pas que l'article en est la cause.",
    "Consensus des sources" to "Accord du ton des titres de chaque source sur un même sujet (repérage par mots-clés). Convergent : aucun titre positif face à un négatif ; divergent : les sources se contredisent.",
    "Calendrier de risque" to "Risque de chaque jour des 7 prochains d'après l'Agenda. 🔴 : décision de taux, inflation, emploi ou PIB majeurs, ou résultats d'une action que vous suivez ; 🟠 : autres annonces, résultats des grandes sociétés, dividende ou split d'une action détenue ; 🟢 : aucun événement majeur.",
    "Quand ne pas trader" to "Moments où même un bon signal a peu de chances d'être bien exécuté : volatilité extrême, liquidité insuffisante, écart achat/vente anormal, résultats dans les 5 jours ou publiés depuis 1 à 2 séances, annonce économique dans les 48 h, marché sans direction, signal trop faible (score composite entre −15 et +15 avec une confiance sous 50) ou dégradé, Bourse de New York fermée. Chaque raison vient d'une mesure ; ce qui n'a pas pu être vérifié est dit.",
    "Marché sans direction" to "ADX sous 20 et sommets et creux qui ne montent ni ne baissent : le prix oscille sans tendance. Les signaux de tendance y échouent plus souvent.",
    "Zones d'action" to "Le plan de la carte Décision sur une échelle de prix : niveau d'invalidation (le stop), zone de sortie (autour du stop, à ½ ATR près), zone d'achat, zone d'attente (au-dessus de la zone d'achat, jusqu'à l'objectif 1 : pas d'entrée) et zone de prise de bénéfices (objectifs 1 à 3). « Vous êtes ici » marque le prix actuel.",
    "Scénario en cours" to "Chaque scénario (haussier, neutre, baissier) a ses conditions vérifiées sur les dernières données : clôture au-delà d'un niveau, volume au-dessus de sa moyenne de 20 séances, RSI au-dessus ou sous 50, ADX. Le scénario « en cours » est celui dont la plus grande part des conditions est remplie ; en cas d'égalité, le neutre. Une lecture de l'instant, pas une prévision.",
    "Contre-argument" to "Le nombre de raisons favorables et défavorables de la carte Décision, et ce qui prouverait le scénario faux : cassure d'un support ou d'une résistance, volume qui retombe sous sa moyenne de 20 séances (ou qui la dépasse de 50 % en cas d'attente), événement à venir.",
    "Pourquoi le signal a changé" to "Quand le verdict, le niveau ou la note d'un actif change, Altim compare les mesures de la décision précédente, gardées sur ce téléphone, aux nouvelles : score composite, familles d'indices (écart d'au moins 10 points), volume relatif, RSI, support et résistance les plus proches, ton des actualités.",
    "Open interest" to "Valeur totale des contrats à terme encore ouverts sur une crypto. S'il monte vite : beaucoup de nouvelles positions à effet de levier, le prix peut ensuite bouger plus brutalement.",
    "Liquidation" to "Fermeture forcée par la plateforme d'une position à effet de levier dont la garantie ne suffit plus. Beaucoup de liquidations d'un même côté accélèrent souvent le mouvement, sans en prédire la suite.",
    "Z-score (écart à la moyenne)" to "Distance du prix à sa moyenne de 20 séances, en écarts types de ces 20 clôtures. Au-delà de ±2,5, l'écart est inhabituel ; il peut se résorber comme marquer le début d'une tendance.",
)

/** "Comprendre" card of the settings: the glossary, folded by default. */
@Composable
fun GlossaryCard() {
    var open by rememberSaveable { mutableStateOf(false) }
    Card(title = "Comprendre") {
        Caption("Signal, zone d'achat, stop, volatilité, flat tax… les mots d'Altim expliqués simplement.")
        TextButton(onClick = { open = !open }) { Text(if (open) "Fermer le lexique" else "Ouvrir le lexique", color = AltimColors.cyan) }
        if (open) {
            GLOSSARY.forEach { (term, text) ->
                Column(Modifier.padding(vertical = 4.dp).semantics(mergeDescendants = true) {}) {
                    Text(term, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, color = Color.White)
                    Text(text, fontSize = 13.sp, color = AltimColors.textSecondary)
                }
            }
        }
    }
}

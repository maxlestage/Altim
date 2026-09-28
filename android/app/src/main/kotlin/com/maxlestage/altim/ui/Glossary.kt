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
    "Rééquilibrage" to "Revenir à la répartition choisie (ex. 40 % crypto, 60 % actions) en vendant ce qui a trop monté et en achetant ce qui a pris du retard.",
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

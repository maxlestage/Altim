package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class NoticesTest {
    private fun btc(): Decision =
        AltimJson.decodeFromString(Decision.serializer(), requireNotNull(javaClass.getResource("/fixtures/decision-btc.json")).readText())

    private fun label(v: Verdict, text: String) = ConfigLabel(v, DecisionLevel.MODERATE, text, "Signal modéré")

    private fun transition(symbol: String, at: Double, missing: List<String> = emptyList(), personal: Boolean = false) = ConfigTransition(
        symbol, Kind.CRYPTO, symbol, personal, at, at - 1000, label(Verdict.WAIT, "ATTENDRE"), label(Verdict.BUY_ZONE, "ZONE D'ACHAT"),
        missing, triggers = emptyList(),
    )

    @Test fun transitionsAlreadyListedAreABaselineThenOnlyNewOnesAreNotifiedOnce() {
        val first = ConfigChanges.apply(ConfigState(), btc(), false, 1000.0).state
        val old = ConfigChanges.apply(first, btc().copy(verdict = Verdict.SELL, label = "VENDRE"), false, 2000.0).state
        val notified = ConfigNotices.baseline(old.transitions)
        val next = ConfigChanges.apply(old, btc().copy(verdict = Verdict.BUY_ZONE, label = "ZONE D'ACHAT"), false, 3000.0)
        val t = assertNotNull(next.transition)
        val (keys, fresh) = ConfigNotices.fresh(notified, next.state.transitions)
        assertEquals(listOf(t), fresh)
        // Same transitions a quarter of an hour later: nothing new.
        assertTrue(ConfigNotices.fresh(keys, next.state.transitions).second.isEmpty())
        // Stored and read back.
        assertEquals(keys, ConfigNotices.parse(ConfigNotices.encode(keys)))
        assertNull(ConfigNotices.parse("{broken"))
        // The single notification: the card's title and lines, a tap opens the asset.
        val n = assertNotNull(ConfigNotices.notice(fresh))
        assertEquals("🚨 BTC — changement de configuration : VENDRE → ZONE D'ACHAT", n.title)
        assertEquals("crypto:BTC", n.asset)
        assertTrue(n.body.startsWith("Pourquoi le signal a changé : ") || n.body.startsWith("Conditions manquantes : "), n.body)
        if (t.missing.isNotEmpty()) assertTrue(n.body.contains("Conditions manquantes : ${t.missing.joinToString(" ; ")}."))
        if (t.triggers.isNotEmpty()) assertTrue(n.body.contains("Ce qui changerait la décision : ${t.triggers.joinToString(" ; ")}."))
    }

    @Test fun personalAndMarketTransitionsHaveTheirOwnKeys() {
        val a = transition("BTC", 5000.0)
        val b = a.copy(personal = true)
        assertTrue(ConfigNotices.key(a) != ConfigNotices.key(b))
        assertEquals("crypto:BTC:i:5000", ConfigNotices.key(a))
    }

    @Test fun severalTransitionsMakeOneGroupedNotification() {
        assertNull(ConfigNotices.notice(emptyList()))
        val list = (1..7).map { transition("A$it", it * 1000.0, missing = if (it == 1) listOf("Cassure de la résistance : pas encore") else emptyList()) }
        val n = assertNotNull(ConfigNotices.notice(list))
        assertEquals("🚨 7 changements de configuration", n.title)
        assertNull(n.asset)
        val lines = n.body.lines()
        assertEquals(6, lines.size)
        assertEquals("A1 : ATTENDRE → ZONE D'ACHAT — Conditions manquantes : Cassure de la résistance : pas encore.", lines[0])
        assertEquals("A2 : ATTENDRE → ZONE D'ACHAT", lines[1])
        assertEquals("… et 2 autre(s) : détail dans le Radar.", lines[5])
    }

    @Test fun aTransitionWithoutDetailsTellsTheLevels() {
        val n = assertNotNull(ConfigNotices.notice(listOf(transition("ETH", 1.0, personal = true))))
        assertEquals("Niveau Signal modéré → Signal modéré.\nMode personnel.", n.body)
    }

    private fun danger(id: String, symbol: String, vararg codes: String) =
        Danger(id, symbol, Kind.STOCK, symbol, codes.map { DangerReason(it, "Texte $it.") })

    @Test fun aLineIsNotifiedWhenItEntersDangerNotWhileItStaysThere() {
        val (s1, f1) = DangerNotices.fresh(null, listOf(danger("h1", "AAPL", "near_stop")))
        assertEquals(listOf("h1"), f1.map { it.id })
        // Still near its stop: no repeat.
        val (s2, f2) = DangerNotices.fresh(s1, listOf(danger("h1", "AAPL", "near_stop")))
        assertTrue(f2.isEmpty())
        // A new reason (stop broken): told once.
        val (s3, f3) = DangerNotices.fresh(s2, listOf(danger("h1", "AAPL", "stop_broken", "loss_over_risk")))
        assertEquals(1, f3.size)
        // Back to a reason already told: nothing.
        val (s4, f4) = DangerNotices.fresh(s3, listOf(danger("h1", "AAPL", "loss_over_risk")))
        assertTrue(f4.isEmpty())
        // Out of danger, then in again: told again.
        val (s5, f5) = DangerNotices.fresh(s4, emptyList())
        assertTrue(f5.isEmpty() && s5.isEmpty())
        assertEquals(1, DangerNotices.fresh(s5, listOf(danger("h1", "AAPL", "near_stop"))).second.size)
        assertEquals(s3, DangerNotices.parse(DangerNotices.encode(s3)))
        // Candles missing this time (near_stop not measurable): the line keeps its codes, no repeat afterwards.
        val (u1, uf) = DangerNotices.fresh(mapOf("h1" to listOf("near_stop")), emptyList(), unmeasured = setOf("h1"))
        assertTrue(uf.isEmpty())
        assertEquals(mapOf("h1" to listOf("near_stop")), u1)
        assertTrue(DangerNotices.fresh(u1, listOf(danger("h1", "AAPL", "near_stop"))).second.isEmpty())
        assertNull(DangerNotices.parse("[1"))
    }

    @Test fun dangerTextsAreTheWebOnes() {
        val one = assertNotNull(DangerNotices.notice(listOf(danger("h1", "AAPL", "stop_broken", "loss_over_risk"))))
        assertEquals("⚠ Position devenue dangereuse dans vos avoirs", one.title)
        assertEquals("AAPL : Texte stop_broken. Texte loss_over_risk.", one.body)
        assertEquals("stock:AAPL", one.asset)
        val two = assertNotNull(DangerNotices.notice(listOf(danger("h1", "AAPL", "near_stop"), danger("h2", "MSFT", "near_stop"))))
        assertEquals("⚠ Positions devenues dangereuses dans vos avoirs", two.title)
        assertEquals("AAPL : Texte near_stop.\nMSFT : Texte near_stop.", two.body)
        assertNull(two.asset)
        assertNull(DangerNotices.notice(emptyList()))
    }

    @Test fun realDangersFromThePortfolioRiskRule() {
        val h = Holding(asset = Asset("AAPL", Kind.STOCK, "Apple"), quantity = 10.0, averagePrice = 200.0, stop = 150.0)
        val p = RiskPortfolio.of(listOf(h), 0.0, mapOf("stock:AAPL" to 140.0), emptyMap())
        val dangers = PortfolioRisk.dangerousPositions(p, RiskSettings.DEFAULT, emptyMap(), mapOf(h.id to h.stop))
        val (_, fresh) = DangerNotices.fresh(null, dangers)
        val n = assertNotNull(DangerNotices.notice(fresh))
        assertTrue(n.body.startsWith("AAPL : Stop cassé : cours 140"), n.body)
    }
}

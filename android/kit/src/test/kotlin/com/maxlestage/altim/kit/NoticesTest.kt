package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/** Background notifications of the configuration changes and dangerous positions: same cases as iOS ChangeNoticesTests. */
class NoticesTest {
    private fun btc(): Decision =
        AltimJson.decodeFromString(Decision.serializer(), requireNotNull(javaClass.getResource("/fixtures/decision-btc.json")).readText())

    private fun transition(d: Decision, symbol: String? = null, at: Double = 2000.0): ConfigTransition {
        val first = if (symbol != null) d.copy(symbol = symbol) else d
        val next = first.copy(verdict = Verdict.BUY_ZONE, label = "ZONE D'ACHAT")
        val s = ConfigChanges.apply(ConfigState(), first, false, at - 1000).state
        return assertNotNull(ConfigChanges.apply(s, next, false, at).transition)
    }

    // ---------- Configuration changes ----------

    @Test fun oneChangeNotifiedOnceWithTheCardWording() {
        val d = btc()
        val t = transition(d)
        val (n, state) = ChangeNotices.configNotice(listOf(t), ChangeNoticeState())
        assertNotNull(n)
        assertEquals("🚨 BTC — changement de configuration : ${d.label} → ZONE D'ACHAT", n.title)
        assertEquals("crypto:BTC", n.asset)
        val lines = n.body.split("\n")
        assertEquals("Conditions manquantes : ${t.missing.joinToString(" ; ")}.", lines.firstOrNull { it.startsWith("Conditions manquantes") })
        assertEquals("Ce qui changerait la décision : ${t.triggers.joinToString(" ; ")}.", lines.firstOrNull { it.startsWith("Ce qui changerait") })
        assertEquals(ChangeNotices.DISCLAIMER, lines.last())
        assertEquals(listOf(ChangeNotices.id(t)), state.configNotified)
        // Same transition again (another check, or found twice): no second notification.
        assertNull(ChangeNotices.configNotice(listOf(t), state).first)
        assertNull(ChangeNotices.configNotice(emptyList(), state).first)
    }

    @Test fun whyItChangedAndFallback() {
        var t = transition(btc()).copy(changes = listOf("Momentum −18 pts (+40 → +22)"), missing = emptyList(), triggers = emptyList())
        assertEquals("Pourquoi le signal a changé : Momentum −18 pts (+40 → +22).\n${ChangeNotices.DISCLAIMER}", ChangeNotices.configText(listOf(t)).body)
        t = t.copy(changes = emptyList())
        assertEquals("Niveau ${t.from.levelLabel} → ${t.to.levelLabel}.\n${ChangeNotices.DISCLAIMER}", ChangeNotices.configText(listOf(t)).body)
    }

    @Test fun severalChangesGrouped() {
        val d = btc()
        val list = listOf("BTC", "ETH", "SOL", "ADA", "XRP", "DOT", "LINK").mapIndexed { i, s -> transition(d, s, 2000.0 + i) }
        val (n, state) = ChangeNotices.configNotice(list, ChangeNoticeState())
        assertNotNull(n)
        assertEquals("🚨 Changements de configuration · 7", n.title)
        assertNull(n.asset)
        val lines = n.body.split("\n")
        // Newest first, 5 listed, the others counted.
        assertTrue(lines[0].startsWith("LINK : ${d.label} → ZONE D'ACHAT (conditions manquantes : "), lines[0])
        assertEquals("+ 2 autre(s) : détail sur le Radar.", lines[5])
        assertEquals(7, state.configNotified.size)
        // One already notified is left out; only the new one is told.
        val more = transition(d, "AVAX", 3000.0)
        val again = ChangeNotices.configNotice(list + more, state).first
        assertEquals(more.title, again?.title)
        assertEquals("crypto:AVAX", again?.asset)
    }

    @Test fun rememberedIdsAreBounded() {
        val s = ChangeNoticeState(configNotified = (0 until ChangeNotices.MAX_REMEMBERED).map { "old$it" })
        val t = transition(btc())
        val (_, state) = ChangeNotices.configNotice(listOf(t), s)
        assertEquals(ChangeNotices.MAX_REMEMBERED, state.configNotified.size)
        assertEquals(ChangeNotices.id(t), state.configNotified.last())
        assertEquals("old1", state.configNotified.first())
    }

    // ---------- Dangerous positions ----------

    private fun danger(id: String, symbol: String, vararg codes: String) =
        Danger(id, symbol, Kind.CRYPTO, symbol, codes.map { DangerReason(it, "Raison $it.") })

    @Test fun newDangerNotifiedOnce() {
        val d = danger("L1", "BTC", "stop_broken")
        val (n, s1) = ChangeNotices.dangerNotice(listOf(d), ChangeNoticeState(), 1000.0)
        assertNotNull(n)
        assertEquals("⚠ Position devenue dangereuse : BTC", n.title)
        assertEquals("BTC : stop cassé.\n${ChangeNotices.DANGER_DETAIL}\n${ChangeNotices.DANGER_ADVICE}", n.body)
        assertEquals("crypto:BTC", n.asset)
        assertEquals(listOf("L1:stop_broken"), s1.dangerActive)
        // Still dangerous at the next checks: nothing more.
        val (again, s2) = ChangeNotices.dangerNotice(listOf(d), s1, 2000.0)
        assertNull(again)
        // Gone, then back within 24 hours: not repeated; after 24 hours: notified again.
        val gone = ChangeNotices.dangerNotice(emptyList(), s2, 3000.0).second
        assertEquals(emptyList(), gone.dangerActive)
        assertNull(ChangeNotices.dangerNotice(listOf(d), gone, 4000.0).first)
        val later = ChangeNotices.dangerNotice(emptyList(), ChangeNotices.dangerNotice(listOf(d), gone, 4000.0).second, 5000.0).second
        assertNotNull(ChangeNotices.dangerNotice(listOf(d), later, 1000 + ChangeNotices.DANGER_COOLDOWN).first)
    }

    @Test fun newReasonOfALineAndGrouping() {
        val first = ChangeNotices.dangerNotice(listOf(danger("L1", "BTC", "near_stop")), ChangeNoticeState(), 0.0).second
        // Near the stop → stop broken: a new state, told with all the reasons of the line; ETH enters too.
        val (n, s) = ChangeNotices.dangerNotice(listOf(danger("L1", "BTC", "stop_broken", "loss_over_risk"), danger("L2", "ETH", "loss_over_risk")), first, 10.0)
        assertNotNull(n)
        assertEquals("⚠ Positions devenues dangereuses : BTC, ETH", n.title)
        assertEquals(
            "BTC : stop cassé ; perte au-delà de votre risque par idée.\nETH : perte au-delà de votre risque par idée.\n${ChangeNotices.DANGER_DETAIL}\n${ChangeNotices.DANGER_ADVICE}",
            n.body,
        )
        assertFalse(n.body.contains("$"), "no amount in a notification")
        assertNull(n.asset)
        // A line still dangerous for an old reason only is not listed.
        val next = ChangeNotices.dangerNotice(listOf(danger("L1", "BTC", "stop_broken"), danger("L2", "ETH", "loss_over_risk"), danger("L3", "SOL", "near_stop")), s, 20.0)
        assertEquals("⚠ Position devenue dangereuse : SOL", next.first?.title)
    }

    @Test fun baselineAndUnknownNearStop() {
        val d = danger("L1", "BTC", "near_stop")
        // Never measured here, but already shown on Mes avoirs: not notified.
        val seen = ChangeNotices.dangerNotice(listOf(d), ChangeNoticeState(), 0.0, baseline = listOf("L1:near_stop"))
        assertNull(seen.first)
        // Candles unreadable: the near-stop state is kept, so coming back readable does not notify it again.
        val unknown = ChangeNotices.dangerNotice(emptyList(), seen.second, 10.0, nearStopUnknown = setOf("L1"))
        assertEquals(listOf("L1:near_stop"), unknown.second.dangerActive)
        assertNull(ChangeNotices.dangerNotice(listOf(d), unknown.second, 20 + ChangeNotices.DANGER_COOLDOWN).first)
        assertEquals(listOf("A:stop_broken", "A:loss_over_risk"), ChangeNotices.dangerKeys(listOf(danger("A", "BTC", "stop_broken", "loss_over_risk"))))
    }

    @Test fun stateStorage() {
        val s = ChangeNoticeState(listOf("a"), listOf("L1:near_stop"), mapOf("L1:near_stop" to 5.0))
        assertEquals(s, ChangeNotices.parse(ChangeNotices.encode(s)))
        val damaged = ChangeNotices.parse("""{"configNotified":3,"dangerActive":["x"],"dangerNotified":"no"}""")
        assertEquals(emptyList(), damaged.configNotified)
        assertEquals(listOf("x"), damaged.dangerActive)
        assertEquals(emptyMap(), damaged.dangerNotified)
        assertNull(ChangeNotices.parse("{}").dangerActive)
        assertEquals(ChangeNoticeState(), ChangeNotices.parse("{broken"))
    }
}

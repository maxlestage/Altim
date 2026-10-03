package com.maxlestage.altim.kit

import kotlin.test.Test
import kotlin.test.assertEquals

/** `JsFormat.fr` against the browser's `toLocaleString("fr-FR")`: the vectors of iOS JSFormatTests. */
class JsFormatTest {
    @Test
    fun roundsTheShortestDecimalLikeIntl() {
        assertEquals("86,6", JsFormat.fr(86.55, max = 1))
        assertEquals("-86,6", JsFormat.fr(9.01 - 95.56, max = 1))
        assertEquals("1,01", JsFormat.fr(1.005, max = 2))
        assertEquals("0,13", JsFormat.fr(0.125, max = 2))
        assertEquals("2,68", JsFormat.fr(2.675, max = 2))
    }

    @Test
    fun verySmallAndVeryLargeNumbers() {
        assertEquals("0,00001", JsFormat.fr(0.00001, max = 6))
        assertEquals("250\u202F000\u202F000\u202F000\u202F000\u202F000\u202F000", JsFormat.fr(2.5e20, max = 0))
        assertEquals("1\u202F234\u202F567,89", JsFormat.fr(1_234_567.891, max = 2))
    }

    @Test
    fun minimumDigitsAndZero() {
        assertEquals("212,40", JsFormat.fr(212.4, min = 2, max = 2))
        assertEquals("3", JsFormat.fr(3.0, max = 2))
        assertEquals("0", JsFormat.fr(-0.001, max = 1))
        assertEquals("1\u202F000", JsFormat.fr(999.96, max = 1))
    }
}

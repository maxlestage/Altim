package com.maxlestage.altim.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import com.maxlestage.altim.kit.Tone

/** Altim's "neon" identity, same colours as the iPhone app. */
object AltimColors {
    val background = Color(0xFF050812)
    val surface = Color(0xFF0F1426)
    val cyan = Color(0xFF00F0FF)
    val magenta = Color(0xFFFF2BD6)
    val violet = Color(0xFF7D4DFF)
    val buy = Color(0xFF38FF87)
    val sell = Color(0xFFFF3B5C)
    val warning = Color(0xFFFFC733)
    val textSecondary = Color.White.copy(alpha = 0.6f)

    // Allocation: categorical palette validated (dark mode, colorblind readers).
    val allocCrypto = Color(0xFF3987E5)
    val allocStock = Color(0xFFD95926)

    val accentGradient = Brush.horizontalGradient(listOf(cyan, violet, magenta))

    fun of(tone: Tone): Color = when (tone) {
        Tone.GOOD -> buy
        Tone.WARN -> warning
        Tone.BAD -> sell
        Tone.NEUTRAL -> cyan
    }

    fun forChange(change: Double?): Color = if ((change ?: 0.0) >= 0) buy else sell
}

fun mono(size: TextUnit = 14.sp, weight: FontWeight = FontWeight.SemiBold) =
    TextStyle(fontFamily = FontFamily.Monospace, fontWeight = weight, fontSize = size)

@Composable
fun AltimTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = AltimColors.cyan,
            onPrimary = Color.Black,
            secondary = AltimColors.violet,
            tertiary = AltimColors.magenta,
            background = AltimColors.background,
            surface = AltimColors.background,
            surfaceContainer = AltimColors.surface,
            surfaceContainerHigh = AltimColors.surface,
            onBackground = Color.White,
            onSurface = Color.White,
            error = AltimColors.sell,
        ),
    ) {
        // White text by default (without it, Text outside a Surface is black on the dark background).
        CompositionLocalProvider(LocalContentColor provides Color.White, content = content)
    }
}

/** Dark background with halos (static: no animation that drains the battery). */
@Composable
fun AppBackground(modifier: Modifier = Modifier, content: @Composable BoxScope.() -> Unit) {
    Box(
        modifier
            .fillMaxSize()
            .background(AltimColors.background)
            .background(Brush.radialGradient(listOf(AltimColors.violet.copy(alpha = 0.28f), Color.Transparent), radius = 1100f))
            .background(
                Brush.radialGradient(
                    listOf(AltimColors.magenta.copy(alpha = 0.10f), Color.Transparent),
                    center = androidx.compose.ui.geometry.Offset(Float.POSITIVE_INFINITY, Float.POSITIVE_INFINITY),
                    radius = 1000f,
                ),
            ),
        content = content,
    )
}

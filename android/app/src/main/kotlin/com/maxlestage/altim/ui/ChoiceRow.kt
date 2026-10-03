package com.maxlestage.altim.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.isSpecified

/** What a segment adds around its label: 12 dp of padding on each side, its border, a little margin. */
private val SEGMENT_CHROME = 30.dp

/**
 * A choice among a few options that never runs past the screen: segmented buttons while every label fits its equal
 * share of the width at the current font scale, otherwise chips that wrap onto several lines (320 dp, font scale up
 * to 2). Nothing scrolls sideways and no label is cut.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun <T> ChoiceRow(
    options: List<Pair<T, String>>,
    selected: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    description: String? = null,
    fontSize: TextUnit = TextUnit.Unspecified,
) {
    val measurer = rememberTextMeasurer()
    val base = MaterialTheme.typography.labelLarge
    val style = if (fontSize.isSpecified) base.copy(fontSize = fontSize) else base
    val density = LocalDensity.current
    val described = if (description != null) Modifier.semantics { contentDescription = description } else Modifier
    BoxWithConstraints(modifier.fillMaxWidth().then(described)) {
        val widest = options.maxOfOrNull { measurer.measure(it.second, style, softWrap = false, maxLines = 1).size.width } ?: 0
        val segment = with(density) { widest.toDp() } + SEGMENT_CHROME
        if (segment * options.size <= maxWidth) {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                options.forEachIndexed { i, (value, label) ->
                    SegmentedButton(
                        selected = selected == value,
                        onClick = { onSelect(value) },
                        shape = SegmentedButtonDefaults.itemShape(i, options.size),
                        colors = SegmentedButtonDefaults.colors(activeContainerColor = AltimColors.cyan.copy(alpha = 0.2f), activeContentColor = AltimColors.cyan),
                        icon = {},
                    ) { Text(label, fontSize = fontSize, maxLines = 1, softWrap = false) }
                }
            }
        } else {
            FlowRow(
                Modifier.fillMaxWidth().selectableGroup(),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                options.forEach { (value, label) ->
                    FilterChip(
                        selected = selected == value,
                        onClick = { onSelect(value) },
                        label = { Text(label, fontSize = fontSize) },
                        colors = FilterChipDefaults.filterChipColors(selectedContainerColor = AltimColors.cyan.copy(alpha = 0.2f), selectedLabelColor = AltimColors.cyan),
                    )
                }
            }
        }
    }
}

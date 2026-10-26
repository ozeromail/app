package com.ozeromail.app

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * How much room the window gives. On its side the controls move from the bottom bars to
 * [rails] at the edges, where the thumbs hold the phone, and the height goes to the mail.
 * A window large both ways, a tablet or an open foldable, shows the list and the message
 * side by side: [split].
 */
@Immutable
data class Room(val rails: Boolean = false, val split: Boolean = false)

val LocalRoom = staticCompositionLocalOf { Room() }

/** Measures the window once for every screen in [content]. */
@Composable
fun ProvideRoom(content: @Composable () -> Unit) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val room = Room(
            rails = maxWidth > maxHeight && maxWidth >= 560.dp,
            split = maxWidth >= 840.dp && maxHeight >= 480.dp,
        )
        CompositionLocalProvider(LocalRoom provides room, content = content)
    }
}

/** How wide the list gets on a wide screen, so the rows stay easy to scan. */
val LIST_MAX = 640.dp

/** How wide a message gets: a newsletter made for 600 px fits at its own size. */
val LETTER_MAX = 720.dp

/** How wide the fields of a message being written get. */
val COMPOSE_MAX = 680.dp

/** How wide the list is beside a message. */
val LIST_PANE = 380.dp

/** The left rail with the inbox tabs, and the narrow one with buttons only. */
val TABS_RAIL = 172.dp
val RAIL = 84.dp

/** Keeps [content] to [max] wide, unless it is unspecified, in the middle of the room it has. */
@Composable
fun Middle(max: Dp, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Box(modifier, contentAlignment = Alignment.TopCenter) {
        Box(Modifier.fillMaxHeight().widthIn(max = max).fillMaxWidth()) { content() }
    }
}

/**
 * A column of controls along the [start] or end edge, held low where the thumb rests: in
 * landscape the bottom bars move here.
 */
@Composable
fun Rail(start: Boolean, width: Dp = RAIL, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    val colors = MaterialTheme.colorScheme
    val line = colors.outlineVariant
    Column(
        modifier
            .fillMaxHeight()
            .width(width)
            .background(colors.bar)
            .drawBehind {
                val x = if (start) size.width else 0f
                drawLine(line, Offset(x, 0f), Offset(x, size.height), 1.dp.toPx())
            }
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Vertical))
            .padding(horizontal = 10.dp, vertical = Gap.barBottom),
        verticalArrangement = Arrangement.spacedBy(Gap.inBar, Alignment.Bottom),
        horizontalAlignment = Alignment.CenterHorizontally,
        content = content,
    )
}

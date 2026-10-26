package com.ozeromail.app

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * How much room the window gives. A phone on its side is [sideways]: its height is short. A
 * window [wide] enough, sideways or a tablet or an open foldable, shows the mailboxes beside the
 * list, and the list beside an open message. A [short] window, a phone on its side, keeps every
 * bar and title tight, since its height is what the mail needs most.
 */
@Immutable
data class Room(val sideways: Boolean = false, val wide: Boolean = false, val short: Boolean = false)

val LocalRoom = staticCompositionLocalOf { Room() }

/** Measures the window once for every screen in [content]. */
@Composable
fun ProvideRoom(content: @Composable () -> Unit) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val sideways = maxWidth > maxHeight && maxWidth >= 560.dp
        val short = maxHeight < 480.dp
        val room = Room(sideways, wide = sideways || (maxWidth >= 840.dp && !short), short = sideways && short)
        CompositionLocalProvider(LocalRoom provides room, content = content)
    }
}

/** How wide the list gets on a wide screen, so the rows stay easy to scan. */
val LIST_MAX = 640.dp

/** How wide a message gets: a newsletter made for 600 px fits at its own size. */
val LETTER_MAX = 720.dp

/** How wide the fields of a message being written get. */
val COMPOSE_MAX = 680.dp

/** The mailboxes beside the list. */
val MAILBOXES = 200.dp

/** The list beside an open message. */
val LIST_PANE = 320.dp

/** Keeps [content] to [max] wide, in the middle of the room it has. */
@Composable
fun Middle(max: Dp, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Box(modifier, contentAlignment = Alignment.TopCenter) {
        Box(Modifier.fillMaxHeight().widthIn(max = max).fillMaxWidth()) { content() }
    }
}

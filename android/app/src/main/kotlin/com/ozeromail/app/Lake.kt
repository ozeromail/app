package com.ozeromail.app

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

/**
 * The note of a list with nothing in it: a small still lake over its title, and under it when it
 * was [checkedAt]. While there are [checking] words, as the first mail is on its way, they take
 * the place of the title and say what it waits for. [grey] water is for plain folders. [mark] is
 * how much of the lake shows; [wordsModifier] moves the words alone, while the lake stays.
 */
@Composable
fun StillNote(
    title: String,
    grey: Boolean = false,
    checking: Pair<String, String?>? = null,
    checkedAt: Long? = null,
    mark: () -> Float = { 1f },
    wordsModifier: Modifier = Modifier,
) {
    val known by animateFloatAsState(if (checking != null) 0f else 1f, Motion.calm(WORDS_MS), label = "words")
    val waitWords = rememberLast(checking)
    val fading by remember { derivedStateOf { known < 1f } }
    Column(
        Modifier.fillMaxWidth().padding(top = 56.dp, bottom = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        LakeMark(if (grey) MaterialTheme.colorScheme.outline else MaterialTheme.colorScheme.accent, Modifier.graphicsLayer { alpha = mark() })
        Box(wordsModifier, contentAlignment = Alignment.TopCenter) {
            Words(title, null, Modifier.graphicsLayer { alpha = known }) { checkedAt?.let { Checked(it) } }
            // Until then the list is not empty yet, only on its way: these words take its place.
            if (fading) {
                waitWords?.let { (title, text) -> Words(title, text, Modifier.graphicsLayer { alpha = 1f - known }) }
            }
        }
    }
}

/** Still water, drawn small: a horizon fading out at both ends, and its reflection under it. */
@Composable
private fun LakeMark(tint: Color, modifier: Modifier) {
    Canvas(modifier.size(72.dp, 18.dp)) {
        val unit = size.width / 72f
        LINES.forEachIndexed { i, (width, alpha) ->
            val w = width * unit
            val left = (size.width - w) / 2
            drawRect(
                Brush.horizontalGradient(listOf(Color.Transparent, tint.copy(alpha = alpha), Color.Transparent), startX = left, endX = left + w),
                topLeft = Offset(left, (1f + i * 7f) * unit),
                size = Size(w, 1.5f * unit),
            )
        }
    }
}

/** The lake's lines, top down: width in 72ths of it, and how strong. */
private val LINES = listOf(72f to 0.9f, 48f to 0.35f, 28f to 0.18f)

/** When the list was checked [at], ms on the clock: just now, then the time. */
@Composable
private fun Checked(at: Long) {
    val context = LocalContext.current
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(at) {
        now = System.currentTimeMillis()
        delay(at + CHECKED_NOW_MS - now)
        now = System.currentTimeMillis()
    }
    Text(
        if (now - at < CHECKED_NOW_MS) stringResource(R.string.lake_checked_now)
        else stringResource(R.string.lake_checked_at, shortDate(context, at / 1000)),
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        style = Type.Caption,
        modifier = Modifier.padding(top = 4.dp),
    )
}

/** Within this long a check is "just now". */
private const val CHECKED_NOW_MS = 60_000L

/** How long the words take to change. */
private const val WORDS_MS = 400

/** An empty list's title and the line under it, and [more] under them. */
@Composable
private fun Words(title: String, text: String?, modifier: Modifier, more: @Composable () -> Unit = {}) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(title, style = Type.Heading, textAlign = TextAlign.Center)
        // A second line only when it tells something: a slow wait.
        text?.let {
            Text(
                it,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.widthIn(max = 280.dp).padding(horizontal = 32.dp),
            )
        }
        more()
    }
}

package com.ozeromail.app

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerId
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch

/** How long a finger rests on a row before it starts choosing. */
private const val HOLD_MS = 350L

/** The ring before a row while choosing. */
val MARK = 22.dp

/**
 * How far a row's text moves over while choosing: the mark takes the text's place at the
 * screen's edge, in line with the title, and the text follows after a gap.
 */
val MARK_ROOM = MARK + 12.dp

/**
 * One stroke over the list: rows between where it began and the finger take the state the
 * first row took; every other row keeps what it had when the stroke began.
 */
private class Stroke(val rows: List<Long>, val from: Int, val before: Set<Long>) {
    val add = rows[from] !in before

    /** In the order the stroke passed the rows, so the last one is where the finger is. */
    fun to(at: Int): Set<Long> {
        val range = rows.subList(minOf(from, at), maxOf(from, at) + 1).let { if (at < from) it.reversed() else it }
        return if (add) before + range else before - range.toSet()
    }
}

/**
 * Choosing messages in a list of them, as on iOS: two fingers drawn over it, or one finger
 * held still on a row and then drawn. Either takes the list from scrolling and the rows from
 * swipes and taps until the fingers lift. [rows] are the messages' ids in order, as the list
 * keys them; [onHold] gives feedback when a held finger starts choosing.
 */
fun Modifier.chooseRows(
    list: LazyListState,
    rows: () -> List<Long>,
    chosen: () -> Set<Long>,
    onChoose: (Set<Long>) -> Unit,
    onHold: () -> Unit,
): Modifier = pointerInput(list) {
    val slop = viewConfiguration.touchSlop
    val edge = EDGE.toPx()

    /**
     * The row under [y], or unless [exact] the nearest one: a stroke may pass the title or
     * the edge.
     */
    fun rowAt(y: Float, rows: List<Long>, exact: Boolean = false): Int? {
        val info = list.layoutInfo
        val at = y + info.viewportStartOffset
        val (row, off) = info.visibleItemsInfo
            .filter { it.key is Long }
            .map { it to if (at < it.offset) it.offset - at else maxOf(0f, at - it.offset - it.size) }
            .minByOrNull { it.second }
            ?: return null
        if (exact && off > 0f) return null
        return rows.indexOf(row.key as Long).takeIf { it >= 0 }
    }

    fun middle(fingers: List<PointerInputChange>) = fingers.sumOf { it.position.y.toDouble() }.toFloat() / fingers.size

    coroutineScope {
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            // Where two fingers began, while they have not moved far enough to choose.
            var twoFrom: Float? = null
            var stroke: Stroke? = null
            var fingers: Set<PointerId> = emptySet()
            while (stroke == null) {
                // Timed by the touch events themselves, so a late frame cannot lose the hold.
                val left = HOLD_MS - (currentEvent.changes.first().uptimeMillis - down.uptimeMillis)
                val event = when {
                    twoFrom != null -> awaitPointerEvent(PointerEventPass.Initial)
                    left <= 0 -> null
                    else -> withTimeoutOrNull(left) { awaitPointerEvent(PointerEventPass.Initial) }
                }
                if (event == null) {
                    // Held still on one row: it is chosen, and the same finger can go on.
                    val ids = rows()
                    val from = rowAt(down.position.y, ids, exact = true) ?: return@awaitEachGesture
                    stroke = Stroke(ids, from, chosen())
                    fingers = setOf(down.id)
                    onHold()
                    break
                }
                val pressed = event.changes.filter { it.pressed }
                if (pressed.isEmpty()) return@awaitEachGesture
                if (pressed.size >= 2) {
                    // Two fingers never scroll the list: they are for choosing.
                    event.changes.forEach { it.consume() }
                    val y = middle(pressed)
                    val start = twoFrom ?: y.also { twoFrom = it }
                    if (abs(y - start) > slop) {
                        val ids = rows()
                        val from = rowAt(start, ids) ?: return@awaitEachGesture
                        stroke = Stroke(ids, from, chosen())
                        fingers = pressed.map { it.id }.toSet()
                    }
                } else if (twoFrom != null || (pressed[0].position - down.position).getDistance() > slop) {
                    // A scroll or a swipe, not a hold; or one of two fingers lifted before choosing.
                    return@awaitEachGesture
                }
            }
            val drawn = stroke ?: return@awaitEachGesture
            onChoose(drawn.to(drawn.from))
            var y = down.position.y
            // Near the top or bottom edge the list scrolls on by itself, so the stroke goes on.
            val scrolling = launch {
                while (true) {
                    withFrameMillis { }
                    // The edges of what shows between the status bar and the dock.
                    val top = list.layoutInfo.beforeContentPadding + edge
                    val bottom = size.height - list.layoutInfo.afterContentPadding - edge
                    val by = when {
                        y < top -> -(top - y) / 3
                        y > bottom -> (y - bottom) / 3
                        else -> 0f
                    }
                    if (by != 0f && list.scrollBy(by) != 0f) rowAt(y, drawn.rows)?.let { onChoose(drawn.to(it)) }
                }
            }
            // The stroke owns every finger until they all lift; it ends as the first one does.
            while (true) {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                event.changes.forEach { it.consume() }
                val pressed = event.changes.filter { it.pressed && it.id in fingers }
                if (pressed.size < fingers.size) break
                y = middle(pressed)
                rowAt(y, drawn.rows)?.let { onChoose(drawn.to(it)) }
            }
            scrolling.cancel()
            while (currentEvent.changes.any { it.pressed }) {
                awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
            }
        }
    }
}

/** How close to the list's edge a stroke makes it scroll on. */
private val EDGE = 56.dp

/** The mark before a row while choosing: a ring, filled when the row is chosen. */
@Composable
fun ChoiceMark(chosen: Boolean, shown: Float, modifier: Modifier = Modifier) {
    val colors = MaterialTheme.colorScheme
    Box(
        modifier
            .graphicsLayer {
                alpha = shown
                translationX = -8.dp.toPx() * (1 - shown)
                scaleX = 0.7f + 0.3f * shown
                scaleY = scaleX
            }
            .size(MARK)
            .then(
                if (chosen) {
                    Modifier.background(colors.accent, CircleShape)
                } else {
                    Modifier.border(1.5.dp, colors.outline, CircleShape)
                },
            ),
        contentAlignment = Alignment.Center,
    ) {
        if (chosen) Glyph(R.drawable.ic_done, null, colors.onAccent, size = 14.dp)
    }
}

/** The choice bar's accent button, as Done or Not spam; its [label] names it with the count, as Done 3. */
class ChoiceMain(val label: String, val icon: Int, val onClick: () -> Unit)

/**
 * What can be done with the chosen messages, where search and a new message usually are: Read or
 * Unread, Delete (for good in the trash) and More as icons on a pill as tall as search, their names
 * shown on a long press; then [main], when one applies, a round accent button where a new message
 * is. Cancel is at the top, where Back is, and the count is in the title.
 */
@Composable
fun ChoiceBar(surface: Modifier, main: ChoiceMain?, unread: Boolean, forever: Boolean, onRead: () -> Unit, onDelete: () -> Unit, onMore: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val height = Bar.controlHeight()
    Row(
        Modifier.fillMaxWidth().padding(horizontal = Gap.bar),
        horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            Modifier.weight(1f).height(height).then(surface).padding(horizontal = Gap.inPill),
            horizontalArrangement = Arrangement.SpaceEvenly,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            @Composable
            fun Action(icon: Int, label: String, onClick: () -> Unit) =
                Named(label) { IconTile(icon, label, onClick, shape = CircleShape, size = height - Gap.inPill * 2) }
            Action(R.drawable.ic_unread, stringResource(if (unread) R.string.action_read else R.string.action_unread), onRead)
            Action(R.drawable.ic_delete, stringResource(if (forever) R.string.action_delete_forever else R.string.action_delete), onDelete)
            Action(R.drawable.ic_more, stringResource(R.string.action_more), onMore)
        }
        main?.let {
            Named(it.label) { IconTile(it.icon, it.label, it.onClick, background = colors.accent, tint = colors.onAccent, shape = CircleShape, size = height) }
        }
    }
}

/** A button without words, whose [label] shows over it on a long press. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Named(label: String, content: @Composable () -> Unit) {
    TooltipBox(
        positionProvider = TooltipDefaults.rememberPlainTooltipPositionProvider(),
        tooltip = { PlainTooltip { Text(label) } },
        state = rememberTooltipState(),
        content = content,
    )
}

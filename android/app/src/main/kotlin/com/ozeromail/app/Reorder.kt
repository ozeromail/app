package com.ozeromail.app

import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.zIndex

/**
 * [items] one under another, put in another order by dragging a row's handle: [row] gets the
 * modifier to put on it. The held row follows the finger and the others make way as it passes
 * half of theirs; [onMove] gets the new order once the finger lets go.
 */
@Composable
fun <T> Reorderable(
    items: List<T>,
    key: (T) -> Any,
    onMove: (List<T>) -> Unit,
    row: @Composable (item: T, handle: Modifier) -> Unit,
) {
    var order by remember(items) { mutableStateOf(items) }
    var held by remember { mutableStateOf<Any?>(null) }
    // How far the held row is from its place, in pixels.
    var offset by remember { mutableFloatStateOf(0f) }
    var rowHeight by remember { mutableIntStateOf(0) }
    val haptics = LocalHapticFeedback.current
    val moved by rememberUpdatedState(onMove)
    Column {
        order.forEach { item ->
            val id = key(item)
            key(id) {
                val holding = held == id
                val handle = Modifier.pointerInput(id, items) {
                    detectVerticalDragGestures(
                        onDragStart = {
                            held = id
                            offset = 0f
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        },
                        onDragEnd = {
                            held = null
                            offset = 0f
                            moved(order)
                        },
                        onDragCancel = {
                            held = null
                            offset = 0f
                        },
                    ) { change, delta ->
                        change.consume()
                        offset += delta
                        val at = order.indexOfFirst { key(it) == id }
                        val half = rowHeight / 2f
                        if (offset > half && at < order.lastIndex) {
                            order = order.toMutableList().apply { add(at + 1, removeAt(at)) }
                            offset -= rowHeight
                        } else if (offset < -half && at > 0) {
                            order = order.toMutableList().apply { add(at - 1, removeAt(at)) }
                            offset += rowHeight
                        }
                    }
                }
                Box(
                    Modifier
                        .onSizeChanged { rowHeight = it.height }
                        .zIndex(if (holding) 1f else 0f)
                        .graphicsLayer { translationY = if (holding) offset else 0f },
                ) { row(item, handle) }
            }
        }
    }
}

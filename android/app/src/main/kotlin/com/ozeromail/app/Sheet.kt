package com.ozeromail.app

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animate
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Our own rather than Material's sheet, whose open and close motion cannot be set.
 *
 * A menu over the screen, rising from the bottom edge: the screen dims, the sheet glides up
 * and follows the finger down. A drag down, a tap outside or back closes it; the back
 * gesture lowers it as the finger moves. Put it last in a full-screen box.
 */
@Composable
fun Sheet(visible: Boolean, onClose: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val colors = MaterialTheme.colorScheme
    Box(Modifier.fillMaxSize()) {
        AnimatedVisibility(
            visible,
            enter = fadeIn(Motion.calm(Motion.SCRIM)),
            exit = fadeOut(Motion.calm(Motion.SCRIM)),
        ) {
            Box(
                Modifier
                    .fillMaxSize()
                    .background(colors.scrim)
                    .clickable(remember { MutableInteractionSource() }, indication = null, enabled = visible, onClick = onClose),
            )
        }
        AnimatedVisibility(
            visible,
            Modifier.align(Alignment.BottomCenter),
            enter = slideInVertically(Motion.calm(Motion.SCREEN)) { it },
            exit = slideOutVertically(Motion.calm(Motion.SCREEN)) { it },
        ) {
            SheetBody(visible, onClose, content)
        }
    }
}

@Composable
private fun SheetBody(visible: Boolean, onClose: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val colors = MaterialTheme.colorScheme
    val close by rememberUpdatedState(onClose)
    val scope = rememberCoroutineScope()
    val flingToClose = with(LocalDensity.current) { 800.dp.toPx() }
    // How far the finger has pulled the sheet down, in pixels.
    var pulled by remember { mutableFloatStateOf(0f) }
    var height by remember { mutableIntStateOf(0) }

    suspend fun settle() {
        animate(pulled, 0f, animationSpec = Motion.settle()) { value, _ -> pulled = value }
    }

    fun release(velocity: Float) {
        if (pulled > height * 0.3f || velocity > flingToClose) close() else scope.launch { settle() }
    }

    // Not while closing: back then belongs to the screen.
    PredictiveBackHandler(enabled = visible) { progress ->
        try {
            progress.collect { pulled = it.progress * height * 0.5f }
            close()
        } catch (e: CancellationException) {
            // The gesture's own job is cancelled with it; the sheet still has to rise back.
            withContext(NonCancellable) { settle() }
            throw e
        }
    }

    // A list scrolled to its top hands the rest of a downward drag to the sheet.
    val handOff = remember {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (available.y >= 0f || pulled <= 0f) return Offset.Zero
                val used = maxOf(available.y, -pulled)
                pulled += used
                return Offset(0f, used)
            }

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (available.y <= 0f || source != NestedScrollSource.UserInput) return Offset.Zero
                pulled += available.y
                return Offset(0f, available.y)
            }

            override suspend fun onPreFling(available: Velocity): Velocity {
                if (pulled <= 0f) return Velocity.Zero
                release(available.y)
                return available
            }
        }
    }

    Column(
        Modifier
            // On its side or on a tablet, a menu stays as wide as on a phone held upright.
            .widthIn(max = SHEET_MAX)
            .fillMaxWidth()
            // Never taller than most of the screen, so the dimmed screen still shows above it.
            .layout { measurable, constraints ->
                val placeable = measurable.measure(constraints.copy(maxHeight = (constraints.maxHeight * 0.88f).toInt()))
                layout(placeable.width, placeable.height) { placeable.place(0, 0) }
            }
            .onSizeChanged { height = it.height }
            .graphicsLayer { translationY = pulled }
            .clip(RoundedCornerShape(topStart = Radius.sheet, topEnd = Radius.sheet))
            .background(colors.background)
            .draggable(
                rememberDraggableState { delta -> pulled = (pulled + delta).coerceAtLeast(0f) },
                Orientation.Vertical,
                onDragStopped = { velocity -> release(velocity) },
            )
            .nestedScroll(handOff)
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal + WindowInsetsSides.Bottom))
            .padding(top = 10.dp, bottom = 16.dp),
    ) {
        Box(
            Modifier
                .align(Alignment.CenterHorizontally)
                .padding(bottom = 12.dp)
                .size(36.dp, 4.dp)
                .background(colors.surfaceContainerHighest, RoundedCornerShape(2.dp)),
        )
        Column(Modifier.verticalScroll(rememberScrollState()), content = content)
    }
}

/** A sheet's title, on the same edge as its rows. */
@Composable
fun SheetTitle(text: String, modifier: Modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, bottom = 10.dp)) {
    Text(
        text,
        style = Type.Heading,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier,
    )
}

/**
 * A row of a sheet: a dot or an icon in a slot of its own, so every label starts at the same
 * place. A row with [onLongClick] does more when held, as [longClickLabel] says.
 */
@Composable
fun SheetRow(
    onClick: () -> Unit,
    lead: @Composable () -> Unit,
    onLongClick: (() -> Unit)? = null,
    longClickLabel: String? = null,
    content: @Composable RowScope.() -> Unit,
) {
    // On its side the rows are lower, so more of them fit the short height.
    val short = LocalRoom.current.short
    val haptics = LocalHapticFeedback.current
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = if (short) 40.dp else 52.dp)
            .combinedClickable(
                onLongClickLabel = longClickLabel,
                onLongClick = onLongClick?.let { held ->
                    {
                        haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                        held()
                    }
                },
                onClick = onClick,
            )
            .padding(horizontal = Gap.sheet, vertical = if (short) 6.dp else 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(Gap.slot), contentAlignment = Alignment.Center) { lead() }
        Spacer(Modifier.width(Gap.afterSlot))
        content()
    }
}

/** How wide a sheet gets. */
private val SHEET_MAX = 560.dp

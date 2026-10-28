package com.ozeromail.app

import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.animateFloatAsState
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
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Our own rather than Material's sheet, whose open and close motion cannot be set.
 *
 * A menu over the screen, rising from the bottom edge: the screen dims, the sheet glides up
 * and follows the finger down. A drag down, a tap outside or back closes it; the back
 * gesture lowers it as the finger moves. With [lift], a finger can also raise it from below,
 * as from the dock. Put it last in a full-screen box.
 */
@Composable
fun Sheet(visible: Boolean, onClose: () -> Unit, lift: Lift? = null, content: @Composable ColumnScope.() -> Unit) {
    val colors = MaterialTheme.colorScheme
    // How much of the sheet is still below the screen, from 0 to 1: the scrim darkens as it rises.
    var hidden by remember { mutableFloatStateOf(0f) }
    // Raised by a finger, the sheet comes up with it rather than on its own.
    val lifted by remember(lift) { derivedStateOf { lift?.risen != null } }
    Box(Modifier.fillMaxSize()) {
        AnimatedVisibility(
            visible,
            enter = if (lifted) EnterTransition.None else fadeIn(Motion.calm(Motion.SCRIM)),
            exit = fadeOut(Motion.calm(Motion.SCRIM)),
        ) {
            Box(
                Modifier
                    .fillMaxSize()
                    .graphicsLayer { alpha = 1f - hidden }
                    .background(colors.scrim)
                    .clickable(remember { MutableInteractionSource() }, indication = null, enabled = visible, onClick = onClose),
            )
        }
        AnimatedVisibility(
            visible,
            Modifier.align(Alignment.BottomCenter),
            enter = if (lifted) EnterTransition.None else slideInVertically(Motion.calm(Motion.SCREEN)) { it },
            exit = slideOutVertically(Motion.calm(Motion.SCREEN)) { it },
        ) {
            SheetBody(visible, onClose, lift, { hidden = it }, content)
        }
    }
}

/**
 * A finger raising a sheet from below its bottom edge, as from the dock: how far it has come
 * up while it holds on, and how fast it went when it let go. Give the same one to the sheet.
 */
@Stable
class Lift {
    /** How far the finger has raised the sheet, in pixels; null when no finger holds it from below. */
    var risen by mutableStateOf<Float?>(null)
        private set

    /** How fast the finger went when it let go, up being less than zero; the sheet then settles. */
    internal var thrown by mutableStateOf<Float?>(null)
        private set

    /** A finger takes hold below the sheet. */
    fun start() {
        risen = 0f
        thrown = null
    }

    /** The finger moved by [delta], up being less than zero. */
    fun drag(delta: Float) {
        risen = ((risen ?: return) - delta).coerceAtLeast(0f)
    }

    /** The finger let go at [velocity]. */
    fun stop(velocity: Float) {
        if (risen != null) thrown = velocity
    }

    internal fun land() {
        risen = null
        thrown = null
    }
}

@Composable
private fun SheetBody(
    visible: Boolean,
    onClose: () -> Unit,
    lift: Lift?,
    onHidden: (Float) -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val close by rememberUpdatedState(onClose)
    val scope = rememberCoroutineScope()
    val flingToClose = with(LocalDensity.current) { 800.dp.toPx() }
    // How far the finger has pulled the sheet down, in pixels.
    var pulled by remember { mutableFloatStateOf(0f) }
    var height by remember { mutableIntStateOf(0) }
    // How far below its place the sheet is: raised from below, it is as low as the finger left it.
    fun lowered(tall: Float = height.toFloat()): Float = lift?.risen?.let { (tall - it).coerceAtLeast(0f) } ?: pulled

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

    if (lift != null) {
        LaunchedEffect(lift) {
            snapshotFlow { lift.thrown }.filterNotNull().collect { velocity ->
                snapshotFlow { height }.first { it > 0 }
                pulled = lowered()
                lift.land()
                // A throw decides; otherwise the sheet opens once it is more than a third of the way up.
                val open = when {
                    velocity < -flingToClose / 2 -> true
                    velocity > flingToClose / 2 -> false
                    else -> pulled < height * 0.66f
                }
                if (open) settle() else close()
            }
        }
    }
    LaunchedEffect(Unit) {
        snapshotFlow { if (height == 0) 1f else (lowered() / height).coerceIn(0f, 1f) }.collect(onHidden)
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
            // Before it is measured, a sheet raised from below stays under the screen.
            .graphicsLayer { translationY = lowered(size.height) }
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
 * place, [start] further in. A row with [onLongClick] does more when held, as [longClickLabel]
 * says. A [fixed] row is as high whatever is at its end; only larger text makes it higher.
 */
@Composable
fun SheetRow(
    onClick: () -> Unit,
    lead: @Composable () -> Unit,
    onLongClick: (() -> Unit)? = null,
    longClickLabel: String? = null,
    fixed: Boolean = false,
    start: Dp = 0.dp,
    content: @Composable RowScope.() -> Unit,
) {
    // On its side the rows are lower, so more of them fit the short height.
    val short = LocalRoom.current.short
    val haptics = LocalHapticFeedback.current
    val min = when {
        short -> 40.dp
        fixed -> 48.dp
        else -> 52.dp
    }
    val pad = if (fixed) 0.dp else if (short) 6.dp else 10.dp
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = min)
            .holdable(haptics, onClick, onLongClick, onLongClickLabel = longClickLabel)
            .padding(start = Gap.sheet + start, end = Gap.sheet, top = pad, bottom = pad),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(Gap.slot), contentAlignment = Alignment.Center) { lead() }
        Spacer(Modifier.width(Gap.afterSlot))
        content()
    }
}

/** Tapped, does [onClick]; held, does [onLongClick], if any, with a buzz. */
fun Modifier.holdable(
    haptics: HapticFeedback,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)?,
    onClickLabel: String? = null,
    onLongClickLabel: String? = null,
): Modifier = combinedClickable(
    onClickLabel = onClickLabel,
    onLongClickLabel = onLongClickLabel,
    onLongClick = onLongClick?.let { held ->
        {
            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
            held()
        }
    },
    onClick = onClick,
)

/** A switch the size of a line of text, for lists of them; the row it is in turns it. */
@Composable
fun SmallSwitch(on: Boolean, enabled: Boolean = true) {
    val colors = MaterialTheme.colorScheme
    val at by animateFloatAsState(if (on) 1f else 0f, Motion.calm(Motion.QUICK), label = "switch")
    Box(
        Modifier
            .padding(start = 12.dp)
            .size(width = 38.dp, height = 22.dp)
            .alpha(if (enabled) 1f else 0.4f)
            .clip(CircleShape)
            .drawBehind { drawRect(lerp(colors.outline, colors.primary, at)) }
            .padding(2.dp)
            .semantics {
                role = Role.Switch
                toggleableState = ToggleableState(on)
                if (!enabled) disabled()
            },
    ) {
        Box(Modifier.offset { IntOffset((16.dp * at).roundToPx(), 0) }.size(18.dp).background(Color.White, CircleShape))
    }
}

/** How wide a sheet gets. */
private val SHEET_MAX = 560.dp

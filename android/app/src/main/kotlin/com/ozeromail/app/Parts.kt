package com.ozeromail.app

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animate
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarData
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.SnackbarVisuals
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalAccessibilityManager
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.ozero_api.Mailbox
import uniffi.ozero_api.undoSeconds

/** A square-ish button holding one icon, as in the bottom bars. One in the accent lifts off the page. */
@Composable
fun IconTile(
    @DrawableRes icon: Int,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    background: Color = Color.Transparent,
    tint: Color = MaterialTheme.colorScheme.onSurface,
    shape: Shape = RoundedCornerShape(Radius.tile),
    size: Dp = 48.dp,
    enabled: Boolean = true,
) {
    val colors = MaterialTheme.colorScheme
    Box(
        modifier
            .size(size)
            .alpha(if (enabled) 1f else 0.4f)
            .liftedIfAccent(colors, background, shape)
            .clip(shape)
            .background(background)
            .clickable(enabled = enabled, onClickLabel = description, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Glyph(icon, description, tint)
    }
}

/** The usual way back, top left, its glyph lined up with the text under it. */
@Composable
fun BackArrow(onBack: () -> Unit, modifier: Modifier = Modifier) {
    val size = 44.dp
    IconTile(
        R.drawable.ic_back,
        stringResource(R.string.action_back),
        onBack,
        modifier.padding(start = Gap.screen - (size - Gap.slot) / 2),
        size = size,
    )
}

/** The rounded shape of the search field and the quick reply: grey and cut into the page, or glass in the dock. */
@Composable
fun PillBox(
    modifier: Modifier = Modifier,
    @DrawableRes icon: Int? = null,
    height: Dp = Bar.controlHeight(),
    onClick: (() -> Unit)? = null,
    content: @Composable RowScope.() -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Row(
        modifier
            .height(height)
            // On glass where it floats over the list, as in the dock.
            .glass(LocalGlass.current, colors, CircleShape)
            .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        icon?.let { Glyph(it, null, colors.onSurfaceVariant) }
        content()
    }
}

/** A [PillBox] that is a button with a grey prompt in it. */
@Composable
fun Pill(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    @DrawableRes icon: Int? = null,
    height: Dp = Bar.controlHeight(),
) {
    PillBox(modifier, icon, height, onClick) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, style = Type.Body, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** A screen's large title, set high where it needs no reach. */
@Composable
fun LargeTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier, style = Type.Display)
}

/** An account's colour with the first letter of its name. */
@Composable
fun AccountInitial(name: String, color: Color, size: Dp) {
    Box(Modifier.size(size).background(color, CircleShape), contentAlignment = Alignment.Center) {
        Text(initials(name).take(1), color = Color.White, fontSize = fixedSp(size / 2), fontWeight = Weight.Bold)
    }
}

/** The see-through background of the bars over the list. */
val ColorScheme.bar: Color
    get() = background.copy(alpha = 0.94f)

/** A hairline along the top or bottom edge. */
fun Modifier.hairline(color: Color, top: Boolean): Modifier = drawBehind {
    val y = if (top) 0f else size.height
    drawLine(color, Offset(0f, y), Offset(size.width, y), 1.dp.toPx())
}

/** A small round mark in an account's colour, a tiny window. */
@Composable
fun AccountDot(color: Color, modifier: Modifier = Modifier, size: Dp = 10.dp) {
    val window = MaterialTheme.colorScheme.window(color)
    Box(modifier.size(size).drawWithCache { onDrawBehind { windowDot(window, this.size.minDimension / 2) } })
}

/** Text that fills a circle of a fixed size, so it does not grow with the font scale. */
@Composable
private fun fixedSp(size: Dp): TextUnit = with(LocalDensity.current) { size.toSp() }

/** The first letters of the first two words, or of the address. */
fun initials(name: String): String =
    name.split(' ', '.', '@', '_', '-')
        .filter { it.isNotBlank() }
        .take(2)
        .joinToString("") { it.take(1) }
        .uppercase()
        .ifEmpty { "?" }

/** The ring of every empty note, the same size on every screen. */
private val NOTE_RING = 80.dp
private val NOTE_GLYPH = 36.dp

/** A round note with a title and a line under it, for empty lists. */
@Composable
fun EmptyNote(@DrawableRes icon: Int, title: String, text: String? = null) {
    val colors = MaterialTheme.colorScheme
    Note(title, text) {
        Box(Modifier.size(NOTE_RING).background(colors.surfaceVariant, CircleShape), contentAlignment = Alignment.Center) {
            Glyph(icon, null, colors.onSurfaceVariant, size = NOTE_GLYPH)
        }
    }
}

@Composable
private fun Note(title: String, text: String?, mark: @Composable () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        mark()
        Text(title, style = Type.Heading, textAlign = TextAlign.Center)
        text?.let {
            Text(
                it,
                color = colors.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.widthIn(max = 280.dp),
            )
        }
    }
}

/** One more button on a toast, before Undo. */
class ToastAction(val label: String, val run: () -> Unit)

/**
 * A toast about mail that moved: besides Undo, it can [go] to the folder the mail is in now,
 * or offer an [extra] action.
 */
class MovedToast(
    override val message: String,
    override val actionLabel: String,
    val go: Mailbox?,
    val extra: ToastAction? = null,
) : SnackbarVisuals {
    override val withDismissAction = false
    override val duration = SnackbarDuration.Indefinite
}

/**
 * Shows [text] with [undo] for [undoSeconds], in place of any other toast; true if Undo was
 * tapped. With [go], it can also show that folder; with [extra], do that.
 */
suspend fun SnackbarHostState.offerUndo(text: String, undo: String, go: Mailbox? = null, extra: ToastAction? = null): Boolean {
    // The older toast is past undo anyway.
    currentSnackbarData?.dismiss()
    // Closes before the core sends, so Undo is never too late.
    val result = withTimeoutOrNull(undoSeconds().toLong() * 1000) { showSnackbar(MovedToast(text, undo, go, extra)) }
    return result == SnackbarResult.ActionPerformed
}

/** How long a toast without Undo stays, as Material's own short and long ones. */
private const val TOAST_SHORT_MS = 4_000L
private const val TOAST_LONG_MS = 10_000L

/**
 * Closes a toast without Undo once its time is over, as Material's own host would; [offerUndo]
 * closes its own. One for each [SnackbarHostState], beside it.
 */
@Composable
fun ToastClock(state: SnackbarHostState) {
    val data = state.currentSnackbarData ?: return
    val shortFor = when (data.visuals.duration) {
        SnackbarDuration.Short -> TOAST_SHORT_MS
        SnackbarDuration.Long -> TOAST_LONG_MS
        SnackbarDuration.Indefinite -> null
    } ?: return
    // Longer where the person asked the phone for more time to read and act.
    val access = LocalAccessibilityManager.current
    LaunchedEffect(data) {
        delay(access?.calculateRecommendedTimeoutMillis(shortFor, containsText = true, containsControls = data.visuals.actionLabel != null) ?: shortFor)
        data.dismiss()
    }
}

/**
 * The toast to show: the current one, or for a moment after it closes still the last, so a toast
 * that takes the place of another changes its words instead of closing and opening again.
 */
@Composable
fun shownToast(state: SnackbarHostState): SnackbarData? {
    val data = state.currentSnackbarData
    var shown by remember { mutableStateOf(data) }
    LaunchedEffect(data) {
        if (data == null) delay(TOAST_GAP_MS)
        shown = data
    }
    return shown
}

/** Between one toast closing and the next one in its place, which a frame may fall into. */
private const val TOAST_GAP_MS = 80L

/**
 * A toast as a glass capsule, one line: the words, and Undo at the end. Tapping the words of a
 * [MovedToast] shows its folder. A new toast changes the words in place.
 */
@Composable
fun ToastPill(data: SnackbarData, onGo: (Mailbox) -> Unit, modifier: Modifier = Modifier, height: Dp = Bar.controlHeight()) {
    val colors = MaterialTheme.colorScheme
    Box(
        modifier
            .heightIn(min = height)
            // Round at the ends as a capsule, and still so with the words on two lines.
            .floatingGlass(LocalGlass.current, colors, RoundedCornerShape(height / 2))
            .semantics { liveRegion = LiveRegionMode.Polite },
    ) {
        AnimatedContent(
            data,
            transitionSpec = { fadeIn(Motion.calm(Motion.SMALL)) togetherWith fadeOut(Motion.calm(Motion.QUICK)) },
            label = "toast words",
        ) { shown ->
            Row(Modifier.fillMaxWidth().heightIn(min = height).height(IntrinsicSize.Min).padding(end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                ToastWords(shown, onGo)
            }
        }
    }
}

@Composable
private fun RowScope.ToastWords(data: SnackbarData, onGo: (Mailbox) -> Unit) {
    val colors = MaterialTheme.colorScheme
    val moved = data.visuals as? MovedToast
    val go = moved?.go
    Row(
        Modifier
            .weight(1f)
            .fillMaxHeight()
            .then(
                if (go != null) {
                    Modifier.clickable(onClickLabel = stringResource(R.string.action_show), role = Role.Button) {
                        data.dismiss()
                        onGo(go)
                    }
                } else {
                    Modifier
                },
            )
            .padding(start = 18.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(data.visuals.message, Modifier.weight(1f, fill = false).padding(vertical = 4.dp), style = Type.Text, maxLines = 2, overflow = TextOverflow.Ellipsis)
        if (go != null) Glyph(R.drawable.ic_next, null, colors.onSurfaceVariant, size = 16.dp)
    }
    moved?.extra?.let { extra ->
        ToastButton(extra.label) {
            data.dismiss()
            extra.run()
        }
    }
    data.visuals.actionLabel?.let { ToastButton(it, strong = true, onClick = data::performAction) }
}

@Composable
private fun ToastButton(label: String, strong: Boolean = false, onClick: () -> Unit) {
    TextButton(onClick, contentPadding = PaddingValues(horizontal = 12.dp)) {
        Text(
            label,
            color = MaterialTheme.colorScheme.primary,
            style = Type.Text,
            fontWeight = if (strong) Weight.SemiBold else Weight.Medium,
            maxLines = 1,
        )
    }
}

/**
 * The toast where the dock has no place for it, as on the message screen: a [ToastPill] low on
 * the screen that rises from below and sinks back, and goes when pulled down.
 */
@Composable
fun Toast(state: SnackbarHostState, onGo: (Mailbox) -> Unit, modifier: Modifier = Modifier) {
    val data = shownToast(state)
    val last = rememberLast(data)
    val density = LocalDensity.current
    val rise = with(density) { 16.dp.roundToPx() }
    AnimatedVisibility(
        data != null,
        modifier,
        enter = fadeIn(Motion.calm(Motion.SMALL)) + slideInVertically(Motion.calm(Motion.NOTE)) { rise } +
            scaleIn(Motion.calm(Motion.NOTE), initialScale = TOAST_FAR_SCALE),
        exit = fadeOut(Motion.calm(Motion.SMALL)) + slideOutVertically(Motion.calm(Motion.NOTE)) { rise } +
            scaleOut(Motion.calm(Motion.NOTE), targetScale = TOAST_FAR_SCALE),
    ) {
        last?.let { shown ->
            // Pulled down past a little way, it goes; let go sooner, it springs back.
            var pull by remember(shown) { mutableFloatStateOf(0f) }
            val far = with(density) { 28.dp.toPx() }
            ToastPill(
                shown,
                onGo,
                Modifier
                    .fillMaxWidth()
                    .padding(horizontal = Gap.bar)
                    .graphicsLayer {
                        translationY = pull
                        alpha = 1f - (pull / (far * 3)).coerceIn(0f, 0.6f)
                    }
                    .draggable(
                        rememberDraggableState { delta -> pull = (pull + delta).coerceAtLeast(0f) },
                        Orientation.Vertical,
                        onDragStopped = { velocity ->
                            if (pull > far || velocity > TOAST_FLING) shown.dismiss() else animate(pull, 0f, animationSpec = Motion.settle()) { value, _ -> pull = value }
                        },
                    ),
            )
        }
    }
}

/** How small a toast is as it rises and as it sinks back. */
private const val TOAST_FAR_SCALE = 0.96f

/** A downward fling that sends a toast away however short the pull, in pixels a second. */
private const val TOAST_FLING = 1_000f

/**
 * Shows [content] for [value] while there is one: it opens and fades in, then closes with
 * the last value still in it.
 */
@Composable
fun <T : Any> Reveal(value: T?, modifier: Modifier = Modifier, content: @Composable (T) -> Unit) {
    val last = rememberLast(value)
    AnimatedVisibility(
        value != null,
        modifier,
        enter = expandVertically(Motion.calm(Motion.NOTE)) + fadeIn(Motion.calm(Motion.NOTE)),
        exit = shrinkVertically(Motion.calm(Motion.NOTE)) + fadeOut(Motion.calm(Motion.SMALL)),
    ) {
        last?.let { content(it) }
    }
}

/** [value], or while it is null the last one it was: for something that closes with it still in it. */
@Composable
fun <T : Any> rememberLast(value: T?): T? {
    val last = remember { mutableStateOf(value) }
    SideEffect { if (value != null) last.value = value }
    return value ?: last.value
}

/** The small spinner inside a button or a tile. */
@Composable
fun Spinner(color: Color) {
    CircularProgressIndicator(Modifier.size(18.dp), color = color, strokeWidth = 2.dp)
}

/** A spinner only for a wait long enough to notice; a saved message shows without one. */
@Composable
fun SlowSpinner() {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(SPINNER_DELAY_MS)
        shown = true
    }
    if (shown) CircularProgressIndicator()
}

/** How long a load runs before its spinner shows. */
private const val SPINNER_DELAY_MS = 400L

/** Tells [onChange] when a finger comes down and when the last one lifts, taking nothing from what is below. */
fun Modifier.whilePressed(onChange: (Boolean) -> Unit): Modifier = pointerInput(Unit) {
    awaitEachGesture {
        awaitFirstDown(requireUnconsumed = false)
        onChange(true)
        do {
            val event = awaitPointerEvent(PointerEventPass.Initial)
        } while (event.changes.any { it.pressed })
        onChange(false)
    }
}

/** Takes the touches its content leaves, so they do not reach what is under it. */
fun Modifier.holdTouches(): Modifier = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) awaitPointerEvent()
    }
}

/** Takes every touch, so a screen on its way out cannot be tapped twice. */
fun Modifier.blockTouches(): Modifier = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
    }
}

/** Opens a link in an app that takes links from a browser; nothing if none does. */
fun openLink(context: Context, link: Uri) {
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, link).addCategory(Intent.CATEGORY_BROWSABLE))
    } catch (_: ActivityNotFoundException) {
        // Nothing opens it.
    }
}

/**
 * Like the system's own bars, a bar's words stop growing past the largest regular text size:
 * its buttons and tabs keep their width, so their labels stay whole.
 */
@Composable
fun BarText(content: @Composable () -> Unit) {
    val density = LocalDensity.current
    if (density.fontScale <= BAR_FONT_SCALE) return content()
    CompositionLocalProvider(LocalDensity provides Density(density.density, BAR_FONT_SCALE)) { content() }
}

private const val BAR_FONT_SCALE = 1.3f

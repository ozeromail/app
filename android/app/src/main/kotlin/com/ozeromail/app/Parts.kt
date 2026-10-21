package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarVisuals
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import uniffi.ozero_api.Mailbox

/** A square-ish button holding one icon, as in the bottom bars. */
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
    Box(
        modifier
            .size(size)
            .alpha(if (enabled) 1f else 0.4f)
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

/** The rounded grey shape of the search field and the quick reply. */
@Composable
fun PillBox(
    modifier: Modifier = Modifier,
    @DrawableRes icon: Int? = null,
    height: Dp = 48.dp,
    content: @Composable RowScope.() -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Row(
        modifier
            .height(height)
            .clip(CircleShape)
            .background(colors.surfaceVariant)
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
    height: Dp = 48.dp,
) {
    PillBox(modifier.clip(CircleShape).clickable(role = Role.Button, onClick = onClick), icon, height) {
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

/** A small round mark in an account's colour. */
@Composable
fun AccountDot(color: Color, modifier: Modifier = Modifier, size: Dp = 10.dp) {
    Box(modifier.size(size).background(color, CircleShape))
}

/** A person's initials on a colour of their own. */
@Composable
fun Avatar(name: String, key: String, modifier: Modifier = Modifier, size: Dp = 44.dp) {
    Box(
        modifier.size(size).background(avatarColor(key), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            initials(name),
            color = Color.White,
            fontWeight = Weight.SemiBold,
            fontSize = fixedSp(size * 0.36f),
        )
    }
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

private val AVATAR_COLORS = listOf(
    0xFF5A6B8E, 0xFF4D8789, 0xFF7D6CAE, 0xFFA87458, 0xFF5B8C62, 0xFF93688A, 0xFF6A7B96,
).map(::Color)

/** The same colour every time for one sender. */
private fun avatarColor(key: String): Color = AVATAR_COLORS[Math.floorMod(key.lowercase().hashCode(), AVATAR_COLORS.size)]

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

/** A toast about mail that moved: besides Undo, it can [go] to the folder the mail is in now. */
class MovedToast(
    override val message: String,
    override val actionLabel: String,
    val go: Mailbox?,
) : SnackbarVisuals {
    override val withDismissAction = false
    override val duration = SnackbarDuration.Indefinite
}

/**
 * The undo toast: dark, rounded, low on the screen; it rises a little and fades in. A
 * [MovedToast] also offers to show the folder, through [onGo].
 */
@Composable
fun Toast(state: SnackbarHostState, onGo: (Mailbox) -> Unit, modifier: Modifier = Modifier) {
    val colors = MaterialTheme.colorScheme
    val rise = with(LocalDensity.current) { 16.dp.roundToPx() }
    AnimatedContent(
        state.currentSnackbarData,
        modifier,
        transitionSpec = {
            val enter = fadeIn(Motion.calm(Motion.SMALL)) + slideInVertically(Motion.calm(Motion.NOTE)) { rise }
            val exit = fadeOut(Motion.calm(Motion.SMALL)) + slideOutVertically(Motion.calm(Motion.NOTE)) { rise }
            (enter togetherWith exit).using(SizeTransform(clip = false) { _, _ -> Motion.calm(Motion.NOTE) })
        },
        label = "toast",
    ) { data ->
        if (data != null) {
            val go = (data.visuals as? MovedToast)?.go
            Snackbar(
                modifier = Modifier.padding(horizontal = Gap.bar).semantics { liveRegion = LiveRegionMode.Polite },
                action = {
                    Row {
                        if (go != null) {
                            TextButton(
                                onClick = {
                                    data.dismiss()
                                    onGo(go)
                                },
                            ) { Text(stringResource(R.string.action_show), color = colors.inversePrimary) }
                        }
                        data.visuals.actionLabel?.let { label ->
                            TextButton(onClick = data::performAction) { Text(label, color = colors.inversePrimary) }
                        }
                    }
                },
                shape = RoundedCornerShape(Radius.medium),
                containerColor = colors.inverseSurface,
                contentColor = colors.inverseOnSurface,
            ) { Text(data.visuals.message) }
        }
    }
}

/**
 * Shows [content] for [value] while there is one: it opens and fades in, then closes with
 * the last value still in it.
 */
@Composable
fun <T : Any> Reveal(value: T?, modifier: Modifier = Modifier, content: @Composable (T) -> Unit) {
    val last = remember { mutableStateOf(value) }
    SideEffect { if (value != null) last.value = value }
    AnimatedVisibility(
        value != null,
        modifier,
        enter = expandVertically(Motion.calm(Motion.NOTE)) + fadeIn(Motion.calm(Motion.NOTE)),
        exit = shrinkVertically(Motion.calm(Motion.NOTE)) + fadeOut(Motion.calm(Motion.SMALL)),
    ) {
        (value ?: last.value)?.let { content(it) }
    }
}

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

/** Takes every touch, so a screen on its way out cannot be tapped twice. */
fun Modifier.blockTouches(): Modifier = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
    }
}

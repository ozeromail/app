package com.ozeromail.app

import android.content.Context
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxDefaults
import androidx.compose.material3.SwipeToDismissBoxState
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import kotlin.math.abs
import uniffi.ozero_api.MessageItem

/** What a swipe on a row does; the person picks one for each side. */
enum class SwipeAction(@StringRes val label: Int, @DrawableRes val icon: Int) {
    DONE(R.string.action_done, R.drawable.ic_done),
    READ(R.string.action_read, R.drawable.ic_unread),
    DELETE(R.string.action_delete, R.drawable.ic_delete),
    SNOOZE(R.string.action_snooze, R.drawable.ic_snooze),
}

/** By default left is Done and right is Read, as in the prototype. */
data class Swipes(val left: SwipeAction = SwipeAction.DONE, val right: SwipeAction = SwipeAction.READ)

/** The person's choices, kept on the phone. */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var swipes: Swipes
        get() = Swipes().let {
            Swipes(action(prefs.getString(LEFT, null)) ?: it.left, action(prefs.getString(RIGHT, null)) ?: it.right)
        }
        set(value) = prefs.edit().putString(LEFT, value.left.name).putString(RIGHT, value.right.name).apply()

    /** The tip about swipes was read, so it does not show again. */
    var swipeTipSeen: Boolean
        get() = prefs.getBoolean(TIP, false)
        set(value) = prefs.edit().putBoolean(TIP, value).apply()

    /** The mailboxes synced on this phone, by [Mailbox] key: only one never synced waits for its first mail. */
    var syncedBoxes: Set<Long>
        get() = ids(SYNCED)
        set(value) = prefs.edit().putStringSet(SYNCED, value.strings()).apply()

    /** The groups of the accounts sheet the person closed or opened. */
    var groups: Groups
        get() = Groups(ids(COLLAPSED), ids(EXPANDED))
        set(value) = prefs.edit().putStringSet(COLLAPSED, value.collapsed.strings()).putStringSet(EXPANDED, value.expanded.strings()).apply()

    private fun ids(key: String) = prefs.getStringSet(key, null).orEmpty().mapNotNull { it.toLongOrNull() }.toSet()

    private fun Set<Long>.strings() = map { it.toString() }.toSet()

    private fun action(name: String?) = SwipeAction.entries.find { it.name == name }

    companion object {
        /** The group of the mailboxes of all accounts together; account ids are positive. */
        const val ALL_ACCOUNTS = 0L

        private const val LEFT = "swipe_left"
        private const val RIGHT = "swipe_right"
        private const val TIP = "swipe_tip_seen"
        private const val COLLAPSED = "collapsed_groups"
        private const val EXPANDED = "expanded_groups"
        private const val SYNCED = "synced_boxes"
    }
}

/**
 * Which groups of the accounts sheet show their folders: account ids, or [Settings.ALL_ACCOUNTS].
 * A group the person closed or opened stays so. One never touched starts open, but with
 * [many] accounts each account's starts closed, so the sheet stays short.
 */
data class Groups(val collapsed: Set<Long>, val expanded: Set<Long>) {
    fun open(group: Long, many: Boolean) = when (group) {
        in collapsed -> false
        in expanded -> true
        else -> group == Settings.ALL_ACCOUNTS || !many
    }

    fun toggle(group: Long, many: Boolean) =
        if (open(group, many)) Groups(collapsed + group, expanded - group) else Groups(collapsed - group, expanded + group)
}

/** Where an action applies: Done and Snooze only where they file mail; Delete in the trash is for good. */
fun SwipeAction.appliesTo(message: MessageItem): Boolean = when (this) {
    SwipeAction.DONE -> message.canArchive
    SwipeAction.SNOOZE -> message.canSnooze
    SwipeAction.READ, SwipeAction.DELETE -> true
}

/**
 * A row that does [swipes] when swiped; a side whose action does not apply to [message] stays
 * still, as the whole row does unless [enabled]. [onSwipe] gets a way to bring the row back.
 * [onPull] hears how far the row is held from its place, so its neighbours can be drawn along.
 *
 * The row slides off a bare strip. The action's icon waits at the edge in a pale round drop;
 * once letting go would act, the drop fills with colour and the word slides out beside it.
 */
@Composable
fun SwipeRow(
    message: MessageItem,
    swipes: Swipes,
    enabled: Boolean,
    onSwipe: (SwipeAction, reset: () -> Unit) -> Unit,
    onPull: (Float) -> Unit,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    // Not saved: the list keeps a row's saved state while it is gone, so a row brought back
    // by Undo would come back swiped and do its action again.
    val density = LocalDensity.current
    val threshold = SwipeToDismissBoxDefaults.positionalThreshold
    val swiped by rememberUpdatedState(onSwipe)
    val pulled by rememberUpdatedState(onPull)
    val current by rememberUpdatedState(swipes)
    fun SwipeToDismissBoxValue.action() = when (this) {
        SwipeToDismissBoxValue.EndToStart -> current.left
        SwipeToDismissBoxValue.StartToEnd -> current.right
        SwipeToDismissBoxValue.Settled -> null
    }
    val state = remember {
        SwipeToDismissBoxState(
            SwipeToDismissBoxValue.Settled,
            density,
            // Read keeps the row in the list: it goes back from where the finger let go as the
            // mark changes, rather than out off the screen and in again.
            confirmValueChange = { value ->
                if (value.action() == SwipeAction.READ) {
                    swiped(SwipeAction.READ) {}
                    false
                } else {
                    true
                }
            },
            positionalThreshold = threshold,
        )
    }
    LaunchedEffect(state.currentValue) {
        val action = state.currentValue.action() ?: return@LaunchedEffect
        swiped(action) { scope.launch { state.reset() } }
    }
    // Only while a finger holds the row: once let go, its neighbours settle back.
    var held by remember { mutableStateOf(false) }
    LaunchedEffect(state) {
        snapshotFlow { if (held) state.offsetOrZero() else 0f }.collect { pulled(it) }
    }
    val colors = MaterialTheme.colorScheme
    SwipeToDismissBox(
        state = state,
        modifier = Modifier.whilePressed { held = it },
        enableDismissFromStartToEnd = enabled && swipes.right.appliesTo(message),
        enableDismissFromEndToStart = enabled && swipes.left.appliesTo(message),
        backgroundContent = {
            val left = state.dismissDirection == SwipeToDismissBoxValue.EndToStart
            val action = if (left) swipes.left else swipes.right
            val label = when {
                action == SwipeAction.READ && message.threadUnread == 0u -> R.string.action_unread
                action == SwipeAction.DELETE && !message.canTrash -> R.string.action_delete_forever
                else -> action.label
            }
            val (fill, on) = action.colors()
            // Whether letting go now would act: the drop fills a little slower than the icon nudges.
            val armed = state.targetValue != SwipeToDismissBoxValue.Settled
            val flood by animateFloatAsState(if (armed) 1f else 0f, Motion.drift(), label = "flood")
            val arm by animateFloatAsState(if (armed) 1f else 0f, Motion.pop(), label = "arm")
            val shown = arm.coerceIn(0f, 1f)
            // A small swell as the drop fills, and the overshoot of the spring past it.
            val swell = arm - shown * shown
            val side = if (left) -1f else 1f
            Row(
                Modifier.fillMaxSize().background(colors.background).padding(horizontal = Gap.screen - 2.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp, if (left) Alignment.End else Alignment.Start),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val word = @Composable {
                    Text(
                        stringResource(label),
                        color = fill,
                        style = Type.Text,
                        fontWeight = Weight.SemiBold,
                        modifier = Modifier.graphicsLayer {
                            alpha = shown
                            translationX = side * (1f - shown) * 8.dp.toPx()
                        },
                    )
                }
                if (left) word()
                // The drop: it grows in as the row uncovers it, and stays put at the edge.
                Box(
                    Modifier
                        .size(36.dp)
                        .graphicsLayer {
                            val seen = ((abs(state.offsetOrZero()) - 14.dp.toPx()) / 36.dp.toPx()).coerceIn(0f, 1f)
                            scaleX = seen * (1f + 0.14f * swell)
                            scaleY = scaleX
                        }
                        .background(fill.copy(alpha = 0.16f + 0.84f * flood), CircleShape),
                    contentAlignment = Alignment.Center,
                ) {
                    Glyph(action.icon, null, lerp(fill, on, flood), size = 20.dp)
                }
                if (!left) word()
            }
        },
    ) {
        Box(Modifier.background(colors.background)) { content() }
    }
}

/** How far the row is from its place; zero before it is first laid out. */
private fun SwipeToDismissBoxState.offsetOrZero(): Float =
    if (dismissDirection == SwipeToDismissBoxValue.Settled) 0f else requireOffset()

/**
 * The row a finger holds and how far it has gone. Its neighbours are drawn a little way after
 * it, as through water: the next row by an eighth, the one after by a twenty-fifth.
 */
@Stable
class Pull {
    private var message by mutableLongStateOf(-1L)
    private var row by mutableIntStateOf(-1)
    private var offset by mutableFloatStateOf(0f)

    /** The row with [id], now at [index], is held [by] from its place; zero lets it go. */
    fun hold(id: Long, index: Int, by: Float) {
        if (by != 0f) {
            message = id
            row = index
            offset = by
        } else if (message == id) {
            message = -1L
            row = -1
            offset = 0f
        }
    }

    fun near(index: Int): Float = when {
        row < 0 -> 0f
        abs(index - row) == 1 -> offset * 0.12f
        abs(index - row) == 2 -> offset * 0.04f
        else -> 0f
    }
}

/** The fill and what sits on it: Done the accent, as everywhere; Read the quiet slate, Delete the warning red. */
@Composable
private fun SwipeAction.colors(): Pair<Color, Color> = with(MaterialTheme.colorScheme) {
    when (this@colors) {
        SwipeAction.DONE -> accent to onAccent
        SwipeAction.READ -> secondary to onSecondary
        SwipeAction.DELETE -> error to onError
        SwipeAction.SNOOZE -> tertiary to onTertiary
    }
}

/** "Left: Done, right: Read". */
@Composable
fun Swipes.summary(): String = stringResource(R.string.swipes_summary, stringResource(left.label), stringResource(right.label))

/** A one-time note above the list on what the swipes do. */
@Composable
fun SwipeTip(swipes: Swipes, onDismiss: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.card, vertical = 4.dp)
            .clip(RoundedCornerShape(Radius.tile))
            .background(colors.primary.copy(alpha = 0.1f))
            .padding(start = Gap.inCard, end = 6.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            stringResource(R.string.swipes_tip, stringResource(swipes.left.label), stringResource(swipes.right.label)),
            style = Type.Text,
            modifier = Modifier.weight(1f),
        )
        TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_got_it)) }
    }
}

/** The choice for each side, inside the accounts sheet. */
@Composable
fun SwipeSettings(swipes: Swipes, onChange: (Swipes) -> Unit) {
    Side(R.string.swipes_left, swipes.left) { onChange(swipes.copy(left = it)) }
    Side(R.string.swipes_right, swipes.right) { onChange(swipes.copy(right = it)) }
}

@Composable
private fun Side(@StringRes title: Int, picked: SwipeAction, onPick: (SwipeAction) -> Unit) {
    val colors = MaterialTheme.colorScheme
    // Four to a row: the name goes under the icon to fit.
    Choices(title, SwipeAction.entries, picked, onPick, height = 56.dp) { action, on ->
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Glyph(action.icon, null, if (on) colors.primary else colors.onSurfaceVariant, size = 18.dp)
            Text(
                stringResource(action.label),
                color = if (on) colors.primary else colors.onSurface,
                style = Type.Label,
                maxLines = 1,
            )
        }
    }
}

/** Equal choices in a row under a caption; the picked one is outlined in the accent colour. */
@Composable
fun <T> Choices(
    @StringRes title: Int,
    options: List<T>,
    picked: T,
    onPick: (T) -> Unit,
    height: Dp = 44.dp,
    content: @Composable (option: T, on: Boolean) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Text(
        stringResource(title),
        color = colors.onSurfaceVariant,
        style = Type.Caption,
        modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 8.dp, bottom = 8.dp),
    )
    // More than four take two rows, so each keeps room for its words.
    val perRow = if (options.size > 4) (options.size + 1) / 2 else options.size
    Column(
        Modifier.fillMaxWidth().padding(horizontal = Gap.sheet).padding(bottom = 8.dp).selectableGroup(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        options.chunked(perRow).forEach { row ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                row.forEach { option ->
                    val on = option == picked
                    val shape = RoundedCornerShape(Radius.medium)
                    Row(
                        Modifier
                            .weight(1f)
                            .height(height)
                            .clip(shape)
                            .background(if (on) colors.primary.copy(alpha = 0.1f) else colors.surfaceVariant)
                            .then(if (on) Modifier.border(1.5.dp, colors.primary, shape) else Modifier)
                            .selectable(selected = on, role = Role.RadioButton, onClick = { onPick(option) }),
                        horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        content(option, on)
                    }
                }
                // A short last row keeps the widths of the first.
                repeat(perRow - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

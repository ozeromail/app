package com.ozeromail.app

import android.content.Context
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.ozero_api.MessageItem

/** What a swipe on a row does; the person picks one for each side. */
enum class SwipeAction(@StringRes val label: Int, @DrawableRes val icon: Int) {
    DONE(R.string.action_done, R.drawable.ic_done),
    READ(R.string.action_read, R.drawable.ic_unread),
    DELETE(R.string.action_delete, R.drawable.ic_delete),
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

/** Where an action applies: Done only where it files mail; Delete in the trash is for good. */
fun SwipeAction.appliesTo(message: MessageItem): Boolean = this != SwipeAction.DONE || message.canArchive

/**
 * A row that does [swipes] when swiped; a side whose action does not apply to [message] stays
 * still, as the whole row does unless [enabled]. [onSwipe] gets a way to bring the row back.
 */
@Composable
fun SwipeRow(
    message: MessageItem,
    swipes: Swipes,
    enabled: Boolean,
    onSwipe: (SwipeAction, reset: () -> Unit) -> Unit,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    // Not saved: the list keeps a row's saved state while it is gone, so a row brought back
    // by Undo would come back swiped and do its action again.
    val density = LocalDensity.current
    val threshold = SwipeToDismissBoxDefaults.positionalThreshold
    val state = remember { SwipeToDismissBoxState(SwipeToDismissBoxValue.Settled, density, positionalThreshold = threshold) }
    val swiped by rememberUpdatedState(onSwipe)
    val current by rememberUpdatedState(swipes)
    LaunchedEffect(state.currentValue) {
        val action = when (state.currentValue) {
            SwipeToDismissBoxValue.EndToStart -> current.left
            SwipeToDismissBoxValue.StartToEnd -> current.right
            SwipeToDismissBoxValue.Settled -> return@LaunchedEffect
        }
        swiped(action) { scope.launch { state.reset() } }
    }
    val colors = MaterialTheme.colorScheme
    SwipeToDismissBox(
        state = state,
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
            val (background, content) = action.colors()
            Row(
                Modifier.fillMaxSize().background(background).padding(horizontal = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp, if (left) Alignment.End else Alignment.Start),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Glyph(action.icon, null, content)
                Text(stringResource(label), color = content, style = Type.Text, fontWeight = Weight.SemiBold)
            }
        },
    ) {
        Box(Modifier.background(colors.background)) { content() }
    }
}

/** The fill and what sits on it: Done the accent, as everywhere; Read the quiet slate, Delete the warning red. */
@Composable
private fun SwipeAction.colors(): Pair<Color, Color> = with(MaterialTheme.colorScheme) {
    when (this@colors) {
        SwipeAction.DONE -> primary to onPrimary
        SwipeAction.READ -> secondary to onSecondary
        SwipeAction.DELETE -> error to onError
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
    Text(
        stringResource(title),
        color = colors.onSurfaceVariant,
        style = Type.Caption,
        modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 8.dp, bottom = 8.dp),
    )
    Row(
        Modifier.fillMaxWidth().padding(horizontal = Gap.sheet).padding(bottom = 8.dp).selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        SwipeAction.entries.forEach { action ->
            val on = action == picked
            val tint = if (on) colors.primary else colors.onSurface
            val shape = RoundedCornerShape(Radius.medium)
            Row(
                Modifier
                    .weight(1f)
                    .height(44.dp)
                    .clip(shape)
                    .background(if (on) colors.primary.copy(alpha = 0.1f) else colors.surfaceVariant)
                    .then(if (on) Modifier.border(1.5.dp, colors.primary, shape) else Modifier)
                    .selectable(selected = on, role = Role.RadioButton, onClick = { onPick(action) }),
                horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Glyph(action.icon, null, if (on) colors.primary else colors.onSurfaceVariant, size = 18.dp)
                Text(stringResource(action.label), color = tint, style = Type.Text, fontWeight = Weight.Medium, maxLines = 1)
            }
        }
    }
}

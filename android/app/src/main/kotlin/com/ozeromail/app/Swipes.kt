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
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
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

    private fun action(name: String?) = SwipeAction.entries.find { it.name == name }

    private companion object {
        const val LEFT = "swipe_left"
        const val RIGHT = "swipe_right"
        const val TIP = "swipe_tip_seen"
    }
}

/** Where an action applies; Read always does, toggling the row. */
fun SwipeAction.appliesTo(message: MessageItem): Boolean = when (this) {
    SwipeAction.DONE -> message.canArchive
    SwipeAction.DELETE -> message.canTrash
    SwipeAction.READ -> true
}

/**
 * A row that does [swipes] when swiped; a side whose action does not apply to [message] stays
 * still. [onSwipe] gets a way to bring the row back.
 */
@Composable
fun SwipeRow(
    message: MessageItem,
    swipes: Swipes,
    onSwipe: (SwipeAction, reset: () -> Unit) -> Unit,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val state = rememberSwipeToDismissBoxState()
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
        enableDismissFromStartToEnd = swipes.right.appliesTo(message),
        enableDismissFromEndToStart = swipes.left.appliesTo(message),
        backgroundContent = {
            val left = state.dismissDirection == SwipeToDismissBoxValue.EndToStart
            val action = if (left) swipes.left else swipes.right
            val label = if (action == SwipeAction.READ && message.threadUnread == 0u) R.string.action_unread else action.label
            Row(
                Modifier.fillMaxSize().background(action.color()).padding(horizontal = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp, if (left) Alignment.End else Alignment.Start),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Glyph(action.icon, null, Color.White)
                Text(stringResource(label), color = Color.White, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            }
        },
    ) {
        Box(Modifier.background(colors.background)) { content() }
    }
}

/** Done is green, as everywhere; Read takes the accent, Delete the warning red. */
@Composable
private fun SwipeAction.color(): Color = when (this) {
    SwipeAction.DONE -> MaterialTheme.done
    SwipeAction.READ -> MaterialTheme.colorScheme.primary
    SwipeAction.DELETE -> MaterialTheme.colorScheme.error
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
            fontSize = 13.5.sp,
            lineHeight = 18.6.sp,
            modifier = Modifier.weight(1f),
        )
        TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_got_it), fontWeight = FontWeight.SemiBold) }
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
        fontSize = 13.sp,
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
                Text(stringResource(action.label), color = tint, fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1)
            }
        }
    }
}

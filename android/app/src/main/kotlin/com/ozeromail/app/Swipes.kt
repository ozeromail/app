package com.ozeromail.app

import android.content.Context
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
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
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.ozero_api.MessageItem

/** What a swipe on a row does; the person picks a short and a long one for each side. */
enum class SwipeAction(@StringRes val label: Int, @DrawableRes val icon: Int) {
    DONE(R.string.action_done, R.drawable.ic_done),
    READ(R.string.action_read, R.drawable.ic_unread),
    DELETE(R.string.action_delete, R.drawable.ic_delete),
    SNOOZE(R.string.action_snooze, R.drawable.ic_snooze),
    FLAG(R.string.swipe_flag, R.drawable.ic_flag),
    MOVE(R.string.swipe_move, R.drawable.ic_folder),
    SPAM(R.string.action_spam, R.drawable.ic_spam),

    /** Not picked: it takes the place of one that does not apply where the message is, see [on]. */
    INBOX(R.string.action_move_to_inbox, R.drawable.ic_inbox),
    ;

    /** Read and Flag change the row and leave it in the list; the rest take it away. */
    val stays: Boolean
        get() = this == READ || this == FLAG

    companion object {
        /** What the person picks from for a swipe. */
        val PICKED = entries - INBOX
    }
}

/**
 * Left is Done and, further, Delete; right is Read and, further, Snooze. A long swipe can be
 * off, null, so the short one is all that side does.
 */
data class Swipes(
    val left: SwipeAction = SwipeAction.DONE,
    val right: SwipeAction = SwipeAction.READ,
    val leftLong: SwipeAction? = SwipeAction.DELETE,
    val rightLong: SwipeAction? = SwipeAction.SNOOZE,
)

/** The sizes of mail's text the person can pick from, against the usual one. */
val TEXT_SCALES = listOf(0.9f, 1f, 1.1f, 1.2f, 1.35f)
const val USUAL_SIZE = 1

/** The person's choices, kept on the phone. */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var swipes: Swipes
        get() = Swipes().let {
            // A long swipe turned off is kept as OFF, so it does not come back as the default.
            // Who picked their swipes before long ones came keeps them off, so a far swipe does what it did.
            val picked = prefs.contains(LEFT)
            fun long(key: String, usual: SwipeAction?) = prefs.getString(key, null).let { name ->
                if (name == OFF) null else action(name) ?: usual.takeIf { !picked }
            }
            Swipes(
                left = action(prefs.getString(LEFT, null)) ?: it.left,
                right = action(prefs.getString(RIGHT, null)) ?: it.right,
                leftLong = long(LEFT_LONG, it.leftLong),
                rightLong = long(RIGHT_LONG, it.rightLong),
            )
        }
        set(value) = prefs.edit()
            .putString(LEFT, value.left.name)
            .putString(RIGHT, value.right.name)
            .putString(LEFT_LONG, value.leftLong?.name ?: OFF)
            .putString(RIGHT_LONG, value.rightLong?.name ?: OFF)
            .apply()

    /** The tip about swipes was read, so it does not show again. */
    var swipeTipSeen: Boolean
        get() = prefs.getBoolean(TIP, false)
        set(value) = prefs.edit().putBoolean(TIP, value).apply()

    /** The size of mail's text, a place in [TEXT_SCALES]. */
    var textSize: Int
        get() = prefs.getInt(TEXT_SIZE, USUAL_SIZE).coerceIn(TEXT_SCALES.indices)
        set(value) = prefs.edit().putInt(TEXT_SIZE, value).apply()

    /** How much larger than usual mail's text is. */
    val textScale: Float
        get() = TEXT_SCALES[textSize]

    /** An account was held in the accounts sheet, so the tip about it does not show again. */
    var holdTipSeen: Boolean
        get() = prefs.getBoolean(HOLD_TIP, false)
        set(value) = prefs.edit().putBoolean(HOLD_TIP, value).apply()

    /** The phone asked for its contacts once, from "To": the line offering them shows no more. */
    var contactsAsked: Boolean
        get() = prefs.getBoolean(CONTACTS_ASKED, false)
        set(value) = prefs.edit().putBoolean(CONTACTS_ASKED, value).apply()

    /** The mailboxes synced on this phone, by [Mailbox] key: only one never synced waits for its first mail. */
    var syncedBoxes: Set<Long>
        get() = ids(SYNCED)
        set(value) = prefs.edit().putStringSet(SYNCED, value.strings()).apply()

    /** The groups of the accounts sheet the person closed or opened. */
    var groups: Groups
        get() = Groups(ids(EXPANDED), prefs.getStringSet(CLOSED, null).orEmpty())
        set(value) = prefs.edit().putStringSet(EXPANDED, value.expanded.strings()).putStringSet(CLOSED, value.closed).apply()

    /** The mailboxes of all accounts below the inboxes in the accounts sheet: their order and which show. */
    var allBoxes: AllBoxes
        get() {
            fun boxes(key: String) = prefs.getString(key, null)?.split(',')?.mapNotNull { name -> AllBox.entries.find { it.name == name } }
            // One added later goes at the end.
            val order = boxes(ALL_ORDER).orEmpty().let { saved -> saved + AllBox.entries.filter { it !in saved } }
            return AllBoxes(order, boxes(ALL_SHOWN)?.toSet() ?: AllBox.SHOWN)
        }
        set(value) = prefs.edit()
            .putString(ALL_ORDER, value.order.joinToString(",") { it.name })
            .putString(ALL_SHOWN, value.shown.joinToString(",") { it.name })
            .apply()

    private fun ids(key: String) = prefs.getStringSet(key, null).orEmpty().mapNotNull { it.toLongOrNull() }.toSet()

    private fun Set<Long>.strings() = map { it.toString() }.toSet()

    private fun action(name: String?) = SwipeAction.PICKED.find { it.name == name }

    companion object {
        private const val LEFT = "swipe_left"
        private const val RIGHT = "swipe_right"
        private const val LEFT_LONG = "swipe_left_long"
        private const val RIGHT_LONG = "swipe_right_long"
        private const val OFF = "OFF"
        private const val TIP = "swipe_tip_seen"
        private const val EXPANDED = "expanded_groups"
        private const val CLOSED = "closed_parts"
        private const val SYNCED = "synced_boxes"
        private const val ALL_ORDER = "all_boxes_order"
        private const val ALL_SHOWN = "all_boxes_shown"
        private const val TEXT_SIZE = "text_size"
        private const val HOLD_TIP = "hold_tip_seen"
        private const val CONTACTS_ASKED = "contacts_asked"
    }
}

/**
 * The accounts whose folders the accounts sheet shows, and the parts of it the person closed.
 * Accounts start closed, so the sheet stays short; parts start open.
 */
data class Groups(val expanded: Set<Long>, val closed: Set<String> = emptySet()) {
    fun open(account: Long) = account in expanded

    fun toggle(account: Long) = copy(expanded = if (open(account)) expanded - account else expanded + account)

    fun open(part: String) = part !in closed

    fun toggle(part: String) = copy(closed = if (open(part)) closed + part else closed - part)
}

/** Where an action applies: Done and Snooze only where they file mail; Delete in the trash is for good. */
fun SwipeAction.appliesTo(message: MessageItem): Boolean = when (this) {
    SwipeAction.DONE -> message.canArchive
    SwipeAction.SNOOZE -> message.canSnooze
    SwipeAction.MOVE -> message.canMove
    SwipeAction.SPAM -> message.canSpam
    SwipeAction.INBOX -> message.canMoveToInbox
    SwipeAction.READ, SwipeAction.DELETE, SwipeAction.FLAG -> true
}

/**
 * The swipes as they work on [message], so every folder has them: a short one that does not
 * apply where it lies gives way to Move to Inbox out of the archive, trash, spam or snoozed
 * folder, on one side only, else to the side's long one, as Delete in Sent. A side with nothing that applies
 * keeps its short one and stays still.
 */
fun Swipes.on(message: MessageItem): Swipes {
    fun side(short: SwipeAction, long: SwipeAction?, inbox: Boolean): Pair<SwipeAction, SwipeAction?> = when {
        short.appliesTo(message) -> short to long
        inbox && SwipeAction.INBOX.appliesTo(message) -> SwipeAction.INBOX to long
        long != null && long.appliesTo(message) -> long to null
        else -> short to long
    }
    // Move to Inbox on one side only, so the other keeps a choice of the person's own.
    val (left, leftLong) = side(left, leftLong, inbox = true)
    val (right, rightLong) = side(right, rightLong, inbox = left != SwipeAction.INBOX)
    return Swipes(left, right, leftLong, rightLong)
}

/** What the action says on [message]: Read on a read one is Unread, Flag on a flagged one takes it off. */
fun SwipeAction.label(message: MessageItem): Int = when {
    this == SwipeAction.READ && message.threadUnread == 0u -> R.string.action_unread
    this == SwipeAction.FLAG && message.flagged -> R.string.action_unflag
    this == SwipeAction.DELETE && !message.canTrash -> R.string.action_delete_forever
    else -> label
}

/** How far a row goes, as a share of its width, before letting go does the short swipe, and the long one. */
private const val SHORT = 0.25f
private const val LONG = 0.6f

/**
 * A row that does [swipes] when swiped; a side whose short action [applies] says no to stays
 * still, as the whole row does unless [enabled]. [label] names an action on the row. [onSwipe] gets a way to bring the row
 * back. [onPull] hears how far the row is held from its place, so its neighbours can be drawn along.
 *
 * The row slides off a bare strip. The short action's icon waits at the edge in a pale round
 * drop; once letting go would act, the drop fills with colour and the word slides out beside
 * it. Further on, past the long swipe's mark, the whole strip floods with the long action's
 * colour and its drop rides along with the row. Each mark crossed, either way, ticks.
 */
@Composable
fun SwipeRow(
    swipes: Swipes,
    enabled: Boolean,
    applies: (SwipeAction) -> Boolean,
    label: (SwipeAction) -> Int,
    onSwipe: (SwipeAction, reset: () -> Unit) -> Unit,
    onPull: (Float) -> Unit,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val density = LocalDensity.current
    val swiped by rememberUpdatedState(onSwipe)
    val pulled by rememberUpdatedState(onPull)
    val current by rememberUpdatedState(swipes)
    val applying by rememberUpdatedState(applies)
    // Not saved: the list keeps a row's saved state while it is gone, so a row brought back
    // by Undo would come back swiped and do its action again.
    var offset by remember { mutableFloatStateOf(0f) }
    var width by remember { mutableIntStateOf(0) }
    var held by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf<Job?>(null) }
    // The stage let go at once the row acts, so the strip keeps showing it as the row goes.
    var acted by remember { mutableStateOf<Int?>(null) }

    // The short and long action of the side the row goes to; a long one that does not apply is no long one.
    fun side(left: Boolean): Pair<SwipeAction, SwipeAction?> {
        val (short, long) = if (left) current.left to current.leftLong else current.right to current.rightLong
        return short to long?.takeIf(applying)
    }
    fun open(left: Boolean) = enabled && applying(side(left).first)
    // 0 while letting go does nothing, 1 for the short action, 2 for the long one.
    val stage by remember {
        derivedStateOf {
            val far = if (width == 0) 0f else abs(offset) / width
            acted ?: when {
                offset == 0f -> 0
                far >= LONG && side(offset < 0f).second != null -> 2
                far >= SHORT -> 1
                else -> 0
            }
        }
    }
    // Only the side and whether the row is out at all, so a drag does not recompose it each frame.
    val out by remember { derivedStateOf { offset != 0f } }
    val leftward by remember { derivedStateOf { offset < 0f } }
    LaunchedEffect(Unit) {
        var last = 0
        snapshotFlow { stage to (if (held) offset else 0f) }.collect { (now, by) ->
            if (held && now != last) {
                haptics.performHapticFeedback(if (now > last) HapticFeedbackType.GestureThresholdActivate else HapticFeedbackType.SegmentTick)
            }
            last = now
            pulled(by)
        }
    }
    fun glide(to: Float, then: () -> Unit = {}) {
        moving?.cancel()
        moving = scope.launch {
            animate(offset, to, animationSpec = Motion.settle()) { value, _ -> offset = value }
            then()
        }
    }
    fun release(velocity: Float) {
        val flick = with(density) { 800.dp.toPx() }
        held = false
        val left = offset < 0f
        val (short, long) = side(left)
        // A quick flick the row's way does the short swipe, as a slow pull that far would not.
        val flicked = stage == 0 && abs(offset) > width * 0.08f && abs(velocity) > flick && (velocity < 0f) == left
        val action = when {
            stage == 2 -> long
            stage == 1 || flicked -> short
            else -> null
        }
        when {
            action == null -> glide(0f)
            action.stays -> {
                swiped(action) {}
                glide(0f)
            }
            else -> {
                acted = stage.coerceAtLeast(1)
                glide(if (left) -width.toFloat() else width.toFloat()) {
                    swiped(action) {
                        moving?.cancel()
                        acted = null
                        offset = 0f
                    }
                }
            }
        }
    }

    val colors = MaterialTheme.colorScheme
    Box(
        Modifier
            .onSizeChanged { width = it.width }
            .draggable(
                rememberDraggableState { delta ->
                    val to = offset + delta
                    offset = when {
                        to > 0f && !open(left = false) -> 0f
                        to < 0f && !open(left = true) -> 0f
                        else -> to.coerceIn(-width.toFloat(), width.toFloat())
                    }
                },
                Orientation.Horizontal,
                enabled = enabled,
                onDragStarted = {
                    moving?.cancel()
                    held = true
                },
                onDragStopped = { release(it) },
            ),
    ) {
        if (out) {
            val left = leftward
            val (short, long) = side(left)
            val action = long?.takeIf { stage == 2 } ?: short
            val far = stage == 2
            val (fill, on) = action.colors()
            // Whether letting go now would act: the drop fills a little slower than the icon nudges.
            val flood by animateFloatAsState(if (far) 1f else 0f, Motion.drift(), label = "flood")
            val pour by animateFloatAsState(if (stage > 0) 1f else 0f, Motion.drift(), label = "pour")
            val arm by animateFloatAsState(if (stage > 0) 1f else 0f, Motion.pop(), label = "arm")
            val armed = arm.coerceIn(0f, 1f)
            // A small swell as the drop fills, and the overshoot of the spring past it.
            val swell = arm - armed * armed
            val way = if (left) -1f else 1f
            var group by remember { mutableIntStateOf(0) }
            Row(
                Modifier
                    .matchParentSize()
                    .background(lerp(colors.background, fill, flood))
                    .padding(horizontal = Gap.screen - 2.dp),
                horizontalArrangement = if (left) Arrangement.End else Arrangement.Start,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Row(
                    Modifier
                        .onSizeChanged { group = it.width }
                        .graphicsLayer {
                            // Flooded, the drop and its word ride just ahead of the row's edge.
                            val ahead = (abs(offset) - group - (Gap.screen + 10.dp).toPx()).coerceAtLeast(0f)
                            translationX = way * ahead * flood
                        },
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    val word = @Composable {
                        Text(
                            stringResource(label(action)),
                            color = lerp(fill, on, flood),
                            style = Type.Text,
                            fontWeight = Weight.SemiBold,
                            modifier = Modifier.graphicsLayer {
                                alpha = armed
                                translationX = way * (1f - armed) * 8.dp.toPx()
                            },
                        )
                    }
                    if (left) word()
                    // The drop: it grows in as the row uncovers it.
                    Box(
                        Modifier
                            .size(36.dp)
                            .graphicsLayer {
                                val seen = ((abs(offset) - 14.dp.toPx()) / 36.dp.toPx()).coerceIn(0f, 1f)
                                scaleX = seen * (1f + 0.14f * swell)
                                scaleY = scaleX
                            }
                            .background(lerp(fill.copy(alpha = 0.16f + 0.84f * pour), on.copy(alpha = 0.22f), flood), CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        Glyph(action.icon, null, lerp(fill, on, maxOf(pour, flood)), size = 20.dp)
                    }
                    if (!left) word()
                }
            }
        }
        Box(
            Modifier
                .offset { IntOffset(offset.roundToInt(), 0) }
                .background(colors.background),
        ) { content() }
    }
}

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

/**
 * The fill and what sits on it: Done the accent, as everywhere; Read the quiet slate, Delete
 * the warning red, Snooze amber, Flag orange as flags are, a folder violet, spam grey.
 */
@Composable
fun SwipeAction.colors(): Pair<Color, Color> = with(MaterialTheme.colorScheme) {
    when (this@colors) {
        SwipeAction.DONE -> accent to onAccent
        SwipeAction.READ -> secondary to onSecondary
        SwipeAction.DELETE -> errorFill to onError
        SwipeAction.SNOOZE -> tertiary to onTertiary
        SwipeAction.FLAG -> Color(0xFFEA7A1E) to Color.White
        SwipeAction.MOVE -> Color(0xFF7A6FF0) to Color.White
        SwipeAction.SPAM -> Color(0xFF7C8799) to Color.White
        SwipeAction.INBOX -> accent to onAccent
    }
}

/** "← Done · Read →". */
@Composable
fun Swipes.summary(): String = stringResource(R.string.swipes_summary, stringResource(left.label), stringResource(right.label))

/** What the tip says the swipes do, with the long ones when they are on, and, with [lift], how the mailboxes open. */
@Composable
private fun Swipes.tip(lift: Boolean): String {
    @Composable
    fun side(short: SwipeAction, long: SwipeAction?): String =
        long?.let { stringResource(R.string.swipes_further, stringResource(short.label), stringResource(it.label)) }
            ?: stringResource(short.label)
    val left = side(left, leftLong)
    val right = side(right, rightLong)
    val said = stringResource(R.string.swipes_tip, left, right)
    return if (lift) said + " " + stringResource(R.string.swipes_tip_lift) else said
}

/** A one-time note above the list on what the swipes do. */
@Composable
fun SwipeTip(swipes: Swipes, lift: Boolean, onDismiss: () -> Unit) {
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
            swipes.tip(lift),
            style = Type.Text,
            modifier = Modifier.weight(1f),
        )
        TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_got_it)) }
    }
}

/** The short and the long choice for each side, on its page of the settings; a long one can be off. */
@Composable
fun SwipeSettings(swipes: Swipes, onChange: (Swipes) -> Unit) {
    Side(R.string.swipes_left, SwipeAction.PICKED, swipes.left) { onChange(swipes.copy(left = it)) }
    Side(R.string.swipes_left_long, SwipeAction.PICKED + null, swipes.leftLong) { onChange(swipes.copy(leftLong = it)) }
    Side(R.string.swipes_right, SwipeAction.PICKED, swipes.right) { onChange(swipes.copy(right = it)) }
    Side(R.string.swipes_right_long, SwipeAction.PICKED + null, swipes.rightLong) { onChange(swipes.copy(rightLong = it)) }
}

@Composable
private fun <T : SwipeAction?> Side(@StringRes title: Int, options: List<T>, picked: T, onPick: (T) -> Unit) {
    val colors = MaterialTheme.colorScheme
    // Four to a row: the name goes under the icon to fit.
    Choices(title, options, picked, onPick, height = 56.dp) { action, on ->
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Glyph(action?.icon ?: R.drawable.ic_close, null, if (on) colors.primary else colors.onSurfaceVariant, size = 18.dp)
            Text(
                stringResource(action?.label ?: R.string.swipes_off),
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
        modifier = Modifier.padding(start = Gap.inset, end = Gap.inset, top = 8.dp, bottom = 8.dp),
    )
    // More than four take two rows, so each keeps room for its words.
    val perRow = (if (options.size > 4) (options.size + 1) / 2 else options.size).coerceAtLeast(1)
    Column(
        Modifier.fillMaxWidth().padding(horizontal = Gap.screen).padding(bottom = 8.dp).selectableGroup(),
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

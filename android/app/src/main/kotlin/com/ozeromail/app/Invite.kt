package com.ozeromail.app

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.icu.util.TimeZone
import android.net.Uri
import android.provider.CalendarContract
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.TextStyle
import uniffi.ozero_api.InviteAnswer
import uniffi.ozero_api.InviteItem
import uniffi.ozero_api.InviteMark
import uniffi.ozero_api.RepeatEvery
import uniffi.ozero_api.RepeatItem
import kotlin.math.abs

/**
 * The meeting a message invites to, over its text: when, where, who, how to join, and what
 * else is on around it. Once answered it says what, and [onChange] answers again; where the
 * meeting lands in no calendar, [onAddToCalendar] puts it in the phone's.
 */
@Composable
fun InviteCard(invite: InviteItem, onChange: (() -> Unit)?, onAddToCalendar: () -> Unit, modifier: Modifier = Modifier) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    val zone = if (invite.allDay) UTC else null
    Column(
        modifier
            .fillMaxWidth()
            .padding(start = Gap.card, end = Gap.card, top = 4.dp, bottom = 12.dp)
            .clip(RoundedCornerShape(Radius.tile))
            .border(1.dp, colors.outlineVariant, RoundedCornerShape(Radius.tile)),
    ) {
        Row(Modifier.padding(start = Gap.inCard, end = Gap.inCard, top = 14.dp, bottom = 12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            CalendarLeaf(invite.start, zone)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                when {
                    invite.cancelled -> Badge(stringResource(R.string.invite_cancelled), colors.error)
                    invite.before != null && invite.answer == null -> Badge(stringResource(R.string.invite_moved), colors.tertiary)
                }
                Text(invite.summary.ifBlank { stringResource(R.string.inbox_no_subject) }, style = Type.Row, fontWeight = Weight.Strong)
                val before = invite.before
                if (before != null) {
                    Text(
                        whenText(context, before.start, before.end, invite.allDay),
                        color = colors.onSurfaceVariant,
                        style = Type.Caption,
                        textDecoration = TextDecoration.LineThrough,
                    )
                }
                Text(
                    whenText(context, invite.start, invite.end, invite.allDay),
                    color = if (before != null) colors.onSurface else colors.onSurfaceVariant,
                    style = Type.Caption,
                    fontWeight = if (before != null) Weight.SemiBold else null,
                )
                theirTime(context, invite)?.let { Text(it, color = colors.onSurfaceVariant, style = Type.Caption) }
            }
        }
        Column(Modifier.padding(start = Gap.inCard, end = Gap.inCard, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            if (invite.location.isNotBlank()) Meta(R.drawable.ic_place) { Text(invite.location, style = Type.Caption) }
            invite.link?.let { link ->
                Meta(R.drawable.ic_video) {
                    Text(
                        stringResource(R.string.invite_join),
                        color = colors.primary,
                        style = Type.Caption,
                        fontWeight = Weight.SemiBold,
                        modifier = Modifier.clickable(role = Role.Button) { openLink(context, Uri.parse(link)) },
                    )
                }
            }
            val organizer = invite.organizerName ?: invite.organizerEmail
            if (organizer != null || invite.others > 0u) {
                Meta(R.drawable.ic_people) {
                    val others = stringResource(R.string.invite_others, invite.others.toInt()).takeIf { invite.others > 0u }
                    val who = organizer?.let { stringResource(R.string.invite_organizer, it) }
                    Text(listOfNotNull(who, others).joinToString(" · "), style = Type.Caption)
                }
            }
            invite.repeats?.let { repeats -> Meta(R.drawable.ic_repeat) { Text(repeatText(repeats), style = Type.Caption) } }
        }
        if (!invite.cancelled && !invite.allDay) DayStrip(invite)
        val answer = invite.answer
        if (!invite.cancelled && answer != null) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .hairline(colors.outlineVariant, top = true)
                    .padding(start = Gap.inCard, end = 10.dp, top = 8.dp, bottom = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                val tint = answerColor(answer)
                Glyph(answerIcon(answer), null, tint, size = 18.dp)
                Text(stringResource(answerStatus(answer)), color = tint, style = Type.Text, fontWeight = Weight.SemiBold, modifier = Modifier.weight(1f))
                if (invite.canAddToCalendar && answer != InviteAnswer.NO) {
                    CardButton(stringResource(R.string.invite_add_to_calendar), onAddToCalendar)
                }
                onChange?.let { CardButton(stringResource(R.string.invite_change), it) }
            }
        }
    }
}

/** The day of the month under its weekday, as on a calendar's leaf. */
@Composable
private fun CalendarLeaf(start: Long, zone: TimeZone?) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    Column(
        Modifier
            .width(48.dp)
            .background(colors.primaryContainer, RoundedCornerShape(12.dp))
            .padding(top = 6.dp, bottom = 7.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(formatDate(context, start, "EEE", zone).uppercase(), color = colors.primary, style = Type.Badge, letterSpacing = 0.8.sp)
        Text(formatDate(context, start, "d", zone), color = colors.primary, style = Type.Heading, fontWeight = Weight.Bold)
    }
}

@Composable
private fun Badge(text: String, tint: Color) {
    Text(
        text,
        color = tint,
        style = Type.Badge,
        modifier = Modifier
            .padding(bottom = 4.dp)
            .background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(50))
            .padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

/** A line of the card under its icon. */
@Composable
private fun Meta(@DrawableRes icon: Int, content: @Composable RowScope.() -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.Top) {
        Glyph(icon, null, MaterialTheme.colorScheme.onSurfaceVariant, Modifier.padding(top = 1.dp), size = 17.dp)
        content()
    }
}

@Composable
private fun CardButton(text: String, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Text(
        text,
        style = Type.Caption,
        fontWeight = Weight.SemiBold,
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .background(colors.surfaceVariant)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
    )
}

/**
 * The hours around the meeting the core picks, with the person's other meetings in them as
 * their invitations tell; one at the same time stands beside it, in red.
 */
@Composable
private fun DayStrip(invite: InviteItem) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    val from = invite.dayFrom
    val to = invite.dayTo
    val hours = ((to - from) / HOUR).toInt()
    val clash = invite.day.any { it.clashes }
    Column(
        Modifier
            .fillMaxWidth()
            .hairline(colors.outlineVariant, top = true)
            .padding(start = Gap.inCard, end = Gap.inCard, top = 10.dp, bottom = 12.dp),
    ) {
        Row(Modifier.fillMaxWidth().padding(bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(formatDate(context, invite.start, "EEEEMMMMd"), color = colors.onSurfaceVariant, style = Type.Badge, modifier = Modifier.weight(1f))
            Text(stringResource(R.string.invite_from_mail), color = colors.onSurfaceVariant, style = Type.Label, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        BoxWithConstraints(Modifier.fillMaxWidth().height(HOUR_HEIGHT * hours)) {
            for (i in 0 until hours) {
                Row(Modifier.offset(y = HOUR_HEIGHT * i - 7.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(formatDate(context, from + i * HOUR, time(context)), color = colors.onSurfaceVariant, style = Type.Label, modifier = Modifier.width(HOUR_LABEL))
                    HorizontalDivider(color = colors.outlineVariant)
                }
            }
            val lane = maxWidth - HOUR_LABEL
            @Composable
            fun Event(start: Long, end: Long, text: String, mine: Boolean, beside: Boolean) {
                val top = HOUR_HEIGHT * ((maxOf(start, from) - from).toFloat() / HOUR)
                val height = HOUR_HEIGHT * ((minOf(end, to) - maxOf(start, from)).toFloat() / HOUR) - 2.dp
                val ink = when {
                    mine -> colors.primary
                    beside -> colors.error
                    else -> colors.onSurfaceVariant
                }
                val x = HOUR_LABEL + if (beside) lane * SPLIT else 0.dp
                val width = if (mine && clash) lane * SPLIT - 4.dp else if (beside) lane * (1 - SPLIT) else lane
                Text(
                    text,
                    color = ink,
                    style = Type.Label,
                    fontWeight = if (mine) Weight.Strong else Weight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier
                        .offset(x = x, y = top + 1.dp)
                        .width(width)
                        .height(height.coerceAtLeast(14.dp))
                        .clip(RoundedCornerShape(7.dp))
                        .background(if (mine) colors.primaryContainer else colors.surfaceVariant)
                        .drawBehind { drawRect(if (mine || beside) ink else colors.outline, Offset.Zero, size.copy(width = 3.dp.toPx())) }
                        .padding(start = 9.dp, end = 6.dp, top = 1.dp),
                )
            }
            for (e in invite.day) Event(e.start, e.end, "${e.summary} · ${formatDate(context, e.start, time(context))}", mine = false, beside = e.clashes)
            Event(invite.start, invite.end, invite.summary, mine = true, beside = false)
        }
    }
}

/**
 * The bottom row while an invitation waits for an answer: No, Maybe and Yes, under the thumb,
 * Yes filled, and the message's other actions in [onMore].
 */
@Composable
fun InviteActions(onAnswer: (InviteAnswer) -> Unit, onMore: () -> Unit, modifier: Modifier = Modifier) {
    val colors = MaterialTheme.colorScheme
    val height = if (LocalRoom.current.short) 44.dp else 50.dp
    Row(
        modifier
            .fillMaxWidth()
            .background(colors.background)
            .navigationBarsPadding()
            .padding(start = Gap.bar, end = Gap.bar, top = Gap.barTop, bottom = Gap.barBottom),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        for (answer in listOf(InviteAnswer.NO, InviteAnswer.MAYBE, InviteAnswer.YES)) {
            WideButton(stringResource(answerLabel(answer)), filled = answer == InviteAnswer.YES, height) { onAnswer(answer) }
        }
        IconTile(R.drawable.ic_more, stringResource(R.string.action_more), onMore, background = colors.surfaceVariant, shape = RoundedCornerShape(Radius.medium), size = height)
    }
}

/** The three answers in a sheet, to answer again; [current] is marked. */
@Composable
fun AnswerSheet(current: InviteAnswer?, onAnswer: (InviteAnswer) -> Unit) {
    val colors = MaterialTheme.colorScheme
    SheetTitle(stringResource(R.string.invite_answer_title))
    for (answer in listOf(InviteAnswer.YES, InviteAnswer.MAYBE, InviteAnswer.NO)) {
        val tint = if (answer == current) answerColor(answer) else colors.onSurface
        SheetAction(answerIcon(answer), stringResource(answerLabel(answer)), tint) { onAnswer(answer) }
    }
}

/** Asks why not, before No goes to [organizer]; the reason may stay empty. */
@Composable
fun ReasonSheet(organizer: String, onSend: (String) -> Unit) {
    val colors = MaterialTheme.colorScheme
    var reason by remember { mutableStateOf("") }
    SheetTitle(stringResource(R.string.invite_reason_title, organizer))
    Text(stringResource(R.string.invite_reason_text), color = colors.onSurfaceVariant, style = Type.Text, modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, bottom = 10.dp))
    Plain(
        reason,
        { reason = it },
        R.string.invite_reason_prompt,
        Type.Body,
        KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        singleLine = false,
        modifier = Modifier
            .padding(horizontal = Gap.sheet)
            .fillMaxWidth()
            .clip(RoundedCornerShape(Radius.medium))
            .background(colors.surfaceVariant)
            .padding(horizontal = 12.dp, vertical = 10.dp)
            .height(64.dp),
    )
    Row(Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 12.dp, bottom = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        WideButton(stringResource(R.string.invite_reason_skip), filled = false, 46.dp) { onSend("") }
        WideButton(stringResource(R.string.compose_send), filled = true, 46.dp) { onSend(reason) }
    }
}

/** A button sharing a row equally with others: in accent when [filled], else outlined. */
@Composable
private fun RowScope.WideButton(text: String, filled: Boolean, height: Dp, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val shape = RoundedCornerShape(Radius.medium)
    Box(
        Modifier
            .weight(1f)
            .height(height)
            .clip(shape)
            .background(if (filled) colors.accent else colors.background)
            .border(1.dp, if (filled) colors.accent else colors.outlineVariant, shape)
            .clickable(role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text,
            color = if (filled) colors.onAccent else colors.onSurface,
            style = Type.Body,
            fontWeight = Weight.SemiBold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** What a list shows of an invitation in place of its text: "Wed, 29 Oct, 11:00 · Room 7". */
@Composable
fun inviteLine(mark: InviteMark): String {
    val context = LocalContext.current
    val day = if (thisYear(mark.start)) "EEEMMMd" else "yEEEMMMd"
    val start = if (mark.allDay) formatDate(context, mark.start, day, UTC) else formatDate(context, mark.start, day + time(context))
    val parts = listOfNotNull(
        stringResource(R.string.invite_cancelled).takeIf { mark.cancelled },
        start,
        mark.location.takeIf { it.isNotBlank() },
    )
    return parts.joinToString(" · ")
}

/** The subject of the answer's mail, where it goes as one. */
@StringRes
fun answerSubject(answer: InviteAnswer): Int = when (answer) {
    InviteAnswer.YES -> R.string.invite_subject_yes
    InviteAnswer.MAYBE -> R.string.invite_subject_maybe
    InviteAnswer.NO -> R.string.invite_subject_no
}

/** "You're going", and so on: the answer as a toast and the card say it. */
@StringRes
fun answerStatus(answer: InviteAnswer): Int = when (answer) {
    InviteAnswer.YES -> R.string.invite_going
    InviteAnswer.MAYBE -> R.string.invite_might_go
    InviteAnswer.NO -> R.string.invite_not_going
}

@StringRes
private fun answerLabel(answer: InviteAnswer): Int = when (answer) {
    InviteAnswer.YES -> R.string.invite_yes
    InviteAnswer.MAYBE -> R.string.invite_maybe
    InviteAnswer.NO -> R.string.invite_no
}

@DrawableRes
private fun answerIcon(answer: InviteAnswer): Int = when (answer) {
    InviteAnswer.YES -> R.drawable.ic_done
    InviteAnswer.MAYBE -> R.drawable.ic_maybe
    InviteAnswer.NO -> R.drawable.ic_close
}

@Composable
private fun answerColor(answer: InviteAnswer): Color = when (answer) {
    InviteAnswer.YES -> if (isSystemInDarkTheme()) YES_NIGHT else YES_DAY
    InviteAnswer.MAYBE -> MaterialTheme.colorScheme.tertiary
    InviteAnswer.NO -> MaterialTheme.colorScheme.error
}

/**
 * "Wednesday, 29 October, 11:00–12:00"; across days both ends in full. A day-long meeting
 * shows its days in UTC, as the core gives them.
 */
private fun whenText(context: Context, start: Long, end: Long, allDay: Boolean): String {
    val day = if (thisYear(start)) "EEEEMMMMd" else "yEEEEMMMMd"
    if (allDay) {
        val last = maxOf(start, end - DAY)
        val first = formatDate(context, start, day, UTC)
        return if (last == start) {
            "$first · ${context.getString(R.string.invite_all_day)}"
        } else {
            "$first – ${formatDate(context, last, day, UTC)}"
        }
    }
    val clock = time(context)
    val zone = ZoneId.systemDefault()
    val sameDay = Instant.ofEpochSecond(start).atZone(zone).toLocalDate() == Instant.ofEpochSecond(end).atZone(zone).toLocalDate()
    return if (sameDay) {
        "${formatDate(context, start, day)}, ${formatDate(context, start, clock)}–${formatDate(context, end, clock)}"
    } else {
        "${formatDate(context, start, day + clock)} – ${formatDate(context, end, day + clock)}"
    }
}

/** The organizer's clock, where it differs from the phone's: "16:00–17:00 their time". */
private fun theirTime(context: Context, invite: InviteItem): String? {
    val offset = invite.offset ?: return null
    if (invite.allDay) return null
    val millis = offset * 60_000
    if (TimeZone.getDefault().getOffset(invite.start * 1000) == millis) return null
    val zone = TimeZone.getTimeZone("GMT%s%02d:%02d".format(java.util.Locale.ROOT, if (offset < 0) "-" else "+", abs(offset) / 60, abs(offset) % 60))
    val clock = time(context)
    val span = "${formatDate(context, invite.start, clock, zone)}–${formatDate(context, invite.end, clock, zone)}"
    return context.getString(R.string.invite_their_time, span)
}

/** "Every week · Mon, Wed", "Every 2 weeks". */
@Composable
private fun repeatText(repeats: RepeatItem): String {
    val n = repeats.interval.toInt()
    val every = if (n <= 1) {
        stringResource(
            when (repeats.every) {
                RepeatEvery.DAY -> R.string.invite_repeat_day
                RepeatEvery.WEEK -> R.string.invite_repeat_week
                RepeatEvery.MONTH -> R.string.invite_repeat_month
                RepeatEvery.YEAR -> R.string.invite_repeat_year
            },
        )
    } else {
        stringResource(
            when (repeats.every) {
                RepeatEvery.DAY -> R.string.invite_repeat_days
                RepeatEvery.WEEK -> R.string.invite_repeat_weeks
                RepeatEvery.MONTH -> R.string.invite_repeat_months
                RepeatEvery.YEAR -> R.string.invite_repeat_years
            },
            n,
        )
    }
    if (repeats.days.isEmpty()) return every
    val locale = LocalConfiguration.current.locales[0]
    val days = repeats.days.joinToString(", ") { java.time.DayOfWeek.of(it.toInt() + 1).getDisplayName(TextStyle.SHORT, locale) }
    return "$every · $days"
}

/**
 * Opens the phone's calendar on a new event filled in from [invite]; it asks for no
 * permission. False if the phone has no calendar.
 */
fun addToCalendar(context: Context, invite: InviteItem): Boolean {
    val intent = Intent(Intent.ACTION_INSERT, CalendarContract.Events.CONTENT_URI)
        .putExtra(CalendarContract.EXTRA_EVENT_BEGIN_TIME, invite.start * 1000)
        .putExtra(CalendarContract.EXTRA_EVENT_END_TIME, invite.end * 1000)
        .putExtra(CalendarContract.EXTRA_EVENT_ALL_DAY, invite.allDay)
        .putExtra(CalendarContract.Events.TITLE, invite.summary)
        .putExtra(CalendarContract.Events.EVENT_LOCATION, invite.location)
    invite.link?.let { intent.putExtra(CalendarContract.Events.DESCRIPTION, it) }
    return try {
        context.startActivity(intent)
        true
    } catch (_: ActivityNotFoundException) {
        false
    }
}

private fun thisYear(seconds: Long): Boolean {
    val zone = ZoneId.systemDefault()
    return Instant.ofEpochSecond(seconds).atZone(zone).year == LocalDate.now(zone).year
}

private val UTC: TimeZone = TimeZone.getTimeZone("UTC")

private const val HOUR = 3_600L
private const val DAY = 86_400L
private val HOUR_HEIGHT = 23.dp
private val HOUR_LABEL = 40.dp

/** Where a meeting at the same time starts beside this one, as a part of the width. */
private const val SPLIT = 0.62f

private val YES_DAY = Color(0xFF1F9D63)
private val YES_NIGHT = Color(0xFF4CC98D)

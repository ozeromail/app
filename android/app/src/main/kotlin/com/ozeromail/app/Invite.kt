package com.ozeromail.app

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.icu.util.TimeZone
import android.net.Uri
import android.provider.CalendarContract
import android.text.format.DateFormat
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
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
 * The meeting a message invites to, over its text, in three lines: what, when, where with how
 * to join; then a line of the day around it, and the meeting at the same time if there is one.
 * Once answered it says what, and [onChange] answers again; where the meeting lands in no
 * calendar, [onAddToCalendar] puts it in the phone's.
 */
@Composable
fun InviteCard(invite: InviteItem, onChange: (() -> Unit)?, onAddToCalendar: () -> Unit, modifier: Modifier = Modifier) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    Column(
        modifier
            .fillMaxWidth()
            .padding(start = Gap.card, end = Gap.card, top = 4.dp, bottom = 12.dp)
            .clip(RoundedCornerShape(Radius.tile))
            .background(colors.surfaceVariant),
    ) {
        Column(Modifier.padding(start = Gap.inCard, end = Gap.inCard, top = 14.dp, bottom = 14.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            when {
                invite.cancelled -> Badge(stringResource(R.string.invite_cancelled), colors.error)
                invite.before != null && invite.answer == null -> Badge(stringResource(R.string.invite_moved), colors.tertiary)
            }
            Text(invite.summary.ifBlank { stringResource(R.string.inbox_no_subject) }, style = Type.Row, fontWeight = Weight.SemiBold)
            invite.before?.let { before ->
                Text(
                    whenText(context, before.start, before.end, invite.allDay),
                    color = colors.onSurfaceVariant,
                    style = Type.Caption,
                    textDecoration = TextDecoration.LineThrough,
                )
            }
            val repeats = invite.repeats?.let { repeatText(it) }
            Text(listOfNotNull(whenText(context, invite.start, invite.end, invite.allDay), repeats).joinToString(" · "), style = Type.Text)
            theirTime(context, invite)?.let { Text(it, color = colors.onSurfaceVariant, style = Type.Caption) }
            val link = invite.link
            if (invite.location.isNotBlank() || link != null) {
                val join = stringResource(R.string.invite_join)
                val place = buildAnnotatedString {
                    append(invite.location)
                    if (link != null) {
                        if (invite.location.isNotBlank()) append(" · ")
                        withLink(
                            LinkAnnotation.Clickable("join", TextLinkStyles(SpanStyle(color = colors.primary, fontWeight = Weight.SemiBold))) {
                                openLink(context, Uri.parse(link))
                            },
                        ) { append(join) }
                    }
                }
                Text(place, color = colors.onSurfaceVariant, style = Type.Text)
            }
            if (!invite.cancelled && !invite.allDay) DayLine(invite)
        }
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

@Composable
private fun Badge(text: String, tint: Color) {
    Text(
        text,
        color = tint,
        style = Type.Badge,
        modifier = Modifier
            .padding(bottom = 4.dp)
            .background(MaterialTheme.colorScheme.background, RoundedCornerShape(50))
            .padding(horizontal = 8.dp, vertical = 2.dp),
    )
}

@Composable
private fun CardButton(text: String, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Text(
        text,
        style = Type.Caption,
        fontWeight = Weight.SemiBold,
        modifier = Modifier
            .clip(CircleShape)
            .background(colors.background)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
    )
}

/**
 * The hours around the meeting the core picks, as one thin line: the meeting in accent, the
 * person's other meetings in grey as their invitations tell, one at the same time in red and
 * named under it.
 */
@Composable
private fun DayLine(invite: InviteItem) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    val from = invite.dayFrom
    val span = (invite.dayTo - from).coerceAtLeast(HOUR).toFloat()
    val clashes = invite.day.filter { it.clashes }
    Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        BoxWithConstraints(
            Modifier
                .fillMaxWidth()
                .height(LINE)
                .clip(RoundedCornerShape(50))
                .background(colors.outlineVariant),
        ) {
            @Composable
            fun Busy(start: Long, end: Long, color: Color) {
                val left = maxWidth * ((maxOf(start, from) - from) / span)
                val width = maxWidth * ((minOf(end, invite.dayTo) - maxOf(start, from)) / span)
                if (width > 0.dp) {
                    Box(Modifier.offset(x = left).width(width).height(LINE).clip(RoundedCornerShape(50)).background(color))
                }
            }
            for (e in invite.day) if (!e.clashes) Busy(e.start, e.end, colors.outline)
            Busy(invite.start, invite.end, colors.accent)
            for (e in clashes) Busy(e.start, e.end, colors.error.copy(alpha = 0.85f))
        }
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            val hours = (span / HOUR).toInt()
            val step = maxOf(1, (hours + 2) / 3)
            for (h in 0..hours step step) {
                // The first and last hours sit at the ends, inside the card.
                val x = maxWidth * (h * HOUR / span)
                val last = h * HOUR >= span
                Text(
                    hourText(context, from + h * HOUR),
                    color = colors.onSurfaceVariant,
                    style = Type.Label,
                    modifier = if (last) Modifier.align(Alignment.TopEnd) else Modifier.offset(x = if (h == 0) 0.dp else x - 6.dp),
                )
            }
        }
        clashes.firstOrNull()?.let { first ->
            val what = "${first.summary}, ${formatDate(context, first.start, time(context))}"
            val more = stringResource(R.string.invite_others, clashes.size - 1).takeIf { clashes.size > 1 }
            Text(
                stringResource(R.string.invite_clash, listOfNotNull(what, more).joinToString(" ")),
                color = colors.error,
                style = Type.Caption,
            )
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
            .then(if (filled) Modifier.lifted(colors, shape, colors.accent) else Modifier)
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
 * "Wed, 29 Oct · 11:00–12:00"; across days both ends in full. A day-long meeting
 * shows its days in UTC, as the core gives them.
 */
private fun whenText(context: Context, start: Long, end: Long, allDay: Boolean): String {
    val day = if (thisYear(start)) "EEEMMMd" else "yEEEMMMd"
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
        "${formatDate(context, start, day)} · ${formatDate(context, start, clock)}–${formatDate(context, end, clock)}"
    } else {
        "${formatDate(context, start, day + clock)} – ${formatDate(context, end, day + clock)}"
    }
}

/** "9", "15": an hour of the day, without minutes or AM and PM. */
private fun hourText(context: Context, seconds: Long): String {
    val hour = Instant.ofEpochSecond(seconds).atZone(ZoneId.systemDefault()).hour
    return if (DateFormat.is24HourFormat(context)) "$hour" else "${(hour + 11) % 12 + 1}"
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
private val LINE = 6.dp

private val YES_DAY = Color(0xFF1F9D63)
private val YES_NIGHT = Color(0xFF4CC98D)

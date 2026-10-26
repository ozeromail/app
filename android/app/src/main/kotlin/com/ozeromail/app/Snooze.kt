package com.ozeromail.app

import android.text.format.DateFormat
import androidx.annotation.StringRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDefaults
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import java.time.DayOfWeek
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.temporal.ChronoUnit
import java.time.temporal.TemporalAdjusters
import uniffi.ozero_api.MessageItem

/** Messages waiting for a time to snooze to: [then] runs once one is picked, [reset] if none is. */
class Snoozing(val messages: List<MessageItem>, val reset: () -> Unit = {}, val then: () -> Unit = {})

/**
 * The times to snooze to, from [now] on: later today and this evening while there is time
 * left, tomorrow morning, the weekend from a weekday, and next week unless that is tomorrow.
 */
fun snoozeChoices(now: LocalDateTime): List<Pair<Int, LocalDateTime>> {
    val today = now.toLocalDate()
    // Up to the next quarter hour, so the time reads round.
    val later = now.plusHours(LATER_HOURS).truncatedTo(ChronoUnit.HOURS).plusMinutes((now.minute + 14) / 15 * 15L)
    val evening = today.atTime(EVENING)
    val tomorrow = today.plusDays(1).atTime(MORNING)
    val weekend = today.with(TemporalAdjusters.next(DayOfWeek.SATURDAY)).atTime(WEEKEND)
    val nextWeek = today.with(TemporalAdjusters.next(DayOfWeek.MONDAY)).atTime(MORNING)
    return buildList {
        if (later.toLocalDate() == today && later.toLocalTime() <= LATEST) add(R.string.snooze_later to later)
        if (now.toLocalTime() < EVENING.minusHours(EVENING_AHEAD_HOURS)) add(R.string.snooze_evening to evening)
        add(R.string.snooze_tomorrow to tomorrow)
        if (today.dayOfWeek < DayOfWeek.SATURDAY && weekend.toLocalDate() != tomorrow.toLocalDate()) {
            add(R.string.snooze_weekend to weekend)
        }
        if (nextWeek.toLocalDate() != tomorrow.toLocalDate()) add(R.string.snooze_next_week to nextWeek)
    }
}

/** Epoch seconds of this time on the phone's clock. */
private fun LocalDateTime.seconds(): Long = atZone(ZoneId.systemDefault()).toEpochSecond()

/**
 * Asks until when to snooze: the usual times, each with when it is, and any date and time.
 * [onPick] gets the time in epoch seconds. Put it last in a full-screen box.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SnoozeSheet(visible: Boolean, onPick: (Long) -> Unit, onClose: () -> Unit) {
    val context = LocalContext.current
    val colors = MaterialTheme.colorScheme
    // Read as the sheet opens; it is not open long enough for the times to go stale.
    val now = remember(visible) { LocalDateTime.now() }
    var picking by remember(visible) { mutableStateOf(false) }
    var day by remember(visible) { mutableStateOf<LocalDate?>(null) }
    Sheet(visible = visible, onClose = onClose) {
        SheetTitle(stringResource(R.string.snooze_title))
        snoozeChoices(now).forEach { (label, time) ->
            Choice(label, snoozeTime(context, time.seconds())) { onPick(time.seconds()) }
        }
        Choice(R.string.snooze_pick, null) { picking = true }
        Text(
            stringResource(R.string.snooze_note),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 8.dp),
        )
    }
    if (picking) {
        val today = now.toLocalDate()
        val dates = rememberDatePickerState(
            initialSelectedDateMillis = today.plusDays(1).utcMillis(),
            selectableDates = object : SelectableDates {
                override fun isSelectableDate(utcTimeMillis: Long) = utcTimeMillis >= today.utcMillis()
                override fun isSelectableYear(year: Int) = year >= today.year
            },
        )
        DatePickerDialog(
            onDismissRequest = { picking = false },
            confirmButton = {
                TextButton(
                    onClick = {
                        day = dates.selectedDateMillis?.let { Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate() }
                        picking = false
                    },
                    enabled = dates.selectedDateMillis != null,
                ) { Text(stringResource(R.string.action_next)) }
            },
            dismissButton = { TextButton(onClick = { picking = false }) { Text(stringResource(R.string.action_cancel)) } },
            colors = DatePickerDefaults.colors(containerColor = colors.background),
        ) {
            DatePicker(dates, showModeToggle = false)
        }
    }
    day?.let { date ->
        val clock = rememberTimePickerState(MORNING.hour, MORNING.minute, DateFormat.is24HourFormat(context))
        val time = date.atTime(clock.hour, clock.minute)
        val past = time <= LocalDateTime.now()
        AlertDialog(
            onDismissRequest = { day = null },
            confirmButton = {
                TextButton(onClick = { onPick(time.seconds()) }, enabled = !past) { Text(stringResource(R.string.action_snooze)) }
            },
            dismissButton = { TextButton(onClick = { day = null }) { Text(stringResource(R.string.action_cancel)) } },
            text = {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    TimePicker(clock)
                    if (past) Text(stringResource(R.string.snooze_past), color = colors.error, style = Type.Caption)
                }
            },
            containerColor = colors.background,
            shape = RoundedCornerShape(Radius.dialog),
        )
    }
}

/** A time to snooze to, with when it is on the right; [time] null for one still to pick. */
@Composable
private fun Choice(@StringRes label: Int, time: String?, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .clickable(onClick = onClick)
            .padding(horizontal = Gap.sheet, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(stringResource(label), color = colors.onSurface, style = Type.Row, modifier = Modifier.weight(1f))
        if (time != null) {
            Text(time, color = colors.onSurfaceVariant, style = Type.Caption)
        } else {
            Glyph(R.drawable.ic_next, null, colors.onSurfaceVariant, size = 16.dp)
        }
    }
}

/** The date picker's own time for [this] day: its midnight in UTC. */
private fun LocalDate.utcMillis(): Long = atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli()

/** Snoozed mail comes back by this hour; past it, Later today is not offered. */
private val LATEST = LocalTime.of(21, 0)
private const val LATER_HOURS = 3L
/** This evening is offered until this long before it. */
private const val EVENING_AHEAD_HOURS = 2L
private val EVENING = LocalTime.of(19, 0)
private val MORNING = LocalTime.of(9, 0)
private val WEEKEND = LocalTime.of(10, 0)

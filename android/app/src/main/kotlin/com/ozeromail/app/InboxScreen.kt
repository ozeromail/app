package com.ozeromail.app

import android.content.Context
import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem

/** The unified inbox: shows what is saved at once, then syncs; pull down to sync again. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun InboxScreen(mail: Mail) {
    var messages by remember { mutableStateOf(emptyList<MessageItem>()) }
    var syncing by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    val scope = rememberCoroutineScope()

    suspend fun load() {
        try {
            messages = mail.inbox()
        } catch (e: CoreException) {
            error = e.text()
        }
    }

    suspend fun sync() {
        syncing = true
        error = null
        try {
            mail.refresh()
        } catch (e: CoreException) {
            error = e.text()
        }
        try {
            load()
        } finally {
            syncing = false
        }
    }

    LaunchedEffect(Unit) {
        load()
        sync()
    }

    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Text(
            stringResource(R.string.inbox_title),
            style = MaterialTheme.typography.headlineLarge,
            fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 12.dp),
        )
        PullToRefreshBox(
            isRefreshing = syncing,
            onRefresh = { if (!syncing) scope.launch { sync() } },
            modifier = Modifier.weight(1f),
        ) {
            LazyColumn(Modifier.fillMaxSize()) {
                if (messages.isEmpty() && !syncing && error == null) {
                    item { Empty() }
                }
                items(messages, key = { it.id }) { message ->
                    MessageRow(message)
                    HorizontalDivider(Modifier.padding(start = 36.dp), color = MaterialTheme.colorScheme.outlineVariant)
                }
            }
        }
        error?.let {
            Text(
                stringResource(it),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
            )
        }
    }
}

@Composable
private fun Empty() {
    Box(Modifier.fillMaxWidth().padding(48.dp), contentAlignment = Alignment.Center) {
        Text(stringResource(R.string.inbox_empty), color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun MessageRow(message: MessageItem) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth().padding(vertical = 10.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Box(Modifier.width(36.dp).padding(top = 7.dp), contentAlignment = Alignment.Center) {
            if (!message.seen) {
                Box(Modifier.size(8.dp).background(colors.primary, CircleShape))
            }
        }
        Column(Modifier.weight(1f).padding(end = 20.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    message.fromName?.takeIf { it.isNotBlank() } ?: message.fromEmail,
                    fontWeight = if (message.seen) FontWeight.Normal else FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Text(shortDate(LocalContext.current, message.date), color = colors.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
            }
            Text(
                message.subject.ifBlank { stringResource(R.string.inbox_no_subject) },
                color = colors.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** Time for today, otherwise the date; the year only when it is not this year. */
private fun shortDate(context: Context, seconds: Long): String {
    val millis = seconds * 1000
    val flags = if (DateUtils.isToday(millis)) {
        DateUtils.FORMAT_SHOW_TIME
    } else {
        DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_ABBREV_MONTH
    }
    return DateUtils.formatDateTime(context, millis, flags)
}

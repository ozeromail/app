package com.ozeromail.app

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageBody
import uniffi.ozero_api.MessageItem

/**
 * A conversation, opened at the list's message: its HTML when it has one, otherwise the
 * text. The other messages, the user's replies among them, sit above it, short; a tap shows
 * one. Reply answers the shown message, or the newest one not from the user, and sits at the
 * bottom; system back returns to the list.
 */
@Composable
fun MessageScreen(mail: Mail, message: MessageItem, onBack: () -> Unit, onReply: (MessageItem) -> Unit) {
    BackHandler(onBack = onBack)
    var thread by remember(message.id) { mutableStateOf(listOf(message)) }
    var shown by remember(message.id) { mutableStateOf(message) }
    // With the id of its message, so a page is never shown under another message's header.
    var body by remember(message.id) { mutableStateOf<Pair<Long, MessageBody>?>(null) }
    var error by remember(message.id) { mutableStateOf<Int?>(null) }
    LaunchedEffect(message.id) {
        try {
            thread = mail.thread(message)
            if (message.threadUnread > 0u) mail.markRead(message)
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    LaunchedEffect(shown.id) {
        try {
            body = shown.id to mail.body(shown)
        } catch (e: CoreException) {
            error = e.text()
        }
    }

    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Header(shown, Modifier.padding(start = 20.dp, end = 20.dp, top = 20.dp))
        val older = thread.filter { it.id != shown.id }
        if (older.isNotEmpty()) {
            Column(Modifier.heightIn(max = 220.dp).verticalScroll(rememberScrollState()).padding(top = 8.dp)) {
                older.forEach { other ->
                    Older(other, onClick = {
                        error = null
                        shown = other
                    })
                }
            }
        }
        HorizontalDivider(Modifier.padding(top = 12.dp), color = colors.outlineVariant)
        val loaded = body?.takeIf { it.first == shown.id }?.second
        val html = loaded?.html
        val failed = error
        val content = Modifier.weight(1f).fillMaxWidth()
        when {
            // A new web view per message: it loads its page only once.
            html != null -> key(shown.id) { HtmlView(html, content) }
            loaded != null -> SelectionContainer(content.verticalScroll(rememberScrollState()).padding(20.dp)) {
                Text(loaded.text)
            }
            failed != null -> Text(stringResource(failed), color = colors.onSurfaceVariant, modifier = content.padding(20.dp))
            else -> Box(content.padding(32.dp), contentAlignment = Alignment.Center) {
                CircularProgressIndicator()
            }
        }
        HorizontalDivider(color = colors.outlineVariant)
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), horizontalArrangement = Arrangement.End) {
            Button(onClick = { onReply(if (shown.sent) thread.lastOrNull { !it.sent } ?: message else shown) }) { Text(stringResource(R.string.message_reply)) }
        }
    }
}

/** Another message of the conversation, on two lines. */
@Composable
private fun Older(message: MessageItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 20.dp, vertical = 6.dp)) {
        Row {
            Text(
                message.sender,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text(
                shortDate(LocalContext.current, message.date),
                color = colors.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        if (message.snippet.isNotBlank()) {
            Text(message.snippet, color = colors.onSurfaceVariant, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun Header(message: MessageItem, modifier: Modifier) {
    val colors = MaterialTheme.colorScheme
    SelectionContainer(modifier) {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                message.subjectText(),
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.SemiBold,
            )
            Text(message.sender, fontWeight = FontWeight.Medium)
            if (message.sender != message.fromEmail) {
                Text(message.fromEmail, color = colors.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
            }
            Text(
                DateUtils.formatDateTime(
                    LocalContext.current,
                    message.date * 1000,
                    DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_SHOW_TIME or DateUtils.FORMAT_SHOW_YEAR,
                ),
                color = colors.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

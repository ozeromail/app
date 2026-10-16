package com.ozeromail.app

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageBody
import uniffi.ozero_api.MessageItem

/** One message: its HTML when it has one, otherwise the text. System back returns to the list. */
@Composable
fun MessageScreen(mail: Mail, message: MessageItem, onBack: () -> Unit) {
    BackHandler(onBack = onBack)
    var body by remember(message.id) { mutableStateOf<MessageBody?>(null) }
    var error by remember(message.id) { mutableStateOf<Int?>(null) }
    LaunchedEffect(message.id) {
        try {
            if (!message.seen) mail.markRead(message)
            body = mail.body(message)
        } catch (e: CoreException) {
            error = e.text()
        }
    }

    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Header(message, Modifier.padding(start = 20.dp, end = 20.dp, top = 20.dp))
        HorizontalDivider(Modifier.padding(top = 12.dp), color = colors.outlineVariant)
        val shown = body
        val html = shown?.html
        val failed = error
        val content = Modifier.weight(1f).fillMaxWidth()
        when {
            html != null -> HtmlView(html, content)
            shown != null -> SelectionContainer(content.verticalScroll(rememberScrollState()).padding(20.dp)) {
                Text(shown.text)
            }
            failed != null -> Text(stringResource(failed), color = colors.onSurfaceVariant, modifier = Modifier.padding(20.dp))
            else -> Box(Modifier.fillMaxWidth().padding(32.dp), contentAlignment = Alignment.Center) {
                CircularProgressIndicator()
            }
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

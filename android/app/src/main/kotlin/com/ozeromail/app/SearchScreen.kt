package com.ozeromail.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem

/** A pause in typing this long starts a search, so each key does not. */
private const val TYPING_PAUSE_MS = 150L

/** A longer pause also asks the servers, for mail the phone does not have. */
private const val SERVER_PAUSE_MS = 700L

/**
 * Search over the saved mail, newest first, as the user types; once typing stops, the servers
 * are asked too and what they find joins the list. The field sits at the bottom, in reach of
 * the thumb; Close or back returns to the inbox.
 */
@Composable
fun SearchScreen(
    mail: Mail,
    query: String,
    onQuery: (String) -> Unit,
    onOpen: (MessageItem) -> Unit,
    onBack: () -> Unit,
) {
    // With the query they answer, so "No messages" never shows for a query still waiting.
    var found by remember { mutableStateOf<Pair<String, List<MessageItem>>?>(null) }
    var error by remember { mutableStateOf<Int?>(null) }
    // The servers are being asked about the query on screen.
    var asking by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    // Not on the way back from a result: the keyboard would cover the results.
    LaunchedEffect(Unit) { if (query.isEmpty()) focus.requestFocus() }
    LaunchedEffect(query) {
        error = null
        if (query.isBlank()) {
            found = null
            return@LaunchedEffect
        }
        delay(TYPING_PAUSE_MS)
        try {
            found = query to mail.search(query)
            delay(SERVER_PAUSE_MS - TYPING_PAUSE_MS)
            asking = true
            try {
                mail.searchServer(query)
            } catch (e: CoreException) {
                // What the servers found so far still shows.
                error = e.text()
            }
            found = query to mail.search(query)
        } catch (e: CoreException) {
            error = e.text()
        } finally {
            asking = false
        }
    }
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        BackArrow(onBack, Modifier.padding(top = 4.dp))
        LazyColumn(Modifier.weight(1f)) {
            val (shownFor, messages) = found ?: ("" to emptyList())
            if (shownFor == query && query.isNotBlank() && messages.isEmpty() && !asking) {
                item { EmptyNote(R.drawable.ic_search, stringResource(R.string.inbox_empty)) }
            }
            itemsIndexed(messages, key = { _, it -> it.id }) { index, message ->
                Column(rowMove()) {
                    if (index > 0) HorizontalDivider(Modifier.padding(start = Gap.screen), color = colors.outlineVariant)
                    MessageRow(message, onClick = { onOpen(message) })
                }
            }
            if (asking) {
                item(key = "server") {
                    Text(
                        stringResource(R.string.search_server),
                        color = colors.onSurfaceVariant,
                        style = Type.Caption,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp).animateItem(),
                    )
                }
            }
        }
        Reveal(error) {
            Text(
                stringResource(it),
                color = colors.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(horizontal = Gap.screen, vertical = 8.dp),
            )
        }
        Row(
            Modifier
                .fillMaxWidth()
                .padding(start = Gap.bar, end = Gap.bar, top = Gap.barTop, bottom = Gap.barBottom),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
        ) {
            PillBox(Modifier.weight(1f), icon = R.drawable.ic_search) {
                Plain(
                    query,
                    onQuery,
                    R.string.search_prompt,
                    Type.Body,
                    KeyboardOptions(imeAction = ImeAction.Done),
                    modifier = Modifier.weight(1f).focusRequester(focus),
                )
            }
            IconTile(R.drawable.ic_close, stringResource(R.string.action_close), onBack, shape = CircleShape)
        }
    }
}

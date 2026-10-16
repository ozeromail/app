package com.ozeromail.app

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem

/** A pause in typing this long starts a search, so each key does not. */
private const val TYPING_PAUSE_MS = 150L

/**
 * Search over the saved mail, newest first, as the user types. The field sits at the bottom,
 * in reach of the thumb; Cancel or system back returns to the inbox.
 */
@Composable
fun SearchScreen(
    mail: Mail,
    query: String,
    onQuery: (String) -> Unit,
    onOpen: (MessageItem) -> Unit,
    onBack: () -> Unit,
) {
    BackHandler(onBack = onBack)
    // With the query they answer, so "No messages" never shows for a query still waiting.
    var found by remember { mutableStateOf<Pair<String, List<MessageItem>>?>(null) }
    var error by remember { mutableStateOf<Int?>(null) }
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
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        LazyColumn(Modifier.weight(1f)) {
            val (shownFor, messages) = found ?: ("" to emptyList())
            if (shownFor == query && query.isNotBlank() && messages.isEmpty()) {
                item { Empty() }
            }
            items(messages, key = { it.id }) { message ->
                MessageRow(message, onClick = { onOpen(message) })
                HorizontalDivider(Modifier.padding(start = 36.dp), color = colors.outlineVariant)
            }
        }
        error?.let {
            Text(
                stringResource(it),
                color = colors.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
            )
        }
        HorizontalDivider(color = colors.outlineVariant)
        Row(Modifier.padding(start = 20.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Plain(
                query,
                onQuery,
                R.string.search_prompt,
                MaterialTheme.typography.bodyLarge,
                KeyboardOptions(imeAction = ImeAction.Done),
                modifier = Modifier.weight(1f).focusRequester(focus),
            )
            TextButton(onClick = onBack) { Text(stringResource(R.string.action_cancel)) }
        }
    }
}

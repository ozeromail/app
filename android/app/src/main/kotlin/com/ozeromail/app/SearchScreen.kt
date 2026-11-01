package com.ozeromail.app

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.PersonCard
import uniffi.ozero_api.PersonFound
import uniffi.ozero_api.SearchFilterItem

/** A pause in typing this long starts a search, so each key does not. */
private const val TYPING_PAUSE_MS = 150L

/** A longer pause also asks the servers, for mail the phone does not have. */
private const val SERVER_PAUSE_MS = 700L

/**
 * Search over the saved mail, newest first, as the user types; once typing stops, the servers
 * are asked too and what they find joins the list. The field sits at the bottom, in reach of
 * the thumb, with filters over it that narrow what the words find, or list all they keep with
 * no words; Close or back returns to the inbox. Before anything is typed, recent searches and
 * the people the user has most mail with grow up from the field. With a [person], all mail with
 * them instead, under their name.
 */
@Composable
fun SearchScreen(
    mail: Mail,
    query: String,
    person: PersonCard?,
    filter: SearchFilterItem,
    accounts: List<AccountItem>,
    onQuery: (String) -> Unit,
    onFilter: (SearchFilterItem) -> Unit,
    onPerson: (PersonCard) -> Unit,
    onOpen: (MessageItem) -> Unit,
    onBack: () -> Unit,
) {
    // An account's filter goes with the account, or with its chip once one account is left.
    val asked = filter.takeIf { f -> f.accountId == null || accounts.size > 1 && accounts.any { it.id == f.accountId } }
        ?: filter.copy(accountId = null)
    val search = query to asked
    // With the search they answer, so "No messages" never shows for a search still waiting.
    var found by remember { mutableStateOf<Pair<Pair<String, SearchFilterItem>, List<MessageItem>>?>(null) }
    var error by remember { mutableStateOf<Int?>(null) }
    // The servers are being asked about the query on screen.
    var asking by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    val starting = person == null && query.isBlank() && asked == SearchFilterItem()
    var recent by remember { mutableStateOf(emptyList<String>()) }
    var people by remember { mutableStateOf(emptyList<PersonFound>()) }
    LaunchedEffect(Unit) {
        launch { runCatching { mail.recentSearches() }.onSuccess { recent = it } }
        launch { runCatching { mail.frequentPeople() }.onSuccess { people = it } }
    }
    // Not on the way back from a result: the keyboard would cover the results.
    LaunchedEffect(Unit) { if (query.isEmpty() && person == null) focus.requestFocus() }
    if (person != null) {
        LaunchedEffect(person) {
            mail.changes.collect {
                try {
                    found = search to mail.mailWith(person.emails)
                } catch (e: CoreException) {
                    error = e.text()
                }
            }
        }
    }
    LaunchedEffect(search) {
        if (person != null) return@LaunchedEffect
        error = null
        if (starting) {
            found = null
            return@LaunchedEffect
        }
        if (query.isNotBlank()) delay(TYPING_PAUSE_MS)
        try {
            found = search to mail.search(query, asked)
            // Filters alone list saved mail only: the servers search by words.
            if (query.isBlank()) return@LaunchedEffect
            delay(SERVER_PAUSE_MS - TYPING_PAUSE_MS)
            asking = true
            try {
                mail.searchServer(query)
            } catch (e: CoreException) {
                // What the servers found so far still shows.
                error = e.text()
            }
            found = search to mail.search(query, asked)
        } catch (e: CoreException) {
            error = e.text()
        } finally {
            asking = false
        }
    }
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        BackArrow(onBack, Modifier.padding(top = 4.dp))
        if (starting) Start(
            recent,
            people,
            onSearch = onQuery,
            onPerson = { found ->
                scope.launch {
                    try {
                        onPerson(mail.person(found.email, found.name))
                    } catch (e: CoreException) {
                        error = e.text()
                    }
                }
            },
            onForget = {
                recent = emptyList()
                scope.launch { runCatching { mail.forgetSearches() } }
            },
            modifier = Modifier.weight(1f),
        ) else LazyColumn(Modifier.weight(1f)) {
            if (person != null) item(key = "title") { LargeTitle(person.name, Modifier.padding(horizontal = Gap.screen, vertical = 8.dp)) }
            val (shownFor, messages) = found ?: (null to emptyList<MessageItem>())
            val answered = if (person != null) found != null else shownFor == search
            if (answered && messages.isEmpty() && !asking) {
                item { EmptyNote(R.drawable.ic_search, stringResource(R.string.inbox_empty)) }
            }
            // No motion in this list: when the servers' answer joins it, rows sliding to their
            // new places or the note fading out would pass over the rows that appear at once.
            itemsIndexed(messages, key = { _, it -> it.id }) { index, message ->
                Column {
                    if (index > 0) HorizontalDivider(Modifier.padding(start = Gap.row), color = colors.outlineVariant)
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
                        modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp),
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
        if (person == null) {
            Filters(asked, accounts, onFilter)
            SearchBar(query, onQuery, focus, onBack)
        }
    }
}

/** The search field at the bottom, in reach of the thumb, and Close beside it. */
@Composable
private fun SearchBar(query: String, onQuery: (String) -> Unit, focus: FocusRequester, onClose: () -> Unit) {
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
        IconTile(R.drawable.ic_close, stringResource(R.string.action_close), onClose, shape = CircleShape)
    }
}

/**
 * What the screen shows before anything is typed, growing up from the field: recent searches
 * nearest the thumb, the people the user has most mail with over them.
 */
@Composable
private fun Start(
    recent: List<String>,
    people: List<PersonFound>,
    onSearch: (String) -> Unit,
    onPerson: (PersonFound) -> Unit,
    onForget: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    Box(modifier.fillMaxWidth(), contentAlignment = Alignment.BottomStart) {
        // Scrolled from the bottom, so a long column keeps its end at the field.
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState(), reverseScrolling = true)) {
            if (people.isNotEmpty()) {
                Caption(stringResource(R.string.search_people))
                Row(
                    Modifier.horizontalScroll(rememberScrollState()).padding(start = Gap.bar, end = Gap.bar, bottom = 2.dp),
                    horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
                ) {
                    people.forEach { PersonTile(it, onClick = { onPerson(it) }) }
                }
            }
            if (recent.isNotEmpty()) {
                Caption(stringResource(R.string.search_recent)) {
                    TextButton(onClick = onForget, contentPadding = PaddingValues(horizontal = 8.dp)) {
                        Text(stringResource(R.string.search_clear), style = Type.Label, fontWeight = Weight.SemiBold)
                    }
                }
                recent.forEach { text ->
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = 48.dp)
                            .clickable(role = Role.Button) { onSearch(text) }
                            .padding(horizontal = Gap.screen),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(Gap.afterSlot),
                    ) {
                        Glyph(R.drawable.ic_search, null, colors.onSurfaceVariant, Modifier.padding(horizontal = 1.dp))
                        Text(text, style = Type.Body, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}

/** A small heading over a part of [Start], with [action] at its end. */
@Composable
private fun Caption(text: String, action: (@Composable () -> Unit)? = null) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 36.dp).padding(start = Gap.screen, end = Gap.screen - 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant, style = Type.Label, fontWeight = Weight.SemiBold)
        action?.invoke()
    }
}

/** A person to tap for all mail with them, as a chip of one line: the first name, then the rest of it. */
@Composable
private fun PersonTile(person: PersonFound, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val first = person.name.substringBefore(' ')
    val rest = person.name.substringAfter(' ', "")
    val text = buildAnnotatedString {
        append(first)
        if (rest.isNotEmpty()) withStyle(SpanStyle(color = colors.onSurfaceVariant, fontWeight = Weight.Regular)) { append(" $rest") }
    }
    Box(
        Modifier
            .chip(colors.surfaceVariant)
            .clickable(onClickLabel = person.name, role = Role.Button, onClick = onClick)
            .padding(horizontal = ChipPadding),
        contentAlignment = Alignment.Center,
    ) {
        Text(text, style = Type.Text, fontWeight = Weight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** A filter in [Filters]: its key, what it says, whether it is on, its mark and what a tap does. */
private class Filter(val key: String, val text: String, val on: Boolean, val mark: @Composable (Color) -> Unit, val toggle: () -> Unit)

/**
 * The row of filters over the field: unread, flagged, with attachments and, with several
 * accounts, one for each of them; an account's filter takes the place of another's. The ones
 * on come first, so what narrows the search is always in sight.
 */
@Composable
private fun Filters(filter: SearchFilterItem, accounts: List<AccountItem>, onFilter: (SearchFilterItem) -> Unit) {
    val all = buildList {
        add(Filter("unread", stringResource(R.string.folder_unread), filter.unread, { Glyph(R.drawable.ic_unread, null, it, size = 16.dp) }) {
            onFilter(filter.copy(unread = !filter.unread))
        })
        add(Filter("flagged", stringResource(R.string.folder_flagged), filter.flagged, { Glyph(R.drawable.ic_flag, null, it, size = 16.dp) }) {
            onFilter(filter.copy(flagged = !filter.flagged))
        })
        add(Filter("files", stringResource(R.string.folder_files), filter.files, { Glyph(R.drawable.ic_attach, null, it, size = 16.dp) }) {
            onFilter(filter.copy(files = !filter.files))
        })
        if (accounts.size > 1) accounts.forEach { account ->
            val on = filter.accountId == account.id
            add(Filter("account-${account.id}", account.label.ifBlank { account.email }, on, { AccountDot(account.tint, size = 8.dp) }) {
                onFilter(filter.copy(accountId = account.id.takeUnless { on }))
            })
        }
    }.sortedByDescending { it.on }
    val row = rememberLazyListState()
    // A chip turned on moves to the front: bring the front into view with it.
    LaunchedEffect(filter) {
        if (row.firstVisibleItemIndex > 0 || row.firstVisibleItemScrollOffset > 0) row.animateScrollToItem(0)
    }
    LazyRow(
        Modifier.fillMaxWidth(),
        state = row,
        contentPadding = PaddingValues(start = Gap.bar, end = Gap.bar, top = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
    ) {
        items(all, key = { it.key }) { SearchChip(it.text, it.on, it.mark, rowMove(), it.toggle) }
    }
}

/** A chip of the search screen: a pill of one line, filled with the accent while [on]. */
@Composable
private fun SearchChip(text: String, on: Boolean, mark: @Composable (Color) -> Unit, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val tint = if (on) colors.onAccent else colors.onSurface
    Row(
        modifier
            .chip(if (on) colors.accent else colors.surfaceVariant)
            .toggleable(on, role = Role.Checkbox, onValueChange = { onClick() })
            .padding(horizontal = ChipPadding),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        mark(tint)
        Text(text, color = tint, style = Type.Text, fontWeight = Weight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The pill every chip on the search screen sits in, people and filters alike: one height, one shape. */
private fun Modifier.chip(fill: Color): Modifier =
    height(34.dp).widthIn(max = 200.dp).clip(CircleShape).background(fill)

private val ChipPadding = 13.dp

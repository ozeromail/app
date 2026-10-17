package com.ozeromail.app

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.text.format.DateUtils
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.AccountMark
import uniffi.ozero_api.Category
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.InboxTab
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.UnsentItem
import uniffi.ozero_api.replyDraft
import uniffi.ozero_api.undoSeconds

/** The unified inbox: shows what is saved at once, then syncs; pull down to sync again. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun InboxScreen(mail: Mail) {
    var messages by remember { mutableStateOf(emptyList<MessageItem>()) }
    var syncing by remember { mutableStateOf(false) }
    // A sync asked for while one runs, say for a new account, runs right after it.
    var syncAgain by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    var open by remember { mutableStateOf<MessageItem?>(null) }
    var composing by remember { mutableStateOf<Composing?>(null) }
    var tab by rememberSaveable { mutableStateOf(Category.PEOPLE) }
    var tabs by remember { mutableStateOf(emptyList<InboxTab>()) }
    var unsent by remember { mutableStateOf(emptyList<UnsentItem>()) }
    var searching by rememberSaveable { mutableStateOf(false) }
    var addingAccount by rememberSaveable { mutableStateOf(false) }
    var accountsOpen by rememberSaveable { mutableStateOf(false) }
    var accounts by remember { mutableStateOf(emptyList<AccountItem>()) }
    var query by rememberSaveable { mutableStateOf("") }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val snackbar = remember { SnackbarHostState() }
    val archived = stringResource(R.string.inbox_archived)
    val undo = stringResource(R.string.action_undo)
    val sending = stringResource(R.string.inbox_sending)

    suspend fun loadList() {
        val shown = tab
        try {
            val found = mail.inbox(shown)
            // A tab picked meanwhile loads its own list.
            if (tab == shown) messages = found
        } catch (e: CoreException) {
            // Never leave the last tab's messages under the new one.
            if (tab == shown) messages = emptyList()
            error = e.text()
        }
    }

    suspend fun counts() {
        try {
            tabs = mail.tabs()
            unsent = mail.unsent()
            accounts = mail.accounts()
        } catch (e: CoreException) {
            error = e.text()
        }
    }

    suspend fun load() {
        loadList()
        counts()
    }

    suspend fun sync() {
        if (syncing) {
            syncAgain = true
            return
        }
        syncing = true
        error = null
        try {
            mail.refresh()
        } catch (e: CoreException) {
            error = e.text()
        }
        // Whatever synced is on screen now, also when one account failed: no notification later.
        try {
            mail.forgetNewMail()
        } catch (e: CoreException) {
            error = e.text()
        }
        MailWorker.clearNotifications(context)
        try {
            load()
        } finally {
            syncing = false
        }
        if (syncAgain) {
            syncAgain = false
            sync()
        }
    }

    LaunchedEffect(tab) {
        loadList()
        listState.scrollToItem(0)
    }
    val askToNotify = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    LaunchedEffect(Unit) {
        // Asked once the inbox shows, so it is clear what the notifications are for.
        if (context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            askToNotify.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        // Only once there is an account: without one a check has nothing to do.
        MailWorker.schedule(context)
        counts()
        sync()
    }

    /** Shows a toast for [undoSeconds]; true if Undo was tapped. One toast at a time. */
    suspend fun offerUndo(text: String): Boolean {
        // The older toast is past undo anyway.
        snackbar.currentSnackbarData?.dismiss()
        // Closes before the core sends, so Undo is never too late.
        val result = withTimeoutOrNull(undoSeconds().toLong() * 1000) {
            snackbar.showSnackbar(text, undo, duration = SnackbarDuration.Indefinite)
        }
        return result == SnackbarResult.ActionPerformed
    }

    /** Sent: back to the list, where the toast can bring the message back to edit it. */
    fun sent(id: Long, message: Composing) {
        composing = null
        open = null
        scope.launch {
            if (!offerUndo(sending)) return@launch
            mail.undoSend(id)?.let { composing = message.copy(draft = it) }
        }
    }

    /** Done: the message leaves the list at once; the toast can take it back for a moment. */
    fun archive(message: MessageItem, reset: () -> Unit) {
        scope.launch {
            val action = try {
                mail.archive(message)
            } catch (e: CoreException) {
                error = e.text()
                null
            }
            if (action != null) {
                messages = messages.filterNot { it.id == message.id }
                counts()
            }
            // Also after success: a list read before the archive may bring the row back.
            reset()
            if (action != null && offerUndo(archived) && mail.undo(action)) load()
        }
    }

    val writing = composing
    if (writing != null) {
        ComposeScreen(mail, writing, onCancel = { composing = null }, onSent = ::sent)
        return
    }

    val opened = open
    if (opened != null) {
        // Back reloads the list: the message now has a snippet.
        MessageScreen(
            mail,
            opened,
            onBack = {
                open = null
                scope.launch { load() }
            },
            onReply = { to -> composing = Composing(replyDraft(to), to) },
            onDone = {
                open = null
                archive(opened) {}
            },
        )
        return
    }

    if (addingAccount) {
        SignInScreen(
            mail,
            onSignedIn = {
                addingAccount = false
                scope.launch { sync() }
            },
            onCancel = { addingAccount = false },
        )
        return
    }

    if (searching) {
        SearchScreen(
            mail,
            query,
            onQuery = { query = it },
            onOpen = { open = it },
            onBack = {
                searching = false
                query = ""
            },
        )
        return
    }

    val colors = MaterialTheme.colorScheme
    val density = LocalDensity.current
    var dockHeight by remember { mutableStateOf(0.dp) }
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    // The compact title shows once the large one has scrolled under the status bar.
    val scrolled by remember { derivedStateOf { listState.firstVisibleItemIndex > 0 || listState.firstVisibleItemScrollOffset > 60 } }
    val mini by animateFloatAsState(if (scrolled) 1f else 0f, label = "title")
    Box(Modifier.fillMaxSize()) {
        PullToRefreshBox(
            isRefreshing = syncing,
            onRefresh = { scope.launch { sync() } },
            modifier = Modifier.fillMaxSize(),
        ) {
            LazyColumn(
                Modifier.fillMaxSize(),
                state = listState,
                contentPadding = PaddingValues(top = top, bottom = dockHeight + 16.dp),
            ) {
                item(key = "title") { Hero(accounts.size > 1) }
                items(unsent, key = { "unsent-${it.id}" }) { failed ->
                    UnsentRow(failed, onClick = {
                        scope.launch {
                            try {
                                mail.undoSend(failed.id)?.let { composing = Composing(it) }
                            } catch (e: CoreException) {
                                error = e.text()
                            }
                            counts()
                        }
                    })
                }
                if (messages.isEmpty() && !syncing && error == null) {
                    item(key = "empty") { if (tab == Category.PEOPLE) InboxZero() else TabEmpty() }
                }
                itemsIndexed(messages, key = { _, it -> it.id }) { index, message ->
                    if (index > 0) HorizontalDivider(Modifier.padding(start = 48.dp), color = colors.outlineVariant)
                    SwipeToDone(onDone = { reset -> archive(message, reset) }) {
                        MessageRow(message, onClick = { open = message })
                    }
                }
                error?.let {
                    item(key = "error") {
                        Text(
                            stringResource(it),
                            color = colors.onSurfaceVariant,
                            fontSize = 13.sp,
                            modifier = Modifier.padding(horizontal = 20.dp, vertical = 12.dp),
                        )
                    }
                }
            }
        }
        // The compact title, over the list once it scrolls.
        Box(
            Modifier
                .fillMaxWidth()
                .graphicsLayer { alpha = mini }
                .background(colors.background.copy(alpha = 0.94f))
                .padding(top = top)
                .height(48.dp)
                .drawBehind { drawLine(colors.outlineVariant, Offset(0f, size.height), Offset(size.width, size.height), 1.dp.toPx()) },
            contentAlignment = Alignment.Center,
        ) {
            Text(stringResource(R.string.inbox_title), fontWeight = FontWeight.SemiBold, fontSize = 16.sp)
        }
        Column(
            Modifier
                .align(Alignment.BottomCenter)
                .onSizeChanged { dockHeight = with(density) { it.height.toDp() } },
        ) {
            // Low on the screen, in reach of the thumb that swiped.
            Toast(snackbar, Modifier.padding(bottom = 8.dp))
            Dock(
                tab = tab,
                tabs = tabs,
                accounts = accounts,
                onTab = { tab = it },
                onAccounts = { accountsOpen = true },
                onSearch = { searching = true },
                onCompose = { composing = Composing(NEW_DRAFT) },
            )
        }
    }
    if (accountsOpen) {
        AccountsSheet(
            accounts,
            onAdd = {
                accountsOpen = false
                addingAccount = true
            },
            onClose = { accountsOpen = false },
        )
    }
}

/** The large title at the top of the list, where it does not need the thumb. */
@Composable
private fun Hero(allAccounts: Boolean) {
    Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 12.dp, bottom = 14.dp)) {
        Text(
            stringResource(R.string.inbox_title),
            fontSize = 32.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = (-0.8).sp,
        )
        if (allAccounts) {
            Text(
                stringResource(R.string.inbox_all_accounts),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = 13.5.sp,
                modifier = Modifier.padding(top = 4.dp),
            )
        }
    }
}

@Composable
private fun InboxZero() {
    val done = MaterialTheme.meaning.done
    EmptyNote(
        R.drawable.ic_done,
        stringResource(R.string.inbox_zero_title),
        stringResource(R.string.inbox_zero_text),
        tint = done,
        ring = done.copy(alpha = 0.14f),
        ringSize = 96.dp,
    )
}

@Composable
private fun TabEmpty() {
    EmptyNote(R.drawable.ic_done, stringResource(R.string.tab_empty_title), stringResource(R.string.tab_empty_text))
}

/**
 * Everything the thumb needs, along the bottom: the tabs, then the accounts button, search
 * and a new message.
 */
@Composable
private fun Dock(
    tab: Category,
    tabs: List<InboxTab>,
    accounts: List<AccountItem>,
    onTab: (Category) -> Unit,
    onAccounts: () -> Unit,
    onSearch: () -> Unit,
    onCompose: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Column(
        Modifier
            .fillMaxWidth()
            .background(colors.background.copy(alpha = 0.94f))
            .drawBehind { drawLine(colors.outlineVariant, Offset.Zero, Offset(size.width, 0f), 1.dp.toPx()) }
            .navigationBarsPadding()
            .padding(top = 10.dp, bottom = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Tabs(tab, tabs, onTab)
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            AccountsButton(accounts, onAccounts)
            Pill(stringResource(R.string.search_prompt), onSearch, Modifier.weight(1f), icon = R.drawable.ic_search)
            IconTile(
                R.drawable.ic_compose,
                stringResource(R.string.compose_new),
                onCompose,
                background = colors.primary,
                tint = colors.onPrimary,
            )
        }
    }
}

/** The accounts' colours, as the logo has them; opens the accounts. */
@Composable
private fun AccountsButton(accounts: List<AccountItem>, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val label = stringResource(R.string.accounts_title)
    Box(
        Modifier
            .size(48.dp)
            .clip(RoundedCornerShape(16.dp))
            .background(colors.surfaceVariant)
            .clickable(onClickLabel = label, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        val one = accounts.singleOrNull()
        if (one != null) {
            Box(Modifier.size(24.dp).background(one.tint, CircleShape), contentAlignment = Alignment.Center) {
                Text(initials(one.displayName.ifBlank { one.email }).take(1), color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                accounts.take(4).chunked(2).forEach { pair ->
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        pair.forEach { AccountDot(it.tint) }
                    }
                }
            }
        }
    }
}

/** The accounts, and a way to add one. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AccountsSheet(accounts: List<AccountItem>, onAdd: () -> Unit, onClose: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    ModalBottomSheet(onDismissRequest = onClose, containerColor = colors.background) {
        Column(Modifier.navigationBarsPadding().padding(bottom = 16.dp)) {
            Text(
                stringResource(R.string.accounts_title),
                fontSize = 18.sp,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.padding(start = 22.dp, end = 22.dp, bottom = 10.dp),
            )
            accounts.forEach { account ->
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 22.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    AccountDot(account.tint, size = 12.dp)
                    Column(Modifier.weight(1f)) {
                        Text(account.displayName.ifBlank { account.email }, fontSize = 15.5.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (account.displayName.isNotBlank() && account.displayName != account.email) {
                            Text(account.email, color = colors.onSurfaceVariant, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
            Row(
                Modifier.fillMaxWidth().clickable(onClick = onAdd).padding(horizontal = 18.dp, vertical = 13.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Glyph(R.drawable.ic_add, null, colors.primary)
                Text(stringResource(R.string.inbox_add_account), color = colors.primary, fontSize = 15.5.sp, fontWeight = FontWeight.Medium)
            }
        }
    }
}

/** The inbox tabs as one segmented control, each with its unread count. */
@Composable
private fun Tabs(tab: Category, tabs: List<InboxTab>, onPick: (Category) -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(colors.surfaceVariant)
            .padding(3.dp)
            .selectableGroup(),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        // Every tab shows at once; the counts come when the core has them.
        Category.entries.forEach { category ->
            val on = category == tab
            val unread = tabs.find { it.category == category }?.unread ?: 0u
            Row(
                Modifier
                    .weight(1f)
                    .height(34.dp)
                    .then(if (on) Modifier.shadow(1.dp, RoundedCornerShape(11.dp)) else Modifier)
                    .clip(RoundedCornerShape(11.dp))
                    .background(if (on) colors.background else Color.Transparent)
                    .selectable(selected = on, role = Role.Tab, onClick = { onPick(category) }),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    stringResource(category.label()),
                    color = if (on) colors.onSurface else colors.onSurfaceVariant,
                    fontSize = 13.5.sp,
                    fontWeight = if (on) FontWeight.SemiBold else FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (unread > 0u) {
                    Text(
                        unread.toString(),
                        color = colors.primary,
                        fontSize = 11.5.sp,
                        fontWeight = FontWeight.Bold,
                        modifier = Modifier.padding(start = 6.dp),
                    )
                }
            }
        }
    }
}

private fun Category.label(): Int = when (this) {
    Category.PEOPLE -> R.string.tab_people
    Category.NOTIFICATIONS -> R.string.tab_notifications
    Category.NEWSLETTERS -> R.string.tab_newsletters
}

/** A swipe to the left marks the row done; [onDone] gets a way to bring the row back. */
@Composable
private fun SwipeToDone(onDone: (reset: () -> Unit) -> Unit, content: @Composable () -> Unit) {
    val scope = rememberCoroutineScope()
    val state = rememberSwipeToDismissBoxState()
    val done by rememberUpdatedState(onDone)
    LaunchedEffect(state.currentValue) {
        if (state.currentValue == SwipeToDismissBoxValue.EndToStart) {
            done { scope.launch { state.reset() } }
        }
    }
    val colors = MaterialTheme.colorScheme
    SwipeToDismissBox(
        state = state,
        enableDismissFromStartToEnd = false,
        backgroundContent = {
            Row(
                Modifier.fillMaxSize().background(MaterialTheme.meaning.done).padding(horizontal = 24.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Glyph(R.drawable.ic_done, null, Color.White)
                Text(stringResource(R.string.action_done), color = Color.White, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
            }
        },
    ) {
        Box(Modifier.background(colors.background)) { content() }
    }
}

/** A message the server refused; a tap opens it to fix. */
@Composable
private fun UnsentRow(unsent: UnsentItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 4.dp)
            .clip(RoundedCornerShape(16.dp))
            .background(colors.error.copy(alpha = 0.1f))
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Text(stringResource(R.string.inbox_not_sent), color = colors.error, fontWeight = FontWeight.SemiBold, fontSize = 14.5.sp)
        Text(
            unsent.subject.ifBlank { stringResource(R.string.inbox_no_subject) },
            color = colors.onSurfaceVariant,
            fontSize = 13.5.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The account's colour, from the core. */
val AccountMark.tint: Color
    get() = Color(0xFF000000 or color.toLong())

val AccountItem.tint: Color
    get() = Color(0xFF000000 or color.toLong())

/**
 * A row of a list. Unread shows as a blue dot and bold text; with several accounts, a strip
 * of the account's colour on the left and its name by the time mark it.
 */
@Composable
internal fun MessageRow(message: MessageItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val account = message.account
    val unread = message.threadUnread > 0u
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .then(
                if (account == null) {
                    Modifier
                } else {
                    Modifier.drawBehind {
                        val inset = 10.dp.toPx()
                        val width = 4.dp.toPx()
                        // Rounded on the right only: the left corners sit off the edge.
                        drawRoundRect(
                            account.tint,
                            topLeft = Offset(-width, inset),
                            size = Size(width * 2, size.height - inset * 2),
                            cornerRadius = CornerRadius(3.dp.toPx()),
                        )
                    }
                },
            )
            .padding(start = 18.dp, end = 16.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Box(Modifier.width(20.dp).padding(top = 6.dp)) {
            if (unread) Box(Modifier.padding(start = 6.dp).size(9.dp).background(colors.primary, CircleShape))
        }
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    message.sender,
                    fontSize = 15.5.sp,
                    fontWeight = if (unread) FontWeight.Bold else FontWeight.Medium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (message.threadCount > 1u) {
                    Text(
                        message.threadCount.toString(),
                        color = colors.onSurfaceVariant,
                        fontSize = 11.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier
                            .padding(start = 6.dp)
                            .background(colors.surfaceContainerHighest, CircleShape)
                            .padding(horizontal = 6.dp, vertical = 1.dp),
                    )
                }
                Spacer(Modifier.weight(1f).widthIn(min = 8.dp))
                account?.let {
                    Text(
                        it.name,
                        color = colors.onSurfaceVariant,
                        fontSize = 11.5.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.widthIn(max = 70.dp).padding(end = 8.dp),
                    )
                }
                Text(
                    shortDate(LocalContext.current, message.date),
                    color = if (unread) colors.onSurface else colors.onSurfaceVariant,
                    fontSize = 12.5.sp,
                )
            }
            Text(
                message.subjectText(),
                fontSize = 14.5.sp,
                fontWeight = if (unread) FontWeight.SemiBold else FontWeight.Normal,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (message.snippet.isNotBlank()) {
                Text(
                    message.snippet,
                    color = colors.onSurfaceVariant,
                    fontSize = 13.5.sp,
                    lineHeight = 18.6.sp,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** The name, or the address when there is none. */
val MessageItem.sender: String
    get() = fromName?.takeIf { it.isNotBlank() } ?: fromEmail

@Composable
fun MessageItem.subjectText(): String = subject.ifBlank { stringResource(R.string.inbox_no_subject) }

/** Time for today, otherwise the date; the year only when it is not this year. */
fun shortDate(context: Context, seconds: Long): String {
    val millis = seconds * 1000
    val flags = if (DateUtils.isToday(millis)) {
        DateUtils.FORMAT_SHOW_TIME
    } else {
        DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_ABBREV_MONTH
    }
    return DateUtils.formatDateTime(context, millis, flags)
}

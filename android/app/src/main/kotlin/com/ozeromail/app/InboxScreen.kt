package com.ozeromail.app

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.text.format.DateUtils
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.TextButton
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
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
    var error by remember { mutableStateOf<Int?>(null) }
    var open by remember { mutableStateOf<MessageItem?>(null) }
    var composing by remember { mutableStateOf<Composing?>(null) }
    var tab by rememberSaveable { mutableStateOf(Category.PEOPLE) }
    var tabs by remember { mutableStateOf(emptyList<InboxTab>()) }
    var unsent by remember { mutableStateOf(emptyList<UnsentItem>()) }
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
        } catch (e: CoreException) {
            error = e.text()
        }
    }

    suspend fun load() {
        loadList()
        counts()
    }

    suspend fun sync() {
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
            onReply = { message -> composing = Composing(replyDraft(message), message) },
        )
        return
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
            LazyColumn(Modifier.fillMaxSize(), state = listState) {
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
                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                }
                if (messages.isEmpty() && !syncing && error == null) {
                    item { Empty() }
                }
                items(messages, key = { it.id }) { message ->
                    SwipeToDone(onDone = { reset -> archive(message, reset) }) {
                        MessageRow(message, onClick = { open = message })
                    }
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
        // Low on the screen, in reach of the thumb that swiped.
        SnackbarHost(snackbar)
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.weight(1f)) { Tabs(tab, tabs, onPick = { tab = it }) }
            TextButton(
                onClick = { composing = Composing(NEW_DRAFT) },
                modifier = Modifier.padding(end = 8.dp),
            ) {
                Text(stringResource(R.string.compose_new))
            }
        }
    }
}

/** The inbox tabs at the bottom, each with its unread count. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Tabs(tab: Category, tabs: List<InboxTab>, onPick: (Category) -> Unit) {
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
        // Every tab shows at once; the counts come when the core has them.
        Category.entries.forEachIndexed { index, category ->
            val name = stringResource(category.label())
            val unread = tabs.find { it.category == category }?.unread ?: 0u
            SegmentedButton(
                selected = category == tab,
                onClick = { onPick(category) },
                shape = SegmentedButtonDefaults.itemShape(index, Category.entries.size),
                icon = {},
            ) {
                Text(if (unread > 0u) "$name $unread" else name, maxLines = 1)
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
            Box(
                Modifier.fillMaxSize().background(colors.primary).padding(horizontal = 20.dp),
                contentAlignment = Alignment.CenterEnd,
            ) {
                Text(stringResource(R.string.action_done), color = colors.onPrimary, fontWeight = FontWeight.SemiBold)
            }
        },
    ) {
        Box(Modifier.background(colors.background)) { content() }
    }
}

/** A message the server refused; a tap opens it to fix. */
@Composable
private fun UnsentRow(unsent: UnsentItem, onClick: () -> Unit) {
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(start = 36.dp, end = 20.dp, top = 10.dp, bottom = 10.dp)) {
        Text(stringResource(R.string.inbox_not_sent), color = MaterialTheme.colorScheme.error, fontWeight = FontWeight.SemiBold)
        Text(
            unsent.subject.ifBlank { stringResource(R.string.inbox_no_subject) },
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun Empty() {
    Box(Modifier.fillMaxWidth().padding(48.dp), contentAlignment = Alignment.Center) {
        Text(stringResource(R.string.inbox_empty), color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun MessageRow(message: MessageItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = 10.dp),
        verticalAlignment = Alignment.Top,
    ) {
        val unread = message.threadUnread > 0u
        Box(Modifier.width(36.dp).padding(top = 7.dp), contentAlignment = Alignment.Center) {
            if (unread) {
                Box(Modifier.size(8.dp).background(colors.primary, CircleShape))
            }
        }
        Column(Modifier.weight(1f).padding(end = 20.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        message.sender,
                        fontWeight = if (unread) FontWeight.SemiBold else FontWeight.Normal,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (message.threadCount > 1u) {
                        Text(
                            message.threadCount.toString(),
                            color = colors.onSurfaceVariant,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.padding(start = 6.dp),
                        )
                    }
                }
                Spacer(Modifier.width(8.dp))
                Text(shortDate(LocalContext.current, message.date), color = colors.onSurfaceVariant, style = MaterialTheme.typography.bodySmall)
            }
            Text(
                message.subjectText(),
                color = colors.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (message.snippet.isNotBlank()) {
                Text(
                    message.snippet,
                    color = colors.onSurfaceVariant,
                    style = MaterialTheme.typography.bodySmall,
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

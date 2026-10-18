package com.ozeromail.app

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.text.format.DateUtils
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.compose.PredictiveBackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.EnterExitState
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.ExperimentalAnimationApi
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.ExperimentalTransitionApi
import androidx.compose.animation.core.SeekableTransitionState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.rememberTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.PullToRefreshDefaults
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
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
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.AccountMark
import uniffi.ozero_api.Category
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.InboxTab
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.UnsentItem
import uniffi.ozero_api.undoSeconds

/**
 * The unified inbox: shows what is saved at once, then syncs; pull down to sync again.
 * [onNoAccounts] runs once the last account is removed.
 */
@OptIn(ExperimentalTransitionApi::class)
@Composable
fun InboxScreen(mail: Mail, onNoAccounts: () -> Unit) {
    var messages by remember { mutableStateOf(emptyList<MessageItem>()) }
    var syncing by remember { mutableStateOf(false) }
    // A sync asked for while one runs, say for a new account, runs right after it.
    var syncAgain by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    var open by remember { mutableStateOf<MessageItem?>(null) }
    var composing by remember { mutableStateOf<Composing?>(null) }
    // One answer at a time: a second tap would replace the one being written.
    var replying by remember { mutableStateOf(false) }
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
    // The report of the crash that closed the app last time, until the person decides.
    var crash by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        crash = withContext(Dispatchers.IO) { Crashes.last(context) }
    }
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

    suspend fun loadAccounts() {
        try {
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
        // The first read may have failed; a sync is a good moment to try again.
        if (accounts.isEmpty()) loadAccounts()
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

    // Only change when one is added.
    LaunchedEffect(addingAccount) {
        if (!addingAccount) loadAccounts()
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

    val screen = screenOf(composing, open, addingAccount, searching)
    // Where back leads: the same screens with the top one gone.
    val behind: Screen? = when (screen) {
        Screen.Inbox -> null
        Screen.Search, Screen.AddAccount -> Screen.Inbox
        is Screen.Read -> screenOf(null, null, addingAccount, searching)
        is Screen.Write -> screenOf(null, open, addingAccount, searching)
    }

    fun closeSearch() {
        searching = false
        query = ""
    }

    fun back() {
        when (screen) {
            Screen.Inbox -> Unit
            Screen.Search -> closeSearch()
            Screen.AddAccount -> addingAccount = false
            // Back reloads the list: the message now has a snippet.
            is Screen.Read -> {
                open = null
                scope.launch { load() }
            }
            is Screen.Write -> composing = null
        }
    }

    // The screens follow the back gesture: the one on top slides away under the finger.
    val screens = remember { SeekableTransitionState<Screen>(screen) }
    LaunchedEffect(screen) { screens.animateTo(screen) }
    val top by rememberUpdatedState(screen)
    PredictiveBackHandler(enabled = behind != null) { progress ->
        // Read as the gesture starts; a screen that opens meanwhile, say a reply, cancels it.
        val from = top
        val target = behind
        fun still() = target != null && top == from
        try {
            progress.collect { if (still() && target != null) screens.seekTo(Motion.follow(it.progress), target) }
            if (still()) back() else screens.animateTo(top)
        } catch (e: CancellationException) {
            // The gesture's own job is cancelled with it; the screen still has to slide back.
            withContext(NonCancellable) { screens.animateTo(top) }
            throw e
        }
    }

    val colors = MaterialTheme.colorScheme
    Box(Modifier.fillMaxSize()) {
        rememberTransition(screens, label = "screen").AnimatedContent(transitionSpec = { slide() }) { shown ->
            Box(Modifier.fillMaxSize().background(colors.background)) {
                when (shown) {
                    is Screen.Write -> ComposeScreen(mail, shown.composing, onCancel = ::back, onSent = ::sent)
                    is Screen.Read -> MessageScreen(
                        mail,
                        shown.message,
                        onReply = { to, all ->
                            if (!replying) {
                                replying = true
                                scope.launch {
                                    try {
                                        composing = Composing(mail.reply(to, all), to)
                                    } catch (e: CoreException) {
                                        error = e.text()
                                    } finally {
                                        replying = false
                                    }
                                }
                            }
                        },
                        onDone = {
                            open = null
                            // Back to the inbox, where the toast can undo it.
                            closeSearch()
                            archive(shown.message) {}
                        },
                    )
                    Screen.AddAccount -> SignInScreen(
                        mail,
                        onSignedIn = {
                            addingAccount = false
                            scope.launch { sync() }
                        },
                        onCancel = ::back,
                    )
                    Screen.Search -> SearchScreen(
                        mail,
                        query,
                        onQuery = { query = it },
                        onOpen = { open = it },
                        onBack = ::back,
                    )
                    Screen.Inbox -> InboxList(
                        listState = listState,
                        syncing = syncing,
                        onSync = { scope.launch { sync() } },
                        accounts = accounts,
                        unsent = unsent,
                        messages = messages,
                        tab = tab,
                        tabs = tabs,
                        error = error,
                        snackbar = snackbar,
                        onUnsent = { failed ->
                            scope.launch {
                                try {
                                    mail.undoSend(failed.id)?.let { composing = Composing(it) }
                                } catch (e: CoreException) {
                                    error = e.text()
                                }
                                counts()
                            }
                        },
                        onOpen = { open = it },
                        onArchive = ::archive,
                        onTab = { tab = it },
                        onAccounts = { accountsOpen = true },
                        onSearch = { searching = true },
                        onCompose = { composing = Composing(NEW_DRAFT) },
                    )
                }
                // A screen on its way out takes no more taps.
                if (transition.targetState != EnterExitState.Visible) Box(Modifier.matchParentSize().blockTouches())
            }
        }
        AccountsSheet(
            visible = accountsOpen,
            accounts = accounts,
            onAdd = {
                accountsOpen = false
                addingAccount = true
            },
            onColor = { account, color ->
                scope.launch {
                    try {
                        mail.setAccountColor(account, color)
                    } catch (e: CoreException) {
                        error = e.text()
                    }
                    loadAccounts()
                    loadList()
                }
            },
            onRemove = { account ->
                accountsOpen = false
                scope.launch {
                    try {
                        mail.removeAccount(account)
                    } catch (e: CoreException) {
                        error = e.text()
                        return@launch
                    }
                    MailWorker.clearNotifications(context)
                    // Gone also when the list cannot be read again.
                    accounts = accounts.filterNot { it.id == account.id }
                    loadAccounts()
                    if (accounts.isEmpty()) onNoAccounts() else load()
                }
            },
            onWrite = {
                accountsOpen = false
                composing = Composing(authorDraft(context))
            },
            onClose = { accountsOpen = false },
        )
    }
    crash?.let { report ->
        fun decided() {
            crash = null
            scope.launch(Dispatchers.IO) { Crashes.forget(context) }
        }
        // Dismissed by a stray tap, it asks again next time.
        AlertDialog(
            onDismissRequest = { crash = null },
            confirmButton = {
                TextButton(onClick = {
                    decided()
                    composing = Composing(authorDraft(context, report))
                }) { Text(stringResource(R.string.compose_new)) }
            },
            dismissButton = {
                TextButton(onClick = ::decided) { Text(stringResource(R.string.action_dont_send)) }
            },
            title = { Text(stringResource(R.string.crash_title)) },
            text = { Text(stringResource(R.string.crash_text)) },
            containerColor = MaterialTheme.colorScheme.background,
            shape = RoundedCornerShape(Radius.dialog),
        )
    }
}

/** The screen on top: a message being written, then one being read, then the rest. */
private fun screenOf(composing: Composing?, open: MessageItem?, addingAccount: Boolean, searching: Boolean): Screen =
    composing?.let { Screen.Write(it) }
        ?: open?.let { Screen.Read(it) }
        ?: when {
            addingAccount -> Screen.AddAccount
            searching -> Screen.Search
            else -> Screen.Inbox
        }

/** What fills the screen; a deeper screen slides over the one it came from. */
private sealed class Screen(val depth: Int) {
    data object Inbox : Screen(0)
    data object Search : Screen(1)
    data object AddAccount : Screen(1)
    data class Read(val message: MessageItem) : Screen(2)
    data class Write(val composing: Composing) : Screen(3)
}

/**
 * A screen pushed in from the right, as on iOS, with the one under it drifting left and
 * dimming; a new message rises from the bottom. Back plays it in reverse.
 */
@OptIn(ExperimentalAnimationApi::class)
private fun AnimatedContentTransitionScope<Screen>.slide(): ContentTransform {
    val forward = targetState.depth > initialState.depth
    val moving = if (forward) targetState else initialState
    val move = Motion.calm<IntOffset>(Motion.SCREEN)
    val fade = Motion.calm<Float>(Motion.SCREEN)
    val drift = { width: Int -> -width * 28 / 100 }
    return when {
        moving is Screen.Write && forward ->
            ContentTransform(slideInVertically(move) { it }, ExitTransition.KeepUntilTransitionsFinished, 1f, null)
        moving is Screen.Write ->
            ContentTransform(EnterTransition.None, slideOutVertically(move) { it }, -1f, null)
        forward ->
            ContentTransform(slideInHorizontally(move) { it }, slideOutHorizontally(move, drift) + fadeOut(fade, 0.6f), 1f, null)
        else ->
            ContentTransform(slideInHorizontally(move, drift) + fadeIn(fade, 0.6f), slideOutHorizontally(move) { it }, -1f, null)
    }
}

/**
 * The list under its large title, the compact title once it scrolls, and the dock with the
 * toast and errors above it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun InboxList(
    listState: LazyListState,
    syncing: Boolean,
    onSync: () -> Unit,
    accounts: List<AccountItem>,
    unsent: List<UnsentItem>,
    messages: List<MessageItem>,
    tab: Category,
    tabs: List<InboxTab>,
    error: Int?,
    snackbar: SnackbarHostState,
    onUnsent: (UnsentItem) -> Unit,
    onOpen: (MessageItem) -> Unit,
    onArchive: (MessageItem, reset: () -> Unit) -> Unit,
    onTab: (Category) -> Unit,
    onAccounts: () -> Unit,
    onSearch: () -> Unit,
    onCompose: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val density = LocalDensity.current
    var dockHeight by remember { mutableStateOf(0.dp) }
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    // The compact title shows once the large one has scrolled under the status bar.
    val titleGone = with(density) { 24.dp.roundToPx() }
    val scrolled by remember { derivedStateOf { listState.firstVisibleItemIndex > 0 || listState.firstVisibleItemScrollOffset > titleGone } }
    val pull = rememberPullToRefreshState()
    val mini by animateFloatAsState(if (scrolled) 1f else 0f, Motion.calm(Motion.SMALL), label = "title")
    Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))) {
        PullToRefreshBox(
            isRefreshing = syncing,
            onRefresh = onSync,
            modifier = Modifier.fillMaxSize(),
            state = pull,
            // Below the status bar, where the compact title would cover it.
            indicator = {
                PullToRefreshDefaults.Indicator(
                    state = pull,
                    isRefreshing = syncing,
                    modifier = Modifier.align(Alignment.TopCenter).padding(top = top + 48.dp),
                )
            },
        ) {
            LazyColumn(
                Modifier.fillMaxSize(),
                state = listState,
                contentPadding = PaddingValues(top = top, bottom = dockHeight + 16.dp),
            ) {
                item(key = "title") { Hero(accounts.size > 1) }
                items(unsent, key = { "unsent-${it.id}" }) { failed ->
                    UnsentRow(failed, rowMove(), onClick = { onUnsent(failed) })
                }
                if (messages.isEmpty() && !syncing && error == null) {
                    item(key = "empty") { if (tab == Category.PEOPLE) InboxZero() else TabEmpty() }
                }
                itemsIndexed(messages, key = { _, it -> it.id }) { index, message ->
                    Column(rowMove()) {
                        if (index > 0) HorizontalDivider(Modifier.padding(start = 48.dp), color = colors.outlineVariant)
                        SwipeToDone(onDone = { reset -> onArchive(message, reset) }) {
                            MessageRow(message, onClick = { onOpen(message) })
                        }
                    }
                }
            }
        }
        // The compact title, over the list once it scrolls.
        if (mini > 0f) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .graphicsLayer { alpha = mini }
                    .background(colors.bar)
                    .padding(top = top)
                    .height(48.dp)
                    .hairline(colors.outlineVariant, top = false),
                contentAlignment = Alignment.Center,
            ) {
                Text(stringResource(R.string.inbox_title), fontWeight = FontWeight.SemiBold, fontSize = 16.sp)
            }
        }
        Column(Modifier.align(Alignment.BottomCenter)) {
            // Low on the screen, in reach of the thumb that swiped.
            Toast(snackbar, Modifier.padding(bottom = 8.dp))
            Reveal(error) {
                Text(
                    stringResource(it),
                    color = colors.onSurfaceVariant,
                    fontSize = 13.sp,
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(colors.bar)
                        .padding(horizontal = Gap.screen, vertical = 8.dp),
                )
            }
            Dock(
                modifier = Modifier.onSizeChanged { dockHeight = with(density) { it.height.toDp() } },
                tab = tab,
                tabs = tabs,
                accounts = accounts,
                onTab = onTab,
                onAccounts = onAccounts,
                onSearch = onSearch,
                onCompose = onCompose,
            )
        }
    }
}

/** The large title at the top of the list, where it does not need the thumb. */
@Composable
private fun Hero(allAccounts: Boolean) {
    Column(Modifier.padding(start = Gap.screen, end = Gap.screen, top = 12.dp, bottom = 14.dp)) {
        LargeTitle(stringResource(R.string.inbox_title))
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
    val done = MaterialTheme.done
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
    modifier: Modifier,
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
        modifier
            .fillMaxWidth()
            .background(colors.bar)
            .hairline(colors.outlineVariant, top = true)
            .navigationBarsPadding()
            .padding(top = Gap.barTop, bottom = Gap.barBottom),
        verticalArrangement = Arrangement.spacedBy(Gap.inBar),
    ) {
        Tabs(tab, tabs, onTab)
        // The same edge as the tabs above, so the accounts button lines up with them.
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Gap.bar),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
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
            .clip(RoundedCornerShape(Radius.tile))
            .background(colors.surfaceVariant)
            .clickable(onClickLabel = label, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        val one = accounts.singleOrNull()
        if (one != null) {
            AccountInitial(one.name, one.tint, 24.dp)
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

/** The inbox tabs as one segmented control, each with its unread count; the thumb slides to the one picked. */
@Composable
private fun Tabs(tab: Category, tabs: List<InboxTab>, onPick: (Category) -> Unit) {
    val colors = MaterialTheme.colorScheme
    val thumb = RoundedCornerShape(Radius.thumb)
    BoxWithConstraints(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.bar)
            .clip(RoundedCornerShape(Radius.medium))
            .background(colors.surfaceVariant)
            .padding(3.dp),
    ) {
        val count = Category.entries.size
        val gap = 2.dp
        val width = (maxWidth - gap * (count - 1)) / count
        val x by animateDpAsState((width + gap) * Category.entries.indexOf(tab), Motion.settle(), label = "thumb")
        Box(
            Modifier
                .offset { IntOffset(x.roundToPx(), 0) }
                .size(width, 34.dp)
                .shadow(1.dp, thumb)
                .background(colors.background, thumb),
        )
        Row(Modifier.selectableGroup(), horizontalArrangement = Arrangement.spacedBy(gap)) {
            // Every tab shows at once; the counts come when the core has them.
            Category.entries.forEach { category ->
                val on = category == tab
                val unread = tabs.find { it.category == category }?.unread ?: 0u
                val label by animateColorAsState(if (on) colors.onSurface else colors.onSurfaceVariant, Motion.calm(Motion.SMALL), label = "tab")
                Row(
                    Modifier
                        .weight(1f)
                        .height(34.dp)
                        .clip(thumb)
                        .selectable(selected = on, role = Role.Tab, onClick = { onPick(category) }),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        stringResource(category.label()),
                        color = label,
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
                Modifier.fillMaxSize().background(MaterialTheme.done).padding(horizontal = 24.dp),
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
private fun UnsentRow(unsent: UnsentItem, modifier: Modifier, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(
        modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.card, vertical = 4.dp)
            .clip(RoundedCornerShape(Radius.tile))
            .background(colors.error.copy(alpha = 0.1f))
            .clickable(onClick = onClick)
            .padding(horizontal = Gap.inCard, vertical = 12.dp),
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
    get() = rgb(color)

val AccountItem.tint: Color
    get() = rgb(color)

/** The account's name, or its address when it has none. */
val AccountItem.name: String
    get() = displayName.ifBlank { email }

fun rgb(color: UInt) = Color(0xFF000000 or color.toLong())

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

/** The date and time, as a message header shows them. */
fun longDate(context: Context, seconds: Long): String = DateUtils.formatDateTime(
    context,
    seconds * 1000,
    DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_SHOW_TIME or DateUtils.FORMAT_ABBREV_MONTH,
)

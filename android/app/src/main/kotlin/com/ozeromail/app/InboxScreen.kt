package com.ozeromail.app

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.SystemClock
import android.text.format.DateUtils
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.compose.BackHandler
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
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.rememberTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.calculateTargetValue
import androidx.compose.animation.core.exponentialDecay
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
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
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.input.pointer.util.addPointerInputChange
import androidx.compose.ui.platform.LocalHapticFeedback
import kotlin.math.abs
import kotlin.math.roundToInt
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.Saver
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
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.LifecycleStartEffect
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
import uniffi.ozero_api.FolderItem
import uniffi.ozero_api.FolderRole
import uniffi.ozero_api.InboxTab
import uniffi.ozero_api.Mailbox
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.UnsentItem
import uniffi.ozero_api.undoSeconds

/** How soon after a sync a return to the app syncs again. */
private const val RESYNC_MS = 30_000L

/** A folder by its id, which is positive; the unified mailboxes as their role below zero. */
private val MailboxSaver = Saver<Mailbox, Long>(
    save = {
        when (it) {
            is Mailbox.Folder -> it.id
            is Mailbox.Unified -> -1L - it.role.ordinal
        }
    },
    restore = { if (it >= 0) Mailbox.Folder(it) else Mailbox.Unified(FolderRole.entries[(-1L - it).toInt()]) },
)

/**
 * The unified inbox, or another mailbox picked in the accounts sheet: shows what is saved at
 * once, then syncs; pull down to sync again. [onNoAccounts] runs once the last account is
 * removed.
 */
@OptIn(ExperimentalTransitionApi::class)
@Composable
fun InboxScreen(mail: Mail, onNoAccounts: () -> Unit) {
    var messages by remember { mutableStateOf(emptyList<MessageItem>()) }
    var mailbox by rememberSaveable(stateSaver = MailboxSaver) { mutableStateOf<Mailbox>(Mail.INBOX) }
    var syncing by remember { mutableStateOf(false) }
    // A sync asked for while one runs, say for a new account, runs right after it.
    var syncAgain by remember { mutableStateOf(emptySet<Mailbox>()) }
    // When the last sync that went through began.
    var lastSync by remember { mutableLongStateOf(-RESYNC_MS) }
    // Only a pull shows the spinner; syncs on their own stay quiet over the saved mail.
    var pulled by remember { mutableStateOf(false) }
    // Until the saved list is read, an empty one says nothing.
    var listLoaded by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    var open by remember { mutableStateOf<MessageItem?>(null) }
    var composing by remember { mutableStateOf<Composing?>(null) }
    // One answer at a time: a second tap would replace the one being written.
    var replying by remember { mutableStateOf(false) }
    var tab by rememberSaveable { mutableStateOf(Category.PEOPLE) }
    var tabs by remember { mutableStateOf(emptyList<InboxTab>()) }
    var inboxEmpty by remember { mutableStateOf(false) }
    var unsent by remember { mutableStateOf(emptyList<UnsentItem>()) }
    var searching by rememberSaveable { mutableStateOf(false) }
    var addingAccount by rememberSaveable { mutableStateOf(false) }
    var accountsOpen by rememberSaveable { mutableStateOf(false) }
    var accounts by remember { mutableStateOf(emptyList<AccountItem>()) }
    var folders by remember { mutableStateOf(emptyList<FolderItem>()) }
    val context = LocalContext.current
    val settings = remember { Settings(context) }
    var swipes by remember { mutableStateOf(settings.swipes) }
    var groups by remember { mutableStateOf(settings.groups) }
    var swipeTip by remember { mutableStateOf(!settings.swipeTipSeen) }
    var query by rememberSaveable { mutableStateOf("") }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    // The report of the crash that closed the app last time, until the person decides.
    var crash by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        crash = withContext(Dispatchers.IO) { Crashes.last(context) }
    }
    val archived = stringResource(R.string.inbox_archived)
    val trashed = stringResource(R.string.inbox_trashed)
    val undo = stringResource(R.string.action_undo)
    val sending = stringResource(R.string.inbox_sending)

    suspend fun loadList() {
        val shown = tab to mailbox
        try {
            val found = if (mailbox == Mail.INBOX) mail.inbox(tab) else mail.mailbox(mailbox)
            // A tab or mailbox picked meanwhile loads its own list.
            if (tab to mailbox == shown) messages = found
        } catch (e: CoreException) {
            // Never leave the last list's messages under the new one.
            if (tab to mailbox == shown) messages = emptyList()
            error = e.text()
        }
        listLoaded = true
    }

    suspend fun counts() {
        try {
            tabs = mail.tabs()
            unsent = mail.unsent()
            inboxEmpty = mail.inboxEmpty()
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

    /** Saves a change to an account, then shows it in the sheet and on the rows. */
    fun changeAccount(change: suspend () -> Unit) {
        scope.launch {
            try {
                change()
            } catch (e: CoreException) {
                error = e.text()
            }
            loadAccounts()
            loadList()
        }
    }

    /**
     * Syncs [box] in every account; false if one failed. One asked for while one runs follows
     * it.
     */
    suspend fun sync(box: Mailbox = Mail.INBOX): Boolean {
        if (syncing) {
            syncAgain += box
            return true
        }
        syncing = true
        error = null
        val started = SystemClock.elapsedRealtime()
        val synced = try {
            mail.refresh(box)
            if (box == Mail.INBOX) lastSync = started
            true
        } catch (e: CoreException) {
            error = e.text()
            false
        }
        if (box == Mail.INBOX) {
            // Whatever synced is on screen now, also when one account failed: no notification later.
            try {
                mail.forgetNewMail()
            } catch (e: CoreException) {
                error = e.text()
            }
            MailWorker.clearNotifications(context)
            mail.prefetch()
        }
        // The first read may have failed; a sync is a good moment to try again.
        if (accounts.isEmpty()) loadAccounts()
        try {
            load()
        } finally {
            syncing = false
        }
        syncAgain.firstOrNull()?.let {
            syncAgain -= it
            return sync(it)
        }
        pulled = false
        return synced
    }

    // Only change when one is added.
    LaunchedEffect(addingAccount) {
        if (!addingAccount) loadAccounts()
    }
    LaunchedEffect(tab, mailbox) {
        loadList()
        listState.scrollToItem(0)
    }
    // Another mailbox brings in what changed in it; the inbox keeps itself up to date.
    LaunchedEffect(mailbox) {
        if (mailbox != Mail.INBOX) sync(mailbox)
    }
    // Folders show once a sync has listed them, so they are read each time the sheet opens.
    LaunchedEffect(accountsOpen) {
        if (!accountsOpen && folders.isNotEmpty()) return@LaunchedEffect
        try {
            folders = mail.folders()
        } catch (e: CoreException) {
            error = e.text()
        }
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
    }
    // On opening and on every return to the app: mail that came meanwhile shows without a pull.
    // Not again right after one, as when back from the file picker or a link.
    LifecycleEventEffect(Lifecycle.Event.ON_START) {
        if (SystemClock.elapsedRealtime() - lastSync > RESYNC_MS) {
            scope.launch {
                sync()
                if (mailbox != Mail.INBOX) sync(mailbox)
            }
        }
    }
    // While the app is on screen, new mail shows as it arrives.
    LifecycleStartEffect(accounts.map { it.id }) {
        val watching = scope.launch {
            mail.watch(accounts) { sync() }
        }
        onStopOrDispose { watching.cancel() }
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

    /** The message leaves the list at once; the toast can take it back for a moment. */
    fun remove(message: MessageItem, toast: String, move: suspend (MessageItem) -> List<Long>, reset: () -> Unit = {}) {
        scope.launch {
            val action = try {
                move(message)
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
            if (action != null && offerUndo(toast) && mail.undo(action)) load()
        }
    }

    /** Done: into the archive. */
    fun archive(message: MessageItem, reset: () -> Unit) = remove(message, archived, mail::archive, reset)

    /** Marks the row read, or unread if it is read; it stays in the list either way. */
    fun toggleRead(message: MessageItem, reset: () -> Unit) {
        val read = message.threadUnread > 0u
        scope.launch {
            try {
                mail.markRead(message, seen = read)
                messages = messages.map {
                    if (it.id == message.id) it.copy(seen = read, threadUnread = if (read) 0u else 1u) else it
                }
                counts()
            } catch (e: CoreException) {
                error = e.text()
            }
            reset()
        }
    }

    fun swiped(message: MessageItem, action: SwipeAction, reset: () -> Unit) = when (action) {
        SwipeAction.DONE -> archive(message, reset)
        SwipeAction.DELETE -> remove(message, trashed, mail::trash, reset)
        SwipeAction.READ -> toggleRead(message, reset)
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

    val openFolder = (mailbox as? Mailbox.Folder)?.let { box -> folders.find { it.id == box.id } }

    // Back from another mailbox leads to the inbox.
    BackHandler(enabled = screen == Screen.Inbox && mailbox != Mail.INBOX && !accountsOpen) { mailbox = Mail.INBOX }

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
                        onDelete = {
                            open = null
                            closeSearch()
                            remove(shown.message, trashed, mail::trash)
                        }.takeIf { shown.message.canTrash },
                        onUnread = {
                            open = null
                            scope.launch {
                                try {
                                    mail.markRead(shown.message, seen = false)
                                } catch (e: CoreException) {
                                    error = e.text()
                                }
                                load()
                            }
                        },
                        onDone = {
                            open = null
                            // Back to the list, where the toast can undo it.
                            closeSearch()
                            archive(shown.message) {}
                        }.takeIf { shown.message.canArchive },
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
                        refreshing = pulled,
                        // The first mail of an empty list is on its way: the water waits, without words.
                        waiting = syncing && listLoaded && messages.isEmpty(),
                        onSync = {
                            // Over an empty list the lake below ripples instead: one water, not two.
                            pulled = messages.isNotEmpty()
                            scope.launch { sync(mailbox) }
                        },
                        mailbox = mailbox,
                        title = mailboxTitle(mailbox, openFolder),
                        owner = mailboxOwner(mailbox, openFolder, accounts),
                        accounts = accounts,
                        unsent = if (mailbox == Mail.INBOX) unsent else emptyList(),
                        swipes = swipes,
                        swipeTip = swipeTip,
                        onSwipeTip = {
                            swipeTip = false
                            settings.swipeTipSeen = true
                        },
                        messages = messages,
                        tab = tab,
                        tabs = tabs,
                        inboxEmpty = inboxEmpty,
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
                        onSwipe = ::swiped,
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
            folders = folders,
            mailbox = mailbox,
            swipes = swipes,
            onMailbox = {
                accountsOpen = false
                mailbox = it
            },
            onSwipes = {
                swipes = it
                settings.swipes = it
            },
            groups = groups,
            onGroups = {
                groups = it
                settings.groups = it
            },
            onAdd = {
                accountsOpen = false
                addingAccount = true
            },
            onColor = { account, color ->
                changeAccount { mail.setAccountColor(account, color) }
            },
            onLabel = { account, label ->
                changeAccount { mail.setAccountLabel(account, label) }
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
                    folders = folders.filterNot { it.accountId == account.id }
                    // Its folders, and with them the mailboxes together, may be gone.
                    mailbox = Mail.INBOX
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
    refreshing: Boolean,
    waiting: Boolean,
    onSync: () -> Unit,
    mailbox: Mailbox,
    title: String,
    owner: String?,
    accounts: List<AccountItem>,
    unsent: List<UnsentItem>,
    swipes: Swipes,
    swipeTip: Boolean,
    onSwipeTip: () -> Unit,
    messages: List<MessageItem>,
    tab: Category,
    tabs: List<InboxTab>,
    inboxEmpty: Boolean,
    error: Int?,
    snackbar: SnackbarHostState,
    onUnsent: (UnsentItem) -> Unit,
    onOpen: (MessageItem) -> Unit,
    onSwipe: (MessageItem, SwipeAction, reset: () -> Unit) -> Unit,
    onTab: (Category) -> Unit,
    onAccounts: () -> Unit,
    onSearch: () -> Unit,
    onCompose: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val density = LocalDensity.current
    var dockHeight by remember { mutableStateOf(0.dp) }
    // Whether this tab just had mail: then the done note draws itself in as the last message
    // leaves, and not on opening the app or switching to an empty tab.
    // Right after a switch the list is still the old tab's, so it does not count.
    val oldTabList = remember(tab, mailbox) { messages }
    // With nothing in any tab there is one lake under them all, which stays put as they change.
    val inbox = mailbox == Mail.INBOX
    val allEmpty = inbox && inboxEmpty
    var hadMail by remember(tab, mailbox) { mutableStateOf(false) }
    LaunchedEffect(messages) { hadMail = messages.isNotEmpty() && messages !== oldTabList }
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    // The compact title shows once the large one has scrolled under the status bar.
    val titleGone = with(density) { 24.dp.roundToPx() }
    val scrolled by remember { derivedStateOf { listState.firstVisibleItemIndex > 0 || listState.firstVisibleItemScrollOffset > titleGone } }
    val pull = rememberPullToRefreshState()
    // Where the tabs are between two of them, in tabs: the thumb and the list follow it.
    val glide = remember { Animatable(0f) }
    val glideScope = rememberCoroutineScope()
    val pick: (Category, Float, Float) -> Unit = { next, from, velocity ->
        onTab(next)
        glideScope.launch {
            glide.snapTo(from)
            glide.animateTo(0f, thumbSpring(velocity), velocity)
        }
    }
    // The list leaves to the side the thumb moves away from, and the next one comes in from the other.
    val slide = Modifier.graphicsLayer {
        val at = glide.value
        translationX = -at * size.width * 0.35f
        alpha = 1f - (abs(at) * 1.2f).coerceAtMost(1f)
    }
    val mini by animateFloatAsState(if (scrolled) 1f else 0f, Motion.calm(Motion.SMALL), label = "title")
    Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))) {
        PullToRefreshBox(
            isRefreshing = refreshing,
            onRefresh = onSync,
            modifier = Modifier.fillMaxSize(),
            state = pull,
            // The list moves down with the finger and still water shows in the gap it leaves.
            indicator = {
                if (messages.isNotEmpty() || refreshing) {
                    PullWater(
                        fraction = { pull.distanceFraction },
                        refreshing = refreshing,
                        top = top,
                        modifier = Modifier.fillMaxSize(),
                    )
                }
            },
        ) {
            LazyColumn(
                Modifier.fillMaxSize().graphicsLayer { translationY = pullGap(pull.distanceFraction) },
                state = listState,
                contentPadding = PaddingValues(top = top, bottom = dockHeight + 16.dp),
            ) {
                item(key = "title") { Hero(title, owner) }
                items(unsent, key = { "unsent-${it.id}" }) { failed ->
                    UnsentRow(failed, rowMove().then(slide), onClick = { onUnsent(failed) })
                }
                if (swipeTip && messages.isNotEmpty()) {
                    item(key = "tip") { Box(rowMove().then(slide)) { SwipeTip(swipes, onSwipeTip) } }
                }
                if (messages.isEmpty() && (!syncing || waiting) && error == null) {
                    item(key = "empty") {
                        // New mail covers the water rather than cutting it off.
                        Box(Modifier.animateItem(fadeInSpec = null, placementSpec = null, fadeOutSpec = Motion.calm(Motion.SMALL)).then(if (allEmpty) Modifier else slide)) {
                            // The lake keeps its water as the tabs change, and stays put when every one is empty.
                            key(mailbox) {
                                val (title, text) = when {
                                    !inbox -> R.string.tab_empty_title to R.string.folder_empty
                                    allEmpty || tab == Category.PEOPLE -> R.string.inbox_zero_title to R.string.inbox_zero_text
                                    else -> R.string.tab_empty_title to R.string.tab_empty_text
                                }
                                StillNote(
                                    stringResource(title),
                                    stringResource(text),
                                    drawIn = hadMail && inbox,
                                    grey = !inbox,
                                    waiting = waiting,
                                )
                            }
                        }
                    }
                }
                itemsIndexed(messages, key = { _, it -> it.id }) { index, message ->
                    Column(rowMove().then(slide)) {
                        if (index > 0) HorizontalDivider(Modifier.padding(start = Gap.screen), color = colors.outlineVariant)
                        SwipeRow(message, swipes, onSwipe = { action, reset -> onSwipe(message, action, reset) }) {
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
                Text(title, fontWeight = FontWeight.SemiBold, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
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
                tabs = tabs.takeIf { inbox },
                glide = glide,
                accounts = accounts,
                onTab = pick,
                onAccounts = onAccounts,
                onSearch = onSearch,
                onCompose = onCompose,
            )
        }
    }
}

/** The mailbox's name: ours for a role, the server's for [folder], an account's own. */
@Composable
private fun mailboxTitle(mailbox: Mailbox, folder: FolderItem?): String = when (mailbox) {
    is Mailbox.Unified -> mailbox.role.title("")
    is Mailbox.Folder -> folder?.title().orEmpty()
}

/** Whose mail it is, when there are several accounts: all of theirs, or [folder]'s account's. */
@Composable
private fun mailboxOwner(mailbox: Mailbox, folder: FolderItem?, accounts: List<AccountItem>): String? {
    if (accounts.size < 2) return null
    return when (mailbox) {
        is Mailbox.Unified -> stringResource(R.string.inbox_all_accounts)
        is Mailbox.Folder -> accounts.find { it.id == folder?.accountId }?.name
    }
}

/** The large title at the top of the list, where it does not need the thumb. */
@Composable
private fun Hero(title: String, subtitle: String?) {
    Column(Modifier.padding(start = Gap.screen, end = Gap.screen, top = 12.dp, bottom = 14.dp)) {
        LargeTitle(title)
        if (subtitle != null) {
            Text(
                subtitle,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                fontSize = 13.5.sp,
                modifier = Modifier.padding(top = 4.dp),
            )
        }
    }
}

/**
 * Everything the thumb needs, along the bottom: the inbox's tabs (null elsewhere), then the
 * accounts button, search and a new message.
 */
@Composable
private fun Dock(
    modifier: Modifier,
    tab: Category,
    tabs: List<InboxTab>?,
    glide: Animatable<Float, AnimationVector1D>,
    accounts: List<AccountItem>,
    onTab: (Category, from: Float, velocity: Float) -> Unit,
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
        tabs?.let { Tabs(tab, it, glide, onTab) }
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

/**
 * The inbox tabs as one segmented control, each with its unread count. The thumb follows a
 * finger dragged across them and settles on the nearest tab, or the next one on a flick; a tap
 * glides it there. [onPick] gets the tab, where the thumb is from it and how fast it goes, in tabs.
 */
@Composable
private fun Tabs(tab: Category, tabs: List<InboxTab>, glide: Animatable<Float, AnimationVector1D>, onPick: (Category, from: Float, velocity: Float) -> Unit) {
    val colors = MaterialTheme.colorScheme
    val thumb = RoundedCornerShape(Radius.thumb)
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val index = Category.entries.indexOf(tab)
    val last = Category.entries.size - 1
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
        val step = width + gap
        Box(
            Modifier
                .offset { IntOffset(((index + glide.value) * step.toPx()).roundToInt(), 0) }
                .size(width, 34.dp)
                .shadow(1.dp, thumb)
                .background(colors.background, thumb),
        )
        Row(
            Modifier
                .selectableGroup()
                .pointerInput(index, step) {
                    val stepPx = step.toPx()
                    val velocity = VelocityTracker()
                    var at = 0f
                    detectHorizontalDragGestures(
                        onDragStart = {
                            velocity.resetTracking()
                            at = glide.value
                        },
                        onDragEnd = {
                            val flick = velocity.calculateVelocity().x / stepPx
                            val pos = index + at
                            // Where the thumb would coast to decides the tab, so a flick carries on.
                            val coast = exponentialDecay<Float>().calculateTargetValue(pos, flick).roundToInt()
                            val target = coast.coerceIn(0, last)
                            // Flung past the first or last tab, the edge takes the speed.
                            val speed = if (coast == target) flick else 0f
                            if (target == index) {
                                scope.launch { glide.animateTo(0f, thumbSpring(speed), speed) }
                            } else {
                                haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                                onPick(Category.entries[target], pos - target, speed)
                            }
                        },
                        onDragCancel = { scope.launch { glide.animateTo(0f, Motion.settle()) } },
                    ) { change, dx ->
                        change.consume()
                        velocity.addPointerInputChange(change)
                        // A little past the first and last tab, then no further.
                        at = (at + dx / stepPx).coerceIn(-index - 0.15f, last - index + 0.15f)
                        scope.launch { glide.snapTo(at) }
                    }
                },
            horizontalArrangement = Arrangement.spacedBy(gap),
        ) {
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
                        .selectable(selected = on, role = Role.Tab, onClick = { if (!on) onPick(category, (index - Category.entries.indexOf(category)).toFloat(), 0f) }),
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
    // The dot goes out quickly and in place: read is the most frequent change there is.
    val dot by animateFloatAsState(if (unread) 1f else 0f, Motion.calm(Motion.QUICK), label = "dot")
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
            .padding(start = Gap.screen, end = 16.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                // The sender and count take what is left, so the time always ends at the edge.
                Row(Modifier.weight(1f).padding(end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        message.participants(),
                        fontSize = 15.5.sp,
                        fontWeight = if (unread) FontWeight.Bold else FontWeight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier
                            .weight(1f, fill = false)
                            // The unread dot hangs in the margin, so text lines up with the title.
                            .drawBehind {
                                if (dot > 0f) {
                                    val x = if (layoutDirection == LayoutDirection.Ltr) -9.dp.toPx() else size.width + 9.dp.toPx()
                                    drawCircle(colors.primary.copy(alpha = dot), 4.dp.toPx() * (0.4f + 0.6f * dot), Offset(x, size.height / 2))
                                }
                            },
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
                }
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

/** Who wrote in a conversation, "Anna, me, Boris"; for one person, their whole name. */
@Composable
fun MessageItem.participants(): String {
    if (senders.size < 2) return sender
    val me = stringResource(R.string.message_me)
    return senders.joinToString(", ") { if (it.me) me else firstName(it.name) }
}

/** "Anna" of "Anna Lee", "Lee, Anna" or "anna@x.com". */
private fun firstName(name: String): String {
    val given = name.substringBefore('@').substringAfter(", ").trim()
    return given.substringBefore(' ').ifEmpty { name }
}

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

/** A released thumb: a little give after a fling, since the finger threw it; flat otherwise. */
private fun thumbSpring(velocity: Float): SpringSpec<Float> = if (abs(velocity) > FLING) Motion.fling() else Motion.settle()

/** Tabs a second past which a released thumb was flung. */
private const val FLING = 2f

package com.ozeromail.app

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.icu.text.DateFormat
import android.os.SystemClock
import android.text.format.DateUtils
import android.util.Log
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
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.offset
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
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
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
import kotlin.math.sqrt
import kotlin.math.min
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.VisibilityThreshold
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.graphics.ColorProducer
import androidx.compose.ui.graphics.lerp
import androidx.compose.foundation.text.BasicText
import androidx.compose.ui.unit.Density
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.LifecycleStartEffect
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.AccountMark
import uniffi.ozero_api.Category
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftItem
import uniffi.ozero_api.FolderItem
import uniffi.ozero_api.FolderRole
import uniffi.ozero_api.InboxTab
import uniffi.ozero_api.Mailbox
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.UnsentItem
import uniffi.ozero_api.undoSeconds
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.util.Date

/** How soon after a sync a return to the app syncs again. */
private const val RESYNC_MS = 30_000L

/** How long syncs on their own may fail on the network before the error shows. */
private const val BLIP_MS = 60_000L

/** How many more rows the list reads at a time, and how far from its end it starts to. */
private const val PAGE = 100
private const val AHEAD = 20

/** Rows read of each other tab: what shows of its page as it glides past. */
private const val PAGE_ROWS = 12

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
    // A sync asked for while one runs, say for a new account, runs right after it; quiet if
    // every ask for it was.
    var syncAgain by remember { mutableStateOf(emptyMap<Mailbox, Boolean>()) }
    // When the last sync that went through began.
    var lastSync by remember { mutableLongStateOf(-RESYNC_MS) }
    // Since when syncs on their own have met no network, if they do.
    var offlineSince by remember { mutableStateOf<Long?>(null) }
    // Only a pull shows the spinner; syncs on their own stay quiet over the saved mail.
    var pulled by remember { mutableStateOf(false) }
    // Mailboxes synced since the app opened: until then, their empty list waits for its mail.
    var syncedBoxes by remember { mutableStateOf(emptySet<Mailbox>()) }
    // Until the saved list is read, an empty one says nothing.
    var listLoaded by remember { mutableStateOf(false) }
    // How many rows the list reads; scrolling near its end reads more.
    var listSize by remember { mutableIntStateOf(PAGE) }
    // The saved mail ended before `listSize`: more has to come from the servers.
    var savedEnds by remember { mutableStateOf(false) }
    // Older mail is on its way from the servers.
    var loadingOlder by remember { mutableStateOf(false) }
    // Counts the loads that found more older mail, so the list looks at its end again after each.
    var olderLoads by remember { mutableIntStateOf(0) }
    var error by remember { mutableStateOf<Int?>(null) }
    var open by remember { mutableStateOf<MessageItem?>(null) }
    var composing by remember { mutableStateOf<Composing?>(null) }
    // One answer or forward at a time: a second tap would replace the one being written.
    var writing by remember { mutableStateOf(false) }
    var tab by rememberSaveable { mutableStateOf(Category.PEOPLE) }
    var tabs by remember { mutableStateOf(emptyList<InboxTab>()) }
    // The first rows of the other tabs, so a tab glides in with its mail already there.
    var others by remember { mutableStateOf(emptyMap<Category, List<MessageItem>>()) }
    // The list shown is the next tab's first rows, until its saved list is read: it has no end to scroll to yet.
    var preview by remember { mutableStateOf(false) }
    var inboxEmpty by remember { mutableStateOf(false) }
    var unsent by remember { mutableStateOf(emptyList<UnsentItem>()) }
    var searching by rememberSaveable { mutableStateOf(false) }
    var draftsOpen by rememberSaveable { mutableStateOf(false) }
    var addingAccount by rememberSaveable { mutableStateOf(false) }
    // The account being signed in to again, by id.
    var signingIn by rememberSaveable { mutableStateOf<Long?>(null) }
    var accountsOpen by rememberSaveable { mutableStateOf(false) }
    var accounts by remember { mutableStateOf(emptyList<AccountItem>()) }
    var folders by remember { mutableStateOf(emptyList<FolderItem>()) }
    val context = LocalContext.current
    val settings = remember { Settings(context) }
    var swipes by remember { mutableStateOf(settings.swipes) }
    var groups by remember { mutableStateOf(settings.groups) }
    var swipeTip by remember { mutableStateOf(!settings.swipeTipSeen) }
    var query by rememberSaveable { mutableStateOf("") }
    // Messages chosen to act on together, in the order they were chosen.
    var chosen by remember { mutableStateOf(emptySet<Long>()) }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    // The report of the crash that closed the app last time, until the person decides.
    var crash by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        crash = withContext(Dispatchers.IO) { Crashes.last(context) }
    }
    val archived = stringResource(R.string.inbox_archived)
    val movedBack = stringResource(R.string.inbox_moved_back)
    val trashed = stringResource(R.string.inbox_trashed)
    val deleted = stringResource(R.string.inbox_deleted)
    val emptied = stringResource(R.string.inbox_emptied)
    val reverted = stringResource(R.string.inbox_reverted)
    val undo = stringResource(R.string.action_undo)
    val sending = stringResource(R.string.inbox_sending)
    val draftSaved = stringResource(R.string.draft_saved)
    val draftDeleted = stringResource(R.string.draft_deleted)

    val drafts by mail.drafts.collectAsState()

    suspend fun loadList() {
        val shown = tab to mailbox
        try {
            val size = listSize
            val found = if (mailbox == Mail.INBOX) mail.inbox(tab, size) else mail.mailbox(mailbox, size)
            // A tab or mailbox picked meanwhile loads its own list.
            if (tab to mailbox == shown) {
                messages = found
                preview = false
                savedEnds = found.size < size
            }
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

    suspend fun loadOthers() {
        if (mailbox != Mail.INBOX) return
        val showing = tab
        try {
            val read = coroutineScope {
                Category.entries.filter { it != showing }.map { it to async { mail.inbox(it, PAGE_ROWS) } }
            }.map { (category, rows) -> category to rows.await() }
            others = others + read
        } catch (e: CoreException) {
            Log.w("Ozero", "other tabs", e)
        }
    }

    suspend fun load() {
        loadList()
        counts()
        loadOthers()
    }

    /**
     * The list is scrolled near its end: more of the saved mail, or once that ends, older mail
     * from the servers of a mailbox synced since the app opened.
     */
    suspend fun loadMore() {
        if (!savedEnds) {
            listSize += PAGE
            loadList()
            return
        }
        val box = mailbox
        if (loadingOlder || box !in syncedBoxes) return
        loadingOlder = true
        try {
            // Once the servers have nothing older, the core answers without asking them.
            if (mail.older(box)) olderLoads++
        } catch (e: CoreException) {
            error = e.text()
        } finally {
            loadingOlder = false
        }
        loadList()
    }

    /** Saves a change to an account; the sheet and the rows show it once it is saved. */
    fun changeAccount(change: suspend () -> Unit) {
        scope.launch {
            try {
                change()
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }

    /**
     * Syncs [box] in every account; false if one failed. One asked for while one runs follows
     * it. A [quiet] one, not asked for by the user, keeps a short network blip off the screen,
     * and the error already there until it goes through: the app tries again soon on its own.
     */
    suspend fun sync(box: Mailbox = Mail.INBOX, quiet: Boolean = false): Boolean {
        if (syncing) {
            syncAgain += box to (quiet && syncAgain[box] != false)
            return true
        }
        syncing = true
        if (!quiet) error = null
        val started = SystemClock.elapsedRealtime()
        // A refused login shows as its account's card over the list, not as an error.
        var refused = false
        val synced = try {
            refused = mail.refresh(box)
            if (box == Mail.INBOX) lastSync = started
            offlineSince = null
            error = null
            true
        } catch (e: CoreException) {
            // Another error means the network is there.
            val since = if (e is CoreException.Network) offlineSince ?: started else null
            offlineSince = since
            if (quiet && since != null && started - since < BLIP_MS) {
                Log.w("Ozero", "sync failed", e)
            } else {
                error = e.text()
            }
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
        // The first read may have failed; and an account may need signing in again now, or no more.
        if (accounts.isEmpty() || refused) loadAccounts()
        try {
            load()
        } finally {
            syncing = false
        }
        if (synced) syncedBoxes += box
        syncAgain.entries.firstOrNull()?.let { (next, quietly) ->
            syncAgain -= next
            return sync(next, quietly)
        }
        // The pulled drop lands before the gap closes, however quick the sync.
        if (pulled) delay(started + PULLED_MS - SystemClock.elapsedRealtime())
        pulled = false
        return synced
    }

    // Only change when one is added.
    LaunchedEffect(addingAccount) {
        if (!addingAccount) loadAccounts()
    }
    LaunchedEffect(tab, mailbox) {
        chosen = emptySet()
        listSize = PAGE
        loadList()
        listState.scrollToItem(0)
    }
    // Near the end of the list, the next rows come before the finger gets there.
    // Also for a list the core cuts short to nothing.
    LaunchedEffect(tab, mailbox, messages.size, olderLoads, syncedBoxes, preview) {
        if (preview) return@LaunchedEffect
        snapshotFlow {
            val info = listState.layoutInfo
            val last = info.visibleItemsInfo.lastOrNull()?.index ?: return@snapshotFlow false
            last >= info.totalItemsCount - AHEAD
        }.first { it }
        loadMore()
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
        // A failure from before the app was away says nothing about the network now.
        offlineSince = null
        if (SystemClock.elapsedRealtime() - lastSync > RESYNC_MS) {
            scope.launch {
                sync(quiet = true)
                if (mailbox != Mail.INBOX) sync(mailbox, quiet = true)
            }
        }
    }
    // While the app is on screen, new mail shows as it arrives.
    // An account signed in to again starts waiting, with its new secret.
    LifecycleStartEffect(accounts.map { it.id to it.needsSignIn }) {
        val watching = scope.launch {
            mail.watch(accounts) { sync(quiet = true) }
        }
        onStopOrDispose { watching.cancel() }
    }

    /** Shows a toast without undo, in place of any other. */
    suspend fun note(text: String) {
        snackbar.currentSnackbarData?.dismiss()
        snackbar.showSnackbar(text)
    }

    /**
     * Shows a toast for [undoSeconds]; true if Undo was tapped. One toast at a time. With [go],
     * it can also show that folder.
     */
    suspend fun offerUndo(text: String, go: Mailbox? = null): Boolean {
        // The older toast is past undo anyway.
        snackbar.currentSnackbarData?.dismiss()
        // Closes before the core sends, so Undo is never too late.
        val result = withTimeoutOrNull(undoSeconds().toLong() * 1000) {
            snackbar.showSnackbar(MovedToast(text, undo, go))
        }
        return result == SnackbarResult.ActionPerformed
    }

    // What changed the mail, here or in the background, shows at once; so does an action the
    // server refused and the phone undid.
    LaunchedEffect(Unit) {
        mail.changes.collect {
            load()
            loadAccounts()
            try {
                // After the toast on screen, which may still offer Undo.
                if (mail.takeReverted()) scope.launch { snackbar.showSnackbar(reverted) }
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }

    /**
     * Opens compose with the draft [make] gives for [original], once at a time; not if the
     * user has left the message meanwhile, as a forward may wait for its text to download.
     */
    fun write(original: MessageItem, make: suspend () -> DraftItem) {
        if (writing) return
        writing = true
        val reading = open?.id
        scope.launch {
            try {
                val draft = make()
                if (open?.id == reading) composing = Composing(draft, original)
            } catch (e: CoreException) {
                error = e.text()
            } finally {
                writing = false
            }
        }
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

    /** Closed with something written: the toast says where it went. */
    fun kept() {
        scope.launch { note(draftSaved) }
    }

    /** Thrown away: the toast can bring it back, saved again, to go on writing. */
    fun discarded(draft: DraftItem, original: MessageItem?) {
        composing = null
        scope.launch {
            // Nothing written is not worth bringing back.
            if (draft.isBlank()) return@launch note(draftDeleted)
            if (!offerUndo(draftDeleted)) return@launch
            try {
                composing = Composing(draft.copy(id = mail.saveDraft(draft)), original)
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }

    /** A draft from the list, with the message it answers if that is still saved. */
    fun openDraft(draft: DraftItem) {
        scope.launch {
            val original = draft.replyTo?.let { id ->
                try {
                    mail.message(id)
                } catch (_: CoreException) {
                    null
                }
            }
            composing = Composing(draft, original)
        }
    }

    /**
     * The messages leave the list at once, for their account's folder of [role] if they go to
     * one; the toast can take them all back for a moment, or show that folder.
     */
    fun remove(gone: List<MessageItem>, toast: String, role: FolderRole?, move: suspend (List<MessageItem>) -> List<Long>, reset: () -> Unit = {}) {
        val account = gone.map { it.accountId }.distinct().singleOrNull()
        val go = folders.find { it.accountId == account && it.role == role }?.let { Mailbox.Folder(it.id) }
        scope.launch {
            val actions = try {
                move(gone)
            } catch (e: CoreException) {
                error = e.text()
                reset()
                return@launch
            }
            val moved = gone.map { it.id }.toSet()
            messages = messages.filterNot { it.id in moved }
            // Also after success: a list read before the archive may bring the row back.
            reset()
            if (offerUndo(toast, go.takeIf { it != mailbox })) mail.undo(actions)
        }
    }

    /** Delete: into the trash, or for good from there. */
    fun delete(gone: List<MessageItem>, reset: () -> Unit = {}) {
        val forever = gone.none { it.canTrash }
        remove(gone, if (forever) deleted else trashed, FolderRole.TRASH.takeUnless { forever }, mail::delete, reset)
    }

    /** Empties the open trash or spam for good, on the server too; the toast can take it back for a moment. */
    fun empty() {
        val from = mailbox
        scope.launch {
            val actions = try {
                mail.emptyMailbox(from)
            } catch (e: CoreException) {
                error = e.text()
                return@launch
            }
            if (mailbox == from) messages = emptyList()
            if (offerUndo(emptied)) mail.undo(actions)
        }
    }

    /** Done: into the archive. */
    fun archive(gone: List<MessageItem>, reset: () -> Unit = {}) = remove(gone, archived, FolderRole.ARCHIVE, mail::archive, reset)

    /** Back into the inbox, out of the archive, trash or spam. */
    fun moveToInbox(gone: List<MessageItem>) = remove(gone, movedBack, FolderRole.INBOX, mail::moveToInbox)

    /** Marks the rows [read] or unread; they stay in the list either way. */
    fun setRead(rows: List<MessageItem>, read: Boolean, reset: () -> Unit = {}) {
        scope.launch {
            try {
                mail.markRead(rows, seen = read)
            } catch (e: CoreException) {
                error = e.text()
                reset()
                return@launch
            }
            val marked = rows.map { it.id }.toSet()
            messages = messages.map {
                if (it.id in marked) it.copy(seen = read, threadUnread = if (read) 0u else 1u) else it
            }
            reset()
        }
    }

    fun swiped(message: MessageItem, action: SwipeAction, reset: () -> Unit) = when (action) {
        SwipeAction.DONE -> archive(listOf(message), reset)
        SwipeAction.DELETE -> delete(listOf(message), reset)
        // Read if it is unread, unread if it is read.
        SwipeAction.READ -> setRead(listOf(message), message.threadUnread > 0u, reset)
    }

    /** The chosen messages, as the list shows them; acting on them ends the choosing. */
    fun choice(act: (List<MessageItem>) -> Unit) {
        val rows = messages.filter { it.id in chosen }
        chosen = emptySet()
        act(rows)
    }

    val again = accounts.find { it.id == signingIn }
    val screen = screenOf(composing, open, addingAccount, again, searching, draftsOpen)
    // Where back leads: the same screens with the top one gone.
    val behind: Screen? = when (screen) {
        Screen.Inbox -> null
        Screen.Search, Screen.AddAccount, is Screen.SignInAgain, Screen.Drafts -> Screen.Inbox
        is Screen.Read -> screenOf(null, null, addingAccount, again, searching, draftsOpen)
        is Screen.Write -> screenOf(null, open, addingAccount, again, searching, draftsOpen)
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
            is Screen.SignInAgain -> signingIn = null
            Screen.Drafts -> draftsOpen = false
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
                    is Screen.Write -> ComposeScreen(
                        mail,
                        shown.composing,
                        onCancel = ::back,
                        onSent = ::sent,
                        onKept = ::kept,
                        onDiscard = { discarded(it, shown.composing.original) },
                    )
                    Screen.Drafts -> DraftsScreen(mail, snackbar, onOpen = ::openDraft, onBack = ::back)
                    is Screen.Read -> MessageScreen(
                        mail,
                        shown.message,
                        onReply = { to, all -> write(to) { mail.reply(to, all) } },
                        onForward = { original ->
                            write(original) { mail.forward(original, forwardLabels(context), longDate(context, original.date)) }
                        },
                        onDelete = {
                            open = null
                            closeSearch()
                            delete(listOf(shown.message))
                        },
                        onUnread = {
                            open = null
                            scope.launch {
                                try {
                                    mail.markRead(listOf(shown.message), seen = false)
                                } catch (e: CoreException) {
                                    error = e.text()
                                }
                            }
                        },
                        onMoveToInbox = {
                            open = null
                            closeSearch()
                            moveToInbox(listOf(shown.message))
                        }.takeIf { shown.message.canMoveToInbox },
                        onDone = {
                            open = null
                            // Back to the list, where the toast can undo it.
                            closeSearch()
                            archive(listOf(shown.message))
                        }.takeIf { shown.message.canArchive },
                        onBack = ::back,
                    )
                    Screen.AddAccount -> SignInScreen(
                        mail,
                        onSignedIn = {
                            addingAccount = false
                            scope.launch { sync() }
                        },
                        onCancel = ::back,
                    )
                    is Screen.SignInAgain -> SignInScreen(
                        mail,
                        onSignedIn = {
                            signingIn = null
                            scope.launch { sync() }
                        },
                        onCancel = ::back,
                        again = shown.account,
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
                        listLoaded = listLoaded,
                        // The first mail of an empty list is on its way: the water waits, without words.
                        waiting = syncing && listLoaded && messages.isEmpty() && mailbox !in syncedBoxes,
                        onSync = {
                            pulled = true
                            scope.launch { sync(mailbox) }
                        },
                        mailbox = mailbox,
                        title = mailboxTitle(mailbox, openFolder),
                        owner = mailboxOwner(mailbox, openFolder, accounts),
                        accounts = accounts,
                        unsent = if (mailbox == Mail.INBOX) unsent else emptyList(),
                        signedOut = if (mailbox == Mail.INBOX) accounts.filter { it.needsSignIn } else emptyList(),
                        onSignIn = { signingIn = it.id },
                        swipes = swipes,
                        swipeTip = swipeTip,
                        onSwipeTip = {
                            swipeTip = false
                            settings.swipeTipSeen = true
                        },
                        messages = messages,
                        loadingOlder = loadingOlder,
                        tab = tab,
                        tabs = tabs,
                        others = others,
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
                        onTab = { next ->
                            // The next tab's rows are there at once, so its page comes in with them;
                            // the saved list follows. The one left keeps its rows to leave with.
                            others = others + (tab to messages)
                            others[next]?.let {
                                messages = it
                                preview = true
                            }
                            tab = next
                        },
                        onAccounts = { accountsOpen = true },
                        onSearch = { searching = true },
                        onCompose = { composing = Composing(NEW_DRAFT) },
                        onGo = { mailbox = it },
                        onBack = { mailbox = Mail.INBOX }.takeIf { mailbox != Mail.INBOX },
                        empty = mailboxRole(mailbox, openFolder)?.emptyLabel()?.let { it to { empty() } },
                        chosen = chosen,
                        onChoose = { chosen = it },
                        onRead = { read -> choice { setRead(it, read) } },
                        onDelete = { choice { delete(it) } },
                        onDone = { choice { rows -> archive(rows.filter { it.canArchive }) } },
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
            drafts = drafts.size,
            onDrafts = {
                accountsOpen = false
                draftsOpen = true
            },
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
            onSignIn = { account ->
                accountsOpen = false
                signingIn = account.id
            },
            onKeepDays = { account, days ->
                changeAccount { mail.setKeepDays(account, days) }
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
private fun screenOf(
    composing: Composing?,
    open: MessageItem?,
    addingAccount: Boolean,
    again: AccountItem?,
    searching: Boolean,
    drafts: Boolean,
): Screen =
    composing?.let { Screen.Write(it) }
        ?: open?.let { Screen.Read(it) }
        ?: when {
            addingAccount -> Screen.AddAccount
            again != null -> Screen.SignInAgain(again)
            searching -> Screen.Search
            drafts -> Screen.Drafts
            else -> Screen.Inbox
        }

/** What fills the screen; a deeper screen slides over the one it came from. */
private sealed class Screen(val depth: Int) {
    data object Inbox : Screen(0)
    data object Search : Screen(1)
    data object AddAccount : Screen(1)
    data class SignInAgain(val account: AccountItem) : Screen(1)
    data object Drafts : Screen(1)
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
    listLoaded: Boolean,
    waiting: Boolean,
    /** A pull asks for a sync; the empty list answers it with its own water. */
    onSync: () -> Unit,
    mailbox: Mailbox,
    title: String,
    owner: String?,
    accounts: List<AccountItem>,
    unsent: List<UnsentItem>,
    /** Accounts whose server refused the login, each with a card to sign in again. */
    signedOut: List<AccountItem>,
    onSignIn: (AccountItem) -> Unit,
    swipes: Swipes,
    swipeTip: Boolean,
    onSwipeTip: () -> Unit,
    messages: List<MessageItem>,
    /** Older mail is on its way from the servers. */
    loadingOlder: Boolean,
    tab: Category,
    tabs: List<InboxTab>,
    /** The first rows of the other tabs, shown as their pages glide past. */
    others: Map<Category, List<MessageItem>>,
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
    onGo: (Mailbox) -> Unit,
    /** Back to the inbox from another mailbox; null in the inbox. */
    onBack: (() -> Unit)?,
    empty: Pair<Int, () -> Unit>?,
    /** Messages chosen to act on together; acting on them ends the choosing. */
    chosen: Set<Long>,
    onChoose: (Set<Long>) -> Unit,
    onRead: (read: Boolean) -> Unit,
    onDelete: () -> Unit,
    onDone: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val density = LocalDensity.current
    val haptics = LocalHapticFeedback.current
    // Only what the list still shows: a sync may take a chosen message away.
    val picked = messages.filter { it.id in chosen }
    val choosing = picked.isNotEmpty()
    val mode by animateFloatAsState(if (choosing) 1f else 0f, Motion.settle(), label = "choosing")
    var more by remember { mutableStateOf(false) }
    BackHandler(enabled = choosing) { onChoose(emptySet()) }
    // The gestures outlive a composition; they read the list as it is now.
    val rowsNow by rememberUpdatedState(messages)
    val chosenNow by rememberUpdatedState(chosen)
    val chooseNow by rememberUpdatedState(onChoose)
    val shownTitle = if (choosing) stringResource(R.string.select_count, picked.size) else title
    // The hint only stands in for a line that is there anyway: the rows must not move under the finger.
    val shownOwner = if (choosing && owner != null) stringResource(R.string.select_hint) else owner
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
    // Half way through the title, under the back arrow's 4 + 44 + 2 dp.
    val titleGone = with(density) { 62.dp.roundToPx() }
    val scrolled by remember { derivedStateOf { listState.firstVisibleItemIndex > 0 || listState.firstVisibleItemScrollOffset > titleGone } }
    val pull = rememberPullToRefreshState()
    // There is one water, under the title, and it stays put: a pull over an empty list's lake
    // gathers a drop over it, and a pull on a list moves the rows down to uncover the same water.
    val lake = messages.isEmpty() && (!syncing || listLoaded) && error == null
    // Each pull that lets go past a third of the way drops a drop, sized by how far it went.
    val drops = remember { MutableSharedFlow<Float>(extraBufferCapacity = 1) }
    // After a sync the gap stays open until its water has calmed, so the calm is seen; and after
    // a tug, until its drop is in. The finger can pull again meanwhile, from there.
    var waterCalm by remember { mutableStateOf(true) }
    val held = remember { Animatable(0f) }
    val holdTo = if (!lake && (refreshing || !waterCalm)) 1f else 0f
    LaunchedEffect(holdTo) { held.animateTo(holdTo, Motion.settle()) }
    val lakeNow by rememberUpdatedState(lake)
    val holdNow by rememberUpdatedState(holdTo)
    var pressing by remember { mutableStateOf(false) }
    // How far the list is down, in pulls: with the finger, or held, and a finger pulls on from where it is held.
    val opened = { maxOf(pull.distanceFraction, held.value + if (pressing) pull.distanceFraction * 0.6f else 0f) }
    val sink = if (lake) Modifier else Modifier.graphicsLayer { translationY = pullGap(opened()) }
    // Where the title ends on screen, or null once it has scrolled away.
    val underTitle = {
        val info = listState.layoutInfo
        info.visibleItemsInfo.firstOrNull { it.key == "title" }?.let { (it.offset + it.size - info.viewportStartOffset).toFloat() }
    }
    // Where the tabs are between two of them, in tabs: the thumb and the list follow it.
    val glide = remember { Animatable(0f) }
    val page = Category.entries.indexOf(tab)
    // The page just left, which keeps going from where it was let go: from the row at the top.
    var leaving by remember { mutableStateOf<Leaving?>(null) }
    val glideScope = rememberCoroutineScope()
    // The tab the thumb counts from. It changes in the same moment as the glide, so the thumb
    // never sees one without the other, while the tab itself changes at once.
    var thumbTab by remember { mutableIntStateOf(page) }
    LaunchedEffect(tab) { if (!glide.isRunning) thumbTab = page }
    // Where the thumb is, in tabs from the first: it moves on without a jump as a tab is picked,
    // so every page reads where it is from it.
    val thumb = { thumbTab + glide.value }
    val pick: (Category, Float, Float) -> Unit = { next, from, velocity ->
        val info = listState.layoutInfo
        leaving = info.visibleItemsInfo.firstOrNull { it.key is Long }?.let { row ->
            val skip = messages.indexOfFirst { it.id == row.key }.coerceAtLeast(0)
            val gap = if (lake) 0f else with(density) { pullGap(opened()) }
            Leaving(tab, messages.drop(skip).take(PAGE_ROWS), row.offset - info.viewportStartOffset + gap)
        }
        onTab(next)
        glideScope.launch {
            glide.snapTo(from)
            thumbTab = Category.entries.indexOf(next)
            glide.animateTo(0f, thumbSpring(velocity), velocity)
        }
    }
    // The row a finger holds, which draws its neighbours along, and where the last swiped row was.
    val rowPull = remember { Pull() }
    var closedAt by remember(tab, mailbox) { mutableIntStateOf(Int.MAX_VALUE) }
    // Once the rows have closed up, later moves are not held back by it.
    LaunchedEffect(closedAt) {
        delay(CLOSING_MS)
        closedAt = Int.MAX_VALUE
    }
    // How many messages are scrolled past, so the rows on screen pour in from the top one down.
    val leading = 1 + signedOut.size + unsent.size + (if (swipeTip && messages.isNotEmpty()) 1 else 0) + (if (lake) 1 else 0)
    val firstRow by remember(leading) { derivedStateOf { (listState.firstVisibleItemIndex - leading).coerceAtLeast(0) } }
    // The list leaves to the side the thumb moves away from, and the next one comes in from the other.
    val slide = Modifier.glided { thumb() - page }
    val mini by animateFloatAsState(if (scrolled) 1f else 0f, Motion.calm(Motion.SMALL), label = "title")
    Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Horizontal))) {
        // The other tabs' pages, under the list: the one left goes on from where the finger let it
        // go, and the next shows under the finger before it is picked.
        if (inbox && !allEmpty) {
            Category.entries.forEachIndexed { at, category ->
                if (category == tab) return@forEachIndexed
                val from = leaving?.takeIf { it.tab == category }
                key(category) {
                    Page(
                        at = at,
                        rows = from?.rows ?: others[category].orEmpty(),
                        words = emptyWords(inbox = true, allEmpty = false, category),
                        water = !lake,
                        thumb = thumb,
                        top = { from?.top ?: underTitle() ?: with(density) { (top + 48.dp).toPx() } },
                        onGone = { if (leaving?.tab == category) leaving = null },
                    )
                }
            }
        }
        PullToRefreshBox(
            // The list is never held by the pull itself, so a finger can pull again at any time:
            // every pull drops a drop, and one that goes all the way syncs, unless one runs.
            isRefreshing = false,
            onRefresh = { if (!refreshing) onSync() },
            modifier = Modifier.fillMaxSize().whilePressed { down ->
                if (down) {
                    pressing = true
                    return@whilePressed
                }
                val f = pull.distanceFraction
                if (f >= TUG) drops.tryEmit(f)
                // The list stays where the finger left it, and is held or let go from there.
                val at = opened()
                pressing = false
                if (!lakeNow && at > held.value) {
                    glideScope.launch {
                        held.snapTo(at)
                        held.animateTo(holdNow, Motion.settle())
                    }
                }
            },
            state = pull,
            // The rows move down with the finger and still water shows in the gap they leave.
            indicator = {
                if (!lake) {
                    PullWater(
                        fraction = { pull.distanceFraction },
                        depth = { with(density) { pullGap(opened()) } },
                        refreshing = refreshing,
                        top = underTitle,
                        drops = drops,
                        onCalm = { waterCalm = it },
                        modifier = Modifier.fillMaxSize(),
                    )
                }
            },
        ) {
            LazyColumn(
                Modifier
                    .fillMaxSize()
                    .chooseRows(
                        listState,
                        rows = { rowsNow.map { it.id } },
                        chosen = { chosenNow.filterTo(LinkedHashSet()) { id -> rowsNow.any { it.id == id } } },
                        onChoose = { chooseNow(it) },
                        onHold = { haptics.performHapticFeedback(HapticFeedbackType.LongPress) },
                    ),
                state = listState,
                contentPadding = PaddingValues(top = top, bottom = dockHeight + 16.dp),
            ) {
                item(key = "title") { Hero(shownTitle, shownOwner, onBack) }
                items(signedOut, key = { "signed-out-${it.id}" }) { account ->
                    Afloat(thumb, page, depth = { 0 }, modifier = rowMove().then(sink)) { SignedOutRow(account, onClick = { onSignIn(account) }) }
                }
                items(unsent, key = { "unsent-${it.id}" }) { failed ->
                    Afloat(thumb, page, depth = { 0 }, modifier = rowMove().then(sink)) { UnsentRow(failed, onClick = { onUnsent(failed) }) }
                }
                if (swipeTip && messages.isNotEmpty()) {
                    item(key = "tip") { Afloat(thumb, page, depth = { 0 }, modifier = rowMove().then(sink)) { SwipeTip(swipes, onSwipeTip) } }
                }
                if (lake) {
                    item(key = "empty") {
                        // New mail covers the water rather than cutting it off.
                        // One water for every tab: only the words go with the tabs. It fades in, without
                        // moving, when the tabs come from a list with mail.
                        var fresh by remember { mutableStateOf(glide.isRunning || glide.value != 0f) }
                        LaunchedEffect(fresh) {
                            if (!fresh) return@LaunchedEffect
                            snapshotFlow { glide.isRunning || glide.value != 0f }.first { !it }
                            fresh = false
                        }
                        val fade = if (fresh) Modifier.graphicsLayer { alpha = glidedAlpha(thumb() - page) } else Modifier
                        Box(Modifier.animateItem(fadeInSpec = null, placementSpec = null, fadeOutSpec = Motion.calm(Motion.SMALL)).then(fade)) {
                            key(mailbox) {
                                val (title, text) = emptyWords(inbox, allEmpty, tab)
                                StillNote(
                                    stringResource(title),
                                    stringResource(text),
                                    drawIn = hadMail && inbox,
                                    grey = !inbox,
                                    waiting = waiting,
                                    refreshing = refreshing,
                                    pulled = { pull.distanceFraction },
                                    drops = drops,
                                    glide = thumb,
                                    wordsModifier = if (allEmpty) Modifier else slide,
                                )
                            }
                        }
                    }
                }
                itemsIndexed(messages, key = { _, it -> it.id }) { index, message ->
                    // Rows below one that left close up after it, each a little later than the one above.
                    val behind = if (index >= closedAt) index - closedAt + 1 else 0
                    Afloat(
                        thumb,
                        page,
                        depth = { (index - firstRow).coerceIn(0, 8) },
                        modifier = rowMove(Motion.behind(behind.coerceAtMost(6), Motion.CLOSE, IntOffset.VisibilityThreshold)).then(sink),
                        pull = rowPull,
                        index = index,
                    ) {
                        if (index > 0) HorizontalDivider(Modifier.padding(start = Gap.screen), color = colors.outlineVariant)
                        SwipeRow(
                            message,
                            swipes,
                            enabled = !choosing,
                            onSwipe = { action, reset ->
                                // Read leaves the row where it is: nothing below has a gap to close.
                                if (action != SwipeAction.READ) closedAt = index
                                onSwipe(message, action, reset)
                            },
                            onPull = { rowPull.hold(message.id, index, it) },
                        ) {
                            MessageRow(
                                message,
                                choosing = mode,
                                chosen = message.id in chosen,
                                onClick = {
                                    when {
                                        !choosing -> onOpen(message)
                                        message.id in chosen -> onChoose(chosen - message.id)
                                        else -> onChoose(chosen + message.id)
                                    }
                                },
                            )
                        }
                    }
                }
                if (loadingOlder) {
                    item(key = "older") {
                        Text(
                            stringResource(R.string.inbox_loading_older),
                            color = colors.onSurfaceVariant,
                            style = Type.Caption,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp).animateItem(),
                        )
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
                Text(shownTitle, style = Type.Row, fontWeight = Weight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                onBack?.let { BackArrow(it, Modifier.align(Alignment.CenterStart)) }
            }
        }
        Column(Modifier.align(Alignment.BottomCenter)) {
            // Low on the screen, in reach of the thumb that swiped.
            Toast(snackbar, onGo = onGo, modifier = Modifier.padding(bottom = 8.dp))
            Reveal(error) {
                Text(
                    stringResource(it),
                    color = colors.onSurfaceVariant,
                    style = Type.Caption,
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
                thumb = thumb,
                accounts = accounts,
                onTab = pick,
                onAccounts = onAccounts,
                onSearch = onSearch,
                onCompose = onCompose,
                empty = empty.takeIf { messages.isNotEmpty() && !choosing },
                choosing = mode,
                choiceBar = {
                    ChoiceBar(
                        done = picked.count { it.canArchive },
                        unread = picked.any { it.threadUnread > 0u },
                        forever = picked.isNotEmpty() && picked.none { it.canTrash },
                        onCancel = { onChoose(emptySet()) },
                        onRead = { onRead(picked.any { it.threadUnread > 0u }) },
                        onDelete = onDelete,
                        onMore = { more = true },
                        onDone = onDone.takeIf { picked.any { it.canArchive } },
                    )
                },
            )
        }
        // The rarer ways to choose, in a sheet like every menu.
        Sheet(visible = more, onClose = { more = false }) {
            SheetAction(R.drawable.ic_done, stringResource(R.string.action_select_all), colors.onSurface) {
                more = false
                onChoose(chosen + messages.map { it.id })
            }
            // The sender of the message chosen last.
            messages.find { it.id == chosen.lastOrNull() }?.let { last ->
                SheetAction(R.drawable.ic_inbox, stringResource(R.string.action_all_from, last.sender), colors.onSurface) {
                    more = false
                    onChoose(chosen + messages.filter { it.fromEmail == last.fromEmail }.map { it.id })
                }
            }
        }
    }
}

/** The mailbox's name: ours for a role, the server's for [folder], an account's own. */
@Composable
private fun mailboxTitle(mailbox: Mailbox, folder: FolderItem?): String = when (mailbox) {
    is Mailbox.Unified -> mailbox.role.title("")
    is Mailbox.Folder -> folder?.title().orEmpty()
}

/** What the mailbox holds: its role, or [folder]'s. */
private fun mailboxRole(mailbox: Mailbox, folder: FolderItem?): FolderRole? = when (mailbox) {
    is Mailbox.Unified -> mailbox.role
    is Mailbox.Folder -> folder?.role
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
internal fun Hero(title: String, subtitle: String?, onBack: (() -> Unit)?) {
    Column {
        // The arrow's row is there even without it, so every title sits at the same height.
        Box(Modifier.padding(top = 4.dp).height(44.dp)) { onBack?.let { BackArrow(it) } }
        Column(Modifier.padding(start = Gap.screen, end = Gap.screen, top = 2.dp, bottom = 14.dp)) {
            LargeTitle(title)
            if (subtitle != null) {
                Text(
                    subtitle,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = Type.Caption,
                    modifier = Modifier.padding(top = 2.dp),
                )
            }
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
    /** Where the tabs' thumb is, in tabs from the first. */
    thumb: () -> Float,
    accounts: List<AccountItem>,
    onTab: (Category, from: Float, velocity: Float) -> Unit,
    onAccounts: () -> Unit,
    onSearch: () -> Unit,
    onCompose: () -> Unit,
    empty: Pair<Int, () -> Unit>?,
    /** From 0 to 1 as choosing begins: [choiceBar] takes the place of search. */
    choosing: Float,
    choiceBar: @Composable () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val rise = with(LocalDensity.current) { 16.dp.toPx() }
    Column(
        modifier
            .fillMaxWidth()
            .background(colors.bar)
            .hairline(colors.outlineVariant, top = true)
            .navigationBarsPadding()
            .padding(top = Gap.barTop, bottom = Gap.barBottom),
        verticalArrangement = Arrangement.spacedBy(Gap.inBar),
    ) {
        tabs?.let { Tabs(tab, it, glide, thumb, onTab) }
        empty?.let { (label, onClick) -> EmptyButton(stringResource(label), onClick) }
        // As tall as the taller of the two, so the dock keeps its height as they change.
        Box(Modifier.height(52.dp), contentAlignment = Alignment.Center) {
            if (choosing < 1f) {
                // The same edge as the tabs above, so the accounts button lines up with them.
                Row(
                    Modifier
                        .fillMaxWidth()
                        .graphicsLayer {
                            alpha = 1f - choosing
                            translationY = rise * choosing
                        }
                        .then(if (choosing > 0.5f) Modifier.blockTouches() else Modifier)
                        .padding(horizontal = Gap.bar),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
                ) {
                    AccountsButton(accounts, onAccounts)
                    Pill(stringResource(R.string.search_prompt), onSearch, Modifier.weight(1f), icon = R.drawable.ic_search)
                    IconTile(
                        R.drawable.ic_compose,
                        stringResource(R.string.compose_new),
                        onCompose,
                        background = colors.accent,
                        tint = colors.onAccent,
                    )
                }
            }
            if (choosing > 0f) {
                Box(
                    Modifier
                        .graphicsLayer {
                            alpha = choosing
                            translationY = rise * (1f - choosing)
                        }
                        .then(if (choosing < 0.5f) Modifier.blockTouches() else Modifier),
                ) { choiceBar() }
            }
        }
    }
}

/** Empties the open trash or spam for good, where the inbox has its tabs. */
@Composable
private fun EmptyButton(label: String, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Box(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.bar)
            .height(40.dp)
            .clip(RoundedCornerShape(Radius.medium))
            .background(colors.surfaceVariant)
            .clickable(role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = colors.error, style = Type.Text, fontWeight = Weight.Medium)
    }
}

/** What the button that empties a folder says; only the trash and spam can be emptied. */
private fun FolderRole.emptyLabel(): Int? = when (this) {
    FolderRole.TRASH -> R.string.action_empty_trash
    FolderRole.JUNK -> R.string.action_empty_junk
    else -> null
}

/** The accounts' colours, overlapping; opens the accounts. */
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
private fun Tabs(
    tab: Category,
    tabs: List<InboxTab>,
    glide: Animatable<Float, AnimationVector1D>,
    thumbAt: () -> Float,
    onPick: (Category, from: Float, velocity: Float) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val thumb = RoundedCornerShape(Radius.thumb)
    val scope = rememberCoroutineScope()
    val haptics = LocalHapticFeedback.current
    val index = Category.entries.indexOf(tab)
    val last = Category.entries.size - 1
    val thumbNow by rememberUpdatedState(thumbAt)
    // The thumb's two edges ride springs of their own: the front one leads and the back one is
    // held back as if by water, so the thumb draws out while it moves and gathers as it stops.
    val leftEdge = remember { Animatable(index.toFloat()) }
    val rightEdge = remember { Animatable(index.toFloat()) }
    LaunchedEffect(Unit) {
        // An edge leads when the thumb moves its way.
        launch { leftEdge.follow({ to -> edgeSpring(lead = to < leftEdge.value) }) { thumbNow() } }
        rightEdge.follow({ to -> edgeSpring(lead = to > rightEdge.value) }) { thumbNow() }
    }
    var pressed by remember { mutableStateOf(false) }
    val press by animateFloatAsState(if (pressed) 1f else 0f, Motion.settle(), label = "press")
    BoxWithConstraints(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.bar)
            .clip(RoundedCornerShape(Radius.medium))
            .background(colors.surfaceVariant)
            .whilePressed { pressed = it }
            .padding(3.dp),
    ) {
        val count = Category.entries.size
        val gap = 2.dp
        val width = (maxWidth - gap * (count - 1)) / count
        val step = width + gap
        // How long the thumb is drawn out between its edges.
        val long: Density.() -> Float = { width.toPx() + abs(rightEdge.value - leftEdge.value) * step.toPx() }
        Box(
            Modifier
                .layout { measurable, _ ->
                    val from = min(leftEdge.value, rightEdge.value)
                    val placeable = measurable.measure(Constraints.fixed(long().roundToInt(), 34.dp.roundToPx()))
                    layout(placeable.width, placeable.height) { placeable.placeRelative((from * step.toPx()).roundToInt(), 0) }
                }
                .graphicsLayer {
                    // It keeps its area, so it thins as it draws out; under a finger it dips a little.
                    scaleX = 1f - 0.03f * press
                    scaleY = sqrt(width.toPx() / long()) * (1f - 0.06f * press)
                }
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
            Category.entries.forEachIndexed { at, category ->
                val on = category == tab
                val unread = tabs.find { it.category == category }?.unread ?: 0u
                // The label darkens as the thumb comes over it, not all at once; read while drawing,
                // so the glide does not recompose the tabs on every frame.
                val label = ColorProducer { lerp(colors.onSurfaceVariant, colors.onSurface, (1f - abs(at - thumbAt())).coerceIn(0f, 1f)) }
                // A count that changes gives a small nudge, as a float does when something touches it.
                // Not when the counts first come in from the core.
                val nudge = remember { Animatable(0f) }
                val known = tabs.any { it.category == category }
                var counted by remember { mutableStateOf<UInt?>(null) }
                LaunchedEffect(unread, known) {
                    if (!known) return@LaunchedEffect
                    val before = counted
                    counted = unread
                    if (before != null && before != unread) nudge.animateTo(0f, Motion.nudge(), initialVelocity = 9f)
                }
                Row(
                    Modifier
                        .weight(1f)
                        .height(34.dp)
                        .clip(thumb)
                        .selectable(selected = on, role = Role.Tab, onClick = { if (!on) onPick(category, (index - at).toFloat(), 0f) }),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    BasicText(
                        stringResource(category.label()),
                        color = label,
                        style = Type.Text.copy(fontWeight = if (on) Weight.SemiBold else Weight.Medium),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false),
                    )
                    if (unread > 0u) {
                        Text(
                            unread.toString(),
                            color = colors.primary,
                            style = Type.Badge,
                            fontWeight = Weight.Bold,
                            modifier = Modifier.padding(start = 6.dp).graphicsLayer {
                                scaleX = 1f + 0.18f * nudge.value
                                scaleY = scaleX
                            },
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

/** An account whose server refused the login, calm as nothing is lost; a tap signs in again. */
@Composable
private fun SignedOutRow(account: AccountItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    NoticeRow(
        stringResource(R.string.inbox_signed_out, account.name),
        stringResource(R.string.inbox_signed_out_text),
        colors.primary,
        colors.surfaceContainer,
        onClick,
    )
}

/** A message the server refused; a tap opens it to fix. */
@Composable
private fun UnsentRow(unsent: UnsentItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    NoticeRow(
        stringResource(R.string.inbox_not_sent),
        unsent.subject.ifBlank { stringResource(R.string.inbox_no_subject) },
        colors.error,
        colors.error.copy(alpha = 0.1f),
        onClick,
    )
}

/** A card over the list that asks for a tap: a [title] in [tint] over one line of [text]. */
@Composable
private fun NoticeRow(title: String, text: String, tint: Color, background: Color, onClick: () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.card, vertical = 4.dp)
            .clip(RoundedCornerShape(Radius.tile))
            .background(background)
            .clickable(onClick = onClick)
            .padding(horizontal = Gap.inCard, vertical = 12.dp),
    ) {
        Text(title, color = tint, style = Type.Subject, fontWeight = Weight.SemiBold)
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, style = Type.Text, maxLines = 2, overflow = TextOverflow.Ellipsis)
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
 * of the account's colour on the left and its name by the time mark it. While [choosing]
 * (0 to 1 as it begins), the row moves over for its mark, filled when [chosen].
 */
@Composable
internal fun MessageRow(message: MessageItem, onClick: () -> Unit, choosing: Float = 0f, chosen: Boolean = false) {
    val colors = MaterialTheme.colorScheme
    val account = message.account
    val unread = message.threadUnread > 0u
    // The dot goes out quickly and in place: read is the most frequent change there is.
    val dot by animateFloatAsState(if (unread) 1f else 0f, Motion.calm(Motion.QUICK), label = "dot")
    val tint by animateColorAsState(if (chosen) colors.accent.copy(alpha = 0.08f) else Color.Transparent, Motion.calm(Motion.QUICK), label = "chosen")
    Box(Modifier.fillMaxWidth().background(tint)) {
        if (choosing > 0f) ChoiceMark(chosen, choosing, Modifier.align(Alignment.CenterStart).padding(start = 14.dp))
        MessageLine(message, unread, dot, account, MARK_ROOM * choosing, Modifier.clickable(onClick = onClick))
    }
}

@Composable
private fun MessageLine(message: MessageItem, unread: Boolean, dot: Float, account: AccountMark?, shift: Dp, modifier: Modifier) {
    val colors = MaterialTheme.colorScheme
    Row(
        modifier
            .fillMaxWidth()
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
            .padding(start = Gap.screen + shift, end = 16.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                // The sender and count take what is left, so the time always ends at the edge.
                Row(Modifier.weight(1f).padding(end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        message.participants(),
                        style = Type.Row,
                        fontWeight = if (unread) Weight.Strong else Weight.Medium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier
                            .weight(1f, fill = false)
                            // The unread dot hangs in the margin, so text lines up with the title.
                            .drawBehind {
                                if (dot > 0f) {
                                    val x = if (layoutDirection == LayoutDirection.Ltr) -9.dp.toPx() else size.width + 9.dp.toPx()
                                    drawCircle(colors.accent.copy(alpha = dot), 4.dp.toPx() * (0.4f + 0.6f * dot), Offset(x, size.height / 2))
                                }
                            },
                    )
                    if (message.threadCount > 1u) {
                        Text(
                            message.threadCount.toString(),
                            color = colors.onSurfaceVariant,
                            style = Type.Badge,
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
                        style = Type.Label,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.widthIn(max = 70.dp).padding(end = 8.dp),
                    )
                }
                Text(
                    // Not out yet: once the server has it, its date shows.
                    if (message.sending) stringResource(R.string.inbox_sending) else shortDate(LocalContext.current, message.date),
                    color = if (unread) colors.onSurface else colors.onSurfaceVariant,
                    style = Type.Caption,
                )
            }
            Text(
                message.subjectText(),
                style = Type.Subject,
                fontWeight = if (unread) Weight.Firm else Weight.Regular,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (message.snippet.isNotBlank()) {
                Text(
                    message.snippet,
                    color = colors.onSurfaceVariant,
                    style = Type.Text,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (message.attachments.isNotEmpty()) AttachmentChips(message.attachments)
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
fun shortDate(context: Context, seconds: Long): String =
    format(context, seconds, if (DateUtils.isToday(seconds * 1000)) time(context) else day(seconds))

/** The date and time, as a message header shows them. */
fun longDate(context: Context, seconds: Long): String = format(context, seconds, day(seconds) + time(context))

/**
 * [seconds] by an ICU skeleton, in the language the app shows: with a per-app language that is
 * not the system's, DateUtils would still name months in the system's.
 */
private fun format(context: Context, seconds: Long, skeleton: String): String =
    DateFormat.getInstanceForSkeleton(skeleton, context.resources.configuration.locales[0])
        .format(Date(seconds * 1000))

/** Day and short month, and the year when it is not this one. */
private fun day(seconds: Long): String {
    val zone = ZoneId.systemDefault()
    val thisYear = Instant.ofEpochSecond(seconds).atZone(zone).year == LocalDate.now(zone).year
    return if (thisYear) "MMMd" else "yMMMd"
}

/** Hours and minutes, on the clock the user chose. */
private fun time(context: Context): String = if (android.text.format.DateFormat.is24HourFormat(context)) "Hm" else "hm"

/** A released thumb: a little give after a fling, since the finger threw it; flat otherwise. */
private fun thumbSpring(velocity: Float): SpringSpec<Float> = if (abs(velocity) > FLING) Motion.fling() else Motion.settle()

/** Tabs a second past which a released thumb was flung. */
private const val FLING = 2f

/** A tab thumb's edge: the front one leads, quicker than the finger's spring; the back one follows through the water. */
private fun edgeSpring(lead: Boolean): SpringSpec<Float> = spring(dampingRatio = 1f, stiffness = if (lead) 1540f else 315f)

/** The least a pulled refresh lasts: the drop's fall and its first rings. */
private const val PULLED_MS = 1100L

/** How long rows below one that left are held back as they close up; then moves are even again. */
private const val CLOSING_MS = 2000L

/**
 * A row afloat on the tab [page]. It follows the tabs' [thumb] on a spring of its own, softer the
 * further [depth] it is down the screen, so a tab's list pours in from the top; and, given a
 * [pull], it is drawn a little way after the row a finger holds.
 */
@Composable
private fun Afloat(
    thumb: () -> Float,
    page: Int,
    depth: () -> Int,
    modifier: Modifier,
    pull: Pull? = null,
    index: Int = 0,
    content: @Composable () -> Unit,
) {
    val along = remember { Animatable(thumb()) }
    val drawn = remember { Animatable(0f) }
    val depthNow by rememberUpdatedState(depth)
    val thumbNow by rememberUpdatedState(thumb)
    LaunchedEffect(Unit) { along.follow({ Motion.behind(depthNow(), Motion.POUR) }) { thumbNow() } }
    if (pull != null) LaunchedEffect(pull, index) { drawn.follow({ Motion.drift() }) { pull.near(index) } }
    Column(modifier.glided { along.value - page }.graphicsLayer { translationX = drawn.value }) { content() }
}

/**
 * The page of the tab [at], under the list while the thumb is near: its first [rows], or its
 * [words] once it is empty, over still [water] where the list has none of its own to share.
 */
@Composable
private fun Page(at: Int, rows: List<MessageItem>, words: Pair<Int, Int>, water: Boolean, thumb: () -> Float, top: () -> Float, onGone: () -> Unit) {
    val thumbNow by rememberUpdatedState(thumb)
    var near by remember { mutableStateOf(false) }
    // Rows further down follow later, so the page stays a little after the thumb has gone.
    LaunchedEffect(at) {
        snapshotFlow { abs(at - thumbNow()) < 0.99f }.collectLatest { now ->
            if (now) {
                near = true
            } else {
                delay(PAGE_LINGER_MS)
                near = false
                onGone()
            }
        }
    }
    if (!near) return
    Column(Modifier.fillMaxWidth().offset { IntOffset(0, top().roundToInt()) }) {
        if (rows.isEmpty()) {
            // The water stays where it is and only shows; the words go with the page.
            StillNote(
                stringResource(words.first),
                stringResource(words.second),
                drawIn = false,
                water = { if (water) glidedAlpha(thumb() - at) else 0f },
                wordsModifier = Modifier.glided { thumb() - at },
            )
        } else {
            val colors = MaterialTheme.colorScheme
            rows.forEachIndexed { index, message ->
                Afloat(thumb, at, depth = { index.coerceAtMost(8) }, modifier = Modifier) {
                    if (index > 0) HorizontalDivider(Modifier.padding(start = Gap.screen), color = colors.outlineVariant)
                    MessageRow(message, onClick = {})
                }
            }
        }
    }
}

/** A page as it was left: its [tab], the rows from the top one on screen, and where that one was. */
private class Leaving(val tab: Category, val rows: List<MessageItem>, val top: Float)

/** What an empty list says: an empty folder, the whole inbox or its first tab done, or another tab done. */
private fun emptyWords(inbox: Boolean, allEmpty: Boolean, tab: Category): Pair<Int, Int> = when {
    !inbox -> R.string.tab_empty_title to R.string.folder_empty
    allEmpty || tab == Category.PEOPLE -> R.string.inbox_zero_title to R.string.inbox_zero_text
    else -> R.string.tab_empty_title to R.string.tab_empty_text
}

/** How long a page stays after the thumb has left it, while its lower rows catch up. */
private const val PAGE_LINGER_MS = 1200L

/** How far a pull must go, in pulls, to drop a drop when let go. */
private const val TUG = 1f / 3f



/** Something on a tab's page, [at] tabs from its place: it leaves to the side the thumb moves away from and fades. */
private fun Modifier.glided(at: () -> Float): Modifier = graphicsLayer {
    translationX = -at() * size.width * 0.35f
    alpha = glidedAlpha(at())
}

/** How much of a list shows [at] a glide: it is gone before the tabs are halfway. */
private fun glidedAlpha(at: Float) = 1f - (abs(at) * 1.2f).coerceAtMost(1f)

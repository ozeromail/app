@file:OptIn(ExperimentalFoundationApi::class, ExperimentalLayoutApi::class)

package com.ozeromail.app

import android.content.ClipData
import android.net.Uri
import android.util.Log
import androidx.activity.compose.BackHandler
import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.cancel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.plus
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftItem
import uniffi.ozero_api.FolderItem
import uniffi.ozero_api.ImagesPage
import uniffi.ozero_api.InviteAnswer
import uniffi.ozero_api.InviteItem
import uniffi.ozero_api.Mailbox
import uniffi.ozero_api.MessageBody
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.PeopleItem
import uniffi.ozero_api.PersonItem
import uniffi.ozero_api.RemoteImage
import uniffi.ozero_api.Unsubscribed
import uniffi.ozero_api.richEscape
import uniffi.ozero_api.richShown

/**
 * A conversation, oldest first, on one page that scrolls title and all. What was unread
 * when it opened is open, and the page starts at the first of it; the rest sit closed as a
 * line each, and a tap opens or closes any of them. Under each open one in a conversation,
 * Reply and Forward answer that very message.
 *
 * Reply, Delete, Snooze, More and Done sit in one row at the bottom, in reach of the thumb;
 * the row steps aside while the page is read down and comes back on the way up and at the
 * end. A reply is written low on the same page, the message still in sight above it; one
 * more tap takes it to the whole screen.
 */
@Composable
fun MessageScreen(
    mail: Mail,
    message: MessageItem,
    /** A message to write on the whole screen: a reply taken there, or one to a person. */
    onWrite: (Composing) -> Unit,
    /** A new message to someone the message is from or went to. */
    onWriteTo: (String) -> Unit,
    /** A reply closed with something written: it is in the drafts. */
    onKept: () -> Unit,
    onForward: (MessageItem) -> Unit,
    onDelete: () -> Unit,
    onUnread: () -> Unit,
    onMoveToInbox: (() -> Unit)?,
    /** Into another folder of its account, named as shown; null where mail does not move from. */
    onMove: ((FolderItem, String) -> Unit)?,
    onSpam: (() -> Unit)?,
    onFlag: (Boolean) -> Unit,
    onDone: (() -> Unit)?,
    onSnooze: (() -> Unit)?,
    /** Answers a meeting invitation of the conversation, with a comment for its organizer. */
    onAnswer: (invitation: MessageItem, invite: InviteItem, answer: InviteAnswer, comment: String) -> Unit,
    onBack: () -> Unit,
    /** Beside the list: the actions fit one row, and the list keeps the start edge clear. */
    pane: Boolean = false,
    /** On its own screen, the list's toasts, so Undo shows over the next message; [onGo] shows a folder. */
    toasts: SnackbarHostState? = null,
    onGo: (Mailbox) -> Unit = {},
) {
    var sheet by remember(message.id) { mutableStateOf<ReaderSheet?>(null) }
    // Whether the person left the list it came from here, and the question before they do.
    var left by remember(message.id) { mutableStateOf(false) }
    var leaving by remember(message.id) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val own = remember { SnackbarHostState() }
    val snackbar = toasts ?: own
    val files = remember(message.id) { Files(mail, context, scope, snackbar) }
    var thread by remember(message.id) { mutableStateOf(listOf(message)) }
    var error by remember(message.id) { mutableStateOf<Int?>(null) }
    // What the user opened or closed.
    val opened = remember(message.id) { mutableStateMapOf<Long, Boolean>() }
    // What was unread when it opened, and every message it had then; null until read.
    var unread by remember(message.id) { mutableStateOf<Set<Long>?>(null) }
    var known by remember(message.id) { mutableStateOf(emptySet<Long>()) }
    var showAll by remember(message.id) { mutableStateOf(false) }
    // Loaded bodies and who each message went to, so a message opened again shows at once.
    val bodies = remember(message.id) { Bodies(mail, scope) }
    val people = remember(message.id) { mutableStateMapOf<Long, PeopleItem>() }
    val details = remember(message.id) { mutableStateMapOf<Long, Boolean>() }
    var replying by remember(message.id) { mutableStateOf<Replying?>(null) }
    // A reply on its way to the page: a second tap meanwhile starts nothing more.
    var answering by remember(message.id) { mutableStateOf(false) }
    // The meeting invitations among the messages, by message, and the messages looked at.
    val meetings = remember(message.id) { mutableStateMapOf<Long, InviteItem>() }
    val looked = remember(message.id) { mutableSetOf<Long>() }
    suspend fun readMeeting(m: MessageItem) {
        try {
            val invite = mail.invite(m)
            if (invite == null) meetings.remove(m.id) else meetings[m.id] = invite
        } catch (e: CoreException) {
            // The message reads the same without its card.
            Log.w("Ozero", "invitation not read", e)
        }
    }
    // An invitation is known once its body is saved; each is looked at once here.
    LaunchedEffect(message.id) {
        snapshotFlow { thread.filter { it.id in bodies.saved && it.id !in looked } }.collect { saved ->
            saved.forEach {
                looked += it.id
                readMeeting(it)
            }
        }
    }
    DisposableEffect(bodies) {
        onDispose {
            bodies.cancel()
            expectGone(bodies, "bodies of a closed conversation")
        }
    }
    LaunchedEffect(message.id) {
        try {
            // Read before it is marked read, so what was new opens.
            val first = mail.thread(message).ifEmpty { listOf(message) }
            val new = first.filter { !it.seen }.mapTo(mutableSetOf()) { it.id }
            // What opens comes with who it went to and its body, so the page shows whole at
            // once, not line by line. A body still to download holds it up only a moment.
            val opens = first.filter { it.id in new || (new.isEmpty() && it.id == message.id) }
            for (m in opens) people.load(mail, m)
            withTimeoutOrNull(BODY_WAIT_MS) {
                opens.map { m -> async { runCatching { bodies.of(m) } } }.awaitAll()
            }
            thread = first
            known = first.mapTo(mutableSetOf()) { it.id }
            unread = new
            // The rest downloads quietly, so a closed message shows how it starts and opens at once.
            launch {
                if (!bodies.fill(first)) return@launch
                try {
                    mail.thread(message).takeIf { it.isNotEmpty() }?.let { thread = it }
                } catch (e: CoreException) {
                    error = e.text()
                }
            }
        } catch (e: CoreException) {
            error = e.text()
        }
        try {
            if (message.threadUnread > 0u) mail.markRead(listOf(message))
        } catch (e: CoreException) {
            error = e.text()
        }
        // A reply that comes in meanwhile joins the conversation.
        mail.changes.collect {
            try {
                // Gone from the phone, say moved: what is on screen stays.
                mail.thread(message).takeIf { it.isNotEmpty() }?.let { thread = it }
            } catch (e: CoreException) {
                error = e.text()
            }
            // An answer, or one taken back, changes them.
            meetings.keys.mapNotNull { id -> thread.find { it.id == id } }.forEach { readMeeting(it) }
        }
    }
    // Unread then, new since, or the one from the list when nothing was unread.
    fun isOpen(m: MessageItem): Boolean = opened[m.id] ?: when (val new = unread) {
        null -> m.id == message.id
        else -> m.id in new || m.id !in known || (new.isEmpty() && m.id == message.id)
    }
    // The newest message not from the user.
    val answer = thread.lastOrNull { !it.sent } ?: message
    // The newest invitation, while it waits for an answer: the bottom row asks for it.
    val asking = thread.lastOrNull { it.id in meetings }?.takeIf { meetings[it.id]?.needsAnswer == true }

    // No asks why first.
    fun answerInvite(m: MessageItem, answer: InviteAnswer) {
        val invite = meetings[m.id] ?: return
        if (answer == InviteAnswer.NO) {
            sheet = ReaderSheet.Reason(m, invite)
        } else {
            sheet = null
            onAnswer(m, invite, answer, "")
        }
    }
    val many = thread.size > 1 || message.threadCount > 1u

    fun reply(to: MessageItem, all: Boolean) {
        sheet = null
        if (replying != null || answering) return
        answering = true
        scope.launch {
            try {
                // A failure of either lands in the catch, not in the screen's scope.
                replying = coroutineScope {
                    val draft = async { mail.reply(to, all) }
                    val others = (people[to.id] ?: mail.people(to).also { people[to.id] = it }).others
                    // A reply begun before goes on with its copies.
                    val begun = draft.await()
                    val copied = begun.cc.split(", ").filter { it.isNotBlank() } + if (all) others.map { it.email } else emptyList()
                    Replying(begun, to, others, copied = copied.distinct())
                }
            } catch (e: CoreException) {
                error = e.text()
            } finally {
                answering = false
            }
        }
    }
    val sending = stringResource(R.string.inbox_sending)
    val undo = stringResource(R.string.action_undo)
    val trashed = stringResource(R.string.inbox_trashed)
    val deleted = stringResource(R.string.inbox_deleted)
    // One message of the conversation; the last one left goes as the conversation does.
    fun deleteAlone(m: MessageItem) {
        val rest = thread.filter { it.id != m.id }
        if (rest.isEmpty()) return onDelete()
        scope.launch {
            try {
                val actions = mail.deleteAlone(m)
                thread = mail.thread(rest.first()).ifEmpty { rest }
                if (!snackbar.offerUndo(if (m.canTrash) trashed else deleted, undo)) return@launch
                if (!mail.undo(actions)) error = R.string.inbox_undo_late
                thread = mail.thread(rest.first()).ifEmpty { rest }
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }
    fun sent(id: Long, reply: Replying) {
        replying = null
        scope.launch {
            if (!snackbar.offerUndo(sending, undo)) return@launch
            try {
                mail.undoSend(id)?.let { replying = reply.copy(draft = it) }
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }

    val scroll = rememberScrollState()
    // The bottom row steps aside while the page is read down, and comes back on the way up.
    var barShown by remember(message.id) { mutableStateOf(true) }
    val hiding = remember {
        object : NestedScrollConnection {
            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (consumed.y < -HIDE_AFTER) barShown = false else if (consumed.y > HIDE_AFTER) barShown = true
                return Offset.Zero
            }
        }
    }
    // At the end, where what to do with the message is decided, it is back.
    LaunchedEffect(scroll) {
        snapshotFlow { scroll.value >= scroll.maxValue }.collect { if (it) barShown = true }
    }
    var barHeight by remember { mutableIntStateOf(0) }
    // Where each message starts on the page, to begin at the first unread one.
    val tops = remember(message.id) { mutableStateMapOf<Long, Float>() }
    var landed by remember(message.id) { mutableStateOf(false) }
    val firstUnread = unread?.let { new -> thread.firstOrNull { it.id in new } }
    LaunchedEffect(firstUnread?.id) {
        val id = firstUnread?.id ?: return@LaunchedEffect
        // Read here, not in the screen, so a message moving on the page does not redraw it all.
        val top = snapshotFlow { tops[id] }.filterNotNull().first()
        landed = true
        // The first message is under the title: it opens at the top.
        if (id != thread.first().id) scroll.scrollTo(top.toInt())
    }

    Box(Modifier.fillMaxSize()) {
        val edges = if (pane) WindowInsetsSides.Top else WindowInsetsSides.Top + WindowInsetsSides.Horizontal
        Middle(LETTER_MAX, Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(edges))) {
                BackArrow(onBack, Modifier.padding(top = 4.dp))
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    Column(Modifier.fillMaxSize().nestedScroll(hiding).verticalScroll(scroll)) {
                        val page = this
                        Title(message)
                        // Until the conversation is read only the title shows, so nothing jumps in above.
                        val ready = unread != null || error != null
                        // A long run of closed messages in the middle folds into one line.
                        val middle = 1 until thread.size - 2
                        val folded = !showAll && middle.count() >= 2 && middle.none { isOpen(thread[it]) }
                        if (ready) thread.forEachIndexed { i, m ->
                            if (i == 1) {
                                Reveal(middle.count().takeIf { folded }) { count -> More(count, onClick = { showAll = true }) }
                            }
                            key(m.id) {
                                val at = if (landed) Modifier else Modifier.onGloballyPositioned { tops[m.id] = it.positionInParent().y }
                                val letter = @Composable { modifier: Modifier ->
                                    Letter(
                                        mail,
                                        files,
                                        m,
                                        bodies,
                                        people,
                                        open = isOpen(m),
                                        unread = unread?.contains(m.id) == true,
                                        inThread = many,
                                        details = details[m.id] == true,
                                        onToggle = { opened[m.id] = !isOpen(m) }.takeIf { many },
                                        onDetails = { details[m.id] = details[m.id] != true },
                                        onPerson = { sheet = ReaderSheet.Person(it) },
                                        onReply = { reply(m, all = false) },
                                        onForward = { onForward(m) },
                                        onDelete = { deleteAlone(m) },
                                        invite = meetings[m.id],
                                        onChangeAnswer = { sheet = ReaderSheet.Change(m) },
                                        onAddToCalendar = { invite ->
                                            if (!addToCalendar(context, invite)) {
                                                scope.launch { snackbar.showSnackbar(context.getString(R.string.invite_no_calendar)) }
                                            }
                                        },
                                        modifier = modifier,
                                    )
                                }
                                if (i in middle) {
                                    page.AnimatedVisibility(!folded, at, enter = opening, exit = closing) { letter(Modifier) }
                                } else {
                                    letter(at)
                                }
                            }
                        }
                        Reveal(error) { Text(stringResource(it), color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(Gap.screen)) }
                        // The end of the page clears the row over it.
                        Spacer(Modifier.height(12.dp + with(LocalDensity.current) { barHeight.toDp() }))
                    }
                    if (!pane && replying == null) {
                        Hiding({ barShown }, Modifier.align(Alignment.BottomCenter).onSizeChanged { barHeight = it.height }) {
                            if (asking != null) {
                                InviteActions(onAnswer = { answerInvite(asking, it) }, onMore = { sheet = ReaderSheet.More })
                            } else {
                                Actions(
                                    onReply = { reply(answer, all = false) },
                                    onAnswers = { sheet = ReaderSheet.Answers },
                                    onDelete = onDelete,
                                    forever = !message.canTrash,
                                    onSnooze = onSnooze,
                                    onMore = { sheet = ReaderSheet.More },
                                    onDone = onDone,
                                )
                            }
                        }
                    }
                }
                val writing = replying
                if (writing != null) {
                    QuickReply(
                        mail,
                        writing,
                        onChange = { replying = it },
                        onClose = { kept ->
                            replying = null
                            if (kept) onKept()
                        },
                        onExpand = { draft ->
                            replying = null
                            onWrite(Composing(draft, writing.original))
                        },
                        onSent = { id -> sent(id, writing) },
                    )
                } else if (pane && asking != null) {
                    InviteActions(onAnswer = { answerInvite(asking, it) }, onMore = { sheet = ReaderSheet.More })
                } else if (pane) {
                    PaneActions(
                        onReply = { reply(answer, all = false) },
                        onReplyAll = { reply(answer, all = true) },
                        onForward = { onForward(thread.lastOrNull() ?: message) },
                        onDelete = onDelete,
                        forever = !message.canTrash,
                        onSnooze = onSnooze,
                        onMore = { sheet = ReaderSheet.More },
                        onDone = onDone,
                    )
                }
            }
        }
        // Like every menu, the rarer actions open in a sheet, low in reach of the thumb.
        val colors = MaterialTheme.colorScheme
        val shown = rememberLast(sheet)
        Sheet(visible = sheet != null, onClose = { sheet = null }) {
            @Composable
            fun replyAllAndForward() {
                SheetAction(R.drawable.ic_reply_all, stringResource(R.string.message_reply_all), colors.onSurface) { reply(answer, all = true) }
                SheetAction(R.drawable.ic_forward, stringResource(R.string.message_forward), colors.onSurface) {
                    sheet = null
                    // The newest message is forwarded, the user's own too.
                    onForward(thread.lastOrNull() ?: message)
                }
            }
            when (shown) {
                ReaderSheet.Answers -> {
                    SheetAction(R.drawable.ic_reply, stringResource(R.string.message_reply), colors.onSurface) { reply(answer, all = false) }
                    replyAllAndForward()
                }
                ReaderSheet.More -> {
                    // The answers took the row: the rest of it is here.
                    if (asking != null) {
                        SheetAction(R.drawable.ic_reply, stringResource(R.string.message_reply), colors.onSurface) { reply(answer, all = false) }
                        if (pane) replyAllAndForward()
                        SheetAction(R.drawable.ic_delete, stringResource(if (message.canTrash) R.string.action_delete else R.string.action_delete_forever), colors.onSurface) {
                            sheet = null
                            onDelete()
                        }
                        onSnooze?.let { snooze ->
                            SheetAction(R.drawable.ic_snooze, stringResource(R.string.action_snooze), colors.onSurface) {
                                sheet = null
                                snooze()
                            }
                        }
                        onDone?.let { done ->
                            SheetAction(R.drawable.ic_done, stringResource(R.string.action_done), colors.onSurface) {
                                sheet = null
                                done()
                            }
                        }
                    }
                    // Beside the list they are in the row already.
                    if (!pane) replyAllAndForward()
                    // As the conversation has it now, flagged here or elsewhere.
                    val flagged = (thread.firstOrNull { it.id == message.id } ?: message).flagged
                    SheetAction(R.drawable.ic_flag, stringResource(if (flagged) R.string.action_unflag else R.string.action_flag), colors.onSurface) {
                        sheet = null
                        onFlag(!flagged)
                    }
                    SheetAction(R.drawable.ic_unread, stringResource(R.string.action_mark_unread), colors.onSurface) {
                        sheet = null
                        onUnread()
                    }
                    onMoveToInbox?.let { move ->
                        SheetAction(R.drawable.ic_inbox, stringResource(R.string.action_move_to_inbox), colors.onSurface) {
                            sheet = null
                            move()
                        }
                    }
                    onMove?.let {
                        SheetAction(R.drawable.ic_folder, stringResource(R.string.action_move_to_folder), colors.onSurface) {
                            scope.launch {
                                val targets = try {
                                    mail.moveTargets(message)
                                } catch (e: CoreException) {
                                    sheet = null
                                    snackbar.showSnackbar(context.getString(e.text()))
                                    return@launch
                                }
                                sheet = ReaderSheet.Move(targets)
                            }
                        }
                    }
                    if (message.canUnsubscribe && !left) {
                        SheetAction(R.drawable.ic_unsubscribe, stringResource(R.string.action_unsubscribe), colors.onSurface) {
                            sheet = null
                            leaving = true
                        }
                    }
                    onSpam?.let { spam ->
                        SheetAction(R.drawable.ic_spam, stringResource(R.string.action_spam), colors.onSurface) {
                            sheet = null
                            spam()
                        }
                    }
                }
                is ReaderSheet.Move -> MoveTargets(shown.targets) { folder, title ->
                    sheet = null
                    onMove?.invoke(folder, title)
                }
                is ReaderSheet.Change -> AnswerSheet(meetings[shown.message.id]?.answer) { answerInvite(shown.message, it) }
                is ReaderSheet.Reason -> ReasonSheet(shown.invite.organizerName ?: shown.invite.organizerEmail ?: shown.message.sender) { reason ->
                    sheet = null
                    onAnswer(shown.message, shown.invite, InviteAnswer.NO, reason)
                }
                is ReaderSheet.Person -> PersonSheet(
                    shown.person,
                    onWrite = {
                        sheet = null
                        onWriteTo(shown.person.email)
                    },
                    onCopied = { sheet = null },
                )
                null -> {}
            }
        }
        if (leaving) {
            Unsubscribe(
                sender = message.sender,
                onDismiss = { leaving = false },
                onConfirm = {
                    leaving = false
                    scope.launch {
                        try {
                            when (val done = mail.unsubscribe(message)) {
                                Unsubscribed.Done -> snackbar.showSnackbar(context.getString(R.string.unsubscribe_done))
                                Unsubscribed.Sent -> snackbar.showSnackbar(context.getString(R.string.unsubscribe_sent))
                                is Unsubscribed.Page -> openLink(context, Uri.parse(done.url))
                            }
                            left = true
                        } catch (e: CoreException) {
                            snackbar.showSnackbar(context.getString(e.text()))
                        }
                    }
                },
            )
        }
        AttachmentActions(files)
        FileViewer(files)
        Toast(
            snackbar,
            onGo = onGo,
            modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = if (pane) TOAST_ABOVE_ROW else TOAST_ABOVE_BAR),
        )
    }
}

/** Asks before leaving [sender]'s list, which cannot be taken back. */
@Composable
private fun Unsubscribe(sender: String, onDismiss: () -> Unit, onConfirm: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(onClick = onConfirm) { Text(stringResource(R.string.action_unsubscribe), color = colors.error) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.action_cancel)) }
        },
        title = { Text(stringResource(R.string.unsubscribe_title, sender)) },
        text = { Text(stringResource(R.string.unsubscribe_text)) },
        containerColor = colors.background,
        shape = RoundedCornerShape(Radius.dialog),
    )
}

/** What a sheet of the message screen holds. */
private sealed interface ReaderSheet {
    /** Reply, Reply all and Forward, from a long press on Reply. */
    data object Answers : ReaderSheet

    data object More : ReaderSheet

    /** The folders the message can move to, picked from More. */
    data class Move(val targets: List<FolderItem>) : ReaderSheet

    /** What to do with someone the message is from or went to. */
    data class Person(val person: PersonItem) : ReaderSheet

    /** Another answer to an invitation answered already. */
    data class Change(val message: MessageItem) : ReaderSheet

    /** Why not, before No goes. */
    data class Reason(val message: MessageItem, val invite: InviteItem) : ReaderSheet
}

/** A reply being written low on the page: its draft, the message it answers, and who else may get a copy. */
data class Replying(val draft: DraftItem, val original: MessageItem, val others: List<PersonItem>, val copied: List<String>)

/** The bottom row, stepping aside while [shown] is false; read here, so only the row redraws. */
@Composable
private fun Hiding(shown: () -> Boolean, modifier: Modifier, content: @Composable () -> Unit) {
    AnimatedVisibility(
        shown(),
        modifier,
        enter = slideInVertically(Motion.calm(Motion.SMALL)) { it } + fadeIn(Motion.calm(Motion.SMALL)),
        exit = slideOutVertically(Motion.calm(Motion.SMALL)) { it } + fadeOut(Motion.calm(Motion.SMALL)),
    ) { content() }
}

/** How far a scroll moves before the bottom row steps aside or comes back, in pixels. */
private const val HIDE_AFTER = 2f

/** How many files a message has: as the list knew, or as its body says once loaded. */
private fun fileCount(message: MessageItem, bodies: Bodies): Int =
    maxOf(message.attachments.size, bodies.saved[message.id]?.attachments?.size ?: 0)

/** A message opens slowly from its line down, and closes a little quicker. */
private val opening = expandVertically(Motion.calm(Motion.SCREEN), expandFrom = Alignment.Top) + fadeIn(Motion.calm(Motion.SCREEN))
private val closing = shrinkVertically(Motion.calm(Motion.NOTE), shrinkTowards = Alignment.Top) + fadeOut(Motion.calm(Motion.SMALL))

/**
 * One message: its line, and under it the message when open. In a conversation [onToggle]
 * opens or closes it, and Reply and Forward under it answer this one.
 */
@Composable
private fun Letter(
    mail: Mail,
    files: Files,
    message: MessageItem,
    bodies: Bodies,
    people: MutableMap<Long, PeopleItem>,
    open: Boolean,
    unread: Boolean,
    inThread: Boolean,
    details: Boolean,
    onToggle: (() -> Unit)?,
    onDetails: () -> Unit,
    onPerson: (PersonItem) -> Unit,
    onReply: () -> Unit,
    onForward: () -> Unit,
    onDelete: () -> Unit,
    invite: InviteItem?,
    onChangeAnswer: () -> Unit,
    onAddToCalendar: (InviteItem) -> Unit,
    modifier: Modifier,
) {
    val filesAt = remember { BringIntoViewRequester() }
    LaunchedEffect(message.id, open) {
        if (open) people.load(mail, message)
    }
    Column(modifier.fillMaxWidth()) {
        HorizontalDivider(Modifier.padding(horizontal = Gap.screen), color = MaterialTheme.colorScheme.outlineVariant)
        if (open) {
            From(message, people[message.id], unread, fileCount(message, bodies), filesAt, details, onToggle, onDetails, onPerson)
        } else {
            Closed(message, unread, onToggle)
        }
        AnimatedVisibility(open, enter = opening, exit = closing) {
            Column {
                Reveal(invite) {
                    InviteCard(it, onChange = onChangeAnswer.takeIf { _ -> it.canAnswer }, onAddToCalendar = { onAddToCalendar(it) })
                }
                LetterBody(mail, files, filesAt, message, bodies)
                if (inThread) {
                    Row(Modifier.padding(start = Gap.screen, end = Gap.screen, bottom = 14.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (!message.sent) Chip(R.drawable.ic_reply, stringResource(R.string.message_reply), onReply)
                        Chip(R.drawable.ic_forward, stringResource(R.string.message_forward), onForward)
                        Chip(R.drawable.ic_delete, stringResource(R.string.action_delete), onDelete, MaterialTheme.colorScheme.error)
                    }
                }
            }
        }
    }
}

/** A small rounded button under a message. */
@Composable
private fun Chip(@DrawableRes icon: Int, label: String, onClick: () -> Unit, tint: Color = MaterialTheme.colorScheme.primary) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .heightIn(min = 36.dp)
            .clip(CircleShape)
            .background(colors.surfaceVariant)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Glyph(icon, null, tint, size = 17.dp)
        Text(label, color = tint, style = Type.Caption, fontWeight = Weight.SemiBold)
    }
}

/** The line of closed messages folded away; a tap shows them. */
@Composable
private fun More(count: Int, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxWidth()) {
        HorizontalDivider(Modifier.padding(horizontal = Gap.screen), color = colors.outlineVariant)
        Text(
            stringResource(R.string.message_more, count),
            color = colors.primary,
            style = Type.Text,
            fontWeight = Weight.SemiBold,
            modifier = Modifier
                .fillMaxWidth()
                .clickable(role = Role.Button, onClick = onClick)
                .padding(horizontal = Gap.screen, vertical = 13.dp),
        )
    }
}

/** Reads who [message] went to, once. */
private suspend fun MutableMap<Long, PeopleItem>.load(mail: Mail, message: MessageItem) {
    if (message.id in this) return
    try {
        this[message.id] = mail.people(message)
    } catch (e: CoreException) {
        // The line under the name stays empty; the message reads the same.
        Log.w("Ozero", "recipients not read", e)
    }
}

/** A conversation's bodies, each downloaded once however many ask for it, kept while it is open. */
private class Bodies(private val mail: Mail, scope: CoroutineScope) {
    // A failed or abandoned download leaves the screen and the others be.
    private val scope = scope + SupervisorJob(scope.coroutineContext[Job])
    val saved = mutableStateMapOf<Long, MessageBody>()
    private val loading = mutableMapOf<Long, Deferred<MessageBody>>()

    /** Leaves what is still downloading: the conversation is closed. */
    fun cancel() = scope.cancel()

    suspend fun of(message: MessageItem): MessageBody = saved[message.id] ?: loading.getOrPut(message.id) {
        scope.async(start = CoroutineStart.LAZY) {
            try {
                mail.body(message).also { saved[message.id] = it }
            } finally {
                loading.remove(message.id)
            }
        }
    }.await()

    /** Downloads the missing ones one by one, stopping at a failure, as offline; true if it got any. */
    suspend fun fill(messages: List<MessageItem>): Boolean {
        var got = false
        for (m in messages.filter { it.id !in saved }) {
            try {
                of(m)
                got = true
            } catch (e: CoreException) {
                Log.w("Ozero", "bodies not downloaded", e)
                break
            }
        }
        return got
    }
}

/**
 * A message's page, or its text with the quote folded; it loads when shown, once into
 * [bodies]. It is as tall as it is, and its files follow the text; [filesAt] brings them
 * into sight.
 */
@Composable
private fun LetterBody(
    mail: Mail,
    files: Files,
    filesAt: BringIntoViewRequester,
    message: MessageItem,
    bodies: Bodies,
) {
    // A page whose images load on their own waits for them again, as on the first time.
    var body by remember(message.id) { mutableStateOf(bodies.saved[message.id]?.takeUnless { it.remoteImages > 0u && it.showsImages }) }
    var error by remember(message.id) { mutableStateOf<Int?>(null) }
    // The page again with its remote images.
    var withImages by remember(message.id) { mutableStateOf<ImagesPage?>(null) }
    var imagesError by remember(message.id) { mutableStateOf<Int?>(null) }
    val scope = rememberCoroutineScope()
    suspend fun loadImages() {
        try {
            mail.images(message)?.let { withImages = it }
        } catch (e: CoreException) {
            imagesError = e.text()
        }
    }
    LaunchedEffect(message.id) {
        try {
            val loaded = bodies.of(message)
            if (loaded.remoteImages > 0u && loaded.showsImages) {
                // Wait a moment for the images, so the page does not load twice and jump.
                coroutineScope {
                    val images = async { loadImages() }
                    withTimeoutOrNull(IMAGES_WAIT_MS) { images.await() }
                    body = loaded
                }
            } else {
                body = loaded
            }
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    val loaded = body
    val full = withImages
    val imagesFailed = imagesError
    val imagesNote = when {
        loaded == null || full != null -> null
        imagesFailed != null -> imagesFailed
        loaded.remoteImages > 0u && !loaded.showsImages -> R.string.message_images_hidden
        else -> null
    }
    val failed = error
    val html = loaded?.html
    val styled = loaded?.styled == true
    val shown = when {
        full != null -> Body.Page(full.html, full.images, withImages = true, styled)
        html != null -> Body.Page(html, emptyList(), withImages = false, styled)
        loaded != null -> Body.Plain(loaded.text, loaded.quote)
        failed != null -> Body.Failed(failed)
        else -> Body.Loading
    }
    val attachments = loaded?.attachments.orEmpty()
    // A page grows in steps as it lays out; only the short text and notes animate.
    Column(Modifier.fillMaxWidth().then(if (shown is Body.Page) Modifier else Modifier.animateContentSize(Motion.calm(Motion.NOTE)))) {
        Reveal(imagesNote) { note ->
            HiddenImages(note, onShow = onShow@{
                // The note is closing: the images are here already.
                if (withImages != null) return@onShow
                imagesError = null
                scope.launch {
                    try {
                        mail.allowImages(message)
                        // The sender's other messages load theirs on their own now too.
                        val allowed = bodies.saved[message.id]?.copy(showsImages = true)
                        bodies.saved.clear()
                        allowed?.let { bodies.saved[message.id] = it }
                        loadImages()
                    } catch (e: CoreException) {
                        imagesError = e.text()
                    }
                }
            })
        }
        // The page fades in over the spinner instead of popping in; the old one fades out whole.
        AnimatedContent(
            shown,
            Modifier.fillMaxWidth(),
            transitionSpec = { (fadeIn(Motion.calm(Motion.SMALL)) togetherWith fadeOut(Motion.calm(Motion.SMALL))).using(null) },
            contentKey = { it::class },
            label = "body",
        ) { kind ->
                        when (kind) {
                // A new web view per page: it loads its page only once.
                // A newsletter keeps the light page it was made for, as a card on the screen.
                is Body.Page -> key(kind.withImages) {
                    val page = if (kind.styled) {
                        Modifier.fillMaxWidth().padding(start = 10.dp, end = 10.dp, top = 4.dp, bottom = 10.dp).clip(RoundedCornerShape(Radius.tile))
                    } else {
                        Modifier.fillMaxWidth().padding(top = 4.dp)
                    }
                    // A long page takes a while to draw: a cover with the spinner stays over it until it
                    // shows. The page itself draws all along, as a hidden web view might not.
                    var drawn by remember { mutableStateOf(false) }
                    val cover by animateFloatAsState(if (drawn) 0f else 1f, Motion.calm(Motion.SMALL), label = "cover")
                    Box(Modifier.fillMaxWidth()) {
                        HtmlView(kind.html, kind.images, page, onShown = { drawn = true })
                        if (cover > 0f) {
                            Box(Modifier.graphicsLayer { alpha = cover }.background(MaterialTheme.colorScheme.background)) { BodySpinner() }
                        }
                    }
                }
                is Body.Plain -> PlainText(kind.text, kind.quote, Modifier.fillMaxWidth())
                is Body.Failed -> Text(stringResource(kind.text), color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.fillMaxWidth().padding(Gap.screen))
                Body.Loading -> BodySpinner()
            }
        }
        if (attachments.isNotEmpty()) AttachmentList(files, message, attachments, Modifier.bringIntoViewRequester(filesAt))
    }
}

/** A body still loading: the spinner in the middle of a short space. */
@Composable
private fun BodySpinner() {
    Box(Modifier.fillMaxWidth().height(96.dp), contentAlignment = Alignment.Center) {
        SlowSpinner()
    }
}

/** The text, and the quote under it behind a mark that opens it. */
@Composable
private fun PlainText(text: String, quote: String, modifier: Modifier) {
    var showQuote by remember { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    SelectionContainer(modifier.padding(start = Gap.screen, end = Gap.screen, top = 6.dp, bottom = 18.dp)) {
        Column(Modifier.widthIn(max = 620.dp)) {
            if (text.isNotEmpty()) Text(text, style = readingStyle())
            if (quote.isNotEmpty()) {
                val label = stringResource(R.string.message_quote)
                Text(
                    "•••",
                    color = colors.onSurfaceVariant,
                    style = Type.Caption,
                    fontWeight = Weight.Bold,
                    letterSpacing = 1.sp,
                    modifier = Modifier
                        .padding(top = 10.dp)
                        .clip(RoundedCornerShape(9.dp))
                        .background(colors.surfaceVariant)
                        .clickable(role = Role.Button) { showQuote = !showQuote }
                        .semantics { contentDescription = label }
                        .padding(horizontal = 10.dp, vertical = 1.dp),
                )
                Reveal(quote.takeIf { showQuote }) {
                    Text(it, color = colors.onSurfaceVariant, style = readingStyle(Type.Quote), modifier = Modifier.padding(top = 10.dp))
                }
            }
        }
    }
}

/**
 * The actions under the thumb in one row, each in an equal place; Done, the one used most,
 * takes the last. A long press on Reply offers Reply all and Forward. Snooze and Done are
 * null in a folder where they do not apply, such as Done in the archive; Delete is
 * [forever] in the trash.
 */
@Composable
private fun Actions(
    onReply: () -> Unit,
    onAnswers: () -> Unit,
    onDelete: () -> Unit,
    forever: Boolean,
    onSnooze: (() -> Unit)?,
    onMore: () -> Unit,
    onDone: (() -> Unit)?,
) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .background(colors.background)
            .navigationBarsPadding()
            .padding(start = Gap.bar, end = Gap.bar, top = Gap.barTop, bottom = Gap.barBottom),
        horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
    ) {
        BarAction(R.drawable.ic_reply, stringResource(R.string.message_reply), colors.onSurface, onLongClick = onAnswers, onClick = onReply)
        val delete = if (forever) R.string.action_delete_forever else R.string.action_delete
        BarAction(R.drawable.ic_delete, stringResource(delete), colors.onSurface, onClick = onDelete)
        onSnooze?.let { BarAction(R.drawable.ic_snooze, stringResource(R.string.action_snooze), colors.onSurface, onClick = it) }
        BarAction(R.drawable.ic_more, stringResource(R.string.action_more), colors.onSurface, onClick = onMore)
        onDone?.let { BarAction(R.drawable.ic_done, stringResource(R.string.action_done), colors.onAccent, colors.accent, onClick = it) }
    }
}

/** One of the big buttons along the bottom of a screen: an icon over its name. */
@Composable
fun RowScope.BarAction(
    @DrawableRes icon: Int,
    label: String,
    tint: Color,
    background: Color = Color.Transparent,
    onLongClick: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    Column(
        Modifier
            .weight(1f)
            .height(if (LocalRoom.current.short) 44.dp else 52.dp)
            .clip(RoundedCornerShape(Radius.medium))
            .background(background)
            .combinedClickable(role = Role.Button, onLongClick = onLongClick, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(3.dp, Alignment.CenterVertically),
    ) {
        Glyph(icon, null, tint, size = 21.dp)
        Text(label, color = tint, style = Type.Label, maxLines = 1)
    }
}

/** A note that the remote images are held back, with a way to see them from now on. */
@Composable
private fun HiddenImages(note: Int, onShow: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = Gap.card, end = Gap.card, top = 4.dp, bottom = 8.dp)
            .background(colors.surfaceVariant, RoundedCornerShape(Radius.medium))
            .padding(start = Gap.inCard, end = 8.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            stringResource(note),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            modifier = Modifier.weight(1f),
        )
        Text(
            stringResource(R.string.action_show),
            color = colors.primary,
            style = Type.Caption,
            fontWeight = Weight.SemiBold,
            modifier = Modifier
                .clip(CircleShape)
                .background(colors.background)
                .clickable(role = Role.Button, onClick = onShow)
                .padding(horizontal = 12.dp, vertical = 7.dp),
        )
    }
}

/** The name the user is shown by in a message: "You" when they wrote it. */
@Composable
private fun MessageItem.from(): String = if (sent) stringResource(R.string.message_you) else sender

/**
 * Who wrote an open message, to whom, and when, in two lines: "Anna Lee" and "To you, Oleg
 * and Maria". A tap on the name offers what to do with the person ([onPerson]); a tap on
 * the second line shows every address and the full date ([details]). With [onToggle] a tap
 * elsewhere closes the message. With [files], a paperclip by the date brings them in sight.
 */
@Composable
private fun From(
    message: MessageItem,
    people: PeopleItem?,
    unread: Boolean,
    files: Int,
    filesAt: BringIntoViewRequester,
    details: Boolean,
    onToggle: (() -> Unit)?,
    onDetails: () -> Unit,
    onPerson: (PersonItem) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    val author = PersonItem(name = message.sender, first = message.sender, email = message.fromEmail, me = message.sent)
    Column(Modifier.fillMaxWidth().then(if (onToggle == null) Modifier else Modifier.clickable(onClick = onToggle))) {
        Row(Modifier.fillMaxWidth().padding(start = Gap.screen, end = Gap.screen, top = 12.dp, bottom = 12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Column(Modifier.weight(1f)) {
                Text(
                    message.from(),
                    style = Type.Row,
                    fontWeight = if (unread) Weight.Bold else Weight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = if (message.sent) Modifier else Modifier.clickable(role = Role.Button) { onPerson(author) },
                )
                val line = people?.let { toLine(it) }
                if (!line.isNullOrEmpty()) {
                    Row(
                        Modifier.clickable(role = Role.Button, onClick = onDetails),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Text(line, color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                        val turn by animateFloatAsState(if (details) 180f else 0f, Motion.calm(Motion.SMALL), label = "details")
                        // Big enough to say the line opens.
                        Glyph(R.drawable.ic_chevron_down, null, colors.onSurfaceVariant, size = 16.dp, modifier = Modifier.rotate(turn))
                    }
                }
            }
            Column(Modifier.padding(top = 2.dp), horizontalAlignment = Alignment.End) {
                Text(
                    if (message.sending) stringResource(R.string.inbox_sending) else shortDate(context, message.date),
                    color = colors.onSurfaceVariant,
                    style = Type.Caption,
                )
                if (files > 0) {
                    val scope = rememberCoroutineScope()
                    AttachmentMark(files, onClick = { scope.launch { filesAt.bringIntoView() } })
                }
            }
        }
        Reveal(people?.takeIf { details }) { Details(message, author, it, onPerson) }
    }
}

/** "To you, Oleg and Maria": who a message went to, copies too, by first name. */
@Composable
private fun toLine(people: PeopleItem): String {
    val you = stringResource(R.string.message_you_to)
    val names = people.shown.map { if (it.me) you else it.first }
    return if (names.isEmpty()) "" else stringResource(R.string.message_to_line, joinNames(names))
}

/** Every address of a message and its full date, each person a tap away. */
@Composable
private fun Details(message: MessageItem, author: PersonItem, people: PeopleItem, onPerson: (PersonItem) -> Unit) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    Column(
        Modifier
            .fillMaxWidth()
            .padding(start = Gap.card, end = Gap.card, bottom = 10.dp)
            .background(colors.surfaceVariant, RoundedCornerShape(Radius.medium))
            .padding(horizontal = Gap.inCard, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        @Composable
        fun Field(label: Int, persons: List<PersonItem>) {
            if (persons.isEmpty()) return
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(stringResource(label), color = colors.onSurfaceVariant, style = Type.Caption, modifier = Modifier.widthIn(min = 52.dp))
                FlowRow(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    persons.forEach { person ->
                        Text(
                            if (person.name == person.email) person.email else "${person.name} ${person.email}",
                            style = Type.Caption,
                            modifier = Modifier.clickable(role = Role.Button) { onPerson(person) },
                        )
                    }
                }
            }
        }
        Field(R.string.message_from, listOf(author))
        Field(R.string.compose_to, people.to)
        Field(R.string.compose_cc, people.cc)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(stringResource(R.string.message_date), color = colors.onSurfaceVariant, style = Type.Caption, modifier = Modifier.widthIn(min = 52.dp))
            Text(longDate(context, message.date), style = Type.Caption)
        }
    }
}

/** A closed message in a conversation: who, the start of its text, and when; [onToggle] opens it. */
@Composable
private fun Closed(message: MessageItem, unread: Boolean, onToggle: (() -> Unit)?) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (onToggle == null) Modifier else Modifier.clickable(onClick = onToggle))
            .padding(horizontal = Gap.screen, vertical = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(message.from(), style = Type.Row, fontWeight = if (unread) Weight.Bold else Weight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(message.snippet, color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Text(shortDate(context, message.date), color = colors.onSurfaceVariant, style = Type.Caption, modifier = Modifier.padding(top = 2.dp))
    }
}

/** The subject and the account it came to. */
@Composable
private fun Title(message: MessageItem) {
    val colors = MaterialTheme.colorScheme
    SelectionContainer(Modifier.padding(top = 6.dp)) {
        Column {
            Text(
                message.subjectText(),
                style = Type.Title,
                modifier = Modifier.padding(start = Gap.screen, end = Gap.screen, bottom = 10.dp),
            )
            message.account?.let { account ->
                Row(
                    Modifier
                        .padding(start = Gap.screen, end = Gap.screen, bottom = 8.dp)
                        .background(colors.surfaceVariant, CircleShape)
                        .padding(horizontal = 11.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    AccountDot(account.tint, size = 8.dp)
                    Text(account.name, color = colors.onSurfaceVariant, style = Type.Caption, fontWeight = Weight.Medium)
                }
            }
        }
    }
}

/** What to do with someone a message is from or went to. */
@Composable
private fun PersonSheet(person: PersonItem, onWrite: () -> Unit, onCopied: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    SheetTitle(person.name, Modifier.padding(start = Gap.sheet, end = Gap.sheet))
    if (person.name != person.email) {
        Text(person.email, color = colors.onSurfaceVariant, style = Type.Text, modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, bottom = 10.dp))
    }
    SheetAction(R.drawable.ic_compose, stringResource(R.string.message_write_to), colors.onSurface, onWrite)
    SheetAction(R.drawable.ic_share, stringResource(R.string.message_copy_address), colors.onSurface) {
        scope.launch {
            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText(person.email, person.email)))
            onCopied()
        }
    }
}

/**
 * A reply written low on the page, over the keyboard: to the sender, with the others it
 * went to a tap away as copies. It is kept as a draft as it is written; [onExpand] takes it
 * to the whole screen, and [onClose] says whether it was kept.
 */
@Composable
private fun QuickReply(
    mail: Mail,
    replying: Replying,
    onChange: (Replying) -> Unit,
    onClose: (kept: Boolean) -> Unit,
    onExpand: (DraftItem) -> Unit,
    onSent: (Long) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val scope = rememberCoroutineScope()
    val start = replying.draft
    val keeper = remember(start) { mail.keeper(start.id) }
    // Written as plain text, its marks kept as they are.
    var text by remember(start) { mutableStateOf(richShown(start.text)) }
    var busy by remember { mutableStateOf(false) }
    var error by remember(start) { mutableStateOf<Int?>(null) }
    fun current() = start.copy(text = richEscape(text), cc = replying.copied.joinToString(", "))
    SaveOnPause(keeper, ::current)
    // Leaving the page, or the app put away, keeps what is written; after a send this does nothing.
    fun keep() = current().let { if (!it.isBlank()) keeper.save(it) }
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { keep() }
    DisposableEffect(keeper) { onDispose { keep() } }
    fun close() {
        val draft = current()
        if (draft.isBlank()) {
            keeper.discard()
            onClose(false)
        } else {
            keeper.save(draft)
            onClose(true)
        }
    }
    BackHandler(onBack = ::close)
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Column(
        Modifier
            .fillMaxWidth()
            .background(colors.background)
            .hairline(colors.outlineVariant, top = true)
            .navigationBarsPadding()
            .imePadding()
            .padding(top = 8.dp),
    ) {
        FlowRow(
            Modifier.fillMaxWidth().padding(horizontal = Gap.screen, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            val to = if (start.to.equals(replying.original.fromEmail, ignoreCase = true)) replying.original.sender else start.to
            Person(to, onClick = null)
            replying.others.filter { it.email in replying.copied }.forEach { person ->
                Person(stringResource(R.string.message_copy_person, person.name), onClick = {
                    onChange(replying.copy(copied = replying.copied - person.email))
                })
            }
            val left = replying.others.filter { it.email !in replying.copied }
            if (left.isNotEmpty()) {
                Person(stringResource(R.string.message_add_people, joinNames(left.map { it.first })), primary = true, onClick = {
                    onChange(replying.copy(copied = replying.copied + left.map { it.email }))
                })
            }
        }
        Plain(
            text,
            { text = it },
            R.string.message_reply_prompt,
            Type.Writing,
            KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            singleLine = false,
            modifier = Modifier
                .padding(horizontal = Gap.screen, vertical = 6.dp)
                .heightIn(min = 54.dp, max = 162.dp)
                .focusRequester(focus),
        )
        Reveal(error) { Text(stringResource(it), color = colors.error, style = Type.Caption, modifier = Modifier.padding(horizontal = Gap.screen)) }
        Row(
            Modifier.fillMaxWidth().padding(horizontal = Gap.bar, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
        ) {
            IconTile(R.drawable.ic_close, stringResource(R.string.action_close), ::close, shape = CircleShape, size = 44.dp)
            IconTile(
                R.drawable.ic_expand,
                stringResource(R.string.message_full_screen),
                {
                    scope.launch {
                        try {
                            val draft = current()
                            onExpand(draft.copy(id = keeper.handOff(draft)))
                        } catch (e: CoreException) {
                            error = e.text()
                        }
                    }
                },
                shape = CircleShape,
                size = 44.dp,
                enabled = !busy,
            )
            Spacer(Modifier.weight(1f))
            IconTile(
                R.drawable.ic_send,
                stringResource(R.string.compose_send),
                {
                    if (!busy) {
                        busy = true
                        error = null
                        scope.launch {
                            try {
                                onSent(keeper.send(current()))
                            } catch (e: CoreException) {
                                error = e.text()
                            } finally {
                                busy = false
                            }
                        }
                    }
                },
                Modifier.padding(end = 4.dp),
                background = colors.accent,
                tint = colors.onAccent,
                shape = CircleShape,
                size = 44.dp,
                enabled = !busy,
            )
        }
    }
}

/** Someone a reply goes to, as a chip; [onClick] takes them off or adds them. */
@Composable
private fun Person(text: String, onClick: (() -> Unit)?, primary: Boolean = false) {
    val colors = MaterialTheme.colorScheme
    Text(
        text,
        color = if (primary) colors.primary else colors.onSurface,
        style = Type.Caption,
        fontWeight = Weight.Medium,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier
            .clip(CircleShape)
            .background(colors.surfaceVariant)
            .then(if (onClick == null) Modifier else Modifier.clickable(role = Role.Button, onClick = onClick))
            .padding(horizontal = 10.dp, vertical = 5.dp),
    )
}

/** What a message's body area shows. */
private sealed interface Body {
    data object Loading : Body
    data class Page(val html: String, val images: List<RemoteImage?>, val withImages: Boolean, val styled: Boolean) : Body
    data class Plain(val text: String, val quote: String) : Body
    data class Failed(val text: Int) : Body
}

/** Where a toast sits: over the row of actions. */
private val TOAST_ABOVE_BAR = 80.dp

/** Over the one row of them beside the list. */
private val TOAST_ABOVE_ROW = 72.dp

/** Beside the list, where the height is short: the answers and the actions in one row. */
@Composable
private fun PaneActions(
    onReply: () -> Unit,
    onReplyAll: () -> Unit,
    onForward: () -> Unit,
    onDelete: () -> Unit,
    forever: Boolean,
    onSnooze: (() -> Unit)?,
    onMore: () -> Unit,
    onDone: (() -> Unit)?,
) {
    val colors = MaterialTheme.colorScheme
    val button = paneButton()
    Row(
        Modifier
            .fillMaxWidth()
            .background(colors.bar)
            .hairline(colors.outlineVariant, top = true)
            .navigationBarsPadding()
            .padding(horizontal = Gap.bar, vertical = if (LocalRoom.current.short) 6.dp else Gap.barTop),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
    ) {
        Pill(stringResource(R.string.message_reply_prompt), onClick = onReply, modifier = Modifier.weight(1f), height = button)
        IconTile(R.drawable.ic_reply_all, stringResource(R.string.message_reply_all), onReplyAll, shape = CircleShape, size = button)
        IconTile(R.drawable.ic_forward, stringResource(R.string.message_forward), onForward, shape = CircleShape, size = button)
        IconTile(
            R.drawable.ic_delete,
            stringResource(if (forever) R.string.action_delete_forever else R.string.action_delete),
            onDelete,
            shape = CircleShape,
            size = button,
        )
        onSnooze?.let { IconTile(R.drawable.ic_snooze, stringResource(R.string.action_snooze), it, shape = CircleShape, size = button) }
        IconTile(R.drawable.ic_more, stringResource(R.string.action_more), onMore, shape = CircleShape, size = button)
        onDone?.let {
            IconTile(R.drawable.ic_done, stringResource(R.string.action_done), it, background = colors.accent, tint = colors.onAccent, shape = CircleShape, size = button)
        }
    }
}

/** The round buttons along the bottom of a message: smaller on its side, where the height is short. */
@Composable
private fun paneButton(): Dp = if (LocalRoom.current.short) 40.dp else 46.dp

/** How long a page waits for its images before it shows without them. */
private const val IMAGES_WAIT_MS = 2_000L

/** How long a conversation waits for the bodies that open before it shows. */
private const val BODY_WAIT_MS = 250L

package com.ozeromail.app

import android.util.Log
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftFileItem
import uniffi.ozero_api.DraftItem
import uniffi.ozero_api.DraftRow

/**
 * Keeps a message being written on the phone as it changes. Saves run one at a time in
 * [scope], each with the newest text and the id the first one got, so they outlive the screen
 * and the last one still lands after it closes. Once the message is sent or thrown away,
 * nothing more is saved.
 */
class DraftKeeper(private val mail: Mail, private val scope: CoroutineScope, id: Long) {
    /** The saved draft, or 0 while there is none. */
    @Volatile
    var id = id
        private set
    private val latest = AtomicReference<DraftItem?>(null)
    private val lock = Mutex()
    private var done = false
    // What the last save wrote, so the same text is not written again.
    private var written: DraftItem? = null

    /** Saves [draft] soon; one with nothing written throws the saved one away. */
    fun save(draft: DraftItem) {
        latest.set(draft)
        scope.launch {
            lock.withLock {
                // A later call already saved it.
                val newest = latest.getAndSet(null) ?: return@withLock
                if (done || newest == written) return@withLock
                try {
                    id = when {
                        !newest.isBlank() -> mail.saveDraft(newest.copy(id = id))
                        id != 0L -> 0L.also { mail.deleteDraft(id) }
                        else -> 0L
                    }
                    written = newest
                } catch (e: CoreException) {
                    // Gone: sent or thrown away elsewhere; a later save tries again otherwise.
                    if (e is CoreException.Gone) done = true
                    Log.w("Ozero", "draft not saved", e)
                }
            }
        }
    }

    /** Sends [draft], and the saved draft goes with it. */
    suspend fun send(draft: DraftItem): Long = lock.withLock {
        latest.set(null)
        mail.send(draft.copy(id = id)).also { done = true }
    }

    /** Adds [file] to the draft, saving [draft] first while there is no saved one to add it to. */
    suspend fun attach(draft: DraftItem, file: PickedFile): DraftFileItem = lock.withLock {
        if (done) throw CoreException.Gone()
        if (id == 0L) id = mail.saveDraft(draft)
        mail.attachFile(id, file)
    }

    /**
     * Saves [draft] now and stops saving, for another screen to go on with it. Gives the id it
     * is saved under.
     */
    suspend fun handOff(draft: DraftItem): Long = lock.withLock {
        latest.set(null)
        if (!done) {
            id = mail.saveDraft(draft.copy(id = id))
            done = true
        }
        id
    }

    /** Takes [file] off the draft, in turn with saves and the send, so a send after it goes without it. */
    suspend fun detach(file: DraftFileItem) = lock.withLock { mail.detachFile(file.id) }

    /**
     * Throws the saved draft away, and stops saving. Gives the id it had once saves on their way
     * have landed, 0 for none.
     */
    fun discard(): Deferred<Long> {
        latest.set(null)
        return scope.async {
            lock.withLock {
                done = true
                if (id != 0L) {
                    try {
                        mail.deleteDraft(id)
                    } catch (e: CoreException) {
                        Log.w("Ozero", "draft not deleted", e)
                    }
                }
                id
            }
        }
    }
}

/** How long typing pauses before the draft is saved, so each key does not save it. */
private const val SAVE_PAUSE_MS = 1_000L

/** Saves what [current] gives with [save] once typing pauses; [onChange] hears each change first. */
@Composable
fun SaveOnPause(
    keeper: DraftKeeper,
    current: () -> DraftItem,
    onChange: () -> Unit = {},
    save: (DraftItem) -> Unit = keeper::save,
) {
    val latest by rememberUpdatedState(current)
    LaunchedEffect(keeper) {
        snapshotFlow { latest() }.drop(1).collectLatest { draft ->
            onChange()
            delay(SAVE_PAUSE_MS)
            save(draft)
        }
    }
}

/** Nothing worth keeping: no text or files, and for a new message no recipients or subject either. */
fun DraftItem.isBlank(): Boolean = text.isBlank() && files.isEmpty() && (replyTo != null || (to.isBlank() && cc.isBlank() && subject.isBlank()))

/**
 * The drafts on the phone, last saved first, in rows like mail's; a tap opens one to go on
 * writing, and the swipe that deletes mail throws one away, which the toast can bring back.
 */
@Composable
fun DraftsScreen(mail: Mail, swipes: Swipes, snackbar: SnackbarHostState, onOpen: (DraftItem) -> Unit, onBack: () -> Unit) {
    val drafts by mail.drafts.collectAsState()
    val colors = MaterialTheme.colorScheme
    val scope = rememberCoroutineScope()
    val pull = remember { Pull() }
    val draftSwipes = remember(swipes) { swipes.forDrafts() }
    val deleted = stringResource(R.string.draft_deleted)
    val undo = stringResource(R.string.action_undo)
    fun delete(draft: DraftItem, reset: () -> Unit) {
        scope.launch {
            try {
                mail.deleteDraft(draft.id)
            } catch (e: CoreException) {
                Log.w("Ozero", "draft not deleted", e)
                return@launch reset()
            }
            if (!snackbar.offerUndo(deleted, undo)) return@launch
            try {
                mail.restoreDraft(draft)
            } catch (e: CoreException) {
                Log.w("Ozero", "draft not restored", e)
            }
        }
    }
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Hero(stringResource(R.string.folder_drafts), null, onBack)
        LazyColumn(Modifier.weight(1f)) {
            if (drafts.isEmpty()) {
                item { EmptyNote(R.drawable.ic_compose, stringResource(R.string.drafts_empty)) }
            }
            itemsIndexed(drafts, key = { _, it -> it.draft.id }) { index, row ->
                Column(rowMove().graphicsLayer { translationX = pull.near(index) }) {
                    if (index > 0) HorizontalDivider(Modifier.padding(start = Gap.row), color = colors.outlineVariant)
                    SwipeRow(
                        draftSwipes,
                        enabled = true,
                        applies = { it == SwipeAction.DELETE },
                        label = { it.label },
                        onSwipe = { _, reset -> delete(row.draft, reset) },
                        onPull = { pull.hold(row.draft.id, index, it) },
                    ) {
                        DraftLine(row, Modifier.clickable { onOpen(row.draft) })
                    }
                }
            }
        }
        Toast(snackbar, onGo = {}, modifier = Modifier.padding(bottom = 8.dp))
    }
}

/**
 * Only Delete applies to a draft: a short swipe to the side whose swipes delete mail, else to
 * the left. The other side's action never applies, so that side stays still.
 */
private fun Swipes.forDrafts(): Swipes {
    val right = right == SwipeAction.DELETE || rightLong == SwipeAction.DELETE
    val left = left == SwipeAction.DELETE || leftLong == SwipeAction.DELETE || !right
    return Swipes(
        left = if (left) SwipeAction.DELETE else SwipeAction.DONE,
        right = if (right) SwipeAction.DELETE else SwipeAction.READ,
        leftLong = null,
        rightLong = null,
    )
}

/** As a row of mail: who it goes to, its account and when it was saved, then the subject, the start of the text and its files. */
@Composable
private fun DraftLine(row: DraftRow, modifier: Modifier) {
    val colors = MaterialTheme.colorScheme
    val draft = row.draft
    Column(
        modifier
            .fillMaxWidth()
            .padding(start = Gap.row, end = Gap.screen, top = 16.dp, bottom = 16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                row.people.ifBlank { stringResource(R.string.drafts_no_recipients) },
                style = Type.Row,
                fontWeight = Weight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).padding(end = 8.dp),
            )
            row.account?.let { AccountTag(it) }
            Text(shortDate(LocalContext.current, draft.saved), color = colors.onSurfaceVariant, style = Type.Caption)
        }
        Text(
            draft.subject.ifBlank { stringResource(R.string.inbox_no_subject) },
            style = Type.Subject,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (row.snippet.isNotBlank()) {
            Text(
                row.snippet,
                color = colors.onSurfaceVariant,
                style = Type.Text,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (draft.files.isNotEmpty()) AttachmentChips(draft.files.map { it.name })
    }
}

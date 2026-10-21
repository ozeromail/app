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
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftItem

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

    /** Throws the saved draft away, and stops saving. */
    fun discard() {
        latest.set(null)
        scope.launch {
            lock.withLock {
                done = true
                if (id != 0L) {
                    try {
                        mail.deleteDraft(id)
                    } catch (e: CoreException) {
                        Log.w("Ozero", "draft not deleted", e)
                    }
                }
            }
        }
    }
}

/** Nothing worth keeping: no text, and for a new message no recipients or subject either. */
fun DraftItem.isBlank(): Boolean = text.isBlank() && (replyTo != null || (to.isBlank() && subject.isBlank()))

/**
 * The drafts on the phone, last saved first; a tap opens one to go on writing. The toast
 * shows here too, so a draft thrown away from here can come back.
 */
@Composable
fun DraftsScreen(mail: Mail, snackbar: SnackbarHostState, onOpen: (DraftItem) -> Unit, onBack: () -> Unit) {
    val drafts by mail.drafts.collectAsState()
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Hero(stringResource(R.string.folder_drafts), null, onBack)
        LazyColumn(Modifier.weight(1f)) {
            if (drafts.isEmpty()) {
                item { EmptyNote(R.drawable.ic_compose, stringResource(R.string.drafts_empty)) }
            }
            itemsIndexed(drafts, key = { _, it -> it.id }) { index, draft ->
                Column(rowMove()) {
                    if (index > 0) HorizontalDivider(Modifier.padding(start = Gap.screen), color = colors.outlineVariant)
                    DraftRow(draft, onClick = { onOpen(draft) })
                }
            }
        }
        Toast(snackbar, onGo = {}, modifier = Modifier.padding(bottom = 8.dp))
    }
}

/** Who it goes to and when it was saved, then the subject and the start of the text. */
@Composable
private fun DraftRow(draft: DraftItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(
        Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(start = Gap.screen, end = 16.dp, top = 12.dp, bottom = 12.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                draft.to.ifBlank { stringResource(R.string.drafts_no_recipients) },
                style = Type.Row,
                fontWeight = Weight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).padding(end = 8.dp),
            )
            Text(shortDate(LocalContext.current, draft.saved), color = colors.onSurfaceVariant, style = Type.Caption)
        }
        Text(
            draft.subject.ifBlank { stringResource(R.string.inbox_no_subject) },
            style = Type.Subject,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (draft.text.isNotBlank()) {
            Text(
                draft.text.trim(),
                color = colors.onSurfaceVariant,
                style = Type.Text,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

package com.ozeromail.app

import android.content.Context
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import java.io.IOException
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftFileItem
import uniffi.ozero_api.DraftItem
import uniffi.ozero_api.ForwardLabels
import uniffi.ozero_api.ImageSize
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.RecentFileItem
import uniffi.ozero_api.RichBlock
import uniffi.ozero_api.fileLimit
import uniffi.ozero_api.richBlocks
import uniffi.ozero_api.richChangeImage
import uniffi.ozero_api.richInsertImages
import uniffi.ozero_api.richMoveImage
import uniffi.ozero_api.richRemoveImage

/** A blank message; account 0 is none, so compose starts from the first account. */
val NEW_DRAFT = DraftItem(0, 0, "", "", "", null, "", 0)


/** The header words of a forwarded message, in the app's language. */
fun forwardLabels(context: Context) = ForwardLabels(
    context.getString(R.string.compose_forward_title),
    context.getString(R.string.compose_forward_from),
    context.getString(R.string.compose_forward_date),
    context.getString(R.string.compose_forward_subject),
)

/** A message being written, and the one it answers or forwards. */
data class Composing(val draft: DraftItem, val original: MessageItem? = null)

/**
 * A new message, a reply or a forward, on the whole screen. The fields sit low, near the
 * keyboard; the message being answered or forwarded stays on top. [onSent] gets the outbox
 * id for undo.
 *
 * What is written is kept as a draft on the phone a moment after typing stops, when the app
 * goes to the background and when the screen closes; [onKept] then says it is in the drafts.
 * [onDiscard] gets what was thrown away, for undo.
 */
@Composable
fun ComposeScreen(
    mail: Mail,
    composing: Composing,
    onCancel: () -> Unit,
    onSent: (Long, Composing) -> Unit,
    onKept: () -> Unit,
    onDiscard: (DraftItem) -> Unit,
) {
    val start = composing.draft
    var to by rememberSaveable { mutableStateOf(start.to) }
    var cc by rememberSaveable { mutableStateOf(start.cc) }
    // Kept while it is emptied, so it does not jump away.
    var hasCc by rememberSaveable { mutableStateOf(start.cc.isNotBlank()) }
    var subject by rememberSaveable { mutableStateOf(start.subject) }
    // With its styles and images as marks; the editor shows what they mean.
    val textState = rememberSaveable { mutableStateOf(start.text) }
    var text by textState
    // Where the caret is in the text, for photos to go after its line.
    var caret by rememberSaveable { mutableIntStateOf(start.text.length) }
    var accountId by rememberSaveable { mutableStateOf(start.accountId) }
    var signature by rememberSaveable { mutableStateOf(start.signature) }
    // Not saveable: a long quote would not fit in the saved state.
    var quote by remember { mutableStateOf(start.quote) }
    var quotable by remember { mutableStateOf(true) }
    var accounts by remember { mutableStateOf(emptyList<AccountItem>()) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    var picking by remember { mutableStateOf(false) }
    var files by remember { mutableStateOf(start.files) }
    // The names of picked files still being read, in the order they were picked.
    var adding by remember { mutableStateOf(emptyList<String>()) }
    var attaching by remember { mutableStateOf(false) }
    var recent by remember { mutableStateOf(emptyList<RecentFileItem>()) }
    // Photos made lighter here, by file, with where the full one is while the screen is open.
    val photos = remember { mutableStateMapOf<Long, Photo>() }
    // The image whose menu is open, and the last one, which the closing menu still shows.
    var chosenImage by remember { mutableStateOf<Long?>(null) }
    var lastImage by remember { mutableStateOf<Long?>(null) }
    // The address a link is getting, and where it goes.
    var linking by remember { mutableStateOf<Linking?>(null) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val keeper = remember { mail.keeper(start.id) }
    // Whether anything was written here; until then a saved draft stays as it was.
    var changed by remember { mutableStateOf(false) }
    // Sent or thrown away: nothing more to keep.
    var finished by remember { mutableStateOf(false) }

    // A forward sends the message's files too, from the account it came to: their names, read
    // once its body is saved.
    var forwarded by remember { mutableStateOf<MessageItem?>(null) }
    LaunchedEffect(Unit) {
        try {
            accounts = mail.accounts()
            forwarded = start.forward?.let { mail.message(it) }
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    // A new message starts from the first account.
    val from = accounts.find { it.id == accountId } ?: accounts.firstOrNull()

    // Account 0 is kept as none, and opens with the first account again.
    fun current() = DraftItem(0, accountId, to, subject, text, start.replyTo, quote, 0, start.forward, files, signature, cc)
    fun keep() {
        if (changed && !finished) keeper.save(current())
    }
    SaveOnPause(keeper, ::current, onChange = { changed = true }, save = { if (!finished) keeper.save(it) })
    // The process may be killed in the background at any time.
    // Something written here or before, which Delete throws away.
    val hasDraft = changed || start.id != 0L
    LifecycleEventEffect(Lifecycle.Event.ON_STOP) { keep() }
    DisposableEffect(keeper) {
        onDispose {
            keep()
            // Read here, not from the first composition.
            if (!finished && (changed || start.id != 0L) && !current().isBlank()) onKept()
        }
    }

    fun discard() {
        finished = true
        val thrown = current().copy(accountId = from?.id ?: accountId)
        val id = keeper.discard()
        // Its id, so undo brings it back with its files.
        scope.launch { onDiscard(thrown.copy(id = id.await())) }
    }

    /**
     * Adds the files [named] gives the names of, one at a time so a message never holds more than fits in
     * it: [read] gets each by its index and name whole, or null when it takes more than the
     * room left. [then] gets those added, by index, whatever went wrong.
     */
    fun addFiles(
        named: suspend () -> List<String>,
        read: suspend (Int, String, Long) -> PickedFile?,
        then: (List<Pair<Int, DraftFileItem>>) -> Unit = {},
    ) {
        error = null
        scope.launch {
            val names = named()
            adding = adding + names
            var done = 0
            val added = mutableListOf<Pair<Int, DraftFileItem>>()
            try {
                for ((index, name) in names.withIndex()) {
                    val room = fileLimit().toLong() - files.sumOf { it.size.toLong() }
                    val picked = read(index, name, room) ?: throw CoreException.TooLarge()
                    val file = keeper.attach(current(), picked)
                    files = files + file
                    added += index to file
                    adding = adding - name
                    done++
                }
            } catch (e: CoreException) {
                error = e.text()
            } catch (_: IOException) {
                error = R.string.compose_file_unreadable
            } catch (_: SecurityException) {
                error = R.string.compose_file_unreadable
            } finally {
                // What did not come stops showing; another pick's files of the same name stay.
                names.drop(done).forEach { adding = adding - it }
                then(added)
            }
        }
    }

    fun add(uris: List<Uri>) {
        if (uris.isEmpty()) return
        addFiles({ uris.map { pickedName(context, it) } }, { index, name, room -> readPicked(context, uris[index], name, room) })
    }
    // Photos go into the text after the caret's line, lighter; videos go with the files.
    fun addMedia(uris: List<Uri>) {
        if (uris.isEmpty()) return
        val lighter = mutableSetOf<Int>()
        addFiles(
            { uris.map { pickedName(context, it) } },
            { index, name, room ->
                val light = lighterPhoto(context, uris[index], name)
                if (light == null) {
                    readPicked(context, uris[index], name, room)
                } else {
                    lighter += index
                    light.takeIf { it.data.size <= room }
                }
            },
        ) { added ->
            val shown = added.filter { (_, file) -> file.mime.startsWith("image/") }
            shown.forEach { (index, file) -> if (index in lighter) photos[file.id] = Photo(uris[index], file.name, full = false) }
            if (shown.isNotEmpty()) {
                text = richInsertImages(text, caret.coerceIn(0, text.length).toUInt(), shown.map { it.second.id })
            }
        }
    }
    val pickMedia = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(), ::addMedia)
    val pickFiles = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments(), ::add)

    fun addFromMail(file: RecentFileItem) {
        // A second tap while it downloads adds nothing.
        if (file.name in adding) return
        addFiles({ listOf(file.name) }, { _, _, room -> if (file.size.toLong() > room) null else mail.downloadFile(file) })
    }
    // Read each time the sheet opens, as mail keeps coming.
    LaunchedEffect(attaching) {
        if (attaching) {
            try {
                // A forwarded message's files go with it already.
                recent = mail.recentFiles().filter { it.message != start.forward }
            } catch (e: CoreException) {
                // The pickers are there still; text() logs why the list is not.
                e.text()
            }
        }
    }

    fun detach(file: DraftFileItem) {
        files = files - file
        photos.remove(file.id)
        scope.launch {
            try {
                keeper.detach(file)
            } catch (e: CoreException) {
                error = e.text()
            }
        }
    }

    /** The image of [file] taken out of the text: it goes as a file, or with [drop] not at all. */
    fun unshow(file: Long, drop: Boolean) {
        text = richRemoveImage(text, file)
        if (drop) files.find { it.id == file }?.let(::detach)
    }

    /** The photo of [file] as it was taken, or lighter again: the other one takes its place. */
    fun swapPhoto(file: DraftFileItem, photo: Photo, size: ImageSize) {
        error = null
        scope.launch {
            try {
                val room = fileLimit().toLong() - files.sumOf { it.size.toLong() }
                val other = if (photo.full) lighterPhoto(context, photo.uri, photo.name) else readPicked(context, photo.uri, photo.name, room)
                if (other == null || other.data.size > room) throw CoreException.TooLarge()
                val added = keeper.attach(current(), other)
                files = files + added
                photos[added.id] = photo.copy(full = !photo.full)
                text = richChangeImage(text, file.id, added.id, size)
                detach(file)
            } catch (e: CoreException) {
                error = e.text()
            } catch (_: IOException) {
                error = R.string.compose_file_unreadable
            } catch (_: SecurityException) {
                error = R.string.compose_file_unreadable
            }
        }
    }

    fun send() {
        if (busy || from == null) return
        busy = true
        error = null
        val draft = current().copy(accountId = from.id)
        scope.launch {
            try {
                val id = keeper.send(draft)
                finished = true
                onSent(id, composing.copy(draft = draft))
            } catch (e: CoreException) {
                error = e.text()
            } finally {
                busy = false
            }
        }
    }

    val colors = MaterialTheme.colorScheme
    val ime = WindowInsets.ime
    val density = LocalDensity.current
    // Only whether it shows: the inset itself changes on every frame of the keyboard sliding.
    val keyboard by remember { derivedStateOf { ime.getBottom(density) > 0 } }
    val sideways = LocalRoom.current.sideways
    // On its side, while the text is typed, To and Subject fold into one line, so the text has
    // the little height the keyboard leaves; a tap on the line brings them back.
    var writingText by remember { mutableStateOf(false) }
    var unfolded by remember { mutableStateOf(false) }
    LaunchedEffect(keyboard) { if (!keyboard) unfolded = false }
    val folded = sideways && keyboard && writingText && !unfolded
    // A forward or a new message starts where it is missing: who it goes to.
    val toFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { if (to.isBlank() && !folded) toFocus.requestFocus() }
    val original = composing.original
    // The images the text shows, and the files that go under it.
    val shownImages = remember(text) { richBlocks(text).filterIsInstance<RichBlock.Image>() }.associate { it.file to it.size }
    val images = rememberImages(mail, shownImages.keys.toList(), LocalContext.current.resources.displayMetrics.widthPixels)
    val addresses = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next, autoCorrectEnabled = false)
    val fields = @Composable {
        if (!folded) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Plain(to, { to = it }, R.string.compose_to, Type.Body, addresses, modifier = Modifier.weight(1f).focusRequester(toFocus))
                if (!hasCc) {
                    Text(
                        stringResource(R.string.compose_cc),
                        color = colors.primary,
                        style = Type.Caption,
                        fontWeight = Weight.SemiBold,
                        modifier = Modifier
                            .clip(CircleShape)
                            .clickable(role = Role.Button) { hasCc = true }
                            .padding(horizontal = 10.dp, vertical = 6.dp),
                    )
                }
            }
            if (hasCc) {
                HorizontalDivider(color = colors.outlineVariant)
                Plain(cc, { cc = it }, R.string.compose_cc, Type.Body, addresses)
            }
            HorizontalDivider(color = colors.outlineVariant)
            Plain(
                subject,
                { subject = it },
                R.string.compose_subject,
                Type.Title,
                KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Next),
            )
        }
        RichBody(
            textState,
            files,
            images,
            chosen = chosenImage,
            placeholder = stringResource(R.string.compose_text),
            style = Type.Writing,
            onCaret = { caret = it },
            onImage = {
                chosenImage = it
                lastImage = it
            },
            onLink = { url, apply -> linking = Linking(url, apply) },
            modifier = Modifier
                .onFocusChanged { writingText = it.hasFocus }
                .then(
                    if (sideways) {
                        Modifier.heightIn(min = 72.dp)
                    } else {
                        // Room for a few lines from the start, less while the keyboard takes the screen.
                        Modifier.heightIn(min = if (keyboard) 72.dp else 168.dp, max = if (keyboard) 168.dp else 320.dp)
                    },
                )
                .verticalScroll(rememberScrollState()),
        )
        val attached = files.filter { it.id !in shownImages }
        if (attached.isNotEmpty() || adding.isNotEmpty()) FileChips(attached, adding, ::detach)
        // A forward is its quote, so it stays.
        val forwarding = start.forward != null
        val forwardedFiles = forwarded?.takeIf { it.accountId == from?.id }?.attachments.orEmpty()
        if (forwardedFiles.isNotEmpty()) AttachmentChips(forwardedFiles)
        // Its own for this message: a change here stays here. Taken out, a tap brings it back.
        if (signature.isNotEmpty() || from?.signsWith?.isNotBlank() == true) {
            Plain(
                signature,
                { signature = it },
                R.string.account_signature,
                Type.Text,
                KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                singleLine = false,
                color = colors.onSurfaceVariant,
                modifier = Modifier
                    .semantics { contentDescription = context.getString(R.string.compose_signature) }
                    .onFocusChanged { if (it.isFocused && signature.isEmpty()) from?.let { a -> signature = a.signsWith } },
            )
        }
        if (quote.isNotEmpty()) {
            Quote(quote.takeIf { original == null }, forwarding, onRemove = { quote = "" }.takeUnless { forwarding })
        } else if (original != null && quotable) {
            Pill(stringResource(R.string.compose_add_quote), height = 32.dp, onClick = {
                val header = context.getString(R.string.compose_quote_header, longDate(context, original.date), original.sender)
                scope.launch {
                    try {
                        quote = mail.quote(original, header)
                        // Nothing to quote.
                        quotable = quote.isNotEmpty()
                    } catch (e: CoreException) {
                        error = e.text()
                    }
                }
            })
        }
    }
    val errorLine = @Composable {
        Reveal(error) {
            Text(stringResource(it), color = colors.error, modifier = Modifier.padding(horizontal = Gap.screen, vertical = 8.dp))
        }
    }
    // The files still being read go with it, so it waits for them.
    val ready = !busy && from != null && adding.isEmpty()
    val attachTile = @Composable {
        IconTile(R.drawable.ic_attach, stringResource(R.string.compose_attach), { attaching = true }, shape = CircleShape)
    }
    val sendTile = @Composable {
        IconTile(
            R.drawable.ic_send,
            stringResource(R.string.compose_send),
            ::send,
            Modifier.width(56.dp),
            background = colors.accent,
            tint = colors.onAccent,
            shape = CircleShape,
            enabled = ready,
        )
    }
    Box(Modifier.fillMaxSize()) {
        if (sideways) {
            // Close and the sender at the top left, Send at the right edge by the keyboard.
            Row(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
                Middle(COMPOSE_MAX, Modifier.weight(1f)) {
                    Column(Modifier.fillMaxSize()) {
                        Row(
                            Modifier.fillMaxWidth().padding(horizontal = Gap.bar, vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
                        ) {
                            IconTile(R.drawable.ic_close, stringResource(R.string.action_close), onCancel, shape = CircleShape)
                            if (folded) {
                                Text(
                                    listOf(to.ifBlank { stringResource(R.string.compose_to) }, subject.ifBlank { stringResource(R.string.compose_subject) }).joinToString(" · "),
                                    color = colors.onSurfaceVariant,
                                    style = Type.Text,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier
                                        .weight(1f)
                                        .clip(CircleShape)
                                        .clickable(role = Role.Button) { unfolded = true }
                                        .padding(horizontal = 8.dp, vertical = 12.dp),
                                )
                            } else {
                                if (hasDraft) {
                                    IconTile(R.drawable.ic_delete, stringResource(R.string.action_delete_draft), ::discard, shape = CircleShape)
                                }
                                Box(Modifier.weight(1f)) {
                                    if (from != null) Sender(from, enabled = accounts.size > 1, onClick = { picking = true })
                                }
                            }
                        }
                        // With the keyboard down there is room for the message being answered.
                        if (original != null) Original(mail, original, if (keyboard) Modifier.height(0.dp) else Modifier.weight(1f))
                        Column(
                            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = Gap.screen, vertical = 8.dp),
                            verticalArrangement = Arrangement.spacedBy(12.dp),
                        ) { fields() }
                        errorLine()
                    }
                }
                Column(
                    Modifier.fillMaxHeight().padding(Gap.bar),
                    verticalArrangement = Arrangement.spacedBy(Gap.inBar, Alignment.Bottom),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    attachTile()
                    sendTile()
                }
            }
        } else {
            Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
                // The message being answered takes the room the fields leave; alone, the fields sit low.
                // With the keyboard up the card may close to nothing, so Send always stays in sight.
                if (original != null) Original(mail, original, Modifier.weight(1f).heightIn(min = if (keyboard) 0.dp else 112.dp))
                // As much room under the text as at its sides, so it never touches the bar.
                Column(
                    (if (original == null) Modifier.weight(1f) else Modifier).padding(start = Gap.screen, end = Gap.screen, bottom = Gap.screen),
                    verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.Bottom),
                ) { fields() }
                errorLine()
                Row(
                    Modifier
                        .fillMaxWidth()
                        .hairline(colors.outlineVariant, top = true)
                        .padding(start = Gap.bar, end = Gap.bar, top = Gap.barTop, bottom = Gap.barBottom),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
                ) {
                    IconTile(R.drawable.ic_close, stringResource(R.string.action_close), onCancel, shape = CircleShape)
                    if (hasDraft) {
                        IconTile(R.drawable.ic_delete, stringResource(R.string.action_delete_draft), ::discard, shape = CircleShape)
                    }
                    Box(Modifier.weight(1f)) {
                        if (from != null) Sender(from, enabled = accounts.size > 1, onClick = { picking = true })
                    }
                    attachTile()
                    sendTile()
                }
            }
        }
        // Where the files come from, low in reach of the thumb like every menu.
        Sheet(visible = attaching, onClose = { attaching = false }) {
            SheetTitle(stringResource(R.string.compose_attach))
            SheetAction(R.drawable.ic_photo, stringResource(R.string.compose_attach_photos), colors.onSurface) {
                attaching = false
                pickMedia.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo))
            }
            SheetAction(R.drawable.ic_folder, stringResource(R.string.compose_attach_files), colors.onSurface) {
                attaching = false
                pickFiles.launch(arrayOf("*/*"))
            }
            if (recent.isNotEmpty()) {
                SheetTitle(stringResource(R.string.compose_attach_recent), Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 16.dp, bottom = 4.dp))
                recent.forEach { file ->
                    RecentFileRow(file) {
                        attaching = false
                        addFromMail(file)
                    }
                }
            }
        }
        // What can be done with an image in the text, low in reach of the thumb like every menu.
        Sheet(visible = chosenImage != null, onClose = { chosenImage = null }) {
            val id = chosenImage ?: lastImage
            val file = files.find { it.id == id }
            val size = shownImages[id]
            if (id != null && file != null && size != null) {
                ImageMenu(
                    size,
                    photos[id],
                    onSize = { text = richChangeImage(text, id, id, it) },
                    onMove = { up -> text = richMoveImage(text, id, up) },
                    onSwap = { photo ->
                        chosenImage = null
                        swapPhoto(file, photo, size)
                    },
                    onUnshow = { drop ->
                        chosenImage = null
                        unshow(id, drop)
                    },
                )
            }
        }
        LinkSheet(linking, onClose = { linking = null })
        // Like every menu, the other accounts open in a sheet, low in reach of the thumb.
        Sheet(visible = picking, onClose = { picking = false }) {
            accounts.forEach { account ->
                SheetRow(
                    onClick = {
                        picking = false
                        // Its signature comes too, unless this message's was changed.
                        if (signature == from?.signsWith) signature = account.signsWith
                        accountId = account.id
                    },
                    lead = { AccountDot(account.tint, size = 12.dp) },
                ) {
                    Text(account.email, style = Type.Row, fontWeight = Weight.Regular, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    if (account.id == from?.id) Glyph(R.drawable.ic_done, null, colors.accent)
                }
            }
        }
    }
}

/** The message being answered, whole, scrolling in the room it gets. */
@Composable
private fun Original(mail: Mail, message: MessageItem, modifier: Modifier) {
    val colors = MaterialTheme.colorScheme
    var text by remember(message.id) { mutableStateOf(message.snippet) }
    LaunchedEffect(message.id) {
        try {
            mail.body(message).text.takeIf { it.isNotBlank() }?.let { text = it }
        } catch (_: CoreException) {
            // The snippet stays.
        }
    }
    // The card is as tall as its text, up to the room it is given; the rest stays clear.
    Box(modifier.fillMaxWidth()) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(start = Gap.card, end = Gap.card, top = 12.dp, bottom = 10.dp)
                .clip(RoundedCornerShape(Radius.tile))
                .background(colors.surfaceVariant),
        ) {
            Row(Modifier.padding(start = Gap.inCard, end = Gap.inCard, top = 12.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    message.sender,
                    style = Type.Text,
                    fontWeight = Weight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(shortDate(LocalContext.current, message.date), color = colors.onSurfaceVariant, style = Type.Caption)
            }
            Text(
                text,
                color = colors.onSurfaceVariant,
                style = Type.Quote,
                modifier = Modifier
                    .verticalScroll(rememberScrollState())
                    .padding(start = Gap.inCard, end = Gap.inCard, bottom = 12.dp),
            )
        }
    }
}

/**
 * The quoted original under the text, or with [text] null only that it goes there: its text is
 * in the card above already. A way to take it out if [onRemove] is set.
 */
@Composable
private fun Quote(text: String?, forwarding: Boolean, onRemove: (() -> Unit)?) {
    val colors = MaterialTheme.colorScheme
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(
            text ?: stringResource(if (forwarding) R.string.compose_forwarded else R.string.compose_quoted),
            color = colors.onSurfaceVariant,
            style = Type.Text,
            maxLines = if (text == null) 1 else 6,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .weight(1f)
                .drawBehind { drawLine(colors.outlineVariant, Offset.Zero, Offset(0f, size.height), 3.dp.toPx()) }
                .padding(start = 12.dp, top = 2.dp, bottom = 2.dp),
        )
        if (onRemove != null) IconTile(R.drawable.ic_close, stringResource(R.string.action_remove), onRemove, size = 32.dp)
    }
}

/** The account the message goes from, as a chip by Send; a tap lists the others. */
@Composable
private fun Sender(from: AccountItem, enabled: Boolean, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .height(44.dp)
            .clip(CircleShape)
            .background(colors.surfaceVariant)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(start = 8.dp, end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        AccountInitial(from.name, from.tint, 22.dp)
        Text(from.email, style = Type.Text, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** A field with no frame: just the text, or its placeholder. */
@Composable
internal fun Plain(
    value: String,
    onChange: (String) -> Unit,
    @StringRes placeholder: Int,
    style: TextStyle,
    keyboard: KeyboardOptions,
    singleLine: Boolean = true,
    color: Color = MaterialTheme.colorScheme.onSurface,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    BasicTextField(
        value = value,
        onValueChange = onChange,
        textStyle = style.copy(color = color),
        singleLine = singleLine,
        keyboardOptions = keyboard,
        cursorBrush = SolidColor(colors.primary),
        modifier = modifier.fillMaxWidth(),
        decorationBox = { field ->
            Box {
                if (value.isEmpty()) Text(stringResource(placeholder), style = style, color = colors.onSurfaceVariant)
                field()
            }
        },
    )
}

/** A photo made lighter to send, with where the full one is and which of the two goes. */
data class Photo(val uri: Uri, val name: String, val full: Boolean)

/** A link getting an address: the one it has, if any, and what takes the new one, or null to take it off. */
class Linking(val url: String?, val apply: (String?) -> Unit)

/** The size of an image in the text, moving it, the other photo, and taking it out of the text. */
@Composable
private fun ImageMenu(
    size: ImageSize,
    photo: Photo?,
    onSize: (ImageSize) -> Unit,
    onMove: (Boolean) -> Unit,
    onSwap: (Photo) -> Unit,
    onUnshow: (Boolean) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    SheetTitle(stringResource(R.string.compose_image))
    Row(
        Modifier.fillMaxWidth().padding(start = Gap.sheet, end = Gap.sheet, bottom = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        listOf(
            ImageSize.SMALL to R.string.compose_image_small,
            ImageSize.MEDIUM to R.string.compose_image_medium,
            ImageSize.FULL to R.string.compose_image_full,
        ).forEach { (option, label) ->
            val on = option == size
            Text(
                stringResource(label),
                color = if (on) colors.onAccent else colors.onSurface,
                style = Type.Text,
                fontWeight = Weight.SemiBold,
                maxLines = 1,
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .weight(1f)
                    .clip(CircleShape)
                    .background(if (on) colors.accent else colors.surfaceVariant)
                    .clickable(role = Role.RadioButton, onClick = { onSize(option) })
                    .padding(vertical = 10.dp),
            )
        }
    }
    SheetRow({ onMove(true) }, lead = { Glyph(R.drawable.ic_chevron_down, null, colors.onSurface, Modifier.rotate(180f)) }) {
        Text(stringResource(R.string.compose_image_up), style = Type.Row)
    }
    SheetRow({ onMove(false) }, lead = { Glyph(R.drawable.ic_chevron_down, null, colors.onSurface) }) {
        Text(stringResource(R.string.compose_image_down), style = Type.Row)
    }
    if (photo != null) {
        SheetAction(R.drawable.ic_photo, stringResource(if (photo.full) R.string.compose_image_lighter else R.string.compose_image_original), colors.onSurface) {
            onSwap(photo)
        }
    }
    SheetAction(R.drawable.ic_attach, stringResource(R.string.compose_image_to_file), colors.onSurface) { onUnshow(false) }
    SheetAction(R.drawable.ic_delete, stringResource(R.string.action_remove), colors.error) { onUnshow(true) }
}

/** Where a link goes: an address to type, and a way to take the link off when it has one. */
@Composable
private fun LinkSheet(linking: Linking?, onClose: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    // The last one, which the closing sheet still shows.
    var last by remember { mutableStateOf(linking) }
    if (linking != null) last = linking
    var url by remember(last) { mutableStateOf(last?.url ?: "https://") }
    val focus = remember { FocusRequester() }
    Sheet(visible = linking != null, onClose = onClose) {
        val shown = last ?: return@Sheet
        SheetTitle(stringResource(R.string.compose_format_link))
        fun done() {
            onClose()
            shown.apply(url.trim().takeUnless { it.isEmpty() || it == "https://" })
        }
        Plain(
            url,
            { url = it },
            R.string.compose_format_link,
            Type.Body,
            KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done, autoCorrectEnabled = false),
            modifier = Modifier.padding(horizontal = Gap.sheet, vertical = 8.dp).focusRequester(focus),
        )
        LaunchedEffect(shown) { focus.requestFocus() }
        SheetAction(R.drawable.ic_done, stringResource(R.string.action_done), colors.primary, ::done)
        if (shown.url != null) {
            SheetAction(R.drawable.ic_close, stringResource(R.string.compose_link_remove), colors.error) {
                onClose()
                shown.apply(null)
            }
        }
    }
}

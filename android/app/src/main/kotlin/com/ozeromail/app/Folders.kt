package com.ozeromail.app

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.round
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.FolderItem
import uniffi.ozero_api.FolderRole
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.MoveSuggestion

/** What the move sheet offers: where the mail can go, the folders to put first, and whether a new folder can be made for it. */
class MoveChoice(val targets: List<FolderItem>, val suggestions: List<MoveSuggestion>, val canCreate: Boolean)

/** A folder in a list of them, [depth] steps in, named [title]; [inside] when folders of the list are inside it. */
data class TreeRow(val folder: FolderItem, val title: String, val depth: Int, val inside: Boolean)

/**
 * An account's own [folders] as a tree, [depth] steps in: each with the ones inside it under it,
 * unless [shows] says to keep those hidden. Among folders side by side the pinned come first,
 * in the person's order, then the rest by name.
 */
fun folderTree(folders: List<FolderItem>, shows: (Long) -> Boolean = { true }, depth: Int = 0): List<TreeRow> {
    val ids = folders.map { it.id }.toSet()
    val inside = folders.groupBy { folder -> folder.parent?.takeIf { it in ids } }
    val order = compareBy<FolderItem>({ !it.pinned }, { it.rank ?: UInt.MAX_VALUE }, { it.shownName(ids).lowercase() })
    fun under(parent: Long?, at: Int): List<TreeRow> = inside[parent].orEmpty().sortedWith(order).flatMap { folder ->
        val more = !inside[folder.id].isNullOrEmpty()
        listOf(TreeRow(folder, folder.shownName(ids), at, more)) + if (more && shows(folder.id)) under(folder.id, at + 1) else emptyList()
    }
    return under(null, depth)
}

/** Inside a folder that shows too, among [ids], or at the top, its own name; else the whole path. */
fun FolderItem.shownName(ids: Set<Long>): String = if (parent == null || parent in ids) shortName else name

/** The folders this one is inside, outermost first, then itself, by their own names. */
fun FolderItem.path(byId: Map<Long, FolderItem>): String =
    generateSequence(this) { folder -> folder.parent?.let(byId::get) }.take(MAX_DEPTH).toList().asReversed().joinToString(" / ") { it.shortName }

/** [folder] and the folders inside it, among [folders]. */
private fun withInside(folder: FolderItem, folders: List<FolderItem>): Set<Long> {
    val byId = folders.associateBy { it.id }
    return folders.filter { other ->
        generateSequence(other) { it.parent?.let(byId::get) }.take(MAX_DEPTH).any { it.id == folder.id }
    }.map { it.id }.toSet() + folder.id
}

/** Deeper than any server nests: a guard against a loop in what a server said. */
private const val MAX_DEPTH = 32

/** Past this many folders, the move sheet has a search. */
private const val SEARCH_FROM = 8

/**
 * Where [messages] can go, as the move sheet shows it: the mail on top, the folders [choice]
 * suggests, then the inbox, the archive and the account's own folders as a tree whose branches
 * close as in [groups]. With many folders, a search; where the account can have new folders, a
 * way to make one and move the mail into it at once.
 */
@Composable
fun MovePicker(
    mail: Mail,
    messages: List<MessageItem>,
    choice: MoveChoice,
    folders: List<FolderItem>,
    groups: Groups,
    onGroups: (Groups) -> Unit,
    onPick: (FolderItem, String) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val account = messages.firstOrNull()?.accountId
    val own = folders.filter { it.accountId == account && it.role == FolderRole.CUSTOM }
    // The name of a folder to make for the mail, while the form for it shows.
    var making by rememberSaveable { mutableStateOf<String?>(null) }
    var query by rememberSaveable { mutableStateOf("") }
    making?.let { name ->
        FolderForm(
            title = stringResource(R.string.folder_new),
            places = own,
            folder = null,
            startName = name,
            startParent = null,
            action = R.string.folder_create_and_move,
            onBack = { making = null },
            onDone = { leaf, parent ->
                val made = mail.createFolder(account ?: return@FolderForm, parent, leaf)
                onPick(made, made.shortName)
            },
        )
        return
    }
    SheetTitle(stringResource(R.string.move_title))
    MailCard(messages)
    val byId = own.associateBy { it.id }
    val targets = choice.targets
    if (targets.size > SEARCH_FROM) {
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            placeholder = { Text(stringResource(R.string.move_find)) },
            leadingIcon = { Glyph(R.drawable.ic_search, null, colors.onSurfaceVariant, size = ICON) },
            singleLine = true,
            shape = RoundedCornerShape(Radius.medium),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            modifier = Modifier.fillMaxWidth().padding(start = Gap.sheet, end = Gap.sheet, bottom = 8.dp),
        )
    }
    val find = query.trim()
    if (find.isNotEmpty()) {
        targets.map { it to it.title() }.filter { (_, title) -> title.contains(find, ignoreCase = true) }.forEach { (folder, title) ->
            PickRow(folder.role.icon(), title, under = folder.parentPath(byId)) { onPick(folder, title) }
        }
        if (choice.canCreate) NewFolderRow(stringResource(R.string.folder_new_named, find)) { making = find }
        return
    }
    if (targets.isEmpty()) {
        Text(
            stringResource(R.string.move_none),
            color = colors.onSurfaceVariant,
            style = Type.Row,
            modifier = Modifier.padding(horizontal = Gap.sheet, vertical = 12.dp),
        )
    }
    if (choice.suggestions.isNotEmpty()) {
        Caption(stringResource(R.string.move_suggested))
        choice.suggestions.forEach { suggestion ->
            val folder = suggestion.folder
            val title = folder.title()
            val why = if (suggestion.count > 0u) {
                stringResource(R.string.move_from_sender, suggestion.count.toInt(), suggestion.sender)
            } else {
                stringResource(R.string.move_lately)
            }
            val under = folder.parentPath(byId)
            PickRow(folder.role.icon(), title, under = if (under != null) "$under · $why" else why) { onPick(folder, title) }
        }
        Caption(stringResource(R.string.move_all))
    }
    targets.filter { it.role != FolderRole.CUSTOM }.sortedBy { it.role.rank() }.forEach { folder ->
        val title = folder.title()
        PickRow(folder.role.icon(), title) { onPick(folder, title) }
    }
    folderTree(targets.filter { it.role == FolderRole.CUSTOM }, groups::showsInside).forEach { row ->
        val folder = row.folder
        PickRow(
            R.drawable.ic_folder,
            row.title,
            depth = row.depth,
            end = if (row.inside) {
                { Twisty(row.title, groups.showsInside(folder.id)) { onGroups(groups.toggleInside(folder.id)) } }
            } else {
                null
            },
        ) { onPick(folder, row.title) }
    }
    if (choice.canCreate) NewFolderRow(stringResource(R.string.folder_new)) { making = "" }
}

/** Where a folder is, for a list that names it alone; null at the top. */
private fun FolderItem.parentPath(byId: Map<Long, FolderItem>): String? = parent?.let(byId::get)?.path(byId)

/** What is being moved, on top of the move sheet: the message's sender and subject, or how many messages. */
@Composable
private fun MailCard(messages: List<MessageItem>) {
    val colors = MaterialTheme.colorScheme
    val one = messages.singleOrNull()
    Column(
        Modifier
            .fillMaxWidth()
            .padding(start = Gap.sheet, end = Gap.sheet, bottom = 10.dp)
            .clip(RoundedCornerShape(Radius.medium))
            .background(colors.surfaceContainer)
            .padding(horizontal = 14.dp, vertical = 10.dp),
    ) {
        Text(
            one?.sender ?: stringResource(R.string.move_messages, messages.size),
            style = Type.Text,
            fontWeight = Weight.SemiBold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        val subject = (one ?: messages.firstOrNull())?.subject.orEmpty()
        if (subject.isNotEmpty()) {
            Text(subject, color = colors.onSurfaceVariant, style = Type.Text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** A folder to pick, [depth] steps in, with where it is or why it is offered [under] its name. */
@Composable
private fun PickRow(icon: Int, title: String, under: String? = null, depth: Int = 0, end: (@Composable () -> Unit)? = null, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    SheetRow(onClick, lead = { RowIcon(icon, open = false) }, fixed = true, start = indent(depth)) {
        Column(Modifier.weight(1f)) {
            Text(title, style = Type.Row, fontWeight = Weight.Regular, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (under != null) Text(under, color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        end?.invoke()
    }
}

/** A row that makes a folder, in the accent colour. */
@Composable
fun NewFolderRow(text: String, depth: Int = 0, onClick: () -> Unit) {
    val accent = MaterialTheme.colorScheme.primary
    SheetRow(onClick, lead = { Glyph(R.drawable.ic_add, null, accent, size = ICON) }, fixed = true, start = indent(depth)) {
        Text(text, color = accent, style = Type.Row, fontWeight = Weight.Regular, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
    }
}

/** At the end of a folder's row: shows or hides the folders inside it. */
@Composable
fun Twisty(folder: String, open: Boolean, onToggle: () -> Unit) {
    val inside = stringResource(R.string.folder_inside_of, folder)
    Box(
        Modifier
            .minimumInteractiveComponentSize()
            .clip(CircleShape)
            .clickable(onClickLabel = stringResource(if (open) R.string.action_collapse else R.string.action_expand), role = Role.Button, onClick = onToggle)
            .semantics { contentDescription = inside },
        contentAlignment = Alignment.Center,
    ) { Chevron(open, 16.dp) }
}

/** What the folder sheet shows: a held folder's actions, its card, or the form for a new one. */
sealed interface FolderSheet {
    data class Menu(val folder: FolderItem) : FolderSheet

    /** With [deleting], the question before deleting it is open. */
    data class Card(val folder: FolderItem, val deleting: Boolean = false) : FolderSheet

    /** A new folder of [account], inside [parent] or at the top. */
    data class New(val account: Long, val parent: FolderItem?) : FolderSheet
}

/**
 * The sheet for one of the account's own folders, as [shown] says: what holding it offers,
 * pinning first; its card with its name, where it is, the pin and Delete; or a new folder. Each
 * change shows at once and reaches the server in the background; [onChanged] reloads the
 * folders, [onDeleted] offers to undo a delete.
 */
@Composable
fun FolderSheets(
    mail: Mail,
    shown: FolderSheet?,
    accounts: List<AccountItem>,
    folders: List<FolderItem>,
    onShow: (FolderSheet?) -> Unit,
    onPin: (FolderItem, Boolean) -> Unit,
    onChanged: () -> Unit,
    onDeleted: (FolderItem) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Sheet(visible = shown != null, onClose = { onShow(null) }) {
        val sheet = rememberLast(shown) ?: return@Sheet
        when (sheet) {
            is FolderSheet.Menu -> {
                val folder = folders.find { it.id == sheet.folder.id } ?: sheet.folder
                SheetTitle(folder.shortName)
                SheetAction(R.drawable.ic_pin, stringResource(if (folder.pinned) R.string.action_unpin else R.string.action_pin), colors.onSurface) {
                    onShow(null)
                    onPin(folder, !folder.pinned)
                }
                if (accounts.find { it.id == folder.accountId }?.editsFolders == true) {
                    SheetAction(R.drawable.ic_add, stringResource(R.string.folder_new_inside), colors.onSurface) { onShow(FolderSheet.New(folder.accountId, folder)) }
                    SheetAction(R.drawable.ic_compose, stringResource(R.string.folder_rename), colors.onSurface) { onShow(FolderSheet.Card(folder)) }
                    SheetAction(R.drawable.ic_folder, stringResource(R.string.folder_move), colors.onSurface) { onShow(FolderSheet.Card(folder)) }
                    SheetAction(R.drawable.ic_delete, stringResource(R.string.folder_delete_ask), colors.error) { onShow(FolderSheet.Card(folder, deleting = true)) }
                }
            }
            is FolderSheet.Card -> {
                val folder = folders.find { it.id == sheet.folder.id } ?: sheet.folder
                FolderCard(mail, folder, folders.filter { it.accountId == folder.accountId && it.role == FolderRole.CUSTOM }, sheet.deleting, onShow, onPin, onChanged, onDeleted)
            }
            is FolderSheet.New -> FolderForm(
                title = stringResource(R.string.folder_new),
                places = folders.filter { it.accountId == sheet.account && it.role == FolderRole.CUSTOM },
                folder = null,
                startName = "",
                startParent = sheet.parent?.id,
                action = R.string.folder_create,
                onBack = null,
                onDone = { name, parent ->
                    mail.createFolder(sheet.account, parent, name)
                    onChanged()
                    onShow(null)
                },
            )
        }
    }
}

/** A folder's card: its name and where it is, saved together; its pin; Delete, asked first with what goes with it. */
@Composable
private fun FolderCard(
    mail: Mail,
    folder: FolderItem,
    own: List<FolderItem>,
    deleting: Boolean,
    onShow: (FolderSheet?) -> Unit,
    onPin: (FolderItem, Boolean) -> Unit,
    onChanged: () -> Unit,
    onDeleted: (FolderItem) -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val scope = rememberCoroutineScope()
    var count by remember(folder.id) { mutableStateOf<Int?>(null) }
    var failed by remember(folder.id) { mutableStateOf<Int?>(null) }
    LaunchedEffect(folder.id) {
        count = try {
            mail.folderMail(folder)
        } catch (e: CoreException) {
            null
        }
    }
    FolderForm(
        title = stringResource(R.string.folder_title),
        places = own,
        folder = folder,
        startName = folder.shortName,
        startParent = folder.parent,
        action = R.string.action_save,
        onBack = null,
        onDone = { name, parent ->
            mail.changeFolder(folder, parent, name)
            onChanged()
            onShow(null)
        },
    ) {
        SheetRow(onClick = { onPin(folder, !folder.pinned) }, lead = { RowIcon(R.drawable.ic_pin, open = false) }, fixed = true) {
            Text(stringResource(R.string.folder_pin_in_list), style = Type.Row, fontWeight = Weight.Regular, modifier = Modifier.weight(1f))
            SmallSwitch(folder.pinned)
        }
        if (!deleting) {
            TextButton(
                onClick = { onShow(FolderSheet.Card(folder, deleting = true)) },
                modifier = Modifier.padding(horizontal = Gap.sheet - 12.dp, vertical = 4.dp),
            ) { Text(stringResource(R.string.folder_delete), color = colors.error) }
            return@FolderForm
        }
        val inside = own.any { it.parent == folder.id }
        Column(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = Gap.sheet, vertical = 8.dp)
                .clip(RoundedCornerShape(Radius.medium))
                .background(colors.errorContainer)
                .padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            val question = if (inside) R.string.folder_delete_question_inside else R.string.folder_delete_question
            Text(stringResource(question, folder.shortName, count ?: 0), color = colors.onErrorContainer, style = Type.Text)
            failed?.let { Text(stringResource(it), color = colors.error, style = Type.Caption) }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = { onShow(FolderSheet.Card(folder)) }) { Text(stringResource(R.string.folder_keep)) }
                Button(
                    onClick = {
                        scope.launch {
                            try {
                                mail.deleteFolder(folder)
                            } catch (e: CoreException) {
                                failed = e.text()
                                return@launch
                            }
                            onShow(null)
                            onDeleted(folder)
                        }
                    },
                    colors = ButtonDefaults.buttonColors(containerColor = colors.error, contentColor = colors.onError),
                ) { Text(stringResource(R.string.action_delete)) }
            }
        }
    }
}

/**
 * A folder's name and the folder of the account's own it goes inside, from [places], or the top
 * level; for [folder], the folders inside it are not places for it. [action] sends them to
 * [onDone], which throws a [CoreException] for a name that cannot be there; [more] goes below.
 */
@Composable
fun FolderForm(
    title: String,
    places: List<FolderItem>,
    folder: FolderItem?,
    startName: String,
    startParent: Long?,
    @StringRes action: Int,
    onBack: (() -> Unit)?,
    onDone: suspend (name: String, parent: FolderItem?) -> Unit,
    more: @Composable ColumnScope.() -> Unit = {},
) {
    val colors = MaterialTheme.colorScheme
    val scope = rememberCoroutineScope()
    var name by rememberSaveable(folder?.id) { mutableStateOf(startName) }
    var parent by rememberSaveable(folder?.id) { mutableStateOf(startParent) }
    var choosing by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    val focus = remember { FocusRequester() }
    // A new folder starts with its name; a card opens on what is there.
    LaunchedEffect(Unit) { if (folder == null) focus.requestFocus() }
    val allowed = if (folder == null) places else withInside(folder, places).let { out -> places.filter { it.id !in out } }
    val byId = places.associateBy { it.id }
    val inside = parent?.let(byId::get)
    val ready = !busy && name.isNotBlank() && (folder == null || name.trim() != folder.shortName || parent != folder.parent)
    fun send() {
        if (!ready) return
        busy = true
        error = null
        scope.launch {
            try {
                onDone(name.trim(), inside)
            } catch (e: CoreException) {
                error = e.text()
            } finally {
                busy = false
            }
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        onBack?.let { BackArrow(it) }
        SheetTitle(title, Modifier.weight(1f).padding(start = if (onBack == null) Gap.sheet else 4.dp, end = Gap.sheet, bottom = 10.dp))
    }
    OutlinedTextField(
        value = name,
        onValueChange = {
            name = it
            error = null
        },
        label = { Text(stringResource(R.string.folder_name)) },
        isError = error != null,
        supportingText = error?.let { { Text(stringResource(it)) } },
        singleLine = true,
        shape = RoundedCornerShape(Radius.medium),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { send() }),
        modifier = Modifier.fillMaxWidth().padding(horizontal = Gap.sheet).focusRequester(focus),
    )
    Caption(stringResource(R.string.folder_where))
    SheetRow(onClick = { choosing = !choosing }, lead = { RowIcon(R.drawable.ic_folder, open = false) }, fixed = true) {
        Text(
            inside?.path(byId) ?: stringResource(R.string.folder_top),
            style = Type.Row,
            fontWeight = Weight.Regular,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Chevron(choosing, 16.dp)
    }
    Folding(choosing) {
        PlaceRow(stringResource(R.string.folder_top), depth = 1, on = parent == null) {
            parent = null
            choosing = false
        }
        folderTree(allowed, depth = 1).forEach { row ->
            PlaceRow(row.title, row.depth, on = parent == row.folder.id) {
                parent = row.folder.id
                choosing = false
            }
        }
    }
    Button(
        onClick = { send() },
        enabled = ready,
        modifier = Modifier.fillMaxWidth().padding(horizontal = Gap.sheet, vertical = 12.dp).height(48.dp),
    ) { Text(stringResource(action)) }
    more()
}

/** A place a folder can go, [on] when it is the one picked. */
@Composable
private fun PlaceRow(title: String, depth: Int, on: Boolean, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    SheetRow(onClick, lead = { if (on) Glyph(R.drawable.ic_done, null, colors.primary, size = ICON) }, fixed = true, start = indent(depth - 1)) {
        Text(
            title,
            color = if (on) colors.primary else colors.onSurface,
            style = Type.Row,
            fontWeight = if (on) Weight.SemiBold else Weight.Regular,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

/**
 * A row carried mail can be dropped on, into [folder]: lit while the mail is over it and it
 * takes it, faint while it does not; [onSpring], if any, runs once the mail rests on it a
 * moment, to open what it hides. Without [carry] or [folder], just the row.
 */
@Composable
fun DropPlace(carry: Carry?, folder: FolderItem?, title: String, onSpring: (() -> Unit)? = null, content: @Composable () -> Unit) {
    if (carry == null || folder == null) {
        content()
        return
    }
    val colors = MaterialTheme.colorScheme
    val carrying = carry.messages != null
    val takes = carry.takes(folder)
    val over = carry.over?.folder?.id == folder.id
    DisposableEffect(carry, folder.id) { onDispose { carry.places.remove(folder.id) } }
    if (over && onSpring != null) {
        val spring by rememberUpdatedState(onSpring)
        LaunchedEffect(folder.id) {
            delay(SPRING_MS)
            spring()
        }
    }
    Box(
        Modifier
            .onGloballyPositioned { carry.places[folder.id] = Place(it.boundsInRoot(), folder, title) }
            .graphicsLayer { alpha = if (carrying && !takes) 0.4f else 1f }
            .background(if (over) colors.primary.copy(alpha = 0.14f) else Color.Transparent, RoundedCornerShape(Radius.medium)),
    ) { content() }
}

/** The carried mail under the finger: how much of it, or where it goes when let go there. */
@Composable
fun CarriedMail(carry: Carry) {
    val rows = carry.messages ?: return
    val colors = MaterialTheme.colorScheme
    var origin by remember { mutableStateOf(Offset.Zero) }
    Box(Modifier.fillMaxSize().onGloballyPositioned { origin = it.positionInRoot() }) {
        val place = carry.over
        Text(
            if (place != null) stringResource(R.string.move_to_named, place.title) else stringResource(R.string.move_messages, rows.size),
            color = if (place != null) colors.onAccent else colors.onSurface,
            style = Type.Text,
            fontWeight = Weight.SemiBold,
            maxLines = 1,
            modifier = Modifier
                .offset { (carry.at - origin).round() + IntOffset(16.dp.roundToPx(), -40.dp.roundToPx()) }
                .shadow(6.dp, CircleShape)
                .background(if (place != null) colors.accent else colors.surfaceContainer, CircleShape)
                .padding(horizontal = 14.dp, vertical = 8.dp),
        )
    }
}

package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.FolderItem
import uniffi.ozero_api.FolderRole
import uniffi.ozero_api.Mailbox

/** What the accounts sheet does, besides going to a mailbox. */
class AccountsActions(
    val onDrafts: () -> Unit,
    val onMailbox: (Mailbox) -> Unit,
    val onAllBoxes: (AllBoxes) -> Unit,
    val onInAll: (AccountItem, Boolean) -> Unit,
    val onOrder: (List<AccountItem>) -> Unit,
    val onPin: (FolderItem, Boolean) -> Unit,
    val onGroups: (Groups) -> Unit,
    val onAdd: () -> Unit,
    val onSignIn: (AccountItem) -> Unit,
    /** Opens an account's settings. */
    val onAccount: (AccountItem) -> Unit,
    val onSettings: () -> Unit,
)

/** What the mailboxes list shows; equal while nothing in it changed, so the list is not drawn again. */
@Immutable
data class MailboxesState(
    val accounts: List<AccountItem>,
    val folders: List<FolderItem>,
    val mailbox: Mailbox,
    val drafts: Int,
    /** Unread People mail of each account's inbox, by account id. */
    val unread: Map<Long, Int>,
    val allBoxes: AllBoxes,
    val groups: Groups,
)

/**
 * The mailboxes, for going places: the inboxes, all accounts' and each one's, the mailboxes of
 * all accounts together, then each account's folders. At the bottom, the settings and a way to
 * add an account. Holding an account opens its settings; [holdTip] says so until it was done.
 */
@Composable
fun AccountsSheet(
    visible: Boolean,
    state: MailboxesState,
    actions: AccountsActions,
    holdTip: Boolean,
    onClose: () -> Unit,
) {
    Sheet(visible, onClose) {
        Column {
            SheetTitle(stringResource(R.string.folders_title))
            MailboxList(state, actions)
            SheetFoot(onSettings = actions.onSettings, onAdd = actions.onAdd)
            if (holdTip && state.accounts.size > 1) {
                Text(
                    stringResource(R.string.account_settings_hint),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = Type.Caption,
                    modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 10.dp),
                )
            }
        }
    }
}

/** The mailboxes down the side of a wide screen: the sheet's list of them, always there. */
@Composable
fun Mailboxes(
    state: MailboxesState,
    actions: AccountsActions,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    Column(
        modifier
            .background(colors.bar)
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Vertical))
            .verticalScroll(rememberScrollState())
            .padding(vertical = 8.dp),
    ) {
        MailboxList(state, actions)
        HorizontalDivider(Modifier.padding(vertical = 6.dp), color = colors.outlineVariant)
        SheetAction(R.drawable.ic_settings, stringResource(R.string.settings_title), colors.onSurface, actions.onSettings)
    }
}

/**
 * The drafts, if any, then the inboxes, the mailboxes of all accounts and each account's
 * folders. With one account, its inbox and folders. An account, when held, opens its
 * settings.
 */
@Composable
private fun MailboxList(state: MailboxesState, actions: AccountsActions) {
    val accounts = state.accounts
    val mailbox = state.mailbox
    val inboxes = state.folders.filter { it.role == FolderRole.INBOX }.associateBy { it.accountId }
    if (state.drafts > 0) {
        FolderRow(R.drawable.ic_compose, stringResource(R.string.folder_drafts), open = false, count = state.drafts, onClick = actions.onDrafts)
    }
    val one = accounts.singleOrNull()
    if (one != null) {
        FolderRow(R.drawable.ic_inbox, stringResource(R.string.inbox_title), mailbox == Mail.INBOX, count = state.unread[one.id] ?: 0) {
            actions.onMailbox(Mail.INBOX)
        }
        SignInAgain(one, actions)
        AccountFolders(one, state, actions)
        return
    }
    // Edit turns the inboxes into switches for All inboxes with handles to move them, and the
    // mailboxes of all accounts likewise.
    var editing by rememberSaveable { mutableStateOf(false) }
    val groups = state.groups
    val colors = MaterialTheme.colorScheme
    val open = editing || groups.open(INBOXES)
    Caption(
        stringResource(R.string.inboxes_title),
        open = open,
        onToggle = { actions.onGroups(groups.toggle(INBOXES)) }.takeIf { !editing },
    ) {
        TextButton(onClick = { editing = !editing }) {
            Text(stringResource(if (editing) R.string.action_done else R.string.action_edit), style = Type.Text, fontWeight = Weight.SemiBold)
        }
    }
    Folding(open) {
        val inAll = accounts.filter { it.inAll }
        FolderRow(
            lead = { StackedDots(inAll) },
            title = stringResource(R.string.folder_all_inbox),
            open = mailbox == Mail.INBOX,
            count = inAll.sumOf { state.unread[it.id] ?: 0 },
        ) { actions.onMailbox(Mail.INBOX) }
        if (editing) {
            Reorderable(accounts, key = { it.id }, onMove = { if (it != accounts) actions.onOrder(it) }) { account, handle ->
                // The unified inbox keeps one account at least.
                val last = account.inAll && inAll.size == 1
                SheetRow(onClick = { if (!last) actions.onInAll(account, !account.inAll) }, lead = { AccountDot(account.tint, size = 12.dp) }) {
                    Text(account.name, style = Type.Row, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    SmallSwitch(account.inAll, enabled = !last)
                    Handle(handle)
                }
            }
            Text(
                stringResource(R.string.inboxes_edit_hint),
                color = colors.onSurfaceVariant,
                style = Type.Caption,
                modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, bottom = 8.dp),
            )
        } else {
            accounts.forEach { account ->
                val box = inboxes[account.id]?.let { Mailbox.Folder(it.id) }
                FolderRow(
                    lead = { AccountDot(account.tint, size = 12.dp) },
                    title = account.name,
                    open = box != null && box == mailbox,
                    count = state.unread[account.id] ?: 0,
                    onLongClick = { actions.onAccount(account) },
                    longClickLabel = stringResource(R.string.account_settings),
                ) { if (box != null) actions.onMailbox(box) }
                SignInAgain(account, actions)
            }
        }
    }
    AllAccounts(state, actions, editing)
    Caption(stringResource(R.string.account_folders), open = groups.open(FOLDERS), onToggle = { actions.onGroups(groups.toggle(FOLDERS)) })
    Folding(groups.open(FOLDERS)) {
        accounts.forEach { account ->
            Group(
                account.name,
                groups.open(account.id),
                lead = { AccountDot(account.tint, size = 12.dp) },
                onToggle = { actions.onGroups(groups.toggle(account.id)) },
                onLongClick = { actions.onAccount(account) },
            ) { AccountFolders(account, state, actions) }
        }
    }
}

/** The parts of the list the person can close. */
private const val INBOXES = "inboxes"
private const val ALL_BOXES = "all"
private const val FOLDERS = "folders"

/** Under an account the server signed out, a way to sign in again. */
@Composable
private fun SignInAgain(account: AccountItem, actions: AccountsActions) {
    if (!account.needsSignIn) return
    SheetAction(R.drawable.ic_next, stringResource(R.string.signin_again_title), MaterialTheme.colorScheme.error) { actions.onSignIn(account) }
}

/**
 * The mailboxes of all accounts together the person chose, in their order; while [editing],
 * all of them as switches with handles to move them.
 */
@Composable
private fun AllAccounts(state: MailboxesState, actions: AccountsActions, editing: Boolean) {
    val boxes = state.allBoxes
    val groups = state.groups
    val colors = MaterialTheme.colorScheme
    val open = editing || groups.open(ALL_BOXES)
    Caption(stringResource(R.string.inbox_all_accounts), open = open, onToggle = { actions.onGroups(groups.toggle(ALL_BOXES)) }.takeIf { !editing })
    Folding(open) {
        if (!editing) {
            boxes.order.filter { it in boxes.shown && it.listed(state.folders) }.forEach { box ->
                FolderRow(box.icon, stringResource(box.title), box.mailbox == state.mailbox) { actions.onMailbox(box.mailbox) }
            }
        } else {
            Reorderable(boxes.order, key = { it }, onMove = { actions.onAllBoxes(boxes.copy(order = it)) }) { box, handle ->
                val on = box in boxes.shown
                SheetRow(
                    onClick = { actions.onAllBoxes(boxes.copy(shown = if (on) boxes.shown - box else boxes.shown + box)) },
                    lead = { Glyph(box.icon, null, colors.primary) },
                ) {
                    Text(stringResource(box.title), style = Type.Row, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    SmallSwitch(on)
                    Handle(handle)
                }
            }
        }
    }
}

/** What a row is dragged by to move it. */
@Composable
private fun Handle(modifier: Modifier) {
    Box(modifier.padding(start = 12.dp).size(40.dp), contentAlignment = Alignment.Center) {
        Glyph(R.drawable.ic_drag, stringResource(R.string.action_move), MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/**
 * An account's folders: its system ones in the usual order, then its own. Of those, the pinned
 * ones show, or all while the account has few and none pinned; Other folders shows the rest,
 * each inside the one it is in. Holding one of its own pins or unpins it.
 */
@Composable
private fun AccountFolders(account: AccountItem, state: MailboxesState, actions: AccountsActions) {
    val colors = MaterialTheme.colorScheme
    val haptics = LocalHapticFeedback.current
    val mine = state.folders.filter { it.accountId == account.id && it.role != FolderRole.DRAFTS }
    mine.filter { it.role != FolderRole.CUSTOM && it.role != FolderRole.INBOX }.sortedBy { it.role.rank() }.forEach { folder ->
        val box = Mailbox.Folder(folder.id)
        FolderRow(folder.role.icon(), folder.title(), box == state.mailbox) { actions.onMailbox(box) }
    }
    val own = mine.filter { it.role == FolderRole.CUSTOM }
    if (own.isEmpty()) return
    var all by rememberSaveable(account.id) { mutableStateOf(false) }
    val few = own.none { it.pinned } && own.size <= FEW_FOLDERS
    val shown = treeOrder(if (all || few) own else own.filter { it.pinned })
    val ids = shown.map { it.id }.toSet()
    shown.forEach { folder ->
        val box = Mailbox.Folder(folder.id)
        // Inside a folder that shows too, it goes under it by its own name.
        val nested = folder.parent in ids
        FolderRow(
            lead = { Glyph(if (folder.pinned && (all || few)) R.drawable.ic_pin else R.drawable.ic_folder, null, colors.primary) },
            title = folder.shownName(ids),
            open = box == state.mailbox,
            indent = nested,
            onLongClick = { actions.onPin(folder, !folder.pinned) },
            longClickLabel = stringResource(if (folder.pinned) R.string.action_unpin else R.string.action_pin),
        ) { actions.onMailbox(box) }
    }
    if (few) return
    SheetRow(
        onClick = {
            haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
            all = !all
        },
        lead = { Glyph(if (all) R.drawable.ic_expand else R.drawable.ic_more, null, colors.onSurfaceVariant) },
    ) {
        Text(
            stringResource(if (all) R.string.folders_pinned_only else R.string.folders_other),
            color = colors.onSurfaceVariant,
            style = Type.Row,
            modifier = Modifier.weight(1f),
        )
        if (!all) Text((own.size - shown.size).toString(), color = colors.onSurfaceVariant, style = Type.Caption)
    }
    if (all) {
        Text(
            stringResource(R.string.folders_pin_hint),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            modifier = Modifier.padding(start = Gap.sheet + Gap.slot + Gap.afterSlot, end = Gap.sheet, bottom = 8.dp),
        )
    }
}

/** A folder a message can move to, named and placed as the list shows it. */
data class MoveTarget(val folder: FolderItem, val title: String, val indent: Boolean)

/**
 * Where a message can move, from [targets]: the account's inbox and archive, then its own
 * folders in tree order.
 */
@Composable
fun moveOrder(targets: List<FolderItem>): List<MoveTarget> {
    val (usual, own) = targets.partition { it.role != FolderRole.CUSTOM }
    val ids = targets.map { it.id }.toSet()
    val ownIds = own.map { it.id }.toSet()
    return usual.sortedBy { it.role.rank() }.map { MoveTarget(it, it.title(), indent = false) } +
        treeOrder(own).map { MoveTarget(it, it.shownName(ids), indent = it.parent in ownIds) }
}

/** The folders [moveOrder] lists, to pick one; without any, says so. */
@Composable
fun MoveTargets(targets: List<FolderItem>, onPick: (FolderItem, String) -> Unit) {
    val colors = MaterialTheme.colorScheme
    Caption(stringResource(R.string.move_title))
    if (targets.isEmpty()) {
        Text(
            stringResource(R.string.move_none),
            color = colors.onSurfaceVariant,
            style = Type.Row,
            modifier = Modifier.padding(horizontal = Gap.sheet, vertical = 12.dp),
        )
        return
    }
    moveOrder(targets).forEach { (folder, title, indent) ->
        FolderRow(lead = { Glyph(folder.role.icon(), null, colors.primary) }, title = title, open = false, indent = indent) { onPick(folder, title) }
    }
}

/**
 * [folders] in tree order: each folder with the ones inside it right under it, by the names
 * they show with.
 */
private fun treeOrder(folders: List<FolderItem>): List<FolderItem> {
    val ids = folders.map { it.id }.toSet()
    val inside = folders.groupBy { folder -> folder.parent?.takeIf { it in ids } }
    fun under(parent: Long?): List<FolderItem> =
        inside[parent].orEmpty().sortedBy { it.shownName(ids).lowercase() }.flatMap { listOf(it) + under(it.id) }
    return under(null)
}

/** Inside a folder that shows too, among [ids], or at the top, its own name; else the whole path. */
private fun FolderItem.shownName(ids: Set<Long>): String = if (parent == null || parent in ids) shortName else name

/** An account with no more own folders than this and none pinned shows them all. */
private const val FEW_FOLDERS = 5

/**
 * A small heading over a part of the list, with [action] at its end. With [onToggle] a tap on
 * it hides or shows the part, and a chevron after it says which; with [open] but no [onToggle]
 * the chevron stays, pointing down.
 */
@Composable
private fun Caption(text: String, open: Boolean? = null, onToggle: (() -> Unit)? = null, action: (@Composable () -> Unit)? = null) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .height(40.dp)
            .then(
                if (onToggle == null) Modifier
                else Modifier.clickable(onClickLabel = stringResource(if (open == true) R.string.action_hide else R.string.action_show), onClick = onToggle),
            )
            .padding(start = Gap.sheet, end = Gap.sheet - 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
            Text(text, color = colors.onSurfaceVariant, style = Type.Label)
            if (open != null) Chevron(open, 14.dp)
        }
        action?.invoke()
    }
}

/** After a heading: points down while what it heads shows, to the side while it is hidden. */
@Composable
private fun Chevron(open: Boolean, size: Dp) {
    val turn by animateFloatAsState(if (open) 90f else 0f, Motion.calm(Motion.SMALL), label = "chevron")
    Glyph(R.drawable.ic_next, null, MaterialTheme.colorScheme.onSurfaceVariant, Modifier.padding(start = 4.dp).graphicsLayer { rotationZ = turn }, size = size)
}

/** A part of the list under a [Caption], hiding and showing as calmly as an account's folders. */
@Composable
private fun Folding(open: Boolean, content: @Composable () -> Unit) {
    AnimatedVisibility(
        open,
        enter = expandVertically(Motion.calm(Motion.NOTE)) + fadeIn(Motion.calm(Motion.NOTE)),
        exit = shrinkVertically(Motion.calm(Motion.NOTE)) + fadeOut(Motion.calm(Motion.SMALL)),
    ) {
        Column { content() }
    }
}

/** At the bottom of the sheet: the settings, and a way to add an account. */
@Composable
private fun SheetFoot(onSettings: () -> Unit, onAdd: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(start = Gap.sheet - 6.dp, end = Gap.sheet - 6.dp, top = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        FootButton(R.drawable.ic_settings, stringResource(R.string.settings_title), onSettings, Modifier.weight(1f))
        FootButton(R.drawable.ic_add, stringResource(R.string.account_add_short), onAdd, Modifier.weight(1f))
    }
}

@Composable
private fun FootButton(@DrawableRes icon: Int, text: String, onClick: () -> Unit, modifier: Modifier) {
    val colors = MaterialTheme.colorScheme
    Row(
        modifier
            .height(48.dp)
            .clip(RoundedCornerShape(Radius.medium))
            .background(colors.surfaceVariant)
            .clickable(role = Role.Button, onClick = onClick),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Glyph(icon, null, colors.onSurface, size = 20.dp)
        Text(text, style = Type.Body, fontWeight = Weight.Medium, modifier = Modifier.padding(start = 8.dp))
    }
}

/**
 * A mailbox of all accounts the sheet can show under the inboxes; [SHOWN] are the ones it
 * shows until the person picks.
 */
enum class AllBox(val mailbox: Mailbox, @StringRes val title: Int) {
    UNREAD(Mailbox.Unread, R.string.folder_unread),
    SNOOZED(Mailbox.Unified(FolderRole.SNOOZED), R.string.folder_all_snoozed),
    FLAGGED(Mailbox.Unified(FolderRole.FLAGGED), R.string.folder_all_flagged),
    SENT(Mailbox.Unified(FolderRole.SENT), R.string.folder_all_sent),
    ARCHIVE(Mailbox.Unified(FolderRole.ARCHIVE), R.string.folder_all_archive),
    FILES(Mailbox.Files, R.string.folder_files),
    JUNK(Mailbox.Unified(FolderRole.JUNK), R.string.folder_all_junk),
    TRASH(Mailbox.Unified(FolderRole.TRASH), R.string.folder_all_trash),
    ;

    @get:DrawableRes
    val icon: Int
        get() = when (val box = mailbox) {
            is Mailbox.Unified -> box.role.icon()
            Mailbox.Unread -> R.drawable.ic_unread
            else -> R.drawable.ic_attach
        }

    /** Shown in the list: a role's mailbox once some account has such a folder. */
    fun listed(folders: List<FolderItem>): Boolean = when (val box = mailbox) {
        is Mailbox.Unified -> folders.any { it.role == box.role }
        else -> true
    }

    companion object {
        val SHOWN = setOf(UNREAD, SNOOZED, FLAGGED, SENT, ARCHIVE)
    }
}

/** The mailboxes of all accounts in the person's order, and which of them show. */
data class AllBoxes(val order: List<AllBox>, val shown: Set<AllBox>)

/** System folders first in the usual order, then the account's own. */
private fun FolderRole.rank(): Int = when (this) {
    FolderRole.INBOX -> 0
    FolderRole.SNOOZED -> 1
    FolderRole.FLAGGED -> 2
    FolderRole.SENT -> 3
    FolderRole.DRAFTS -> 4
    FolderRole.ARCHIVE -> 5
    FolderRole.JUNK -> 6
    FolderRole.TRASH -> 7
    FolderRole.CUSTOM -> 8
}

@DrawableRes
fun FolderRole.icon(): Int = when (this) {
    FolderRole.INBOX -> R.drawable.ic_inbox
    FolderRole.SNOOZED -> R.drawable.ic_snooze
    FolderRole.FLAGGED -> R.drawable.ic_flag
    FolderRole.SENT -> R.drawable.ic_send
    FolderRole.DRAFTS -> R.drawable.ic_compose
    FolderRole.ARCHIVE -> R.drawable.ic_archive
    FolderRole.JUNK -> R.drawable.ic_spam
    FolderRole.TRASH -> R.drawable.ic_delete
    FolderRole.CUSTOM -> R.drawable.ic_folder
}

/** Ours for a system folder; [name], the server's, for the account's own. */
@Composable
fun FolderRole.title(name: String): String = when (this) {
    FolderRole.INBOX -> stringResource(R.string.inbox_title)
    FolderRole.SNOOZED -> stringResource(R.string.folder_snoozed)
    FolderRole.FLAGGED -> stringResource(R.string.folder_flagged)
    FolderRole.SENT -> stringResource(R.string.folder_sent)
    FolderRole.DRAFTS -> stringResource(R.string.folder_drafts)
    FolderRole.ARCHIVE -> stringResource(R.string.folder_archive)
    FolderRole.JUNK -> stringResource(R.string.folder_junk)
    FolderRole.TRASH -> stringResource(R.string.folder_trash)
    FolderRole.CUSTOM -> name
}

@Composable
fun FolderItem.title(): String = role.title(name)

/**
 * A group's name, which hides or shows its folders; held, it does [onLongClick].
 */
@Composable
private fun Group(
    text: String,
    open: Boolean,
    lead: @Composable () -> Unit,
    onToggle: () -> Unit,
    onLongClick: (() -> Unit)?,
    folders: @Composable () -> Unit,
) {
    val haptics = LocalHapticFeedback.current
    Row(
        Modifier
            .fillMaxWidth()
            .holdable(
                haptics,
                onToggle,
                onLongClick,
                onClickLabel = stringResource(if (open) R.string.action_collapse else R.string.action_expand),
                onLongClickLabel = stringResource(R.string.account_settings),
            )
            .padding(horizontal = Gap.sheet, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(Gap.slot), contentAlignment = Alignment.Center) { lead() }
        Row(Modifier.weight(1f).padding(start = Gap.afterSlot), verticalAlignment = Alignment.CenterVertically) {
            Text(
                text,
                style = Type.Row,
                fontWeight = Weight.Strong,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            Chevron(open, 16.dp)
        }
    }
    Folding(open, folders)
}

@Composable
private fun FolderRow(@DrawableRes icon: Int, title: String, open: Boolean, count: Int = 0, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    FolderRow(lead = { Glyph(icon, null, colors.primary) }, title = title, open = open, count = count, onClick = onClick)
}

/**
 * A mailbox: the open one in the accent colour, with its unread [count] at the end, none for 0.
 * [indent] puts it under the folder it is inside of.
 */
@Composable
private fun FolderRow(
    lead: @Composable () -> Unit,
    title: String,
    open: Boolean,
    count: Int = 0,
    indent: Boolean = false,
    onLongClick: (() -> Unit)? = null,
    longClickLabel: String? = null,
    onClick: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    SheetRow(onClick, lead = { if (indent) Box(Modifier.size(Gap.slot)) else lead() }, onLongClick = onLongClick, longClickLabel = longClickLabel) {
        if (indent) {
            Box(Modifier.padding(end = Gap.afterSlot).size(Gap.slot), contentAlignment = Alignment.Center) { lead() }
        }
        Text(
            title,
            color = if (open) colors.primary else colors.onSurface,
            style = Type.Row,
            fontWeight = if (open) Weight.SemiBold else Weight.Regular,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (count > 0) Text(count.toString(), color = colors.primary, style = Type.Caption, fontWeight = Weight.SemiBold)
    }
}

/** Up to three account colours overlapping. */
@Composable
private fun StackedDots(accounts: List<AccountItem>) {
    val ring = MaterialTheme.colorScheme.background
    Box(Modifier.size(Gap.slot, 11.dp)) {
        accounts.take(3).forEachIndexed { i, account ->
            AccountDot(account.tint, Modifier.offset(x = 6.5.dp * i).border(1.5.dp, ring, CircleShape), size = 11.dp)
        }
    }
}

/** A row with an icon and a coloured label, as at the end of a sheet. */
@Composable
fun SheetAction(@DrawableRes icon: Int, text: String, color: Color, onClick: () -> Unit) {
    SheetRow(onClick, lead = { Glyph(icon, null, color) }) {
        Text(text, color = color, style = Type.Row)
    }
}

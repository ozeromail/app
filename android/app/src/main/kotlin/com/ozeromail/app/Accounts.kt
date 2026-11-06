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
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
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
    /** What holding one of the account's own folders offers. */
    val onFolderMenu: (FolderItem) -> Unit,
    /** Opens a folder's card, from Edit. */
    val onFolder: (FolderItem) -> Unit,
    val onNewFolder: (AccountItem) -> Unit,
    /** The pinned folders of one account in their new order. */
    val onFolderOrder: (List<FolderItem>) -> Unit,
    val onGroups: (Groups) -> Unit,
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
 * The mailboxes, for going places: all inboxes, then each account's inbox with its folders
 * under it, then the mailboxes of all accounts together. At the bottom, the settings. Edit, beside
 * the title as it changes the whole list, turns the rows into switches to move; an account's
 * settings open from its info button there, or by holding it.
 */
@Composable
fun AccountsSheet(
    visible: Boolean,
    lift: Lift,
    state: MailboxesState,
    actions: AccountsActions,
    onClose: () -> Unit,
) {
    var editing by rememberSaveable { mutableStateOf(false) }
    Sheet(visible, onClose, lift) {
        Column {
            Row(Modifier.fillMaxWidth().padding(end = Gap.sheet - 12.dp), verticalAlignment = Alignment.CenterVertically) {
                SheetTitle(stringResource(R.string.folders_title), Modifier.weight(1f).padding(start = Gap.sheet, end = Gap.sheet, bottom = 10.dp))
                // With one account and no folders to change there is nothing to edit.
                if (state.editable()) EditButton(editing, Modifier.padding(bottom = 10.dp)) { editing = !editing }
            }
            MailboxList(state, actions, editing, onEdit = null, carry = null)
            SettingsRow(actions.onSettings)
        }
    }
}

/** Something to edit: accounts to switch and move, or folders to make, change or order. */
private fun MailboxesState.editable(): Boolean =
    accounts.size > 1 || accounts.any { it.editsFolders } || folders.count { it.pinned } > 1

/** Edit, or Done while editing. */
@Composable
private fun EditButton(editing: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Text(
        stringResource(if (editing) R.string.action_done else R.string.action_edit),
        color = MaterialTheme.colorScheme.primary,
        style = Type.Label,
        fontWeight = Weight.SemiBold,
        modifier = modifier
            .clip(RoundedCornerShape(Radius.medium))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    )
}

/**
 * The mailboxes down the side of a wide screen: the sheet's list of them, always there. With
 * [carry], mail held in the list can be dropped on a folder.
 */
@Composable
fun Mailboxes(
    state: MailboxesState,
    actions: AccountsActions,
    carry: Carry?,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    var editing by rememberSaveable { mutableStateOf(false) }
    Column(
        modifier
            .background(colors.bar)
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Vertical))
            .verticalScroll(rememberScrollState())
            .padding(vertical = 8.dp),
    ) {
        // No title over the side: Edit stays in the inboxes' caption.
        MailboxList(state, actions, editing, onEdit = { editing = !editing }, carry = carry)
        SettingsRow(actions.onSettings)
    }
}

/** After the mailboxes, below a line: the settings, as quiet as a mailbox. */
@Composable
private fun SettingsRow(onSettings: () -> Unit) {
    HorizontalDivider(Modifier.padding(top = 8.dp, bottom = 4.dp), color = MaterialTheme.colorScheme.outlineVariant)
    FolderRow(R.drawable.ic_settings, stringResource(R.string.settings_title), open = false, onClick = onSettings)
}

/**
 * The drafts, if any, then all inboxes, which always shows, the inboxes of the accounts, each
 * with its folders a tap on its arrow away, and the mailboxes of all accounts. With one
 * account, its inbox and folders. An account, when held, opens its settings.
 */
@Composable
private fun MailboxList(state: MailboxesState, actions: AccountsActions, editing: Boolean, onEdit: (() -> Unit)?, carry: Carry?) {
    val accounts = state.accounts
    val mailbox = state.mailbox
    val inboxes = state.folders.filter { it.role == FolderRole.INBOX }.associateBy { it.accountId }
    if (state.drafts > 0) {
        FolderRow(R.drawable.ic_compose, stringResource(R.string.folder_drafts), open = false, count = state.drafts, onClick = actions.onDrafts)
    }
    val one = accounts.singleOrNull()
    if (one != null) {
        DropPlace(carry, inboxes[one.id], stringResource(R.string.inbox_title)) {
            FolderRow(R.drawable.ic_inbox, stringResource(R.string.inbox_title), mailbox == Mail.INBOX, count = state.unread[one.id] ?: 0) {
                actions.onMailbox(Mail.INBOX)
            }
        }
        SignInAgain(one, actions)
        // Beside the list there is no title for Edit to go by.
        if (onEdit != null && state.editable()) Caption(stringResource(R.string.folders_accounts), action = { EditButton(editing, onClick = onEdit) })
        AccountFolders(one, state, actions, editing = editing, carry = carry)
        return
    }
    val inAll = accounts.filter { it.inAll }
    FolderRow(
        lead = { StackedDots(inAll) },
        title = stringResource(R.string.folder_all_inbox),
        open = mailbox == Mail.INBOX,
        count = inAll.sumOf { state.unread[it.id] ?: 0 },
    ) { actions.onMailbox(Mail.INBOX) }
    // Edit turns the inboxes into switches for All inboxes with handles to move them, and the
    // mailboxes of all accounts likewise.
    val groups = state.groups
    val colors = MaterialTheme.colorScheme
    val open = editing || groups.open(INBOXES)
    Caption(
        stringResource(R.string.inboxes_title),
        open = open,
        onToggle = { actions.onGroups(groups.toggle(INBOXES)) }.takeIf { !editing },
        action = onEdit?.let { edit -> { EditButton(editing, onClick = edit) } },
    )
    Folding(open) {
        if (editing) {
            Reorderable(accounts, key = { it.id }, onMove = { if (it != accounts) actions.onOrder(it) }) { account, handle ->
                // The unified inbox keeps one account at least.
                val last = account.inAllFixed
                SheetRow(onClick = { if (!last) actions.onInAll(account, !account.inAll) }, lead = { AccountDot(account.tint) }, fixed = true) {
                    Text(account.name, style = Type.Row, fontWeight = Weight.Regular, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    // Its settings, in sight: holding the row is not something to find on one's own.
                    Box(
                        Modifier
                            .size(END_BUTTON)
                            .clip(CircleShape)
                            .clickable(onClickLabel = stringResource(R.string.account_settings), role = Role.Button) { actions.onAccount(account) },
                        contentAlignment = Alignment.Center,
                    ) { Glyph(R.drawable.ic_settings, stringResource(R.string.account_settings), colors.onSurfaceVariant, size = ICON) }
                    SmallSwitch(account.inAll, enabled = !last)
                    Handle(handle)
                }
            }
            Text(
                stringResource(R.string.inboxes_edit_hint),
                color = colors.onSurfaceVariant,
                style = Type.Caption,
                modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 4.dp, bottom = 8.dp),
            )
            // Then each account's folders, to order, change and add to.
            accounts.filter { account -> account.editsFolders || state.folders.any { it.accountId == account.id && it.pinned } }.forEach { account ->
                Caption(account.name)
                AccountFolders(account, state, actions, editing = true, carry = null)
            }
        } else {
            accounts.forEach { account ->
                val inbox = inboxes[account.id]
                val box = inbox?.let { Mailbox.Folder(it.id) }
                val shown = groups.open(account.id)
                // Mail held over a closed account opens its folders.
                DropPlace(carry, inbox, account.name, onSpring = { actions.onGroups(groups.toggle(account.id)) }.takeIf { !shown }) {
                    FolderRow(
                        lead = { AccountDot(account.tint) },
                        title = account.name,
                        open = box != null && box == mailbox,
                        count = state.unread[account.id] ?: 0,
                        end = { FoldersArrow(account.name, shown) { actions.onGroups(groups.toggle(account.id)) } },
                        onLongClick = { actions.onAccount(account) },
                        longClickLabel = stringResource(R.string.account_settings),
                    ) { if (box != null) actions.onMailbox(box) }
                }
                SignInAgain(account, actions)
                Folding(shown) { AccountFolders(account, state, actions, depth = 1, carry = carry) }
            }
        }
    }
    AllAccounts(state, actions, editing)
}

/** The parts of the list the person can close. */
private const val INBOXES = "inboxes"
private const val ALL_BOXES = "all"

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
                    lead = { RowIcon(box.icon, open = false) },
                    fixed = true,
                ) {
                    Text(stringResource(box.title), style = Type.Row, fontWeight = Weight.Regular, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                    SmallSwitch(on)
                    Handle(handle)
                }
            }
            // These switches hide a row; the inboxes' leave an account out of All inboxes.
            Text(
                stringResource(R.string.all_boxes_edit_hint),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = Type.Caption,
                modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 4.dp, bottom = 8.dp),
            )
        }
    }
}

/** What a row is dragged by to move it. */
@Composable
internal fun Handle(modifier: Modifier) {
    Box(modifier.padding(start = 4.dp).size(END_BUTTON), contentAlignment = Alignment.Center) {
        Glyph(R.drawable.ic_drag, stringResource(R.string.action_move), MaterialTheme.colorScheme.onSurfaceVariant, size = ICON)
    }
}

/** At the end of an account's row: shows or hides its folders under it. */
@Composable
private fun FoldersArrow(account: String, open: Boolean, onToggle: () -> Unit) {
    val folders = stringResource(R.string.account_folders_of, account)
    Box(
        Modifier
            .padding(start = 4.dp)
            .minimumInteractiveComponentSize()
            .clip(CircleShape)
            .clickable(onClickLabel = stringResource(if (open) R.string.action_collapse else R.string.action_expand), role = Role.Button, onClick = onToggle)
            .semantics { contentDescription = folders },
        contentAlignment = Alignment.Center,
    ) { Chevron(open, 18.dp) }
}

/**
 * An account's folders, [depth] steps in: its system ones in the usual order, then its own. Of
 * those, the pinned ones show, or all while the account has few and none pinned, or while mail
 * is carried; Other folders shows the rest, each inside the one it is in, unless the person
 * closed that one. Holding one of its own offers what can be done with it, pinning first.
 * While [editing], its own folders instead: the pinned ones to order by their handles, a tap
 * opening a folder's card, and a way to add one.
 */
@Composable
private fun AccountFolders(
    account: AccountItem,
    state: MailboxesState,
    actions: AccountsActions,
    depth: Int = 0,
    editing: Boolean = false,
    carry: Carry?,
) {
    val colors = MaterialTheme.colorScheme
    val haptics = LocalHapticFeedback.current
    val mine = state.folders.filter { it.accountId == account.id && it.role != FolderRole.DRAFTS }
    val own = mine.filter { it.role == FolderRole.CUSTOM }
    if (editing) {
        EditFolders(account, own, actions, depth)
        return
    }
    mine.filter { it.role != FolderRole.CUSTOM && it.role != FolderRole.INBOX }.sortedBy { it.role.rank() }.forEach { folder ->
        val box = Mailbox.Folder(folder.id)
        val title = folder.title()
        DropPlace(carry, folder, title) {
            FolderRow(folder.role.icon(), title, box == state.mailbox, depth = depth) { actions.onMailbox(box) }
        }
    }
    if (own.isEmpty()) return
    var all by rememberSaveable(account.id) { mutableStateOf(false) }
    val few = own.none { it.pinned } && own.size <= FEW_FOLDERS
    // Carried mail can reach every folder.
    val every = all || few || carry?.messages != null
    val groups = state.groups
    folderTree(if (every) own else own.filter { it.pinned }, groups::showsInside, depth).forEach { row ->
        val folder = row.folder
        val box = Mailbox.Folder(folder.id)
        val open = box == state.mailbox
        val inside = groups.showsInside(folder.id)
        DropPlace(carry, folder, row.title, onSpring = { actions.onGroups(groups.toggleInside(folder.id)) }.takeIf { row.inside && !inside }) {
            FolderRow(
                lead = { RowIcon(if (folder.pinned && every) R.drawable.ic_pin else R.drawable.ic_folder, open) },
                title = row.title,
                open = open,
                depth = row.depth,
                end = if (row.inside) {
                    { Twisty(row.title, inside) { actions.onGroups(groups.toggleInside(folder.id)) } }
                } else {
                    null
                },
                onLongClick = { actions.onFolderMenu(folder) },
                longClickLabel = stringResource(R.string.folder_actions),
            ) { actions.onMailbox(box) }
        }
    }
    if (few) return
    SheetRow(
        onClick = {
            haptics.performHapticFeedback(HapticFeedbackType.TextHandleMove)
            all = !all
        },
        lead = { RowIcon(if (all) R.drawable.ic_expand else R.drawable.ic_more, open = false) },
        fixed = true,
        start = indent(depth),
    ) {
        Text(
            stringResource(if (all) R.string.folders_pinned_only else R.string.folders_other),
            color = colors.onSurfaceVariant,
            style = Type.Row,
            fontWeight = Weight.Regular,
            modifier = Modifier.weight(1f),
        )
        if (!all) Text(own.count { !it.pinned }.toString(), color = colors.onSurfaceVariant, style = Type.Caption)
    }
    // Until the first folder is pinned.
    if (all && own.none { it.pinned }) {
        Text(
            stringResource(R.string.folders_pin_hint),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            modifier = Modifier.padding(start = Gap.sheet + indent(depth + 1), end = Gap.sheet, bottom = 8.dp),
        )
    }
}

/**
 * An account's own folders while editing: the pinned ones first, moved by their handles, then
 * the rest as a tree; a tap opens a folder's card. Then a new folder, where the account can
 * have one.
 */
@Composable
private fun EditFolders(account: AccountItem, own: List<FolderItem>, actions: AccountsActions, depth: Int) {
    val pinned = own.filter { it.pinned }.sortedWith(compareBy({ it.rank ?: UInt.MAX_VALUE }, { it.shortName.lowercase() }))
    Reorderable(pinned, key = { it.id }, onMove = { if (it != pinned) actions.onFolderOrder(it) }) { folder, handle ->
        SheetRow(onClick = { actions.onFolder(folder) }, lead = { RowIcon(R.drawable.ic_pin, open = false) }, fixed = true, start = indent(depth)) {
            Text(folder.shortName, style = Type.Row, fontWeight = Weight.Regular, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            if (pinned.size > 1) Handle(handle)
        }
    }
    folderTree(own.filter { !it.pinned }, depth = depth).forEach { row ->
        FolderRow(R.drawable.ic_folder, row.title, open = false, depth = row.depth) { actions.onFolder(row.folder) }
    }
    if (account.editsFolders) NewFolderRow(stringResource(R.string.folder_new), depth) { actions.onNewFolder(account) }
}

/** An account with no more own folders than this and none pinned shows them all. */
private const val FEW_FOLDERS = 5

/**
 * A small heading over a part of the list, with [action] at its end. With [onToggle] a tap on
 * it hides or shows the part, and a chevron after it says which; with [open] but no [onToggle]
 * the chevron stays, pointing down.
 */
@Composable
internal fun Caption(text: String, open: Boolean? = null, onToggle: (() -> Unit)? = null, action: (@Composable () -> Unit)? = null) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 36.dp)
            .then(
                if (onToggle == null) Modifier
                else Modifier.clickable(onClickLabel = stringResource(if (open == true) R.string.action_hide else R.string.action_show), onClick = onToggle),
            )
            .padding(start = Gap.sheet, end = Gap.sheet - 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
            Text(text, color = colors.onSurfaceVariant, style = Type.Label, fontWeight = Weight.SemiBold)
            if (open != null) {
                Spacer(Modifier.width(4.dp))
                Chevron(open, 14.dp)
            }
        }
        action?.invoke()
    }
}

/** After a heading: points down while what it heads shows, to the side while it is hidden. */
@Composable
internal fun Chevron(open: Boolean, size: Dp) {
    val turn by animateFloatAsState(if (open) 90f else 0f, Motion.calm(Motion.SMALL), label = "chevron")
    Glyph(R.drawable.ic_next, null, MaterialTheme.colorScheme.onSurfaceVariant, Modifier.graphicsLayer { rotationZ = turn }, size = size)
}

/** A part of the list under a [Caption], hiding and showing as calmly as an account's folders. */
@Composable
internal fun Folding(open: Boolean, content: @Composable () -> Unit) {
    AnimatedVisibility(
        open,
        enter = expandVertically(Motion.calm(Motion.NOTE)) + fadeIn(Motion.calm(Motion.NOTE)),
        exit = shrinkVertically(Motion.calm(Motion.NOTE)) + fadeOut(Motion.calm(Motion.SMALL)),
    ) {
        Column { content() }
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
internal fun FolderRole.rank(): Int = when (this) {
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

@Composable
private fun FolderRow(@DrawableRes icon: Int, title: String, open: Boolean, count: Int = 0, depth: Int = 0, onClick: () -> Unit) {
    FolderRow(lead = { RowIcon(icon, open) }, title = title, open = open, count = count, depth = depth, onClick = onClick)
}

/** A mailbox's icon: as quiet as the headings, so colour in the list means an account or the open one. */
@Composable
internal fun RowIcon(@DrawableRes icon: Int, open: Boolean) {
    val colors = MaterialTheme.colorScheme
    Glyph(icon, null, if (open) colors.primary else colors.onSurfaceVariant, size = ICON)
}

/**
 * A mailbox: the open one in the accent colour, with its unread [count] and then [end], if any,
 * at the end; no count for 0. [depth] puts it under the account or folder it is in.
 */
@Composable
private fun FolderRow(
    lead: @Composable () -> Unit,
    title: String,
    open: Boolean,
    count: Int = 0,
    depth: Int = 0,
    end: (@Composable () -> Unit)? = null,
    onLongClick: (() -> Unit)? = null,
    longClickLabel: String? = null,
    onClick: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    SheetRow(onClick, lead = lead, onLongClick = onLongClick, longClickLabel = longClickLabel, fixed = true, start = indent(depth)) {
        Text(
            title,
            color = if (open) colors.primary else colors.onSurface,
            style = Type.Row,
            fontWeight = if (open) Weight.SemiBold else Weight.Regular,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (count > 0) Text(count.toString(), color = if (open) colors.primary else colors.onSurfaceVariant, style = Type.Caption, fontWeight = Weight.Medium)
        end?.invoke()
    }
}

/** How far in a row [depth] steps down goes: one slot and its gap a step. */
internal fun indent(depth: Int): Dp = (Gap.slot + Gap.afterSlot) * depth

/** Up to three account colours overlapping, as wide as the slot. */
@Composable
private fun StackedDots(accounts: List<AccountItem>) {
    val ring = MaterialTheme.colorScheme.background
    Box(Modifier.size(Gap.slot, 10.dp)) {
        accounts.take(3).forEachIndexed { i, account ->
            AccountDot(account.tint, Modifier.offset(x = 7.dp * i).border(1.5.dp, ring, CircleShape))
        }
    }
}

/** Icons in the rows; dots stay at [AccountDot]'s size. */
internal val ICON = 20.dp

/** A handle at a row's end. */
private val END_BUTTON = 40.dp

/** A row with an icon and a coloured label, as at the end of a sheet. */
@Composable
fun SheetAction(@DrawableRes icon: Int, text: String, color: Color, onClick: () -> Unit) {
    SheetRow(onClick, lead = { Glyph(icon, null, color) }) {
        Text(text, color = color, style = Type.Row)
    }
}

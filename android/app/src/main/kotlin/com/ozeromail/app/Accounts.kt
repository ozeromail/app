package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
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
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.FolderItem
import uniffi.ozero_api.FolderRole
import uniffi.ozero_api.Mailbox
import uniffi.ozero_api.NewMail
import uniffi.ozero_api.accountColors

/** What the accounts sheet does, besides going to a mailbox. */
class AccountsActions(
    val onDrafts: () -> Unit,
    val onMailbox: (Mailbox) -> Unit,
    val onAllBoxes: (AllBoxes) -> Unit,
    val onInAll: (AccountItem, Boolean) -> Unit,
    val onOrder: (List<AccountItem>) -> Unit,
    val onPin: (FolderItem, Boolean) -> Unit,
    val onSwipes: (Swipes) -> Unit,
    val onGroups: (Groups) -> Unit,
    val onAdd: () -> Unit,
    val onColor: (AccountItem, Int) -> Unit,
    val onLabel: (AccountItem, String) -> Unit,
    val onSignature: (String) -> Unit,
    val onAccountSignature: (AccountItem, String) -> Unit,
    val onSignIn: (AccountItem) -> Unit,
    val onKeepDays: (AccountItem, Int) -> Unit,
    val onNewMail: (AccountItem, NewMail) -> Unit,
    val onRemove: (AccountItem) -> Unit,
    val onWrite: () -> Unit,
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
 * add an account. Holding an account opens its settings, in the same sheet, which grows or
 * shrinks to fit.
 */
@Composable
fun AccountsSheet(
    visible: Boolean,
    state: MailboxesState,
    swipes: Swipes,
    signature: String,
    actions: AccountsActions,
    /** The page the sheet opens on: an account's, as after adding it, or [SETTINGS]. */
    startOn: Long?,
    onClose: () -> Unit,
) {
    val accounts = state.accounts
    Sheet(visible, onClose) {
        // Inside the sheet, so it opens on the list again next time: an account's id, SETTINGS,
        // SWIPES or SIGNATURE.
        var page by rememberSaveable { mutableStateOf<Long?>(null) }
        // Where an account's page goes back to: the list or the settings.
        var from by rememberSaveable { mutableStateOf<Long?>(null) }
        LaunchedEffect(startOn) { if (startOn != null) page = startOn }
        AnimatedContent(
            page.takeIf { it == SETTINGS || it == SWIPES || it == SIGNATURE || accounts.any { a -> a.id == it } },
            transitionSpec = {
                (fadeIn(Motion.calm(Motion.SMALL)) togetherWith fadeOut(Motion.calm(Motion.SMALL)))
                    .using(SizeTransform { _, _ -> Motion.settle() })
            },
            label = "accounts",
        ) { shown ->
            Column {
                val account = accounts.find { it.id == shown }
                fun openAccount(it: AccountItem, back: Long?) {
                    from = back
                    page = it.id
                }
                when {
                    shown == SETTINGS -> AppSettings(
                        accounts,
                        swipes,
                        signature,
                        onBack = { page = null },
                        onAccount = { openAccount(it, SETTINGS) },
                        onOrder = actions.onOrder,
                        onSwipes = { page = SWIPES },
                        onSignature = { page = SIGNATURE },
                        onAdd = actions.onAdd,
                        onWrite = actions.onWrite,
                    )
                    shown == SWIPES -> {
                        SubPage(stringResource(R.string.swipes_title), onBack = { page = SETTINGS })
                        SwipeSettings(swipes, actions.onSwipes)
                    }
                    shown == SIGNATURE -> {
                        SubPage(stringResource(R.string.signature_title), onBack = { page = SETTINGS })
                        SignatureField(signature, stringResource(R.string.signature_title), "", stringResource(R.string.signature_hint), actions.onSignature)
                    }
                    account != null -> AccountSettings(
                        account,
                        onBack = { page = from },
                        // Not for the last account in the unified inbox, which keeps one at least.
                        inAll = account.inAll.takeIf { on -> accounts.size > 1 && !(on && accounts.count { it.inAll } == 1) },
                        onInAll = { actions.onInAll(account, it) },
                        onColor = { actions.onColor(account, it) },
                        onLabel = { actions.onLabel(account, it) },
                        signature = signature,
                        onSignature = { actions.onAccountSignature(account, it) },
                        onSignIn = { actions.onSignIn(account) },
                        onKeepDays = { actions.onKeepDays(account, it) },
                        onNewMail = { actions.onNewMail(account, it) },
                        onRemove = { actions.onRemove(account) },
                    )
                    else -> {
                        SheetTitle(stringResource(R.string.folders_title))
                        MailboxList(state, actions, onAccount = { openAccount(it, null) })
                        SheetFoot(onSettings = { page = SETTINGS }, onAdd = actions.onAdd)
                    }
                }
            }
        }
    }
}

/**
 * The mailboxes down the side of a wide screen: the sheet's list of them, always there. The
 * settings open in the sheet with [onSettings].
 */
@Composable
fun Mailboxes(
    state: MailboxesState,
    actions: AccountsActions,
    onSettings: () -> Unit,
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
        MailboxList(state, actions, onAccount = null)
        HorizontalDivider(Modifier.padding(vertical = 6.dp), color = colors.outlineVariant)
        SheetAction(R.drawable.ic_settings, stringResource(R.string.settings_title), colors.onSurface, onSettings)
    }
}

/** The pages of the sheet besides the list; account pages are named by their ids, which are positive. */
const val SETTINGS = -3L
private const val SWIPES = -1L
private const val SIGNATURE = -2L

/**
 * The drafts, if any, then the inboxes, the mailboxes of all accounts and each account's
 * folders. With one account, its inbox and folders. [onAccount], when held, opens an
 * account's settings; null leaves that out.
 */
@Composable
private fun MailboxList(state: MailboxesState, actions: AccountsActions, onAccount: ((AccountItem) -> Unit)?) {
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
    Caption(stringResource(R.string.inboxes_title))
    val inAll = accounts.filter { it.inAll }
    FolderRow(
        lead = { StackedDots(inAll) },
        title = stringResource(R.string.folder_all_inbox),
        open = mailbox == Mail.INBOX,
        count = inAll.sumOf { state.unread[it.id] ?: 0 },
    ) { actions.onMailbox(Mail.INBOX) }
    accounts.forEach { account ->
        val box = inboxes[account.id]?.let { Mailbox.Folder(it.id) }
        FolderRow(
            lead = { AccountDot(account.tint, size = 12.dp) },
            title = account.name,
            open = box != null && box == mailbox,
            count = state.unread[account.id] ?: 0,
            onLongClick = onAccount?.let { { it(account) } },
            longClickLabel = stringResource(R.string.account_settings),
        ) { if (box != null) actions.onMailbox(box) }
        SignInAgain(account, actions)
    }
    AllAccounts(state, actions)
    Caption(stringResource(R.string.account_folders))
    accounts.forEach { account ->
        Group(
            account.name,
            state.groups.open(account.id),
            lead = { AccountDot(account.tint, size = 12.dp) },
            onToggle = { actions.onGroups(state.groups.toggle(account.id)) },
            onLongClick = onAccount?.let { { it(account) } },
        ) { AccountFolders(account, state, actions) }
    }
}

/** Under an account the server signed out, a way to sign in again. */
@Composable
private fun SignInAgain(account: AccountItem, actions: AccountsActions) {
    if (!account.needsSignIn) return
    SheetAction(R.drawable.ic_next, stringResource(R.string.signin_again_title), MaterialTheme.colorScheme.error) { actions.onSignIn(account) }
}

/**
 * The mailboxes of all accounts together the person chose, in their order; Edit turns them all
 * into switches with handles to move them, and the accounts the unified inbox takes in.
 */
@Composable
private fun AllAccounts(state: MailboxesState, actions: AccountsActions) {
    var editing by rememberSaveable { mutableStateOf(false) }
    val boxes = state.allBoxes
    val colors = MaterialTheme.colorScheme
    Caption(stringResource(R.string.inbox_all_accounts)) {
        TextButton(onClick = { editing = !editing }) {
            Text(stringResource(if (editing) R.string.action_done else R.string.action_edit), style = Type.Text, fontWeight = Weight.SemiBold)
        }
    }
    if (!editing) {
        boxes.order.filter { it in boxes.shown && it.listed(state.folders) }.forEach { box ->
            FolderRow(box.icon, stringResource(box.title), box.mailbox == state.mailbox) { actions.onMailbox(box.mailbox) }
        }
        return
    }
    Reorderable(boxes.order, key = { it }, onMove = { actions.onAllBoxes(boxes.copy(order = it)) }) { box, handle ->
        val on = box in boxes.shown
        SheetRow(
            onClick = { actions.onAllBoxes(boxes.copy(shown = if (on) boxes.shown - box else boxes.shown + box)) },
            lead = { Glyph(box.icon, null, colors.primary) },
        ) {
            Text(stringResource(box.title), style = Type.Row, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            Switch(on, onCheckedChange = null)
            Handle(handle)
        }
    }
    Caption(stringResource(R.string.all_in))
    state.accounts.forEach { account ->
        // The unified inbox keeps one account at least.
        val last = account.inAll && state.accounts.count { it.inAll } == 1
        SheetRow(onClick = { if (!last) actions.onInAll(account, !account.inAll) }, lead = { AccountDot(account.tint, size = 12.dp) }) {
            Text(account.name, style = Type.Row, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            Switch(account.inAll, onCheckedChange = null, enabled = !last)
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

/** A small heading over a part of the list, with [action] at its end. */
@Composable
private fun Caption(text: String, action: (@Composable () -> Unit)? = null) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(40.dp)
            .padding(start = Gap.sheet, end = Gap.sheet - 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, style = Type.Label, modifier = Modifier.weight(1f))
        action?.invoke()
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
 * The settings: the accounts, in an order of the person's, each opening its own page; the
 * swipes, the signature and a letter to the author.
 */
@Composable
private fun AppSettings(
    accounts: List<AccountItem>,
    swipes: Swipes,
    signature: String,
    onBack: () -> Unit,
    onAccount: (AccountItem) -> Unit,
    onOrder: (List<AccountItem>) -> Unit,
    onSwipes: () -> Unit,
    onSignature: () -> Unit,
    onAdd: () -> Unit,
    onWrite: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    SubPage(stringResource(R.string.settings_title), onBack)
    Caption(stringResource(R.string.accounts_title))
    Reorderable(accounts, key = { it.id }, onMove = { if (it != accounts) onOrder(it) }) { account, handle ->
        SheetRow(onClick = { onAccount(account) }, lead = { AccountDot(account.tint, size = 12.dp) }) {
            Column(Modifier.weight(1f)) {
                Text(account.name, style = Type.Row, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (account.name != account.email) {
                    Text(account.email, color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (accounts.size > 1) Handle(handle)
        }
    }
    SheetAction(R.drawable.ic_add, stringResource(R.string.inbox_add_account), colors.primary, onAdd)
    if (accounts.size > 1) {
        Text(
            stringResource(R.string.account_settings_hint),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            modifier = Modifier.padding(horizontal = Gap.sheet).padding(bottom = 8.dp),
        )
    }
    Caption(stringResource(R.string.mail_settings))
    SheetRow(onClick = onSwipes, lead = { Glyph(R.drawable.ic_swipe, null, colors.primary) }) {
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.swipes_title), style = Type.Row)
            Text(swipes.summary(), color = colors.onSurfaceVariant, style = Type.Caption)
        }
    }
    SheetRow(onClick = onSignature, lead = { Glyph(R.drawable.ic_compose, null, colors.primary) }) {
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.signature_title), style = Type.Row)
            Text(
                signature.lines().firstOrNull { it.isNotBlank() } ?: stringResource(R.string.signature_none),
                color = colors.onSurfaceVariant,
                style = Type.Caption,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
    SheetAction(R.drawable.ic_compose, stringResource(R.string.feedback_write), colors.onSurface, onWrite)
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
    val colors = MaterialTheme.colorScheme
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
            // Points down while the folders show, to the side while they are hidden.
            Glyph(R.drawable.ic_next, null, colors.onSurfaceVariant, Modifier.padding(start = 4.dp).rotate(if (open) 90f else 0f), size = 16.dp)
        }
    }
    AnimatedVisibility(
        open,
        enter = expandVertically(Motion.calm(Motion.NOTE)) + fadeIn(Motion.calm(Motion.NOTE)),
        exit = shrinkVertically(Motion.calm(Motion.NOTE)) + fadeOut(Motion.calm(Motion.SMALL)),
    ) {
        Column { folders() }
    }
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

/** A sheet page's title with a way back to the list. */
@Composable
private fun SubPage(title: String, onBack: () -> Unit) {
    // The back arrow is centred on the rows' icon slot, so the title starts where their labels do.
    val back = 44.dp
    val overhang = (back - Gap.slot) / 2
    Row(Modifier.padding(start = Gap.sheet - overhang, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        IconTile(R.drawable.ic_back, stringResource(R.string.folders_title), onBack, size = back)
        SheetTitle(title, Modifier.weight(1f).padding(start = Gap.afterSlot - overhang, end = Gap.sheet))
    }
}

/**
 * One account: its name, whether it is in All inboxes, its colour, when new mail comes, how
 * much of its mail the phone keeps,
 * and a way to remove it. One the server signed out offers to sign in again first.
 */
@Composable
private fun AccountSettings(
    account: AccountItem,
    onBack: () -> Unit,
    /** Whether its inbox is part of the unified one; null with one account. */
    inAll: Boolean?,
    onInAll: (Boolean) -> Unit,
    onColor: (Int) -> Unit,
    onLabel: (String) -> Unit,
    signature: String,
    onSignature: (String) -> Unit,
    onSignIn: () -> Unit,
    onKeepDays: (Int) -> Unit,
    onNewMail: (NewMail) -> Unit,
    onRemove: () -> Unit,
) {
    var confirming by rememberSaveable { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    SubPage(account.name, onBack)
    if (account.needsSignIn) {
        Text(
            stringResource(R.string.account_signed_out),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            modifier = Modifier.padding(horizontal = Gap.sheet),
        )
        SheetAction(R.drawable.ic_next, stringResource(R.string.signin_again_title), colors.primary, onSignIn)
    }
    LabelField(account, onLabel)
    if (inAll != null) {
        SheetRow(onClick = { onInAll(!inAll) }, lead = { Glyph(R.drawable.ic_inbox, null, colors.primary) }) {
            Text(stringResource(R.string.all_in), style = Type.Row, modifier = Modifier.weight(1f))
            Switch(inAll, onCheckedChange = null)
        }
    }
    // Empty, it shows the default signature it goes out with.
    SignatureField(account.signature, stringResource(R.string.account_signature), signature, stringResource(R.string.account_signature_hint), onSignature)
    Text(
        stringResource(R.string.account_color),
        color = colors.onSurfaceVariant,
        style = Type.Caption,
        modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 8.dp, bottom = 12.dp),
    )
    Swatches(account.colorIndex.toInt(), onColor)
    Choices(R.string.account_new_mail, NEW_MAIL.keys.toList(), account.newMail, onNewMail) { option, on ->
        Text(
            stringResource(NEW_MAIL.getValue(option)),
            color = if (on) colors.primary else colors.onSurface,
            style = Type.Text,
            fontWeight = Weight.Medium,
            maxLines = 1,
        )
    }
    Text(
        stringResource(
            when (account.newMail) {
                NewMail.Manually -> R.string.account_new_mail_manually_hint
                is NewMail.Every -> R.string.account_new_mail_every_hint
            },
        ),
        color = colors.onSurfaceVariant,
        style = Type.Caption,
        modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, bottom = 12.dp),
    )
    Choices(R.string.account_keep, KEEP_DAYS.keys.toList(), account.keepDays.toInt(), onKeepDays) { days, on ->
        Text(
            stringResource(KEEP_DAYS.getValue(days)),
            color = if (on) colors.primary else colors.onSurface,
            style = Type.Text,
            fontWeight = Weight.Medium,
            maxLines = 1,
        )
    }
    Text(
        stringResource(R.string.account_keep_hint),
        color = colors.onSurfaceVariant,
        style = Type.Caption,
        modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, bottom = 12.dp),
    )
    SheetAction(R.drawable.ic_close, stringResource(R.string.account_remove), colors.error) { confirming = true }
    if (confirming) {
        AlertDialog(
            onDismissRequest = { confirming = false },
            confirmButton = {
                TextButton(onClick = {
                    confirming = false
                    onRemove()
                }) {
                    Text(stringResource(R.string.action_remove), color = colors.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirming = false }) { Text(stringResource(R.string.action_cancel)) }
            },
            title = { Text(stringResource(R.string.account_remove_title)) },
            text = { Text(stringResource(R.string.account_remove_text)) },
            containerColor = colors.background,
            shape = RoundedCornerShape(Radius.dialog),
        )
    }
}

/** When the phone looks for an account's new mail with the app closed, and what to call it. */
private val NEW_MAIL = linkedMapOf(
    NewMail.Every(15u) to R.string.account_new_mail_quarter,
    NewMail.Every(60u) to R.string.account_new_mail_hourly,
    NewMail.Manually to R.string.account_new_mail_manually,
)

/** How many days back an account's mail stays on the phone, 0 for all of it, and what to call it. */
private val KEEP_DAYS = linkedMapOf(
    3 to R.string.account_keep_three_days,
    7 to R.string.account_keep_week,
    30 to R.string.account_keep_month,
    90 to R.string.account_keep_quarter,
    365 to R.string.account_keep_year,
    0 to R.string.account_keep_all,
)

/** The person's own name for the account, saved with Save, on Done and when the page closes. */
@Composable
private fun LabelField(account: AccountItem, onLabel: (String) -> Unit) {
    var label by rememberSaveable(account.id) { mutableStateOf(account.label) }
    val saved by rememberUpdatedState(account.label)
    val latest by rememberUpdatedState(label)
    val save by rememberUpdatedState(onLabel)
    val focus = LocalFocusManager.current
    DisposableEffect(account.id) {
        onDispose { if (latest.trim() != saved) save(latest) }
    }
    OutlinedTextField(
        value = label,
        onValueChange = { label = it },
        label = { Text(stringResource(R.string.account_name)) },
        // What the account is called without a name of its own.
        placeholder = { Text(account.email) },
        supportingText = {
            val sender = account.displayName.trim()
            val hint = stringResource(R.string.account_name_hint)
            // The sender name is not what the list shows, so it is said here.
            Text(if (sender.isEmpty() || sender == account.email) hint else stringResource(R.string.account_sender, hint, sender))
        },
        trailingIcon = if (label.trim() != account.label) {
            {
                TextButton(onClick = {
                    focus.clearFocus()
                    onLabel(label)
                }) { Text(stringResource(R.string.action_save)) }
            }
        } else {
            null
        },
        shape = RoundedCornerShape(Radius.medium),
        singleLine = true,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = {
            focus.clearFocus()
            if (label.trim() != account.label) onLabel(label)
        }),
        modifier = Modifier.fillMaxWidth().padding(horizontal = Gap.sheet).padding(top = 8.dp),
    )
}

/**
 * A signature of a few lines under [label], [placeholder] while empty; saved when it loses focus
 * and when the page closes.
 */
@Composable
private fun SignatureField(saved: String, label: String, placeholder: String, hint: String, onSave: (String) -> Unit) {
    var text by rememberSaveable { mutableStateOf(saved) }
    // What was last sent, so leaving after a save does not send it again.
    var sent by rememberSaveable { mutableStateOf(saved) }
    fun save() {
        if (text.trimEnd() == sent) return
        sent = text.trimEnd()
        onSave(text)
    }
    val leave by rememberUpdatedState(::save)
    DisposableEffect(Unit) {
        onDispose { leave() }
    }
    OutlinedTextField(
        value = text,
        onValueChange = { text = it },
        label = { Text(label) },
        placeholder = { Text(placeholder) },
        supportingText = { Text(hint) },
        shape = RoundedCornerShape(Radius.medium),
        minLines = 3,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.sheet)
            .padding(top = 8.dp)
            .onFocusChanged { if (!it.isFocused) save() },
    )
}

/** The account colours in two rows; the current one has a ring. */
@Composable
private fun Swatches(current: Int, onPick: (Int) -> Unit) {
    // In rainbow order, grey last, so each row reads as a run of hues; the saved place stays the same.
    val rows = remember { accountColors().withIndex().sortedBy { (_, color) -> hueOrder(color) }.chunked(5) }
    val ring = MaterialTheme.colorScheme.onSurface
    Column(
        Modifier.fillMaxWidth().padding(horizontal = Gap.sheet).padding(bottom = 18.dp).selectableGroup(),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        rows.forEach { row ->
            // Edge to edge, as wide as the rows above.
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                row.forEach { (index, color) ->
                    val on = index == current
                    Box(
                        Modifier
                            .size(48.dp)
                            .then(if (on) Modifier.border(2.dp, ring, CircleShape) else Modifier)
                            .padding(4.dp)
                            .clip(CircleShape)
                            .background(rgb(color))
                            .selectable(selected = on, role = Role.RadioButton, onClick = { onPick(index) }),
                        contentAlignment = Alignment.Center,
                    ) {
                        if (on) Glyph(R.drawable.ic_done, null, Color.White, size = 18.dp)
                    }
                }
            }
        }
    }
}

/** Where a colour falls from red round to pink; greys go after all of them. */
private fun hueOrder(color: UInt): Float {
    val hsv = FloatArray(3)
    android.graphics.Color.colorToHSV(color.toInt(), hsv)
    // Reds just under 360° belong at the start.
    return if (hsv[1] < 0.35f) 1000f else (hsv[0] + 20f) % 360f
}

/** A row with an icon and a coloured label, as at the end of a sheet. */
@Composable
fun SheetAction(@DrawableRes icon: Int, text: String, color: Color, onClick: () -> Unit) {
    SheetRow(onClick, lead = { Glyph(icon, null, color) }) {
        Text(text, color = color, style = Type.Row)
    }
}

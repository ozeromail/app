package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.expandVertically
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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.FolderItem
import uniffi.ozero_api.FolderRole
import uniffi.ozero_api.Mailbox
import uniffi.ozero_api.accountColors

/**
 * The mailboxes of all accounts together, then each account's folders, and below them the
 * swipes and a way to add an account. A tap on a group's name hides or shows its folders; its
 * More button, or Swipes, opens the settings in the same sheet, which grows or shrinks to fit.
 */
@Composable
fun AccountsSheet(
    visible: Boolean,
    accounts: List<AccountItem>,
    folders: List<FolderItem>,
    mailbox: Mailbox,
    swipes: Swipes,
    onMailbox: (Mailbox) -> Unit,
    onSwipes: (Swipes) -> Unit,
    collapsed: Set<Long>,
    onCollapsed: (Set<Long>) -> Unit,
    onAdd: () -> Unit,
    onColor: (AccountItem, Int) -> Unit,
    onLabel: (AccountItem, String) -> Unit,
    onRemove: (AccountItem) -> Unit,
    onWrite: () -> Unit,
    onClose: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Sheet(visible, onClose) {
        // Inside the sheet, so it opens on the list again next time: an account's id, or
        // SWIPES.
        var page by rememberSaveable { mutableStateOf<Long?>(null) }
        AnimatedContent(
            page.takeIf { it == SWIPES || accounts.any { a -> a.id == it } },
            transitionSpec = {
                (fadeIn(Motion.calm(Motion.SMALL)) togetherWith fadeOut(Motion.calm(Motion.SMALL)))
                    .using(SizeTransform { _, _ -> Motion.settle() })
            },
            label = "accounts",
        ) { shown ->
            Column {
                val account = accounts.find { it.id == shown }
                when {
                    shown == SWIPES -> {
                        SubPage(stringResource(R.string.swipes_title), onBack = { page = null })
                        SwipeSettings(swipes, onSwipes)
                    }
                    account != null -> AccountSettings(
                        account,
                        onBack = { page = null },
                        onColor = { onColor(account, it) },
                        onLabel = { onLabel(account, it) },
                        onRemove = { onRemove(account) },
                    )
                    else -> {
                        SheetTitle(stringResource(R.string.folders_title))
                        Folders(
                            accounts,
                            folders,
                            mailbox,
                            collapsed,
                            onMailbox,
                            onToggle = { group -> onCollapsed(if (group in collapsed) collapsed - group else collapsed + group) },
                            onAccount = { page = it.id },
                        )
                        HorizontalDivider(Modifier.padding(vertical = 6.dp), color = colors.outlineVariant)
                        SheetRow(onClick = { page = SWIPES }, lead = { Glyph(R.drawable.ic_swipe, null, colors.primary) }) {
                            Column(Modifier.weight(1f)) {
                                Text(stringResource(R.string.swipes_title), fontSize = 15.5.sp, fontWeight = FontWeight.Medium)
                                Text(swipes.summary(), color = colors.onSurfaceVariant, fontSize = 13.sp)
                            }
                        }
                        SheetAction(R.drawable.ic_add, stringResource(R.string.inbox_add_account), colors.primary, onAdd)
                        SheetAction(R.drawable.ic_compose, stringResource(R.string.feedback_write), colors.onSurface, onWrite)
                    }
                }
            }
        }
    }
}

/** The page of the sheet with the swipes; account pages are named by their ids, which are positive. */
private const val SWIPES = -1L

/**
 * With several accounts, their mailboxes together first; then each account under its name.
 * A group in [collapsed] shows only its name. The open mailbox is in the accent colour.
 */
@Composable
private fun Folders(
    accounts: List<AccountItem>,
    folders: List<FolderItem>,
    mailbox: Mailbox,
    collapsed: Set<Long>,
    onMailbox: (Mailbox) -> Unit,
    onToggle: (Long) -> Unit,
    onAccount: (AccountItem) -> Unit,
) {
    if (accounts.size > 1) {
        Group(stringResource(R.string.inbox_all_accounts), Settings.ALL_ACCOUNTS !in collapsed, { StackedDots(accounts) }, { onToggle(Settings.ALL_ACCOUNTS) }) {
            UNIFIED.filter { role -> role == FolderRole.INBOX || folders.any { it.role == role } }.forEach { role ->
                val box = Mailbox.Unified(role)
                FolderRow(role.icon(), role.title(""), box == mailbox) { onMailbox(box) }
            }
        }
    }
    accounts.forEach { account ->
        Group(
            account.name,
            account.id !in collapsed,
            lead = { AccountDot(account.tint, size = 12.dp) },
            onToggle = { onToggle(account.id) },
            onSettings = { onAccount(account) },
        ) {
            folders.filter { it.accountId == account.id && it.role != FolderRole.DRAFTS }.sortedWith(FOLDER_ORDER).forEach { folder ->
                // With one account its inbox is the unified one, which keeps the tabs.
                val box = if (folder.role == FolderRole.INBOX && accounts.size == 1) Mail.INBOX else Mailbox.Folder(folder.id)
                FolderRow(folder.role.icon(), folder.title(), box == mailbox) { onMailbox(box) }
            }
        }
    }
}

/** The mailboxes shared by all accounts, in order. */
private val UNIFIED = listOf(FolderRole.INBOX, FolderRole.FLAGGED, FolderRole.SENT, FolderRole.ARCHIVE)

/** System folders first in the usual order, then the account's own by name. */
private val FOLDER_ORDER = compareBy<FolderItem>({ it.role.rank() }, { it.name.lowercase() })

private fun FolderRole.rank(): Int = when (this) {
    FolderRole.INBOX -> 0
    FolderRole.FLAGGED -> 1
    FolderRole.SENT -> 2
    FolderRole.DRAFTS -> 3
    FolderRole.ARCHIVE -> 4
    FolderRole.JUNK -> 5
    FolderRole.TRASH -> 6
    FolderRole.CUSTOM -> 7
}

@DrawableRes
fun FolderRole.icon(): Int = when (this) {
    FolderRole.INBOX -> R.drawable.ic_inbox
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

/** A group's name, which hides or shows its folders, and with [onSettings] a way to its settings. */
@Composable
private fun Group(
    text: String,
    open: Boolean,
    lead: @Composable () -> Unit,
    onToggle: () -> Unit,
    onSettings: (() -> Unit)? = null,
    folders: @Composable () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClickLabel = stringResource(if (open) R.string.action_collapse else R.string.action_expand), onClick = onToggle)
            .padding(start = Gap.sheet, end = Gap.sheet - 12.dp, top = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(Gap.slot), contentAlignment = Alignment.Center) { lead() }
        Text(
            text,
            fontSize = 15.sp,
            fontWeight = FontWeight.Bold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false).padding(start = Gap.afterSlot),
        )
        // Points down while the folders show, to the side while they are hidden.
        Glyph(R.drawable.ic_next, null, colors.onSurfaceVariant, Modifier.padding(start = 4.dp).rotate(if (open) 90f else 0f), size = 16.dp)
        Box(Modifier.weight(1f))
        if (onSettings != null) {
            IconTile(R.drawable.ic_more, stringResource(R.string.account_settings), onSettings, tint = colors.onSurfaceVariant, size = 40.dp)
        } else {
            Box(Modifier.size(40.dp))
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
private fun FolderRow(@DrawableRes icon: Int, title: String, open: Boolean, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    SheetRow(onClick, lead = { Glyph(icon, null, colors.primary) }) {
        Text(
            title,
            color = if (open) colors.primary else colors.onSurface,
            fontSize = 15.5.sp,
            fontWeight = if (open) FontWeight.SemiBold else FontWeight.Normal,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
    }
}

/** Up to three account colours overlapping, as the logo has them. */
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

/** One account: its name and colour, and a way to remove it from the phone. */
@Composable
private fun AccountSettings(
    account: AccountItem,
    onBack: () -> Unit,
    onColor: (Int) -> Unit,
    onLabel: (String) -> Unit,
    onRemove: () -> Unit,
) {
    var confirming by rememberSaveable { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    SubPage(account.name, onBack)
    LabelField(account, onLabel)
    Text(
        stringResource(R.string.account_color),
        color = colors.onSurfaceVariant,
        fontSize = 13.sp,
        modifier = Modifier.padding(start = Gap.sheet, end = Gap.sheet, top = 8.dp, bottom = 12.dp),
    )
    Swatches(account.colorIndex.toInt(), onColor)
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

/** The person's own name for the account, saved on Done and when the page closes. */
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
        placeholder = { Text(account.displayName.ifBlank { account.email }) },
        supportingText = { Text(stringResource(R.string.account_name_hint)) },
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
        Text(text, color = color, fontSize = 15.5.sp, fontWeight = FontWeight.Medium)
    }
}

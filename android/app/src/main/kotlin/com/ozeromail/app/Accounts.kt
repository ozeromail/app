package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.accountColors

/** The accounts and a way to add one; a tap on an account opens its settings. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AccountsSheet(
    accounts: List<AccountItem>,
    onAdd: () -> Unit,
    onColor: (AccountItem, Int) -> Unit,
    onRemove: (AccountItem) -> Unit,
    onClose: () -> Unit,
) {
    var editing by rememberSaveable { mutableStateOf<Long?>(null) }
    val colors = MaterialTheme.colorScheme
    ModalBottomSheet(onDismissRequest = onClose, containerColor = colors.background) {
        Column(Modifier.navigationBarsPadding().padding(bottom = 16.dp)) {
            val account = accounts.find { it.id == editing }
            if (account != null) {
                AccountSettings(
                    account,
                    onBack = { editing = null },
                    onColor = { onColor(account, it) },
                    onRemove = { onRemove(account) },
                )
                return@Column
            }
            SheetTitle(stringResource(R.string.accounts_title))
            accounts.forEach { item ->
                Row(
                    Modifier.fillMaxWidth().clickable { editing = item.id }.padding(horizontal = 22.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    AccountDot(item.tint, size = 12.dp)
                    Column(Modifier.weight(1f)) {
                        Text(item.name, fontSize = 15.5.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (item.name != item.email) {
                            Text(item.email, color = colors.onSurfaceVariant, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
            SheetAction(R.drawable.ic_add, stringResource(R.string.inbox_add_account), colors.primary, onAdd)
        }
    }
}

/** One account: its colour, and a way to remove it from the phone. */
@Composable
private fun AccountSettings(account: AccountItem, onBack: () -> Unit, onColor: (Int) -> Unit, onRemove: () -> Unit) {
    var confirming by rememberSaveable { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    Row(Modifier.padding(start = 8.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        IconTile(R.drawable.ic_back, stringResource(R.string.accounts_title), onBack, size = 40.dp)
        SheetTitle(account.name, Modifier.weight(1f).padding(start = 2.dp, end = 22.dp))
    }
    Text(
        stringResource(R.string.account_color),
        color = colors.onSurfaceVariant,
        fontSize = 13.sp,
        modifier = Modifier.padding(start = 22.dp, end = 22.dp, top = 8.dp, bottom = 12.dp),
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
            shape = RoundedCornerShape(20.dp),
        )
    }
}

/** The account colours in two rows; the current one has a ring. */
@Composable
private fun Swatches(current: Int, onPick: (Int) -> Unit) {
    val rows = remember { accountColors().withIndex().chunked(5) }
    val ring = MaterialTheme.colorScheme.onSurface
    Column(
        Modifier.padding(horizontal = 22.dp).padding(bottom = 18.dp).selectableGroup(),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        rows.forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                row.forEach { (index, color) ->
                    val on = index == current
                    Box(
                        Modifier
                            .size(40.dp)
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

@Composable
private fun SheetTitle(text: String, modifier: Modifier = Modifier.padding(start = 22.dp, end = 22.dp, bottom = 10.dp)) {
    Text(
        text,
        fontSize = 18.sp,
        fontWeight = FontWeight.Bold,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier,
    )
}

/** A row with an icon and a coloured label, as at the end of a sheet. */
@Composable
private fun SheetAction(@DrawableRes icon: Int, text: String, color: Color, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 18.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Glyph(icon, null, color)
        Text(text, color = color, fontSize = 15.5.sp, fontWeight = FontWeight.Medium)
    }
}

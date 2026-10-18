package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
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

/**
 * The accounts and a way to add one, in a sheet; a tap on an account opens its settings in
 * the same sheet, which grows or shrinks to fit.
 */
@Composable
fun AccountsSheet(
    visible: Boolean,
    accounts: List<AccountItem>,
    onAdd: () -> Unit,
    onColor: (AccountItem, Int) -> Unit,
    onRemove: (AccountItem) -> Unit,
    onWrite: () -> Unit,
    onClose: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Sheet(visible, onClose) {
        // Inside the sheet, so it opens on the list again next time.
        var editing by rememberSaveable { mutableStateOf<Long?>(null) }
        AnimatedContent(
            accounts.find { it.id == editing },
            transitionSpec = {
                (fadeIn(Motion.calm(Motion.SMALL)) togetherWith fadeOut(Motion.calm(Motion.SMALL)))
                    .using(SizeTransform { _, _ -> Motion.settle() })
            },
            contentKey = { it?.id },
            label = "accounts",
        ) { account ->
            Column {
                if (account != null) {
                    AccountSettings(
                        account,
                        onBack = { editing = null },
                        onColor = { onColor(account, it) },
                        onRemove = { onRemove(account) },
                    )
                } else {
                    SheetTitle(stringResource(R.string.accounts_title))
                    accounts.forEach { item ->
                        SheetRow(onClick = { editing = item.id }, lead = { AccountDot(item.tint, size = 12.dp) }) {
                            Column(Modifier.weight(1f)) {
                                Text(item.name, fontSize = 15.5.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                if (item.name != item.email) {
                                    Text(item.email, color = colors.onSurfaceVariant, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                            }
                        }
                    }
                    SheetAction(R.drawable.ic_add, stringResource(R.string.inbox_add_account), colors.primary, onAdd)
                    HorizontalDivider(Modifier.padding(vertical = 6.dp), color = colors.outlineVariant)
                    SheetAction(R.drawable.ic_compose, stringResource(R.string.feedback_write), colors.onSurface, onWrite)
                }
            }
        }
    }
}

/** One account: its colour, and a way to remove it from the phone. */
@Composable
private fun AccountSettings(account: AccountItem, onBack: () -> Unit, onColor: (Int) -> Unit, onRemove: () -> Unit) {
    var confirming by rememberSaveable { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    // The back arrow is centred on the rows' icon slot, so the name starts where their labels do.
    val back = 44.dp
    val overhang = (back - Gap.slot) / 2
    Row(Modifier.padding(start = Gap.sheet - overhang, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        IconTile(R.drawable.ic_back, stringResource(R.string.accounts_title), onBack, size = back)
        SheetTitle(account.name, Modifier.weight(1f).padding(start = Gap.afterSlot - overhang, end = Gap.sheet))
    }
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
private fun SheetAction(@DrawableRes icon: Int, text: String, color: Color, onClick: () -> Unit) {
    SheetRow(onClick, lead = { Glyph(icon, null, color) }) {
        Text(text, color = color, fontSize = 15.5.sp, fontWeight = FontWeight.Medium)
    }
}

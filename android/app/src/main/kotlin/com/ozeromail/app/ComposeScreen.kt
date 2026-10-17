package com.ozeromail.app

import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftItem
import uniffi.ozero_api.MessageItem

/** A blank message; account 0 is none, so compose starts from the first account. */
val NEW_DRAFT = DraftItem(0, "", "", "", null)

/** A message being written, and the one it answers. */
data class Composing(val draft: DraftItem, val original: MessageItem? = null)

/**
 * A new message or a reply, on the whole screen. The fields sit low, near the keyboard;
 * the message being answered stays on top. [onSent] gets the outbox id for undo.
 */
@Composable
fun ComposeScreen(mail: Mail, composing: Composing, onCancel: () -> Unit, onSent: (Long, Composing) -> Unit) {
    BackHandler(onBack = onCancel)
    val start = composing.draft
    var to by rememberSaveable { mutableStateOf(start.to) }
    var subject by rememberSaveable { mutableStateOf(start.subject) }
    var text by rememberSaveable { mutableStateOf(start.text) }
    var accountId by rememberSaveable { mutableStateOf(start.accountId) }
    var accounts by remember { mutableStateOf(emptyList<AccountItem>()) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    val scope = rememberCoroutineScope()

    LaunchedEffect(Unit) {
        try {
            accounts = mail.accounts()
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    // A new message starts from the first account.
    val from = accounts.find { it.id == accountId } ?: accounts.firstOrNull()

    fun send() {
        if (busy || from == null) return
        busy = true
        error = null
        val draft = DraftItem(from.id, to, subject, text, start.replyTo)
        scope.launch {
            try {
                onSent(mail.send(draft), composing.copy(draft = draft))
            } catch (e: CoreException) {
                error = e.text()
            } finally {
                busy = false
            }
        }
    }

    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
        composing.original?.let { Original(it) }
        Column(
            Modifier.weight(1f).padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.Bottom),
        ) {
            Spacer(Modifier.weight(1f))
            Plain(
                to,
                { to = it },
                R.string.compose_to,
                TextStyle(fontSize = 15.sp),
                KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next, autoCorrectEnabled = false),
            )
            HorizontalDivider(color = colors.outlineVariant)
            Plain(
                subject,
                { subject = it },
                R.string.compose_subject,
                TextStyle(fontSize = 22.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.44).sp),
                KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Next),
            )
            Plain(
                text,
                { text = it },
                R.string.compose_text,
                TextStyle(fontSize = 15.5.sp, lineHeight = 24.sp),
                KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                singleLine = false,
                modifier = Modifier.weight(1f, fill = false),
            )
        }
        error?.let {
            Text(stringResource(it), color = colors.error, modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp))
        }
        Row(
            Modifier
                .fillMaxWidth()
                .drawBehind { drawLine(colors.outlineVariant, Offset.Zero, Offset(size.width, 0f), 1.dp.toPx()) }
                .padding(start = 12.dp, end = 12.dp, top = 10.dp, bottom = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            IconTile(R.drawable.ic_close, stringResource(R.string.action_close), onCancel, shape = CircleShape)
            Box(Modifier.weight(1f)) {
                if (from != null) Sender(from, accounts, onPick = { accountId = it.id })
            }
            val ready = !busy && from != null
            IconTile(
                R.drawable.ic_send,
                stringResource(R.string.compose_send),
                ::send,
                Modifier.width(56.dp).alpha(if (ready) 1f else 0.4f),
                background = colors.primary,
                tint = colors.onPrimary,
                shape = CircleShape,
            )
        }
    }
}

/** The message being answered, kept short. */
@Composable
private fun Original(message: MessageItem) {
    val colors = MaterialTheme.colorScheme
    Column(
        Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 16.dp, top = 12.dp)
            .background(colors.surfaceVariant, RoundedCornerShape(16.dp))
            .padding(horizontal = 14.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                message.sender,
                fontWeight = FontWeight.SemiBold,
                fontSize = 14.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text(shortDate(LocalContext.current, message.date), color = colors.onSurfaceVariant, fontSize = 12.5.sp)
        }
        Text(message.subjectText(), fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (message.snippet.isNotBlank()) {
            Text(
                message.snippet,
                color = colors.onSurfaceVariant,
                fontSize = 14.sp,
                lineHeight = 21.sp,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** The account the message goes from, as a chip by Send; a tap lists the others. */
@Composable
private fun Sender(from: AccountItem, accounts: List<AccountItem>, onPick: (AccountItem) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    Box {
        Row(
            Modifier
                .height(44.dp)
                .clip(CircleShape)
                .background(colors.surfaceVariant)
                .clickable(enabled = accounts.size > 1) { open = true }
                .padding(start = 8.dp, end = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Box(Modifier.size(22.dp).background(from.tint, CircleShape), contentAlignment = Alignment.Center) {
                Text(initials(from.displayName.ifBlank { from.email }).take(1), color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Bold)
            }
            Text(from.email, fontSize = 14.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            accounts.forEach { account ->
                DropdownMenuItem(
                    text = { Text(account.email) },
                    leadingIcon = { AccountDot(account.tint) },
                    onClick = {
                        open = false
                        onPick(account)
                    },
                )
            }
        }
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
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    BasicTextField(
        value = value,
        onValueChange = onChange,
        textStyle = style.copy(color = colors.onSurface),
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

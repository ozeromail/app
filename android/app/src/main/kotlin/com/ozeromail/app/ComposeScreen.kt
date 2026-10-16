package com.ozeromail.app

import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
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
                MaterialTheme.typography.bodyLarge,
                KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next, autoCorrectEnabled = false),
            )
            HorizontalDivider(color = colors.outlineVariant)
            Plain(
                subject,
                { subject = it },
                R.string.compose_subject,
                MaterialTheme.typography.headlineSmall.copy(fontWeight = FontWeight.Bold),
                KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Next),
            )
            Plain(
                text,
                { text = it },
                R.string.compose_text,
                MaterialTheme.typography.bodyLarge,
                KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                singleLine = false,
                modifier = Modifier.weight(1f, fill = false),
            )
        }
        error?.let {
            Text(stringResource(it), color = colors.error, modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp))
        }
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onCancel) { Text(stringResource(R.string.action_cancel)) }
            Box(Modifier.weight(1f)) {
                if (from != null) Sender(from, accounts, onPick = { accountId = it.id })
            }
            Button(onClick = ::send, enabled = !busy && from != null) {
                Text(stringResource(R.string.compose_send))
            }
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
            .padding(16.dp)
            .background(colors.surfaceVariant, RoundedCornerShape(12.dp))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(message.sender, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(message.subjectText(), color = colors.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (message.snippet.isNotBlank()) {
            Text(
                message.snippet,
                color = colors.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 3,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** The account the message goes from; a tap lists the others. */
@Composable
private fun Sender(from: AccountItem, accounts: List<AccountItem>, onPick: (AccountItem) -> Unit) {
    var open by remember { mutableStateOf(false) }
    TextButton(onClick = { open = true }, enabled = accounts.size > 1) {
        Text(from.email, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        accounts.forEach { account ->
            DropdownMenuItem(
                text = { Text(account.email) },
                onClick = {
                    open = false
                    onPick(account)
                },
            )
        }
    }
}

/** A field with no frame: just the text, or its placeholder. */
@Composable
private fun Plain(
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

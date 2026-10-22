package com.ozeromail.app

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftItem
import uniffi.ozero_api.ForwardLabels
import uniffi.ozero_api.MessageItem

/** A blank message; account 0 is none, so compose starts from the first account. */
val NEW_DRAFT = DraftItem(0, 0, "", "", "", null, "", 0)

/** A pause in typing this long saves the draft, so each key does not. */
private const val SAVE_PAUSE_MS = 1_000L

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
    var subject by rememberSaveable { mutableStateOf(start.subject) }
    var text by rememberSaveable { mutableStateOf(start.text) }
    var accountId by rememberSaveable { mutableStateOf(start.accountId) }
    // Not saveable: a long quote would not fit in the saved state.
    var quote by remember { mutableStateOf(start.quote) }
    var quotable by remember { mutableStateOf(true) }
    var accounts by remember { mutableStateOf(emptyList<AccountItem>()) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<Int?>(null) }
    var picking by remember { mutableStateOf(false) }
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
    fun current() = DraftItem(0, accountId, to, subject, text, start.replyTo, quote, 0, start.forward)
    fun keep() {
        if (changed && !finished) keeper.save(current())
    }
    LaunchedEffect(keeper) {
        snapshotFlow { current() }.drop(1).collectLatest { draft ->
            changed = true
            delay(SAVE_PAUSE_MS)
            if (!finished) keeper.save(draft)
        }
    }
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
        keeper.discard()
        onDiscard(current().copy(accountId = from?.id ?: accountId))
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
    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize().safeDrawingPadding().imePadding()) {
            // The message being answered takes the room the fields leave; alone, the fields sit low.
            val original = composing.original
            // With the keyboard up the card may close to nothing, so Send always stays in sight.
            if (original != null) Original(mail, original, Modifier.weight(1f).heightIn(min = if (keyboard) 0.dp else 112.dp))
            Column(
                (if (original == null) Modifier.weight(1f) else Modifier).padding(horizontal = Gap.screen),
                verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.Bottom),
            ) {
                Plain(
                    to,
                    { to = it },
                    R.string.compose_to,
                    Type.Body,
                    KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next, autoCorrectEnabled = false),
                )
                HorizontalDivider(color = colors.outlineVariant)
                Plain(
                    subject,
                    { subject = it },
                    R.string.compose_subject,
                    Type.Title,
                    KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Next),
                )
                Plain(
                    text,
                    { text = it },
                    R.string.compose_text,
                    Type.Writing,
                    KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    singleLine = false,
                    // Room for a few lines from the start, less while the keyboard takes the screen.
                    modifier = Modifier.heightIn(min = if (keyboard) 72.dp else 168.dp, max = if (keyboard) 168.dp else 320.dp),
                )
                // A forward is its quote, so it stays.
                val forwarding = start.forward != null
                val files = forwarded?.takeIf { it.accountId == from?.id }?.attachments.orEmpty()
                if (files.isNotEmpty()) AttachmentChips(files)
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
            Reveal(error) {
                Text(stringResource(it), color = colors.error, modifier = Modifier.padding(horizontal = Gap.screen, vertical = 8.dp))
            }
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
                val ready = !busy && from != null
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
        }
        // Like every menu, the other accounts open in a sheet, low in reach of the thumb.
        Sheet(visible = picking, onClose = { picking = false }) {
            accounts.forEach { account ->
                SheetRow(
                    onClick = {
                        picking = false
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
    Column(
        modifier
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
                .weight(1f, fill = false)
                .verticalScroll(rememberScrollState())
                .padding(start = Gap.inCard, end = Gap.inCard, bottom = 12.dp),
        )
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

package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
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
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.AccountKind
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.NewMail
import uniffi.ozero_api.accountColors

/** What the settings change; each saves at once. */
class SettingsActions(
    val onAdd: () -> Unit,
    val onInAll: (AccountItem, Boolean) -> Unit,
    val onNotify: (AccountItem, Boolean) -> Unit,
    val onColor: (AccountItem, Int) -> Unit,
    val onLabel: (AccountItem, String) -> Unit,
    val onSender: (AccountItem, String) -> Unit,
    val onSignature: (String) -> Unit,
    val onAccountSignature: (AccountItem, String) -> Unit,
    val onSwipes: (Swipes) -> Unit,
    val onSignIn: (AccountItem) -> Unit,
    val onKeepDays: (AccountItem, Int) -> Unit,
    val onNewMail: (AccountItem, NewMail) -> Unit,
    val onRemove: (AccountItem) -> Unit,
    val onWrite: () -> Unit,
)

/** The pages of the settings besides an account's, which go by its id; those are positive. */
const val SETTINGS = -3L
private const val SWIPES = -1L
private const val SIGNATURE = -2L
private const val READING = -4L
private const val NOTIFICATIONS = -5L

/**
 * The settings, a screen of their own: [page] is [SETTINGS], one of its pages or an account's
 * id. [onOpen] goes one page deeper.
 */
@Composable
fun SettingsScreen(
    mail: Mail,
    page: Long,
    accounts: List<AccountItem>,
    swipes: Swipes,
    signature: String,
    actions: SettingsActions,
    onOpen: (Long) -> Unit,
    onBack: () -> Unit,
) {
    val account = accounts.find { it.id == page }
    val title = when (page) {
        SWIPES -> stringResource(R.string.swipes_title)
        SIGNATURE -> stringResource(R.string.signature_title)
        READING -> stringResource(R.string.settings_reading)
        NOTIFICATIONS -> stringResource(R.string.notify_title)
        else -> account?.name ?: stringResource(R.string.settings_title)
    }
    // Whether images load without asking, shared by the first page and Reading, and saved in a
    // scope that outlives the page it was picked on.
    val scope = rememberCoroutineScope()
    var imagesAlways by remember { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(Unit) {
        imagesAlways = try {
            mail.imagesAlways()
        } catch (_: CoreException) {
            null
        }
    }
    val images = ImagesAlways(imagesAlways) { on ->
        imagesAlways = on
        scope.launch {
            try {
                mail.setImagesAlways(on)
            } catch (_: CoreException) {
                imagesAlways = !on
            }
        }
    }
    Column(Modifier.fillMaxSize().safeDrawingPadding()) {
        Hero(title, null, onBack)
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(bottom = 32.dp)) {
            when {
                page == SWIPES -> {
                    SwipePreview(swipes)
                    SwipeSettings(swipes, actions.onSwipes)
                }
                page == SIGNATURE -> SignatureField(
                    signature,
                    stringResource(R.string.signature_title),
                    "",
                    stringResource(R.string.signature_hint),
                    actions.onSignature,
                )
                page == READING -> ReadingSettings(mail, images)
                page == NOTIFICATIONS -> NotificationSettings(accounts, actions)
                account != null -> AccountSettings(
                    account,
                    inAll = account.inAll.takeIf { !account.inAllFixed },
                    signature = signature,
                    actions = actions,
                )
                else -> AppSettings(images.on, accounts, swipes, signature, actions, onOpen)
            }
        }
    }
}

/**
 * The first page: the accounts, then the mail's settings, reading and the app, each row with
 * what it is set to.
 */
@Composable
private fun AppSettings(
    imagesAlways: Boolean?,
    accounts: List<AccountItem>,
    swipes: Swipes,
    signature: String,
    actions: SettingsActions,
    onOpen: (Long) -> Unit,
) {
    val context = LocalContext.current
    val colors = MaterialTheme.colorScheme
    val textSize = remember { Settings(context).textSize }
    Section(stringResource(R.string.accounts_title)) {
        accounts.forEach { account ->
            // The address, or what the account is when its name is the address.
            val place = if (account.name.trim().equals(account.email, ignoreCase = true)) stringResource(account.kind.title()) else account.email
            val quiet = if (account.notify) "" else " · " + stringResource(R.string.account_quiet)
            SettingRow(
                lead = { AccountDot(account.tint, size = 12.dp) },
                title = account.name,
                caption = place + quiet,
                onClick = { onOpen(account.id) },
            )
        }
        SettingRow(
            lead = { Glyph(R.drawable.ic_add, null, colors.primary) },
            title = stringResource(R.string.inbox_add_account),
            color = colors.primary,
            next = false,
            onClick = actions.onAdd,
        )
    }
    Section(stringResource(R.string.mail_settings)) {
        SettingRow(R.drawable.ic_bell, stringResource(R.string.notify_title), notifySummary(accounts)) { onOpen(NOTIFICATIONS) }
        SettingRow(R.drawable.ic_swipe, stringResource(R.string.swipes_title), swipes.summary()) { onOpen(SWIPES) }
        SettingRow(
            R.drawable.ic_compose,
            stringResource(R.string.signature_title),
            signature.lines().firstOrNull { it.isNotBlank() } ?: stringResource(R.string.signature_none),
        ) { onOpen(SIGNATURE) }
    }
    Section(stringResource(R.string.settings_reading)) {
        SettingRow(R.drawable.ic_text_size, stringResource(R.string.reading_size), stringResource(SIZE_NAMES[textSize])) { onOpen(READING) }
        SettingRow(
            R.drawable.ic_photo,
            stringResource(R.string.images_title),
            imagesAlways?.let { stringResource(if (it) R.string.images_always else R.string.images_ask) },
        ) { onOpen(READING) }
    }
    Spacer(Modifier.height(16.dp))
    Section(null) {
        SettingRow(
            lead = { Glyph(R.drawable.ic_letter, null, colors.primary) },
            title = stringResource(R.string.feedback_write),
            color = colors.primary,
            next = false,
            onClick = actions.onWrite,
        )
    }
    Hint("${stringResource(R.string.app_name)} ${remember { appVersion(context) }}", top = 20.dp, align = TextAlign.Center)
}

/** Which accounts notify, in a few words. */
@Composable
private fun notifySummary(accounts: List<AccountItem>): String {
    val (on, off) = accounts.partition { it.notify }
    return when {
        off.isEmpty() -> stringResource(R.string.notify_all)
        on.isEmpty() -> stringResource(R.string.notify_off)
        off.size <= on.size -> stringResource(R.string.notify_but, off.joinToString { it.name })
        else -> stringResource(R.string.notify_only, on.joinToString { it.name })
    }
}

/** A switch for each account, and what a notification is for. */
@Composable
private fun NotificationSettings(accounts: List<AccountItem>, actions: SettingsActions) {
    Section(null) {
        accounts.forEach { account ->
            SettingRow(
                lead = { AccountDot(account.tint, size = 12.dp) },
                title = account.name,
                next = false,
                onClick = { actions.onNotify(account, !account.notify) },
            ) { SmallSwitch(account.notify) }
        }
    }
    Hint(stringResource(R.string.notify_hint))
}

/**
 * One account: who it is and where it lives, then how others see it, what it looks like on
 * this phone, its new mail and its sign-in, and a way to remove it. One the server signed out
 * offers to sign in again first.
 */
@Composable
private fun AccountSettings(
    account: AccountItem,
    /** Whether its inbox is part of the unified one; null when that cannot change. */
    inAll: Boolean?,
    /** The default one, which goes under its mail while it has none of its own. */
    signature: String,
    actions: SettingsActions,
) {
    var confirming by rememberSaveable { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    Row(Modifier.padding(horizontal = Gap.screen).padding(bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        AccountInitial(account.name, account.tint, 40.dp)
        Column(Modifier.padding(start = 14.dp)) {
            Text(account.email, style = Type.Row, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val kind = stringResource(account.kind.title())
            Text(
                listOf(kind, account.server).filter { it.isNotBlank() }.joinToString(" · "),
                color = colors.onSurfaceVariant,
                style = Type.Caption,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
    if (account.needsSignIn) {
        Section(null) {
            SettingRow(
                lead = { Glyph(R.drawable.ic_next, null, colors.error) },
                title = stringResource(R.string.signin_again_title),
                caption = stringResource(R.string.account_signed_out),
                color = colors.error,
                onClick = { actions.onSignIn(account) },
            )
        }
    }
    Caption(stringResource(R.string.account_seen_as))
    NameField(
        key = "sender${account.id}",
        saved = account.senderName,
        label = stringResource(R.string.account_sender_name),
        placeholder = account.email,
        capitalization = KeyboardCapitalization.Words,
    ) { actions.onSender(account, it) }
    // Empty, it shows the default signature it goes out with.
    SignatureField(account.signature, stringResource(R.string.account_signature), signature, stringResource(R.string.account_signature_hint)) {
        actions.onAccountSignature(account, it)
    }
    Caption(stringResource(R.string.account_on_phone))
    NameField(
        key = "label${account.id}",
        saved = account.label,
        label = stringResource(R.string.account_name),
        placeholder = account.email,
        capitalization = KeyboardCapitalization.Sentences,
    ) { actions.onLabel(account, it) }
    Hint(stringResource(R.string.account_color), top = 8.dp)
    Swatches(account.colorIndex.toInt()) { actions.onColor(account, it) }
    if (inAll != null) {
        Section(null) {
            SettingRow(R.drawable.ic_inbox, stringResource(R.string.all_in), next = false, end = { SmallSwitch(inAll) }) {
                actions.onInAll(account, !inAll)
            }
        }
    }
    Caption(stringResource(R.string.account_new_mail))
    Section(null) {
        SettingRow(
            R.drawable.ic_bell,
            stringResource(R.string.notify_title),
            caption = stringResource(R.string.account_notify_hint),
            next = false,
            end = { SmallSwitch(account.notify) },
        ) { actions.onNotify(account, !account.notify) }
    }
    Choices(R.string.account_check, NEW_MAIL.keys.toList(), account.newMail, { actions.onNewMail(account, it) }) { option, on ->
        ChoiceText(stringResource(NEW_MAIL.getValue(option)), on)
    }
    Hint(
        stringResource(
            when (account.newMail) {
                NewMail.Manually -> R.string.account_new_mail_manually_hint
                is NewMail.Every -> R.string.account_new_mail_every_hint
            },
        ),
    )
    Choices(R.string.account_keep, KEEP_DAYS.keys.toList(), account.keepDays.toInt(), { actions.onKeepDays(account, it) }) { days, on ->
        ChoiceText(stringResource(KEEP_DAYS.getValue(days)), on)
    }
    Hint(stringResource(R.string.account_keep_hint))
    if (!account.needsSignIn) {
        Caption(stringResource(R.string.account_sign_in))
        Section(null) {
            // Google asks for its own sign-in; the rest for the new password.
            val google = account.kind == AccountKind.GMAIL
            SettingRow(
                R.drawable.ic_key,
                stringResource(if (google) R.string.signin_again_title else R.string.account_password),
            ) { actions.onSignIn(account) }
        }
    }
    Spacer(Modifier.height(16.dp))
    Section(null) {
        SettingRow(
            lead = { Glyph(R.drawable.ic_close, null, colors.error) },
            title = stringResource(R.string.account_remove),
            color = colors.error,
            next = false,
            onClick = { confirming = true },
        )
    }
    if (confirming) {
        AlertDialog(
            onDismissRequest = { confirming = false },
            confirmButton = {
                TextButton(onClick = {
                    confirming = false
                    actions.onRemove(account)
                }) {
                    Text(stringResource(R.string.action_remove), color = colors.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirming = false }) { Text(stringResource(R.string.action_cancel)) }
            },
            title = { Text(stringResource(R.string.account_remove_title, account.name)) },
            text = { Text(stringResource(R.string.account_remove_text)) },
            containerColor = colors.background,
            shape = RoundedCornerShape(Radius.dialog),
        )
    }
}

/** The name of an account's kind, as its provider calls itself. */
private fun AccountKind.title(): Int = when (this) {
    AccountKind.IMAP -> R.string.signin_imap
    AccountKind.GMAIL -> R.string.kind_gmail
    AccountKind.GRAPH -> R.string.kind_outlook
    AccountKind.EXCHANGE -> R.string.signin_exchange
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

/** What each place in [TEXT_SCALES] is called. */
private val SIZE_NAMES = listOf(
    R.string.reading_size_smaller,
    R.string.reading_size_usual,
    R.string.reading_size_larger,
    R.string.reading_size_large,
    R.string.reading_size_largest,
)

@Composable
private fun ChoiceText(text: String, on: Boolean) {
    Text(
        text,
        color = if (on) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
        style = Type.Text,
        fontWeight = Weight.Medium,
        maxLines = 1,
    )
}

/**
 * The size of mail's text, tried on a paragraph as it changes, then whether images load
 * without asking, and the senders whose images do.
 */
@Composable
private fun ReadingSettings(mail: Mail, images: ImagesAlways) {
    val context = LocalContext.current
    val colors = MaterialTheme.colorScheme
    val scope = rememberCoroutineScope()
    val prefs = remember { Settings(context) }
    var size by rememberSaveable { mutableStateOf(prefs.textSize) }
    var senders by remember { mutableStateOf(emptyList<String>()) }
    LaunchedEffect(Unit) {
        try {
            senders = mail.imageSenders()
        } catch (_: CoreException) {
            // Without them the page still sets the text size.
        }
    }
    Caption(stringResource(R.string.reading_size))
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.screen)
            .clip(RoundedCornerShape(Radius.tile))
            .background(colors.surfaceVariant)
            .padding(horizontal = Gap.card, vertical = 14.dp),
    ) {
        Text(stringResource(R.string.reading_sample_from), color = colors.onSurfaceVariant, style = Type.Caption, fontWeight = Weight.SemiBold)
        Text(stringResource(R.string.reading_sample), style = Type.Reading.scaled(TEXT_SCALES[size]), modifier = Modifier.padding(top = 6.dp))
    }
    Row(Modifier.padding(horizontal = Gap.screen).padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text("A", style = Type.Reading.scaled(0.75f), color = colors.onSurfaceVariant)
        Slider(
            value = size.toFloat(),
            onValueChange = {
                if (it.roundToInt() != size) {
                    size = it.roundToInt()
                    prefs.textSize = size
                }
            },
            valueRange = 0f..TEXT_SCALES.lastIndex.toFloat(),
            steps = TEXT_SCALES.size - 2,
            modifier = Modifier.weight(1f).padding(horizontal = 12.dp),
        )
        Text("A", style = Type.Reading.scaled(1.2f), color = colors.onSurfaceVariant)
    }
    Hint("${stringResource(SIZE_NAMES[size])}. ${stringResource(R.string.reading_size_hint)}")
    val shown = images.on ?: return
    Choices(R.string.images_title, listOf(false, true), shown, images.onPick) { option, on ->
        ChoiceText(stringResource(if (option) R.string.images_always else R.string.images_ask), on)
    }
    Hint(stringResource(if (shown) R.string.images_always_hint else R.string.images_ask_hint))
    if (shown || senders.isEmpty()) return
    Caption(stringResource(R.string.images_allowed))
    Section(null) {
        senders.forEach { email ->
            SettingRow(lead = { Glyph(R.drawable.ic_photo, null, colors.onSurfaceVariant) }, title = email, next = false) {
                IconTile(R.drawable.ic_close, stringResource(R.string.images_forbid), {
                    senders = senders - email
                    scope.launch {
                        try {
                            mail.forbidImages(email)
                        } catch (_: CoreException) {
                            senders = senders + email
                        }
                    }
                }, size = 40.dp, tint = colors.onSurfaceVariant)
            }
        }
    }
}

/** Whether images load without asking, null until read, and how to change it. */
private class ImagesAlways(val on: Boolean?, val onPick: (Boolean) -> Unit)

/** [this] a [scale] larger, line height and all. */
fun TextStyle.scaled(scale: Float): TextStyle = copy(fontSize = fontSize * scale, lineHeight = lineHeight * scale)

/** Mail's text at the size the person picked, read when it first shows. */
@Composable
fun readingStyle(style: TextStyle = Type.Reading): TextStyle {
    val context = LocalContext.current
    val scale = remember { Settings(context).textScale }
    return style.scaled(scale)
}

/**
 * A message row the swipes play on: each time a side changes, it slides that way to show what
 * the swipe now does.
 */
@Composable
private fun SwipePreview(swipes: Swipes) {
    val colors = MaterialTheme.colorScheme
    val reach = with(LocalDensity.current) { 96.dp.toPx() }
    val further = with(LocalDensity.current) { 220.dp.toPx() }
    val offset = remember { Animatable(0f) }
    var last by remember { mutableStateOf(swipes) }
    var action by remember { mutableStateOf(swipes.left) }
    LaunchedEffect(swipes) {
        // Left first on the way in, then whichever swipe changed; a long one that is now off shows the short.
        val (way, long) = when {
            swipes.leftLong != last.leftLong -> -1f to swipes.leftLong
            swipes.rightLong != last.rightLong -> 1f to swipes.rightLong
            swipes.right != last.right && swipes.left == last.left -> 1f to null
            else -> -1f to null
        }
        last = swipes
        action = long ?: if (way < 0f) swipes.left else swipes.right
        offset.animateTo(way * if (long != null) further else reach, Motion.settle())
        offset.animateTo(0f, Motion.settle())
    }
    val left = offset.value < 0f
    val (fill, on) = action.colors()
    Box(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.screen, vertical = 8.dp)
            .height(64.dp)
            .clip(RoundedCornerShape(Radius.tile))
            .background(if (offset.value == 0f) colors.surfaceVariant else fill),
    ) {
        Glyph(
            action.icon,
            null,
            on,
            Modifier.align(if (left) Alignment.CenterEnd else Alignment.CenterStart).padding(horizontal = 24.dp),
        )
        Column(
            Modifier
                .fillMaxSize()
                .offset { IntOffset(offset.value.roundToInt(), 0) }
                .background(colors.surfaceVariant)
                .padding(horizontal = Gap.card),
            verticalArrangement = Arrangement.Center,
        ) {
            Text(stringResource(R.string.swipes_sample_from), style = Type.Row, maxLines = 1)
            Text(stringResource(R.string.swipes_sample_subject), color = colors.onSurfaceVariant, style = Type.Subject, maxLines = 1)
        }
    }
}

/** A part of a page under its [title], its rows on one rounded card. */
@Composable
private fun Section(title: String?, rows: @Composable ColumnScope.() -> Unit) {
    if (title != null) Caption(title)
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.screen)
            .padding(bottom = 4.dp)
            .clip(RoundedCornerShape(Radius.tile))
            .background(MaterialTheme.colorScheme.surfaceVariant),
        content = rows,
    )
}

@Composable
private fun Caption(text: String) {
    Text(
        text,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        style = Type.Label,
        modifier = Modifier.padding(start = Gap.inset, end = Gap.inset, top = 18.dp, bottom = 8.dp),
    )
}

/** A line or two on what a setting does, under it. */
@Composable
private fun Hint(text: String, top: Dp = 0.dp, align: TextAlign = TextAlign.Start) {
    Text(
        text,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        style = Type.Caption,
        textAlign = align,
        modifier = Modifier.fillMaxWidth().padding(start = Gap.inset, end = Gap.inset, top = top, bottom = 8.dp),
    )
}

@Composable
private fun SettingRow(
    @DrawableRes icon: Int,
    title: String,
    value: String? = null,
    caption: String? = null,
    next: Boolean = true,
    end: (@Composable RowScope.() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    SettingRow(lead = { Glyph(icon, null, colors.onSurfaceVariant) }, title = title, caption = caption, value = value, next = next, onClick = onClick, end = end)
}

/**
 * A row of a [Section]: what leads it in a slot of its own, its title with a [caption] under
 * it, what it is set to, and an arrow when it opens a page. A row without [onClick] only says.
 */
@Composable
private fun SettingRow(
    lead: @Composable () -> Unit,
    title: String,
    caption: String? = null,
    value: String? = null,
    color: Color = Color.Unspecified,
    next: Boolean = true,
    onClick: (() -> Unit)? = null,
    end: (@Composable RowScope.() -> Unit)? = null,
) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(start = Gap.card, end = 12.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(Gap.slot), contentAlignment = Alignment.Center) { lead() }
        Column(Modifier.padding(start = Gap.afterSlot).weight(1f)) {
            Text(title, color = color, style = Type.Row, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (caption != null) Text(caption, color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (value != null) {
            Text(
                value,
                color = colors.onSurfaceVariant,
                style = Type.Text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                // The title takes the rest, so the value and the arrow sit at the row's end.
                modifier = Modifier.padding(start = 12.dp).widthIn(max = 200.dp),
                textAlign = TextAlign.End,
            )
        }
        end?.invoke(this)
        if (next && onClick != null) Glyph(R.drawable.ic_next, null, colors.onSurfaceVariant, Modifier.padding(start = 6.dp), size = 16.dp)
    }
}

/**
 * A name of one line, [placeholder] while empty, saved with Save, on Done, when it loses focus
 * and when the page closes.
 */
@Composable
private fun NameField(
    key: String,
    saved: String,
    label: String,
    placeholder: String,
    capitalization: KeyboardCapitalization,
    onSave: (String) -> Unit,
) {
    var text by rememberSaveable(key) { mutableStateOf(saved) }
    // What was last sent, so leaving after a save does not send it again.
    var sent by rememberSaveable(key) { mutableStateOf(saved) }
    val focus = LocalFocusManager.current
    fun save() {
        if (text.trim() == sent) return
        sent = text.trim()
        onSave(text)
    }
    val leave by rememberUpdatedState(::save)
    DisposableEffect(key) {
        onDispose { leave() }
    }
    OutlinedTextField(
        value = text,
        onValueChange = { text = it },
        label = { Text(label) },
        placeholder = { Text(placeholder) },
        trailingIcon = if (text.trim() != sent) {
            {
                TextButton(onClick = {
                    focus.clearFocus()
                    save()
                }) { Text(stringResource(R.string.action_save)) }
            }
        } else {
            null
        },
        shape = RoundedCornerShape(Radius.medium),
        singleLine = true,
        keyboardOptions = KeyboardOptions(capitalization = capitalization, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = {
            focus.clearFocus()
            save()
        }),
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = Gap.screen)
            .onFocusChanged { if (!it.isFocused) save() },
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
            .padding(horizontal = Gap.screen)
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
        Modifier.fillMaxWidth().padding(horizontal = Gap.screen).padding(bottom = 12.dp).selectableGroup(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        rows.forEach { row ->
            // Edge to edge, as wide as the fields above.
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

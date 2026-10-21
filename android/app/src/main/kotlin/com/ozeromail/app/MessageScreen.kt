package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.ImagesPage
import uniffi.ozero_api.MessageBody
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.RemoteImage

/**
 * A conversation, oldest first, on one page. The message opened from the list is open; the
 * others sit closed as a line each, unread ones in bold, and a tap opens or closes any of them.
 * A message on its own fills the screen, so a wide newsletter can be pinched. Reply, Reply
 * all and Done sit at the bottom, in reach of the thumb.
 */
@Composable
fun MessageScreen(
    mail: Mail,
    message: MessageItem,
    onReply: (MessageItem, all: Boolean) -> Unit,
    onDelete: () -> Unit,
    onUnread: () -> Unit,
    onDone: (() -> Unit)?,
    onBack: () -> Unit,
) {
    var more by remember(message.id) { mutableStateOf(false) }
    var thread by remember(message.id) { mutableStateOf(listOf(message)) }
    var error by remember(message.id) { mutableStateOf<Int?>(null) }
    // What the user opened or closed; at first only the message from the list is open.
    val opened = remember(message.id) { mutableStateMapOf<Long, Boolean>() }
    var showAll by remember(message.id) { mutableStateOf(false) }
    // Loaded bodies, so a message opened again shows at once.
    val bodies = remember(message.id) { mutableStateMapOf<Long, MessageBody>() }
    DisposableEffect(bodies) { onDispose { expectGone(bodies, "bodies of a closed conversation") } }
    LaunchedEffect(message.id) {
        try {
            thread = mail.thread(message)
            if (message.threadUnread > 0u) mail.markRead(message)
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    fun isOpen(m: MessageItem) = opened[m.id] ?: (m.id == message.id)
    // The newest message not from the user.
    val answer = thread.lastOrNull { !it.sent } ?: message

    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))) {
            BackArrow(onBack, Modifier.padding(top = 4.dp))
            if (thread.size > 1 || message.threadCount > 1u) {
                Column(Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState())) {
                    Title(message)
                    // A long run of closed messages in the middle folds into one line.
                    val middle = 1 until thread.size - 2
                    val folded = !showAll && middle.count() >= 2 && middle.none { isOpen(thread[it]) }
                    thread.forEachIndexed { i, m ->
                        if (i == 1) {
                            Reveal(middle.count().takeIf { folded }) { count -> More(count, onClick = { showAll = true }) }
                        }
                        key(m.id) {
                            val letter = @Composable { Letter(mail, m, bodies, isOpen(m), onToggle = { opened[m.id] = !isOpen(m) }) }
                            if (i in middle) {
                                AnimatedVisibility(!folded, enter = opening, exit = closing) { letter() }
                            } else {
                                letter()
                            }
                        }
                    }
                    Reveal(error) { Text(stringResource(it), color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(Gap.screen)) }
                    Spacer(Modifier.height(12.dp))
                }
            } else {
                Title(message)
                From(message, open = true)
                HorizontalDivider(Modifier.padding(top = 4.dp), color = MaterialTheme.colorScheme.outlineVariant)
                LetterBody(mail, message, bodies, Modifier.weight(1f), fill = true)
            }
            Column(
                Modifier.fillMaxWidth().navigationBarsPadding().padding(top = Gap.barTop, bottom = Gap.barBottom),
                verticalArrangement = Arrangement.spacedBy(Gap.inBar),
            ) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = Gap.bar),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Gap.inBar),
                ) {
                    Pill(
                        stringResource(R.string.message_reply_prompt),
                        onClick = { onReply(answer, false) },
                        modifier = Modifier.weight(1f),
                        height = 46.dp,
                    )
                    IconTile(
                        R.drawable.ic_reply_all,
                        stringResource(R.string.message_reply_all),
                        { onReply(answer, true) },
                        shape = CircleShape,
                        size = 46.dp,
                    )
                }
                Actions(onDelete = onDelete, forever = !message.canTrash, onMore = { more = true }, onDone = onDone)
            }
        }
        // Like every menu, the rarer actions open in a sheet, low in reach of the thumb.
        Sheet(visible = more, onClose = { more = false }) {
            SheetAction(R.drawable.ic_unread, stringResource(R.string.action_mark_unread), MaterialTheme.colorScheme.onSurface) {
                more = false
                onUnread()
            }
        }
    }
}

/** A message opens slowly from its line down, and closes a little quicker. */
private val opening = expandVertically(Motion.calm(Motion.SCREEN), expandFrom = Alignment.Top) + fadeIn(Motion.calm(Motion.SCREEN))
private val closing = shrinkVertically(Motion.calm(Motion.NOTE), shrinkTowards = Alignment.Top) + fadeOut(Motion.calm(Motion.SMALL))

/** One message of a conversation: its line, and under it the message when open. */
@Composable
private fun Letter(mail: Mail, message: MessageItem, bodies: MutableMap<Long, MessageBody>, open: Boolean, onToggle: () -> Unit) {
    Column(Modifier.fillMaxWidth()) {
        HorizontalDivider(Modifier.padding(horizontal = Gap.screen), color = MaterialTheme.colorScheme.outlineVariant)
        From(message, open, onToggle)
        AnimatedVisibility(open, enter = opening, exit = closing) {
            LetterBody(mail, message, bodies, Modifier.fillMaxWidth(), fill = false)
        }
    }
}

/** The line of closed messages folded away; a tap shows them. */
@Composable
private fun More(count: Int, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxWidth()) {
        HorizontalDivider(Modifier.padding(horizontal = Gap.screen), color = colors.outlineVariant)
        Text(
            stringResource(R.string.message_more, count),
            color = colors.primary,
            style = Type.Text,
            fontWeight = Weight.SemiBold,
            modifier = Modifier
                .fillMaxWidth()
                .clickable(role = Role.Button, onClick = onClick)
                .padding(horizontal = Gap.screen, vertical = 13.dp),
        )
    }
}

/**
 * A message's page, or its text with the quote folded; it loads when shown, once into
 * [bodies]. With [fill] it takes the space it is given and scrolls itself; otherwise it is
 * as tall as it is.
 */
@Composable
private fun LetterBody(mail: Mail, message: MessageItem, bodies: MutableMap<Long, MessageBody>, modifier: Modifier, fill: Boolean) {
    // A page whose images load on their own waits for them again, as on the first time.
    var body by remember(message.id) { mutableStateOf(bodies[message.id]?.takeUnless { it.remoteImages > 0u && it.showsImages }) }
    var error by remember(message.id) { mutableStateOf<Int?>(null) }
    // The page again with its remote images.
    var withImages by remember(message.id) { mutableStateOf<ImagesPage?>(null) }
    var imagesError by remember(message.id) { mutableStateOf<Int?>(null) }
    val scope = rememberCoroutineScope()
    suspend fun loadImages() {
        try {
            mail.images(message)?.let { withImages = it }
        } catch (e: CoreException) {
            imagesError = e.text()
        }
    }
    LaunchedEffect(message.id) {
        try {
            val loaded = bodies[message.id] ?: mail.body(message).also { bodies[message.id] = it }
            if (loaded.remoteImages > 0u && loaded.showsImages) {
                // Wait a moment for the images, so the page does not load twice and jump.
                coroutineScope {
                    val images = async { loadImages() }
                    withTimeoutOrNull(IMAGES_WAIT_MS) { images.await() }
                    body = loaded
                }
            } else {
                body = loaded
            }
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    val loaded = body
    val full = withImages
    val imagesFailed = imagesError
    val imagesNote = when {
        loaded == null || full != null -> null
        imagesFailed != null -> imagesFailed
        loaded.remoteImages > 0u && !loaded.showsImages -> R.string.message_images_hidden
        else -> null
    }
    val failed = error
    val html = loaded?.html
    val styled = loaded?.styled == true
    val shown = when {
        full != null -> Body.Page(full.html, full.images, withImages = true, styled)
        html != null -> Body.Page(html, emptyList(), withImages = false, styled)
        loaded != null -> Body.Plain(loaded.text, loaded.quote)
        failed != null -> Body.Failed(failed)
        else -> Body.Loading
    }
    // A page grows in steps as it lays out; only the short text and notes animate.
    Column(if (fill || shown is Body.Page) modifier else modifier.animateContentSize(Motion.calm(Motion.NOTE))) {
        Reveal(imagesNote) { note ->
            HiddenImages(note, onShow = onShow@{
                // The note is closing: the images are here already.
                if (withImages != null) return@onShow
                imagesError = null
                scope.launch {
                    try {
                        mail.allowImages(message)
                        // The sender's other messages load theirs on their own now too.
                        val allowed = bodies[message.id]?.copy(showsImages = true)
                        bodies.clear()
                        allowed?.let { bodies[message.id] = it }
                        loadImages()
                    } catch (e: CoreException) {
                        imagesError = e.text()
                    }
                }
            })
        }
        // The page fades in over the spinner instead of popping in; the old one fades out whole.
        AnimatedContent(
            shown,
            if (fill) Modifier.weight(1f).fillMaxWidth() else Modifier.fillMaxWidth(),
            transitionSpec = { (fadeIn(Motion.calm(Motion.SMALL)) togetherWith fadeOut(Motion.calm(Motion.SMALL))).using(null) },
            contentKey = { it::class },
            label = "body",
        ) { kind ->
            val content = if (fill) Modifier.fillMaxSize() else Modifier.fillMaxWidth()
            when (kind) {
                // A new web view per page: it loads its page only once.
                // A newsletter keeps the light page it was made for, as a card on the screen.
                is Body.Page -> key(kind.withImages) {
                    val page = if (kind.styled) {
                        content.padding(start = 10.dp, end = 10.dp, top = 4.dp, bottom = 10.dp).clip(RoundedCornerShape(Radius.tile))
                    } else {
                        content.padding(top = 4.dp)
                    }
                    HtmlView(kind.html, kind.images, page)
                }
                is Body.Plain -> PlainText(kind.text, kind.quote, if (fill) content.verticalScroll(rememberScrollState()) else content)
                is Body.Failed -> Text(stringResource(kind.text), color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = content.padding(Gap.screen))
                Body.Loading -> Box(if (fill) content else content.height(96.dp), contentAlignment = Alignment.Center) {
                    SlowSpinner()
                }
            }
        }
    }
}

/** A spinner only for a wait long enough to notice; a saved message shows without one. */
@Composable
private fun SlowSpinner() {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        delay(SPINNER_DELAY_MS)
        shown = true
    }
    if (shown) CircularProgressIndicator()
}

/** The text, and the quote under it behind a mark that opens it. */
@Composable
private fun PlainText(text: String, quote: String, modifier: Modifier) {
    var showQuote by remember { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    SelectionContainer(modifier.padding(start = Gap.screen, end = Gap.screen, top = 6.dp, bottom = 18.dp)) {
        Column(Modifier.widthIn(max = 620.dp)) {
            if (text.isNotEmpty()) Text(text, style = Type.Reading)
            if (quote.isNotEmpty()) {
                val label = stringResource(R.string.message_quote)
                Text(
                    "•••",
                    color = colors.onSurfaceVariant,
                    style = Type.Caption,
                    fontWeight = Weight.Bold,
                    letterSpacing = 1.sp,
                    modifier = Modifier
                        .padding(top = 10.dp)
                        .clip(RoundedCornerShape(9.dp))
                        .background(colors.surfaceVariant)
                        .clickable(role = Role.Button) { showQuote = !showQuote }
                        .semantics { contentDescription = label }
                        .padding(horizontal = 10.dp, vertical = 1.dp),
                )
                Reveal(quote.takeIf { showQuote }) {
                    Text(it, color = colors.onSurfaceVariant, style = Type.Quote, modifier = Modifier.padding(top = 10.dp))
                }
            }
        }
    }
}

/**
 * The actions under the thumb, each in an equal place; Done, the one used most, takes the last.
 * Delete and Done are null in a folder where they do not apply, such as Done in the archive;
 * Delete is [forever] in the trash.
 */
@Composable
private fun Actions(onDelete: () -> Unit, forever: Boolean, onMore: () -> Unit, onDone: (() -> Unit)?) {
    val colors = MaterialTheme.colorScheme
    Row(Modifier.fillMaxWidth().padding(horizontal = Gap.bar), horizontalArrangement = Arrangement.spacedBy(Gap.inBar)) {
        val delete = if (forever) R.string.action_delete_forever else R.string.action_delete
        BarAction(R.drawable.ic_delete, stringResource(delete), colors.onSurface, onClick = onDelete)
        BarAction(R.drawable.ic_more, stringResource(R.string.action_more), colors.onSurface, onClick = onMore)
        onDone?.let { BarAction(R.drawable.ic_done, stringResource(R.string.action_done), colors.onPrimary, colors.primary, it) }
    }
}

/** One of the big buttons along the bottom of a screen: an icon over its name. */
@Composable
fun RowScope.BarAction(@DrawableRes icon: Int, label: String, tint: Color, background: Color = Color.Transparent, onClick: () -> Unit) {
    Column(
        Modifier
            .weight(1f)
            .height(52.dp)
            .clip(RoundedCornerShape(Radius.medium))
            .background(background)
            .clickable(role = Role.Button, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(3.dp, Alignment.CenterVertically),
    ) {
        Glyph(icon, null, tint, size = 21.dp)
        Text(label, color = tint, style = Type.Label, maxLines = 1)
    }
}

/** A note that the remote images are held back, with a way to see them from now on. */
@Composable
private fun HiddenImages(note: Int, onShow: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = Gap.card, end = Gap.card, top = 4.dp, bottom = 8.dp)
            .background(colors.surfaceVariant, RoundedCornerShape(Radius.medium))
            .padding(start = Gap.inCard, end = 8.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            stringResource(note),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            modifier = Modifier.weight(1f),
        )
        Text(
            stringResource(R.string.action_show),
            color = colors.primary,
            style = Type.Caption,
            fontWeight = Weight.SemiBold,
            modifier = Modifier
                .clip(CircleShape)
                .background(colors.background)
                .clickable(role = Role.Button, onClick = onShow)
                .padding(horizontal = 12.dp, vertical = 7.dp),
        )
    }
}

/**
 * Who sent a message and when. Open, it shows the address and the full date; closed, the
 * start of the text. With [onToggle] a tap opens or closes the message.
 */
@Composable
private fun From(message: MessageItem, open: Boolean, onToggle: (() -> Unit)? = null) {
    val colors = MaterialTheme.colorScheme
    val unread = message.threadUnread > 0u
    val detail = when {
        !open -> message.snippet
        message.sender != message.fromEmail -> message.fromEmail
        else -> ""
    }
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (onToggle == null) Modifier else Modifier.clickable(onClick = onToggle))
            .padding(horizontal = Gap.screen, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Avatar(message.sender, message.fromEmail, size = if (open) 40.dp else 32.dp)
        Column(Modifier.weight(1f)) {
            Text(
                message.sender,
                style = Type.Row,
                fontWeight = if (unread) Weight.Bold else Weight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (detail.isNotBlank()) {
                Text(detail, color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        val context = LocalContext.current
        Text(
            if (open) longDate(context, message.date) else shortDate(context, message.date),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            modifier = Modifier.align(Alignment.Top).padding(top = 2.dp),
        )
    }
}

/** The subject and the account it came to. */
@Composable
private fun Title(message: MessageItem) {
    val colors = MaterialTheme.colorScheme
    SelectionContainer(Modifier.padding(top = 6.dp)) {
        Column {
            Text(
                message.subjectText(),
                style = Type.Title,
                modifier = Modifier.padding(start = Gap.screen, end = Gap.screen, bottom = 10.dp),
            )
            message.account?.let { account ->
                Row(
                    Modifier
                        .padding(start = Gap.screen, end = Gap.screen, bottom = 8.dp)
                        .background(colors.surfaceVariant, CircleShape)
                        .padding(horizontal = 11.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    AccountDot(account.tint, size = 8.dp)
                    Text(account.name, color = colors.onSurfaceVariant, style = Type.Caption, fontWeight = Weight.Medium)
                }
            }
        }
    }
}

/** What a message's body area shows. */
private sealed interface Body {
    data object Loading : Body
    data class Page(val html: String, val images: List<RemoteImage?>, val withImages: Boolean, val styled: Boolean) : Body
    data class Plain(val text: String, val quote: String) : Body
    data class Failed(val text: Int) : Body
}

/** How long a load runs before its spinner shows. */
private const val SPINNER_DELAY_MS = 400L

/** How long a page waits for its images before it shows without them. */
private const val IMAGES_WAIT_MS = 2_000L

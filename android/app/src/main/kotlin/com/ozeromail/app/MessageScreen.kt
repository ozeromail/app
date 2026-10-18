package com.ozeromail.app

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.ImagesPage
import uniffi.ozero_api.MessageBody
import uniffi.ozero_api.MessageItem

/**
 * A conversation, opened at the list's message: its HTML when it has one, otherwise the
 * text. The other messages, the user's replies among them, sit above it as cards; a tap
 * shows one. Reply, Reply all and Done sit at the bottom, in reach of the thumb; system back returns
 * to the list.
 */
@Composable
fun MessageScreen(
    mail: Mail,
    message: MessageItem,
    onBack: () -> Unit,
    onReply: (MessageItem, all: Boolean) -> Unit,
    onDone: () -> Unit,
) {
    BackHandler(onBack = onBack)
    var thread by remember(message.id) { mutableStateOf(listOf(message)) }
    var shown by remember(message.id) { mutableStateOf(message) }
    // With the id of its message, so a page is never shown under another message's header.
    var body by remember(message.id) { mutableStateOf<Pair<Long, MessageBody>?>(null) }
    var error by remember(message.id) { mutableStateOf<Int?>(null) }
    // The page again with its remote images, by message id.
    var withImages by remember(message.id) { mutableStateOf<Pair<Long, ImagesPage>?>(null) }
    var imagesError by remember(message.id) { mutableStateOf<Int?>(null) }
    LaunchedEffect(message.id) {
        try {
            thread = mail.thread(message)
            if (message.threadUnread > 0u) mail.markRead(message)
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    // Kept only while [of] is still shown: another message's images can be many megabytes.
    suspend fun loadImages(of: MessageItem) {
        try {
            mail.images(of)?.let { if (shown.id == of.id) withImages = of.id to it }
        } catch (e: CoreException) {
            if (shown.id == of.id) imagesError = e.text()
        }
    }
    LaunchedEffect(shown.id) {
        withImages = null
        imagesError = null
        try {
            val loaded = mail.body(shown)
            if (loaded.remoteImages > 0u && loaded.showsImages) {
                // Wait a moment for the images, so the page does not load twice and jump.
                coroutineScope {
                    val images = async { loadImages(shown) }
                    withTimeoutOrNull(IMAGES_WAIT_MS) { images.await() }
                    body = shown.id to loaded
                }
            } else {
                body = shown.id to loaded
            }
        } catch (e: CoreException) {
            error = e.text()
        }
    }
    val scope = rememberCoroutineScope()
    // The newest message not from the user, when the user's own reply is shown.
    val answer = if (shown.sent) thread.lastOrNull { !it.sent } ?: message else shown

    val colors = MaterialTheme.colorScheme
    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        Header(shown, Modifier.padding(top = 12.dp))
        val older = thread.filter { it.id != shown.id }
        if (older.isNotEmpty()) {
            Column(Modifier.heightIn(max = 220.dp).verticalScroll(rememberScrollState()).padding(top = 12.dp)) {
                older.forEach { other ->
                    Older(other, onClick = {
                        error = null
                        shown = other
                    })
                }
            }
        }
        HorizontalDivider(Modifier.padding(top = 14.dp), color = colors.outlineVariant)
        val loaded = body?.takeIf { it.first == shown.id }?.second
        val full = withImages?.takeIf { it.first == shown.id }?.second
        val html = full?.html ?: loaded?.html
        val imagesFailed = imagesError
        if (loaded != null && full == null && (imagesFailed != null || loaded.remoteImages > 0u && !loaded.showsImages)) {
            HiddenImages(imagesFailed ?: R.string.message_images_hidden, onShow = {
                val of = shown
                imagesError = null
                scope.launch {
                    try {
                        mail.allowImages(of)
                        loadImages(of)
                    } catch (e: CoreException) {
                        imagesError = e.text()
                    }
                }
            })
        }
        val failed = error
        val content = Modifier.weight(1f).fillMaxWidth()
        when {
            // A new web view per message: it loads its page only once.
            html != null -> key(shown.id, full != null) { HtmlView(html, full?.images.orEmpty(), content) }
            loaded != null -> SelectionContainer(content.verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 18.dp)) {
                Text(loaded.text, fontSize = 15.5.sp, lineHeight = 24.8.sp, modifier = Modifier.widthIn(max = 620.dp))
            }
            failed != null -> Text(stringResource(failed), color = colors.onSurfaceVariant, modifier = content.padding(20.dp))
            else -> Box(content.padding(32.dp), contentAlignment = Alignment.Center) {
                CircularProgressIndicator()
            }
        }
        Column(
            Modifier.fillMaxWidth().navigationBarsPadding().padding(top = 8.dp, bottom = 8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
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
            Actions(onDone)
        }
    }
}

/**
 * The actions under the thumb, in four equal places as more will come; Done, the one used
 * most, takes the last.
 */
@Composable
private fun Actions(onDone: () -> Unit) {
    val done = MaterialTheme.done
    Row(Modifier.fillMaxWidth().padding(horizontal = 10.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        repeat(3) { Spacer(Modifier.weight(1f)) }
        Column(
            Modifier
                .weight(1f)
                .clip(RoundedCornerShape(14.dp))
                .background(done)
                .clickable(role = Role.Button, onClick = onDone)
                .padding(vertical = 6.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            Glyph(R.drawable.ic_done, null, Color.White)
            Text(stringResource(R.string.action_done), color = Color.White, fontSize = 11.5.sp)
        }
    }
}

/** A note that the remote images are held back, with a way to see them from now on. */
@Composable
private fun HiddenImages(note: Int, onShow: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 16.dp, top = 12.dp)
            .background(colors.surfaceVariant, RoundedCornerShape(14.dp))
            .padding(start = 14.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            stringResource(note),
            color = colors.onSurfaceVariant,
            fontSize = 13.sp,
            lineHeight = 17.5.sp,
            modifier = Modifier.weight(1f),
        )
        Text(
            stringResource(R.string.action_show),
            color = colors.primary,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier
                .clip(CircleShape)
                .background(colors.background)
                .clickable(role = Role.Button, onClick = onShow)
                .padding(horizontal = 12.dp, vertical = 7.dp),
        )
    }
}

/** Another message of the conversation, as a short card. */
@Composable
private fun Older(message: MessageItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 16.dp, bottom = 8.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(colors.surfaceVariant)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Avatar(message.sender, message.fromEmail, size = 32.dp)
        Column(Modifier.weight(1f)) {
            Text(message.sender, fontWeight = FontWeight.SemiBold, fontSize = 13.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (message.snippet.isNotBlank()) {
                Text(message.snippet, color = colors.onSurfaceVariant, fontSize = 13.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Text(
            shortDate(LocalContext.current, message.date),
            color = colors.onSurfaceVariant,
            fontSize = 12.sp,
            modifier = Modifier.align(Alignment.Top),
        )
    }
}

/** The subject, the account it came to and who sent it. */
@Composable
private fun Header(message: MessageItem, modifier: Modifier) {
    val colors = MaterialTheme.colorScheme
    SelectionContainer(modifier) {
        Column {
            Text(
                message.subjectText(),
                fontSize = 23.sp,
                lineHeight = 28.sp,
                fontWeight = FontWeight.Bold,
                letterSpacing = (-0.46).sp,
                modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 6.dp, bottom = 10.dp),
            )
            message.account?.let { account ->
                Row(
                    Modifier
                        .padding(start = 20.dp, end = 20.dp, bottom = 14.dp)
                        .background(colors.surfaceVariant, CircleShape)
                        .padding(horizontal = 11.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(7.dp),
                ) {
                    AccountDot(account.tint, size = 8.dp)
                    Text(account.name, color = colors.onSurfaceVariant, fontSize = 12.5.sp, fontWeight = FontWeight.Medium)
                }
            }
            Row(
                Modifier.padding(horizontal = 20.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Avatar(message.sender, message.fromEmail)
                Column(Modifier.weight(1f)) {
                    Text(message.sender, fontWeight = FontWeight.SemiBold, fontSize = 15.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (message.sender != message.fromEmail) {
                        Text(message.fromEmail, color = colors.onSurfaceVariant, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
                Text(
                    longDate(LocalContext.current, message.date),
                    color = colors.onSurfaceVariant,
                    fontSize = 12.5.sp,
                    modifier = Modifier.align(Alignment.Top).padding(top = 4.dp),
                )
            }
        }
    }
}

/** How long a page waits for its images before it shows without them. */
private const val IMAGES_WAIT_MS = 2_000L

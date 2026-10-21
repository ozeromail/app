@file:OptIn(ExperimentalFoundationApi::class)

package com.ozeromail.app

import android.app.DownloadManager
import android.content.ActivityNotFoundException
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.provider.MediaStore
import android.text.format.Formatter
import android.webkit.MimeTypeMap
import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Image
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import java.io.File
import java.io.IOException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.ozero_api.AttachmentItem
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem

/** What the app can show of a file itself. */
enum class Preview { Pdf, Image, None }

val AttachmentItem.preview: Preview
    get() = when {
        mime == "application/pdf" || extension == "pdf" -> Preview.Pdf
        mime in IMAGES || extension in IMAGE_EXTENSIONS -> Preview.Image
        else -> Preview.None
    }

private val AttachmentItem.extension: String get() = name.substringAfterLast('.', "").lowercase()

/** The type to hand to other apps: as sent, or from the name when the sender said nothing useful. */
private val AttachmentItem.type: String
    get() = mime.takeUnless { it == "application/octet-stream" }
        ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
        ?: mime

/** A name safe to save under, with an extension when the sender gave none. */
fun AttachmentItem.fileName(untitled: String): String {
    val clean = name.replace(UNSAFE, "_").trim('.', ' ')
    if (clean.isEmpty()) {
        val ext = MimeTypeMap.getSingleton().getExtensionFromMimeType(mime)
        return if (ext == null) untitled else "$untitled.$ext"
    }
    if (clean.toByteArray().size <= MAX_NAME_BYTES) return clean
    // File systems cap a name at 255 bytes; keep the extension so apps still know the type.
    val dot = clean.lastIndexOf('.')
    val ext = if (dot > 0 && clean.length - dot <= MAX_EXTENSION) clean.substring(dot) else ""
    var stem = clean.removeSuffix(ext)
    while ((stem + ext).toByteArray().size > MAX_NAME_BYTES) {
        stem = stem.dropLast(if (stem.length > 1 && stem[stem.length - 1].isLowSurrogate()) 2 else 1)
    }
    return stem.trimEnd('.', ' ') + ext
}

@Composable
fun AttachmentItem.title(): String = attachmentTitle(name)

/** A file's name as sent, or a word for it when it came without one. */
@Composable
fun attachmentTitle(name: String): String = name.ifBlank { stringResource(R.string.attachment_untitled) }

/** The kind at a glance, "PDF", from the name or else the type. */
private val AttachmentItem.kind: String
    get() = extension.uppercase().takeIf { it.length in 1..4 } ?: mime.substringAfter('/').uppercase().take(4)

/** "PDF · 84 kB": the kind, then the size. */
@Composable
private fun AttachmentItem.details(): String =
    "$kind · ${Formatter.formatShortFileSize(LocalContext.current, size.toLong())}"

/** A file being viewed, with the others it is paged with: a message's images. */
class Viewing(val message: MessageItem, val files: List<AttachmentItem>, val first: Int)

/**
 * The attachments of the open conversation: downloads, the viewer, the actions sheet and what
 * they say in a toast. Downloads run in [scope], so they stop when the screen closes.
 */
@Stable
class Files(
    private val mail: Mail,
    private val context: Context,
    private val scope: CoroutineScope,
    private val snackbar: SnackbarHostState,
) {
    /** Files downloading, by [key]; their row shows a spinner. */
    private val loading = mutableStateListOf<String>()
    /** One download at a time: a file asked for again meanwhile is then on the device already. */
    private val downloads = Mutex()
    var viewing by mutableStateOf<Viewing?>(null)
    /** The file whose actions sheet is open. */
    var actions by mutableStateOf<Pair<MessageItem, AttachmentItem>?>(null)

    fun isLoading(message: MessageItem, file: AttachmentItem) = key(message, file) in loading

    /** The files on the device, those not here yet downloaded first, together. */
    suspend fun files(message: MessageItem, items: List<AttachmentItem>): List<File> =
        downloads.withLock { mail.attachments(message, items) }

    /** Shows a PDF or an image here, paged with the message's other images; anything else goes to another app. */
    fun open(message: MessageItem, file: AttachmentItem, all: List<AttachmentItem>) {
        when (file.preview) {
            Preview.Pdf -> viewing = Viewing(message, listOf(file), 0)
            Preview.Image -> {
                val images = all.filter { it.preview == Preview.Image }
                viewing = Viewing(message, images, images.indexOf(file).coerceAtLeast(0))
            }
            Preview.None -> openWith(message, file)
        }
    }

    fun share(message: MessageItem, file: AttachmentItem) = withFile(message, file) {
        val send = Intent(Intent.ACTION_SEND)
            .setType(file.type)
            .putExtra(Intent.EXTRA_STREAM, uri(it))
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        start(Intent.createChooser(send, null))
    }

    fun openWith(message: MessageItem, file: AttachmentItem) = withFile(message, file) {
        val view = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri(it), file.type)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        start(Intent.createChooser(view, null))
    }

    fun save(message: MessageItem, file: AttachmentItem) = withFile(message, file) {
        if (saved(it, file)) say(R.string.attachment_saved, showDownloads = true)
    }

    fun saveAll(message: MessageItem, items: List<AttachmentItem>) {
        scope.launch {
            val got = fetch(message, items) ?: return@launch
            if (got.zip(items).all { (file, item) -> saved(file, item) }) {
                say(context.getString(R.string.attachment_saved_all, items.size), showDownloads = true)
            }
        }
    }

    /** Downloads the file with its row's spinner, then hands it on. */
    private fun withFile(message: MessageItem, file: AttachmentItem, then: suspend (File) -> Unit) {
        scope.launch { fetch(message, listOf(file))?.let { then(it.single()) } }
    }

    /** The files, with their rows' spinners while they download; null if that failed, and said so. */
    private suspend fun fetch(message: MessageItem, items: List<AttachmentItem>): List<File>? {
        val keys = items.map { key(message, it) }
        loading.addAll(keys)
        return try {
            files(message, items)
        } catch (e: CoreException) {
            say(e.text())
            null
        } finally {
            loading.removeAll(keys)
        }
    }

    /** Copies the file into Downloads, where the person finds it in Files; false if that failed, and said so. */
    private suspend fun saved(file: File, item: AttachmentItem): Boolean {
        val ok = withContext(Dispatchers.IO) {
            val resolver = context.contentResolver
            val row = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, file.name)
                put(MediaStore.Downloads.MIME_TYPE, item.type)
                put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, row) ?: return@withContext false
            try {
                val out = resolver.openOutputStream(uri) ?: throw IOException("no stream for $uri")
                out.use { file.inputStream().use { input -> input.copyTo(it) } }
                resolver.update(uri, ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }, null, null)
                true
            } catch (e: IOException) {
                resolver.delete(uri, null, null)
                false
            }
        }
        if (!ok) say(R.string.attachment_not_saved)
        return ok
    }

    private fun uri(file: File) = FileProvider.getUriForFile(context, "${context.packageName}.files", file)

    private fun start(intent: Intent) {
        try {
            context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (e: ActivityNotFoundException) {
            scope.launch { say(R.string.attachment_no_app) }
        }
    }

    private suspend fun say(@StringRes text: Int, showDownloads: Boolean = false) = say(context.getString(text), showDownloads)

    private suspend fun say(text: String, showDownloads: Boolean = false) {
        val action = if (showDownloads) context.getString(R.string.action_show) else null
        val result = snackbar.showSnackbar(text, action, duration = SnackbarDuration.Short)
        if (result == SnackbarResult.ActionPerformed) {
            start(Intent(DownloadManager.ACTION_VIEW_DOWNLOADS))
        }
    }

    private fun key(message: MessageItem, file: AttachmentItem) = "${message.id}/${file.part}"
}

/** The files sent with a message, under it: a row each, and Save all for more than one. */
@Composable
fun AttachmentList(files: Files, message: MessageItem, items: List<AttachmentItem>, modifier: Modifier = Modifier) {
    val colors = MaterialTheme.colorScheme
    Column(modifier.fillMaxWidth().padding(start = Gap.card, end = Gap.card, top = 10.dp, bottom = 6.dp)) {
        Row(Modifier.fillMaxWidth().padding(start = 6.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                stringResource(R.string.message_attachments),
                color = colors.onSurfaceVariant,
                style = Type.Caption,
                fontWeight = Weight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            if (items.size > 1) {
                Text(
                    stringResource(R.string.attachment_save_all),
                    color = colors.primary,
                    style = Type.Caption,
                    fontWeight = Weight.SemiBold,
                    modifier = Modifier
                        .clip(CircleShape)
                        .clickable(role = Role.Button) { files.saveAll(message, items) }
                        .padding(horizontal = 8.dp, vertical = 4.dp),
                )
            }
        }
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(Radius.tile)).background(colors.surfaceContainerLow)) {
            items.forEach { file -> AttachmentRow(files, message, file, items) }
        }
    }
}

@Composable
private fun AttachmentRow(files: Files, message: MessageItem, file: AttachmentItem, all: List<AttachmentItem>) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .fillMaxWidth()
            .clickable { files.open(message, file, all) }
            .padding(start = 12.dp, top = 8.dp, bottom = 8.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        FileTile(file, loading = files.isLoading(message, file))
        Column(Modifier.weight(1f)) {
            Text(file.title(), style = Type.Body, fontWeight = Weight.Medium, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            Text(file.details(), color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1)
        }
        IconTile(
            R.drawable.ic_more,
            stringResource(R.string.action_more),
            onClick = { files.actions = message to file },
            tint = colors.onSurfaceVariant,
            shape = CircleShape,
            size = 40.dp,
        )
    }
}

/** The file's kind in a small tile; a spinner over it while it downloads. */
@Composable
private fun FileTile(file: AttachmentItem, loading: Boolean) {
    val colors = MaterialTheme.colorScheme
    Box(
        Modifier.size(40.dp).clip(RoundedCornerShape(10.dp)).background(colors.primaryContainer),
        contentAlignment = Alignment.Center,
    ) {
        if (loading) {
            Spinner(colors.primary)
        } else {
            Text(
                file.kind,
                color = colors.onPrimaryContainer,
                style = Type.Badge,
                maxLines = 1,
            )
        }
    }
}

/** The paperclip with the number of files, by the date of an open message; a tap leads to them. */
@Composable
fun AttachmentMark(count: Int, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val label = stringResource(R.string.attachment_count, count)
    Row(
        Modifier
            .clip(CircleShape)
            .clickable(onClickLabel = label, role = Role.Button, onClick = onClick)
            .padding(horizontal = 6.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Glyph(R.drawable.ic_attach, label, colors.onSurfaceVariant, size = 15.dp)
        Text(count.toString(), color = colors.onSurfaceVariant, style = Type.Caption)
    }
}

/** The names of a message's files in the list, the first two and how many more. */
@Composable
fun AttachmentChips(names: List<String>) {
    val colors = MaterialTheme.colorScheme
    Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        names.take(CHIPS).forEach { name ->
            Row(
                Modifier
                    .weight(1f, fill = false)
                    .widthIn(max = 168.dp)
                    .border(1.dp, colors.outlineVariant, RoundedCornerShape(8.dp))
                    .padding(start = 6.dp, end = 9.dp, top = 4.dp, bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Glyph(R.drawable.ic_attach, null, colors.onSurfaceVariant, size = 14.dp)
                Text(
                    attachmentTitle(name),
                    color = colors.onSurfaceVariant,
                    style = Type.Caption,
                    maxLines = 1,
                    overflow = TextOverflow.MiddleEllipsis,
                )
            }
        }
        if (names.size > CHIPS) {
            Text(
                "+${names.size - CHIPS}",
                color = colors.onSurfaceVariant,
                style = Type.Caption,
                modifier = Modifier
                    .border(1.dp, colors.outlineVariant, RoundedCornerShape(8.dp))
                    .padding(horizontal = 8.dp, vertical = 4.dp),
            )
        }
    }
}

/** What can be done with one file, low in reach of the thumb. */
@Composable
fun AttachmentActions(files: Files) {
    // The sheet keeps its file while it closes.
    val shown = rememberLast(files.actions)
    Sheet(visible = files.actions != null, onClose = { files.actions = null }) {
        val (message, file) = shown ?: return@Sheet
        val colors = MaterialTheme.colorScheme
        SheetTitle(file.title())
        fun act(action: () -> Unit) {
            files.actions = null
            action()
        }
        if (file.preview != Preview.None) {
            SheetAction(R.drawable.ic_attach, stringResource(R.string.attachment_open), colors.onSurface) {
                act { files.open(message, file, listOf(file)) }
            }
        }
        SheetAction(R.drawable.ic_share, stringResource(R.string.attachment_share), colors.onSurface) {
            act { files.share(message, file) }
        }
        SheetAction(R.drawable.ic_save, stringResource(R.string.attachment_save), colors.onSurface) {
            act { files.save(message, file) }
        }
        SheetAction(R.drawable.ic_open, stringResource(R.string.attachment_open_with), colors.onSurface) {
            act { files.openWith(message, file) }
        }
    }
}

/** A PDF or the message's images over the whole screen; it rises slowly and Back puts it away. */
@Composable
fun FileViewer(files: Files) {
    val viewing = files.viewing
    BackHandler(enabled = viewing != null) { files.viewing = null }
    val shown = rememberLast(viewing)
    AnimatedVisibility(
        viewing != null,
        enter = slideInVertically(Motion.calm(Motion.SCREEN)) { it / 3 } + fadeIn(Motion.calm(Motion.SCREEN)),
        exit = slideOutVertically(Motion.calm(Motion.NOTE)) { it / 3 } + fadeOut(Motion.calm(Motion.SMALL)),
    ) {
        shown?.let { Viewer(files, it) }
    }
}

@Composable
private fun Viewer(files: Files, viewing: Viewing) {
    val colors = MaterialTheme.colorScheme
    val pager = rememberPagerState(viewing.first) { viewing.files.size }
    val file = viewing.files[pager.currentPage.coerceIn(viewing.files.indices)]
    var zoomed by remember { mutableStateOf(false) }
    Column(
        Modifier
            .fillMaxSize()
            .background(colors.background)
            // Takes the touches, so the message underneath gets none.
            .pointerInput(Unit) {}
            .windowInsetsPadding(WindowInsets.safeDrawing),
    ) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconTile(R.drawable.ic_close, stringResource(R.string.action_close), { files.viewing = null }, size = 44.dp, shape = CircleShape)
            Column(Modifier.weight(1f).padding(horizontal = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text(file.title(), style = Type.Row, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                val detail = if (viewing.files.size > 1) {
                    stringResource(R.string.attachment_position, pager.currentPage + 1, viewing.files.size)
                } else {
                    file.details()
                }
                Text(detail, color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1)
            }
            // Keeps the title centred against the close button.
            Box(Modifier.size(44.dp))
        }
        HorizontalPager(pager, Modifier.weight(1f).fillMaxWidth(), userScrollEnabled = !zoomed, key = { viewing.files[it].part }) { page ->
            val shown = viewing.files[page]
            Downloaded(files, viewing, page) { local ->
                when (shown.preview) {
                    Preview.Pdf -> PdfPages(local, onZoom = { zoomed = it })
                    else -> Photo(local, onZoom = { if (page == pager.currentPage) zoomed = it })
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = Gap.bar, vertical = Gap.barBottom), horizontalArrangement = Arrangement.spacedBy(Gap.inBar)) {
            BarAction(R.drawable.ic_share, stringResource(R.string.attachment_share), colors.onSurface) { files.share(viewing.message, file) }
            BarAction(R.drawable.ic_save, stringResource(R.string.attachment_save_short), colors.onSurface) { files.save(viewing.message, file) }
            BarAction(R.drawable.ic_open, stringResource(R.string.attachment_open_with_short), colors.onSurface) { files.openWith(viewing.message, file) }
        }
    }
}

/** What a load came to: a download, a decoded image, an opened PDF. */
private sealed interface Load<out T> {
    data object Waiting : Load<Nothing>
    data class Done<T>(val value: T) : Load<T>
    data class Failed(@StringRes val text: Int) : Load<Nothing>
}

/** Shows [content] for a load once it is done: a spinner until then, or why it failed. */
@Composable
private fun <T> Loaded(load: Load<T>, content: @Composable (T) -> Unit) {
    when (load) {
        Load.Waiting -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { SlowSpinner() }
        is Load.Failed -> Box(Modifier.fillMaxSize().padding(Gap.screen * 2), contentAlignment = Alignment.Center) {
            Text(stringResource(load.text), color = MaterialTheme.colorScheme.onSurfaceVariant, style = Type.Body, textAlign = TextAlign.Center)
        }
        is Load.Done -> content(load.value)
    }
}

/** Page [page]'s file once on the device; the first page asked for downloads every file shown, together. */
@Composable
private fun Downloaded(files: Files, viewing: Viewing, page: Int, content: @Composable (File) -> Unit) {
    val load by produceState<Load<File>>(Load.Waiting, viewing, page) {
        value = try {
            Load.Done(files.files(viewing.message, viewing.files)[page])
        } catch (e: CoreException) {
            Load.Failed(e.text())
        }
    }
    Loaded(load, content)
}

/** An image fitted to the screen, decoded no larger than needed; pinch or double tap to zoom. */
@Composable
private fun Photo(file: File, onZoom: (Boolean) -> Unit) {
    val load by produceState<Load<ImageBitmap>>(Load.Waiting, file) {
        // Null for a file that is not an image after all, or a broken one.
        value = withContext(Dispatchers.IO) { decode(file) }?.let { Load.Done(it) } ?: Load.Failed(R.string.attachment_cant_show)
    }
    Loaded(load) { image ->
        Zoomable(onZoom, Modifier.fillMaxSize()) {
            Image(image, null, Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
        }
    }
}

/** The pages of a PDF one under another, each drawn when it scrolls into sight; pinch or double tap to zoom. */
@Composable
private fun PdfPages(file: File, onZoom: (Boolean) -> Unit) {
    val load by produceState<Load<PdfDocument>>(Load.Waiting, file) {
        val document = withContext(Dispatchers.IO) { PdfDocument.open(file) }
        value = document?.let { Load.Done(it) } ?: Load.Failed(R.string.attachment_cant_show)
        awaitDispose { document?.close() }
    }
    Loaded(load) { doc ->
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val width = with(LocalDensity.current) { maxWidth.roundToPx() }
            Zoomable(onZoom, Modifier.fillMaxSize()) {
                LazyColumn(
                    Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    items(doc.sizes.size) { index -> PdfPage(doc, index, width) }
                }
            }
        }
    }
}

@Composable
private fun PdfPage(doc: PdfDocument, index: Int, width: Int) {
    val size = doc.sizes[index]
    val page by produceState<ImageBitmap?>(null, doc, index, width) {
        value = withContext(Dispatchers.IO) { doc.render(index, width) }
    }
    Box(
        Modifier
            .fillMaxWidth()
            .aspectRatio(size.width.toFloat() / size.height.coerceAtLeast(1))
            .background(Color.White),
    ) {
        page?.let { Image(it, null, Modifier.fillMaxSize(), contentScale = ContentScale.FillBounds) }
    }
}

/** An open PDF; pages render one at a time, as [PdfRenderer] allows. */
private class PdfDocument(private val renderer: PdfRenderer, val sizes: List<IntSize>) {
    private var closed = false

    /** The page drawn on white at about twice [width], for a sharp zoom; null once closed. */
    fun render(index: Int, width: Int): ImageBitmap? = synchronized(this) {
        if (closed) return null
        renderer.openPage(index).use { page ->
            val w = (width * 2).coerceAtMost(MAX_PAGE_PX)
            val h = (w.toLong() * page.height / page.width.coerceAtLeast(1)).toInt().coerceIn(1, MAX_PAGE_PX * 2)
            val bitmap = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
            bitmap.eraseColor(android.graphics.Color.WHITE)
            page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
            bitmap.asImageBitmap()
        }
    }

    fun close() = synchronized(this) {
        if (!closed) renderer.close()
        closed = true
    }

    companion object {
        /** Null for a file that is not a PDF, is broken or has a password. */
        fun open(file: File): PdfDocument? = try {
            val descriptor = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
            // The renderer owns the descriptor once made; before that, or if reading fails, close it here.
            val renderer = try {
                PdfRenderer(descriptor)
            } catch (e: Exception) {
                descriptor.close()
                throw e
            }
            val sizes = try {
                (0 until renderer.pageCount).map { i -> renderer.openPage(i).use { IntSize(it.width, it.height) } }
            } catch (e: Exception) {
                renderer.close()
                throw e
            }
            if (sizes.isEmpty()) {
                renderer.close()
                null
            } else {
                PdfDocument(renderer, sizes)
            }
        } catch (e: SecurityException) {
            null
        }
    }
}

/**
 * Pinch to zoom in up to four times, drag to look around, double tap to zoom in or back out.
 * At normal size, drags pass through to what scrolls or pages underneath. [onZoom] tells when
 * it is zoomed in.
 */
@Composable
private fun Zoomable(onZoom: (Boolean) -> Unit, modifier: Modifier, content: @Composable () -> Unit) {
    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    var bounds by remember { mutableStateOf(IntSize.Zero) }
    fun set(newScale: Float, newOffset: Offset) {
        scale = newScale.coerceIn(1f, MAX_ZOOM)
        val x = bounds.width * (scale - 1) / 2
        val y = bounds.height * (scale - 1) / 2
        offset = Offset(newOffset.x.coerceIn(-x, x), newOffset.y.coerceIn(-y, y))
        onZoom(scale > 1f)
    }
    val state = rememberTransformableState { zoom, pan, _ -> set(scale * zoom, offset + pan) }
    Box(
        modifier
            .clipToBounds()
            .onSizeChanged { bounds = it }
            .pointerInput(Unit) {
                detectTapGestures(onDoubleTap = { set(if (scale > 1f) 1f else DOUBLE_TAP_ZOOM, Offset.Zero) })
            }
            .transformable(state, canPan = { scale > 1f })
            .graphicsLayer {
                scaleX = scale
                scaleY = scale
                translationX = offset.x
                translationY = offset.y
            },
    ) {
        content()
    }
}

private fun decode(file: File): ImageBitmap? = try {
    ImageDecoder.decodeBitmap(ImageDecoder.createSource(file)) { decoder, info, _ ->
        val longest = maxOf(info.size.width, info.size.height)
        var sample = 1
        while (longest / sample > MAX_IMAGE_PX) sample *= 2
        decoder.setTargetSampleSize(sample)
    }.asImageBitmap()
} catch (e: IOException) {
    null
}

private val IMAGES = setOf("image/jpeg", "image/jpg", "image/png", "image/gif", "image/webp", "image/heic", "image/heif", "image/bmp")
private val IMAGE_EXTENSIONS = setOf("jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "bmp")
/** Characters no file system takes in a name, and control characters. */
private val UNSAFE = Regex("""[\\/:*?"<>|\p{Cntrl}]""")
private const val MAX_NAME_BYTES = 200
private const val MAX_EXTENSION = 10
private const val CHIPS = 2
private const val MAX_IMAGE_PX = 2048
private const val MAX_PAGE_PX = 2000
private const val MAX_ZOOM = 4f
private const val DOUBLE_TAP_ZOOM = 2.5f

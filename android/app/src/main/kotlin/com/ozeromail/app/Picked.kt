package com.ozeromail.app

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.provider.OpenableColumns
import android.text.format.Formatter
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.io.ByteArrayOutputStream
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.ozero_api.DraftFileItem
import uniffi.ozero_api.RecentFileItem

/** A file picked to go with a message: its name, type and content. */
class PickedFile(val name: String, val mime: String, val data: ByteArray)

/** The name a picked file shows under; blank when the app it came from gives none. */
suspend fun pickedName(context: Context, uri: Uri): String = withContext(Dispatchers.IO) {
    try {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { row ->
            if (row.moveToFirst() && !row.isNull(0)) row.getString(0) else null
        }.orEmpty()
    } catch (_: SecurityException) {
        // Reading it says so.
        ""
    }
}

/**
 * The file at [uri], read whole if it takes at most [room] bytes; null if it takes more.
 * Throws [IOException] or [SecurityException] when it cannot be read.
 */
suspend fun readPicked(context: Context, uri: Uri, name: String, room: Long): PickedFile? = withContext(Dispatchers.IO) {
    val resolver = context.contentResolver
    val mime = fileType(name, resolver.getType(uri))
    // A name the recipient's app can open it by.
    val title = name.ifBlank { untitledName(context.getString(R.string.attachment_untitled), mime) }
    val size = resolver.openAssetFileDescriptor(uri, "r")?.use { it.length } ?: -1L
    if (size > room) return@withContext null
    val input = resolver.openInputStream(uri) ?: throw IOException("no content")
    input.use {
        // Room for it at once when its size is known; that size may be wrong, so the count is kept.
        val out = ByteArrayOutputStream(if (size in 1..room) size.toInt() else 64 * 1024)
        val chunk = ByteArray(64 * 1024)
        while (true) {
            val n = it.read(chunk)
            if (n < 0) break
            if (out.size() + n > room) return@withContext null
            out.write(chunk, 0, n)
        }
        PickedFile(title, mime, out.toByteArray())
    }
}

/** The most pixels a photo sent in the text has on its long side. */
private const val PHOTO_EDGE = 2048

/** A photo up to this size, as JPEG or PNG, goes as it is. */
private const val LIGHT_BYTES = 1_000_000L

/**
 * The photo at [uri] made lighter to send in the text: at most [PHOTO_EDGE] pixels on its
 * long side, as JPEG, or as PNG where it has see-through parts. Null when it is light
 * already, an animation, or no photo the phone reads.
 */
suspend fun lighterPhoto(context: Context, uri: Uri, name: String): PickedFile? = withContext(Dispatchers.IO) {
    val resolver = context.contentResolver
    val mime = fileType(name, resolver.getType(uri))
    if (!mime.startsWith("image/") || mime == "image/gif") return@withContext null
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    try {
        resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
    } catch (_: IOException) {
        return@withContext null
    }
    val edge = maxOf(bounds.outWidth, bounds.outHeight)
    if (edge <= 0) return@withContext null
    val bytes = resolver.openAssetFileDescriptor(uri, "r")?.use { it.length } ?: -1L
    if (edge <= PHOTO_EDGE && bytes in 0..LIGHT_BYTES && mime in setOf("image/jpeg", "image/png")) return@withContext null
    val bitmap = try {
        // Turned as the camera held it, which the file only notes.
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(resolver, uri)) { decoder, info, _ ->
            val (w, h) = info.size.width to info.size.height
            val long = maxOf(w, h)
            if (long > PHOTO_EDGE) decoder.setTargetSize(w * PHOTO_EDGE / long, h * PHOTO_EDGE / long)
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        }
    } catch (_: IOException) {
        return@withContext null
    }
    val png = bitmap.hasAlpha()
    val out = ByteArrayOutputStream()
    bitmap.compress(if (png) Bitmap.CompressFormat.PNG else Bitmap.CompressFormat.JPEG, if (png) 100 else 85, out)
    bitmap.recycle()
    val base = name.substringBeforeLast('.').ifBlank { context.getString(R.string.attachment_untitled) }
    PickedFile(if (png) "$base.png" else "$base.jpg", if (png) "image/png" else "image/jpeg", out.toByteArray())
}

/**
 * The files added to a message being written, each with a way to take it off; [adding] are
 * the names of those still being read, shown faint.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FileChips(files: List<DraftFileItem>, adding: List<String>, onRemove: (DraftFileItem) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        files.forEach { file ->
            FileChip(attachmentTitle(file.name), Formatter.formatShortFileSize(LocalContext.current, file.size.toLong()), onRemove = { onRemove(file) })
        }
        adding.forEach { name -> FileChip(attachmentTitle(name), null, onRemove = null) }
    }
}

/** A file of saved mail to send on: its name, then who sent it and its size. */
@Composable
fun RecentFileRow(file: RecentFileItem, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    SheetRow(onClick, lead = { Glyph(R.drawable.ic_attach, null, colors.onSurfaceVariant) }) {
        Column(Modifier.weight(1f)) {
            Text(attachmentTitle(file.name), style = Type.Row, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            Text(
                "${file.sender} · ${Formatter.formatShortFileSize(LocalContext.current, file.size.toLong())}",
                color = colors.onSurfaceVariant,
                style = Type.Caption,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** A file's name and size; while [size] is null it is being read. */
@Composable
private fun FileChip(name: String, size: String?, onRemove: (() -> Unit)?) {
    val colors = MaterialTheme.colorScheme
    Row(
        Modifier
            .widthIn(max = 240.dp)
            .alpha(if (size == null) 0.5f else 1f)
            .border(1.dp, colors.outlineVariant, RoundedCornerShape(8.dp))
            .padding(start = 8.dp, end = if (onRemove == null) 10.dp else 0.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Glyph(R.drawable.ic_attach, null, colors.onSurfaceVariant, size = 16.dp)
        Text(
            name,
            style = Type.Caption,
            maxLines = 1,
            overflow = TextOverflow.MiddleEllipsis,
            modifier = Modifier.weight(1f, fill = false).padding(vertical = 8.dp),
        )
        if (size != null) Text(size, color = colors.onSurfaceVariant, style = Type.Caption, maxLines = 1)
        if (onRemove != null) IconTile(R.drawable.ic_close, stringResource(R.string.action_remove), onRemove, size = 32.dp)
    }
}

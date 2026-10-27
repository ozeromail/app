package com.ozeromail.app

import android.graphics.ImageDecoder
import android.text.format.Formatter
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalTextToolbar
import androidx.compose.ui.platform.TextToolbar
import androidx.compose.ui.platform.TextToolbarStatus
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftFileItem
import uniffi.ozero_api.ImageSize
import uniffi.ozero_api.RichBlock
import uniffi.ozero_api.RichStyle
import uniffi.ozero_api.richBlocks
import uniffi.ozero_api.richFix
import uniffi.ozero_api.richLinkAt
import uniffi.ozero_api.richMarks
import uniffi.ozero_api.richToggle

/**
 * The text of a message being written, with its styles and images: text fields between the
 * images, their marks hidden and shown as what they mean (`ozero_domain::rich`). [text] is
 * the whole text, changed as it is typed in. [caret] says where in it the caret is, for
 * images to go after its line; a tap on an image gives it to [onImage]. Choosing words shows
 * the styles over them; a link asks [onLink] for its address with the one it has, if any.
 */
@Composable
fun RichBody(
    text: MutableState<String>,
    files: List<DraftFileItem>,
    images: Map<Long, ImageBitmap>,
    chosen: Long?,
    placeholder: String,
    style: TextStyle,
    onCaret: (Int) -> Unit,
    onImage: (Long) -> Unit,
    onLink: (String?, (String?) -> Unit) -> Unit,
    modifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    val look = remember(colors) { RichLook(colors) }
    // Parsed once for each text, however often it is asked for.
    val parsed = remember { Parsed() }
    val blocks = parsed.blocks(text.value)
    // Each field by the image above it, 0 for the first, so a field keeps its caret.
    val fields = remember { mutableStateMapOf<Long, TextFieldValue>() }
    var focused by remember { mutableStateOf<Long?>(null) }
    val toolbar = remember { FormatBar() }

    /** Where the field under [key] is in the whole text now. */
    fun span(key: Long): IntRange? {
        var above = 0L
        for (block in parsed.blocks(text.value)) {
            when (block) {
                is RichBlock.Text -> if (above == key) return block.start.toInt() until block.end.toInt()
                is RichBlock.Image -> above = block.file
            }
        }
        return null
    }

    fun change(key: Long, value: TextFieldValue) {
        fields[key] = value
        val at = span(key) ?: return
        val whole = text.value
        if (whole.substring(at.first, at.last + 1) != value.text) {
            text.value = whole.replaceRange(at.first, at.last + 1, value.text)
        }
        onCaret(at.first + value.selection.end)
    }

    fun format(style: RichStyle, url: String? = null) {
        val key = focused ?: return
        val value = fields[key] ?: return
        val done = richToggle(value.text, value.selection.min.toUInt(), value.selection.max.toUInt(), style, url)
        change(key, TextFieldValue(done.text, TextRange(done.start.toInt(), done.end.toInt())))
    }
    toolbar.onFormat = { style ->
        if (style == RichStyle.LINK) {
            val value = focused?.let { fields[it] }
            val has = value?.let { richLinkAt(it.text, it.selection.min.toUInt(), it.selection.max.toUInt()) }
            onLink(has) { url -> format(RichStyle.LINK, url) }
        } else {
            format(style)
        }
    }
    toolbar.chosen = { focused?.let { fields[it] }?.selection?.collapsed == false }

    CompositionLocalProvider(LocalTextToolbar provides toolbar) {
        Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            var above = 0L
            for (block in blocks) {
                when (block) {
                    is RichBlock.Text -> {
                        val fieldKey = above
                        key(fieldKey) {
                            val segment = text.value.substring(block.start.toInt(), block.end.toInt())
                            val kept = fields[fieldKey]
                            val value = when {
                                kept == null -> TextFieldValue(segment, TextRange(segment.length))
                                kept.text == segment -> kept
                                else -> TextFieldValue(segment, TextRange(kept.selection.end.coerceAtMost(segment.length)))
                            }
                            RichField(
                                value,
                                onChange = { change(fieldKey, it) },
                                look,
                                placeholder.takeIf { text.value.isEmpty() },
                                style,
                                Modifier.onFocusChanged {
                                    if (it.isFocused) {
                                        focused = fieldKey
                                        span(fieldKey)?.let { at -> onCaret(at.first + value.selection.end) }
                                    } else if (focused == fieldKey) {
                                        focused = null
                                    }
                                },
                            )
                        }
                    }
                    is RichBlock.Image -> {
                        above = block.file
                        key("image", block.file) {
                            RichImage(
                                files.find { it.id == block.file },
                                images[block.file],
                                block.size,
                                chosen = chosen == block.file,
                                onClick = { onImage(block.file) },
                            )
                        }
                    }
                }
            }
        }
        toolbar.Shown()
    }
}

/** The blocks of the last text asked for. */
private class Parsed {
    private var text: String? = null
    private var blocks: List<RichBlock> = emptyList()

    fun blocks(of: String): List<RichBlock> {
        if (of != text) {
            blocks = richBlocks(of)
            text = of
        }
        return blocks
    }
}

/** One run of text between images, its marks hidden. */
@Composable
private fun RichField(
    value: TextFieldValue,
    onChange: (TextFieldValue) -> Unit,
    look: RichLook,
    placeholder: String?,
    style: TextStyle,
    modifier: Modifier,
) {
    val colors = MaterialTheme.colorScheme
    BasicTextField(
        value = value,
        onValueChange = { new ->
            // Typing over a mark may need the rest of it to go, or a list to go on.
            val fixed = if (new.text == value.text) null else richFix(value.text, new.text)
            onChange(fixed?.let { TextFieldValue(it.text, TextRange(it.start.toInt())) } ?: new)
        },
        textStyle = style.copy(color = colors.onSurface),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        visualTransformation = look,
        cursorBrush = SolidColor(colors.primary),
        modifier = modifier.fillMaxWidth(),
        decorationBox = { field ->
            Box {
                if (placeholder != null) Text(placeholder, style = style, color = colors.onSurfaceVariant)
                field()
            }
        },
    )
}

/**
 * The text as it reads: marks hidden, what they hold styled, a list item's dash a bullet.
 * The caret never stops inside hidden marks: a place there counts as after them.
 */
private class RichLook(private val colors: ColorScheme) : VisualTransformation {
    // The field asks again as the caret moves; the text is the same then.
    private var last: Pair<String, TransformedText>? = null

    override fun filter(text: AnnotatedString): TransformedText {
        last?.let { (raw, shown) -> if (raw == text.text) return shown }
        return look(text.text).also { last = text.text to it }
    }

    private fun look(raw: String): TransformedText {
        val marks = richMarks(raw)
        val hidden = BooleanArray(raw.length)
        val bullets = HashSet<Int>()
        for (m in marks) {
            if (m.style == RichStyle.ITEM) {
                bullets += m.openStart.toInt()
                continue
            }
            for (i in m.openStart.toInt() until m.openEnd.toInt()) hidden[i] = true
            for (i in m.closeStart.toInt() until m.closeEnd.toInt()) hidden[i] = true
        }
        // Where each place of the raw text is in the shown text, and back.
        val shownAt = IntArray(raw.length + 1)
        var n = 0
        for (i in raw.indices) {
            shownAt[i] = n
            if (!hidden[i]) n++
        }
        shownAt[raw.length] = n
        val rawAt = IntArray(n + 1)
        for (i in 0..raw.length) rawAt[shownAt[i]] = i
        val shown = buildAnnotatedString {
            for (i in raw.indices) {
                if (!hidden[i]) append(if (i in bullets) '•' else raw[i])
            }
            for (m in marks) {
                val span = spanOf(m.style) ?: continue
                val from = shownAt[m.openEnd.toInt()]
                val to = shownAt[m.closeStart.toInt()]
                if (from < to) addStyle(span, from, to)
            }
        }
        val mapping = object : OffsetMapping {
            override fun originalToTransformed(offset: Int) = shownAt[offset.coerceIn(0, raw.length)]
            override fun transformedToOriginal(offset: Int) = rawAt[offset.coerceIn(0, n)]
        }
        return TransformedText(shown, mapping)
    }

    private fun spanOf(style: RichStyle): SpanStyle? = when (style) {
        RichStyle.BOLD -> SpanStyle(fontWeight = FontWeight.Bold)
        RichStyle.ITALIC -> SpanStyle(fontStyle = FontStyle.Italic)
        RichStyle.STRIKE -> SpanStyle(textDecoration = TextDecoration.LineThrough)
        RichStyle.CODE -> SpanStyle(fontFamily = FontFamily.Monospace, background = colors.surfaceVariant)
        RichStyle.LINK -> SpanStyle(color = colors.primary, textDecoration = TextDecoration.Underline)
        RichStyle.HEADING -> SpanStyle(fontSize = 21.sp, fontWeight = FontWeight.SemiBold)
        RichStyle.QUOTE -> SpanStyle(color = colors.onSurfaceVariant, fontStyle = FontStyle.Italic)
        RichStyle.ITEM, RichStyle.ESCAPE -> null
    }

    // One look for the same colours, so the field does not lay its text out again for nothing.
    override fun equals(other: Any?) = other is RichLook && other.colors == colors
    override fun hashCode() = colors.hashCode()
}

/** An image in the text, as wide as its size says, with its weight on it. */
@Composable
private fun RichImage(file: DraftFileItem?, bitmap: ImageBitmap?, size: ImageSize, chosen: Boolean, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    val width = when (size) {
        ImageSize.SMALL -> 0.4f
        ImageSize.MEDIUM -> 0.66f
        ImageSize.FULL -> 1f
    }
    val shape = RoundedCornerShape(10.dp)
    Box(
        Modifier
            .padding(vertical = 4.dp)
            .fillMaxWidth(width)
            .clip(shape)
            .then(if (chosen) Modifier.border(3.dp, colors.accent, shape) else Modifier)
            .clickable(onClickLabel = stringResource(R.string.compose_image), role = Role.Image, onClick = onClick),
    ) {
        if (bitmap != null) {
            Image(bitmap, file?.name, Modifier.fillMaxWidth(), contentScale = ContentScale.FillWidth)
        } else {
            Box(Modifier.fillMaxWidth().aspectRatio(4f / 3f).background(colors.surfaceVariant))
        }
        if (file != null) {
            Text(
                Formatter.formatShortFileSize(LocalContext.current, file.size.toLong()),
                color = colors.inverseOnSurface,
                style = Type.Caption,
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(8.dp)
                    .clip(RoundedCornerShape(6.dp))
                    .background(colors.inverseSurface.copy(alpha = 0.7f))
                    .padding(horizontal = 6.dp, vertical = 2.dp),
            )
        }
    }
}

/**
 * The menu over chosen text: the styles, and cutting, copying and pasting. It takes the
 * place of the system's, which has no styles.
 */
private class FormatBar : TextToolbar {
    private var at by mutableStateOf<Rect?>(null)
    private var actions by mutableStateOf<List<Pair<Int, () -> Unit>>>(emptyList())
    var onFormat: (RichStyle) -> Unit = {}
    /** Whether words are chosen, so the styles show; read where it shows. */
    var chosen: () -> Boolean = { false }

    override val status: TextToolbarStatus
        get() = if (at == null) TextToolbarStatus.Hidden else TextToolbarStatus.Shown

    override fun showMenu(
        rect: Rect,
        onCopyRequested: (() -> Unit)?,
        onPasteRequested: (() -> Unit)?,
        onCutRequested: (() -> Unit)?,
        onSelectAllRequested: (() -> Unit)?,
    ) {
        actions = listOfNotNull(
            onCutRequested?.let { android.R.string.cut to it },
            onCopyRequested?.let { android.R.string.copy to it },
            onPasteRequested?.let { android.R.string.paste to it },
            onSelectAllRequested?.let { android.R.string.selectAll to it },
        )
        at = rect
    }

    override fun hide() {
        at = null
    }

    @Composable
    fun Shown() {
        val rect = at ?: return
        val colors = MaterialTheme.colorScheme
        Popup(popupPositionProvider = Over(rect), properties = PopupProperties(focusable = false)) {
            Column(
                Modifier
                    .clip(RoundedCornerShape(14.dp))
                    .background(colors.inverseSurface)
                    .padding(4.dp),
            ) {
                if (chosen()) {
                    Row {
                        STYLES.forEach { (style, label, description) ->
                            val says = stringResource(description)
                            BarButton(label ?: says, says, styleOf(style)) {
                                hide()
                                onFormat(style)
                            }
                        }
                    }
                }
                if (actions.isNotEmpty()) {
                    Row {
                        actions.forEach { (label, action) ->
                            val text = stringResource(label)
                            BarButton(text, text, TextStyle()) {
                                hide()
                                action()
                            }
                        }
                    }
                }
            }
        }
    }

    private fun styleOf(style: RichStyle) = when (style) {
        RichStyle.BOLD -> TextStyle(fontWeight = FontWeight.Bold)
        RichStyle.ITALIC -> TextStyle(fontStyle = FontStyle.Italic)
        RichStyle.STRIKE -> TextStyle(textDecoration = TextDecoration.LineThrough)
        RichStyle.CODE -> TextStyle(fontFamily = FontFamily.Monospace)
        RichStyle.HEADING -> TextStyle(fontWeight = FontWeight.ExtraBold)
        else -> TextStyle()
    }

    private companion object {
        /** Each style with what its button shows, its name where null, and says. */
        val STYLES = listOf<Triple<RichStyle, String?, Int>>(
            Triple(RichStyle.BOLD, "B", R.string.compose_format_bold),
            Triple(RichStyle.ITALIC, "I", R.string.compose_format_italic),
            Triple(RichStyle.STRIKE, "S", R.string.compose_format_strike),
            Triple(RichStyle.CODE, "</>", R.string.compose_format_code),
            Triple(RichStyle.LINK, null, R.string.compose_format_link),
            Triple(RichStyle.HEADING, "H", R.string.compose_format_heading),
            Triple(RichStyle.ITEM, "•", R.string.compose_format_list),
            Triple(RichStyle.QUOTE, "❝", R.string.compose_format_quote),
        )
    }
}

@Composable
private fun BarButton(label: String, description: String, style: TextStyle, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Text(
        label,
        color = colors.inverseOnSurface,
        style = Type.Text.merge(style),
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClickLabel = description, role = Role.Button, onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 10.dp),
    )
}

/** Puts the menu over [rect], or under it when there is no room above, within the window. */
private class Over(private val rect: Rect) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val gap = 16
        val above = rect.top.toInt() - popupContentSize.height - gap
        val y = if (above >= 0) above else (rect.bottom.toInt() + gap).coerceAtMost(windowSize.height - popupContentSize.height)
        val x = (rect.center.x.toInt() - popupContentSize.width / 2).coerceIn(0, (windowSize.width - popupContentSize.width).coerceAtLeast(0))
        return IntOffset(x, y)
    }
}

/** The images of a draft's text, read and decoded at most [width] pixels wide, as they come. */
@Composable
fun rememberImages(mail: Mail, files: List<Long>, width: Int): Map<Long, ImageBitmap> {
    val images = remember { mutableStateMapOf<Long, ImageBitmap>() }
    for (file in files) {
        LaunchedEffect(file) {
            if (file in images) return@LaunchedEffect
            try {
                val data = mail.draftFileData(file)
                decoded(data, width)?.let { images[file] = it }
            } catch (_: CoreException) {
                // Gone: the grey box stays.
            }
        }
    }
    return images
}

private suspend fun decoded(data: ByteArray, width: Int): ImageBitmap? = withContext(Dispatchers.Default) {
    try {
        ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(data))) { decoder, info, _ ->
            val w = info.size.width
            if (w > width) decoder.setTargetSize(width, info.size.height * width / w)
        }.asImageBitmap()
    } catch (_: java.io.IOException) {
        null
    }
}

package com.ozeromail.app

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.PersonFound

/** A pause in typing this long looks the people up, so each key does not. */
private const val LOOKUP_PAUSE_MS = 60L

/** A longer one asks the company directories too, which answer from three letters. */
private const val DIRECTORY_PAUSE_MS = 400L

/**
 * Who a message goes to, as one field: the people picked, each a chip with their name, then the
 * address being typed. [value] keeps them as typed addresses, comma apart. While an address is
 * typed, the people it may be rise over the field, the likeliest nearest to it and the thumb;
 * a tap on one takes it.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun Recipients(
    mail: Mail,
    value: String,
    onChange: (String) -> Unit,
    @StringRes placeholder: Int,
    keyboard: KeyboardOptions,
    modifier: Modifier = Modifier,
    fieldModifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val settings = remember { Settings(context) }
    val parts = remember(value) { Typed.of(value) }
    // The names of the people picked here, by address; others show as their address.
    val names = remember { mutableStateMapOf<String, String>() }
    var focused by remember { mutableStateOf(false) }
    var local by remember { mutableStateOf("" to emptyList<PersonFound>()) }
    var directory by remember { mutableStateOf("" to emptyList<PersonFound>()) }
    // Counts the times the contacts came, so the people are looked up again with them.
    var contactsGiven by remember { mutableIntStateOf(0) }
    var offerContacts by remember { mutableStateOf(!PhoneContacts.allowed(context) && !settings.contactsAsked) }
    val askContacts = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        offerContacts = false
        if (granted) {
            scope.launch {
                mail.giveContacts()
                contactsGiven++
            }
        }
    }
    val typing = parts.typing.trim()
    LaunchedEffect(parts.done) {
        // Addresses that came with the draft get the name the mail or the contacts give them.
        val unnamed = parts.done.filter { it !in names }
        if (unnamed.isEmpty()) return@LaunchedEffect
        try {
            names.putAll(unnamed.zip(mail.recipientNames(unnamed)))
        } catch (_: CoreException) {
            // The addresses they are, then.
        }
    }
    LaunchedEffect(typing, contactsGiven) {
        if (typing.isEmpty()) return@LaunchedEffect
        delay(LOOKUP_PAUSE_MS)
        try {
            local = typing to mail.findPeople(typing)
        } catch (_: CoreException) {
            local = typing to emptyList()
        }
    }
    LaunchedEffect(typing) {
        if (typing.isEmpty()) return@LaunchedEffect
        delay(DIRECTORY_PAUSE_MS)
        directory = try {
            typing to mail.searchDirectory(typing)
        } catch (_: CoreException) {
            // Offline or refused: the people on the phone are enough.
            typing to emptyList()
        }
    }

    fun pick(person: PersonFound) {
        names[person.email] = person.name
        onChange(Typed(parts.done + person.email, "").text)
    }

    fun remove(email: String) = onChange(Typed(parts.done - email, parts.typing).text)

    FlowRow(
        modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        parts.done.forEach { email ->
            RecipientChip(names[email] ?: email, Modifier.align(Alignment.CenterVertically)) { remove(email) }
        }
        Plain(
            parts.typing,
            { typed ->
                // A comma, a semicolon or a space after an address makes it a chip.
                val next = Typed.of(Typed(parts.done, "").prefix + typed)
                onChange(next.text)
            },
            placeholder.takeIf { parts.done.isEmpty() },
            Type.Body,
            keyboard,
            modifier = fieldModifier
                .align(Alignment.CenterVertically)
                .widthIn(min = 96.dp)
                .weight(1f)
                .onFocusChanged { focused = it.isFocused },
        )
    }
    if (!focused || typing.isEmpty()) return
    val shown = local.takeIf { it.first == typing }?.second.orEmpty()
    val listed = directory.takeIf { it.first == typing }?.second.orEmpty()
        .filter { found -> shown.none { it.email.equals(found.email, ignoreCase = true) } }
    if (shown.isEmpty() && listed.isEmpty() && !offerContacts) return
    Popup(popupPositionProvider = AboveField, properties = PopupProperties(focusable = false)) {
        // Laid out from the bottom: the likeliest person sits nearest the field.
        LazyColumn(
            Modifier
                .fillMaxWidth()
                .padding(horizontal = Gap.bar)
                .heightIn(max = 320.dp)
                .shadow(8.dp, RoundedCornerShape(Radius.tile))
                .clip(RoundedCornerShape(Radius.tile))
                .background(colors.surface)
                .border(1.dp, colors.outlineVariant, RoundedCornerShape(Radius.tile)),
            reverseLayout = true,
        ) {
            items(shown, key = { "p" + it.email }) { PersonRow(it) { pick(it) } }
            if (listed.isNotEmpty()) {
                items(listed, key = { "d" + it.email }) { PersonRow(it) { pick(it) } }
                item(key = "directory") {
                    Text(
                        stringResource(R.string.compose_directory),
                        color = colors.onSurfaceVariant,
                        style = Type.Label,
                        modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 2.dp),
                    )
                }
            }
            if (offerContacts) {
                item(key = "contacts") {
                    Column {
                        if (shown.isNotEmpty() || listed.isNotEmpty()) HorizontalDivider(color = colors.outlineVariant)
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .clickable(role = Role.Button) {
                                    settings.contactsAsked = true
                                    askContacts.launch(Manifest.permission.READ_CONTACTS)
                                }
                                .padding(horizontal = 16.dp, vertical = 14.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            Glyph(R.drawable.ic_person, null, colors.primary, size = 20.dp)
                            Text(stringResource(R.string.compose_contacts_ask), color = colors.primary, style = Type.Body)
                        }
                    }
                }
            }
        }
    }
}

/** Someone to write to: their name, and the address with their title, if a directory gave one. */
@Composable
private fun PersonRow(person: PersonFound, onClick: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Column(
        Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 9.dp),
    ) {
        Text(person.name, style = Type.Row, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            listOfNotNull(person.email.takeIf { it != person.name }, person.title).joinToString(" · "),
            color = colors.onSurfaceVariant,
            style = Type.Caption,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** Someone picked, by name; the cross takes them off. */
@Composable
private fun RecipientChip(name: String, modifier: Modifier, onRemove: () -> Unit) {
    val colors = MaterialTheme.colorScheme
    Row(
        modifier
            .widthIn(max = 240.dp)
            .clip(CircleShape)
            .background(colors.surfaceVariant)
            .padding(start = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(name, style = Type.Body, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        IconTile(R.drawable.ic_close, stringResource(R.string.compose_remove_person, name), onRemove, size = 32.dp)
    }
}

/** Puts the people over the field, across the window; under it when there is no room above. */
private object AboveField : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val gap = 12
        val above = anchorBounds.top - popupContentSize.height - gap
        return IntOffset(0, if (above >= 0) above else anchorBounds.bottom + gap)
    }
}

/** A field of addresses: those [done], each one whole, then what is being [typing]. */
data class Typed(val done: List<String>, val typing: String) {
    /** The field as kept in the draft. */
    val text: String get() = prefix + typing

    /** What goes before the address being typed. */
    val prefix: String get() = done.joinToString("") { "$it, " }

    companion object {
        /**
         * Splits [text] where a comma or a semicolon ends an address, or white space after a
         * whole one: a name being typed keeps its spaces.
         */
        fun of(text: String): Typed {
            val segments = text.split(',', ';', '\n')
            val done = segments.dropLast(1).map { it.trim() }.filter { it.isNotEmpty() }.toMutableList()
            var typing = segments.last().trimStart()
            while (true) {
                val whole = WHOLE.find(typing) ?: break
                done += whole.groupValues[1]
                typing = typing.substring(whole.range.last + 1)
            }
            return Typed(done.distinctBy { it.lowercase() }, typing)
        }

        /** An address at the start, with white space after it. */
        private val WHOLE = Regex("""^\s*(\S+@\S+)\s+""")
    }
}

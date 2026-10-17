package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** A square-ish button holding one icon, as in the bottom bars. */
@Composable
fun IconTile(
    @DrawableRes icon: Int,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    background: Color = Color.Transparent,
    tint: Color = MaterialTheme.colorScheme.onSurface,
    shape: Shape = RoundedCornerShape(16.dp),
    size: Dp = 48.dp,
    enabled: Boolean = true,
) {
    Box(
        modifier
            .size(size)
            .alpha(if (enabled) 1f else 0.4f)
            .clip(shape)
            .background(background)
            .clickable(enabled = enabled, onClickLabel = description, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Glyph(icon, description, tint)
    }
}

/** The rounded grey shape of the search field and the quick reply. */
@Composable
fun PillBox(
    modifier: Modifier = Modifier,
    @DrawableRes icon: Int? = null,
    height: Dp = 48.dp,
    content: @Composable RowScope.() -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    Row(
        modifier
            .height(height)
            .clip(CircleShape)
            .background(colors.surfaceVariant)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        icon?.let { Glyph(it, null, colors.onSurfaceVariant) }
        content()
    }
}

/** A [PillBox] that is a button with a grey prompt in it. */
@Composable
fun Pill(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    @DrawableRes icon: Int? = null,
    height: Dp = 48.dp,
) {
    PillBox(modifier.clip(CircleShape).clickable(role = Role.Button, onClick = onClick), icon, height) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** A screen's large title, set high where it needs no reach. */
@Composable
fun LargeTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier, fontSize = 32.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.8).sp)
}

/** An account's colour with the first letter of its name. */
@Composable
fun AccountInitial(name: String, color: Color, size: Dp) {
    Box(Modifier.size(size).background(color, CircleShape), contentAlignment = Alignment.Center) {
        Text(initials(name).take(1), color = Color.White, fontSize = fixedSp(size / 2), fontWeight = FontWeight.Bold)
    }
}

/** The see-through background of the bars over the list. */
val ColorScheme.bar: Color
    get() = background.copy(alpha = 0.94f)

/** A hairline along the top or bottom edge. */
fun Modifier.hairline(color: Color, top: Boolean): Modifier = drawBehind {
    val y = if (top) 0f else size.height
    drawLine(color, Offset(0f, y), Offset(size.width, y), 1.dp.toPx())
}

/** A small round mark in an account's colour. */
@Composable
fun AccountDot(color: Color, modifier: Modifier = Modifier, size: Dp = 10.dp) {
    Box(modifier.size(size).background(color, CircleShape))
}

/** A person's initials on a colour of their own. */
@Composable
fun Avatar(name: String, key: String, modifier: Modifier = Modifier, size: Dp = 44.dp) {
    Box(
        modifier.size(size).background(avatarColor(key), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            initials(name),
            color = Color.White,
            fontWeight = FontWeight.SemiBold,
            fontSize = fixedSp(size * 0.36f),
        )
    }
}

/** Text that fills a circle of a fixed size, so it does not grow with the font scale. */
@Composable
private fun fixedSp(size: Dp): TextUnit = with(LocalDensity.current) { size.toSp() }

/** The first letters of the first two words, or of the address. */
fun initials(name: String): String =
    name.split(' ', '.', '@', '_', '-')
        .filter { it.isNotBlank() }
        .take(2)
        .joinToString("") { it.take(1) }
        .uppercase()
        .ifEmpty { "?" }

private val AVATAR_COLORS = listOf(
    0xFF5B6C8F, 0xFF4F8A8B, 0xFF8A6FB0, 0xFFB0775A, 0xFF5E8F5E, 0xFF9A6A87, 0xFF6F7F96,
).map(::Color)

/** The same colour every time for one sender. */
private fun avatarColor(key: String): Color = AVATAR_COLORS[Math.floorMod(key.lowercase().hashCode(), AVATAR_COLORS.size)]

/** A round note with a title and a line under it, for empty lists. */
@Composable
fun EmptyNote(
    @DrawableRes icon: Int,
    title: String,
    text: String? = null,
    tint: Color = MaterialTheme.colorScheme.onSurfaceVariant,
    ring: Color = MaterialTheme.colorScheme.surfaceVariant,
    ringSize: Dp = 64.dp,
) {
    val colors = MaterialTheme.colorScheme
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.size(ringSize).background(ring, CircleShape), contentAlignment = Alignment.Center) {
            Glyph(icon, null, tint, size = ringSize * 0.46f)
        }
        Text(title, fontWeight = FontWeight.Bold, fontSize = 20.sp, textAlign = TextAlign.Center)
        text?.let {
            Text(
                it,
                color = colors.onSurfaceVariant,
                textAlign = TextAlign.Center,
                lineHeight = 21.sp,
                modifier = Modifier.widthIn(max = 280.dp),
            )
        }
    }
}

/** The undo toast: dark, rounded, low on the screen. */
@Composable
fun Toast(state: SnackbarHostState, modifier: Modifier = Modifier) {
    val colors = MaterialTheme.colorScheme
    SnackbarHost(state, modifier) { data ->
        Snackbar(
            data,
            modifier = Modifier.padding(horizontal = 12.dp),
            shape = RoundedCornerShape(14.dp),
            containerColor = colors.inverseSurface,
            contentColor = colors.inverseOnSurface,
            actionColor = colors.primary,
        )
    }
}

package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

// Calm white and grey, one blue for actions, colour only where it means something. No dynamic colour.
private val Light = lightColorScheme(
    primary = Color(0xFF2F7CF6),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE7F0FE),
    background = Color.White,
    onBackground = Color(0xFF1D1F24),
    surface = Color.White,
    onSurface = Color(0xFF1D1F24),
    surfaceVariant = Color(0xFFF4F5F7),
    onSurfaceVariant = Color(0xFF8A8F99),
    surfaceContainerHighest = Color(0xFFE9EBEF),
    inverseSurface = Color(0xFF1D1F24),
    inverseOnSurface = Color.White,
    outlineVariant = Color(0xFFEEF0F2),
    error = Color(0xFFEF4A5C),
    scrim = Color(0x610E1118),
)

private val Dark = darkColorScheme(
    primary = Color(0xFF4C8FF8),
    onPrimary = Color.White,
    primaryContainer = Color(0xFF1F3152),
    background = Color(0xFF1B1C1F),
    onBackground = Color(0xFFECECEE),
    surface = Color(0xFF1B1C1F),
    onSurface = Color(0xFFECECEE),
    surfaceVariant = Color(0xFF25272B),
    onSurfaceVariant = Color(0xFF8E939C),
    surfaceContainerHighest = Color(0xFF2F3136),
    inverseSurface = Color(0xFFECECEE),
    inverseOnSurface = Color(0xFF1B1C1F),
    outlineVariant = Color(0xFF2A2C30),
    error = Color(0xFFF2596A),
    scrim = Color(0x8C000000),
)

/** Done and Inbox Zero: the one green, which Material has no slot for. */
val MaterialTheme.done: Color
    @Composable get() = if (isSystemInDarkTheme()) Color(0xFF52B867) else Color(0xFF43A95A)

@Composable
fun OzeroTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) Dark else Light, typography = Type.Material, content = content)
}

/** One of the app's line icons, in [tint]. */
@Composable
fun Glyph(
    @DrawableRes icon: Int,
    description: String?,
    tint: Color,
    modifier: Modifier = Modifier,
    size: Dp = 22.dp,
) {
    Icon(painterResource(icon), description, modifier.size(size), tint = tint)
}

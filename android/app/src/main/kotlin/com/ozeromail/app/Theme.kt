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

// White paper and the icon's lake: cool greys, one deep blue for actions and Done, colour only where it
// means something. All roles are set, so none of Material's default lilac shows. No dynamic colour.
private val Light = lightColorScheme(
    primary = Color(0xFF1F5CCD),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE2EBFC),
    onPrimaryContainer = Color(0xFF0F2A5E),
    inversePrimary = Color(0xFF9CC4FD),
    secondary = Color(0xFF5B6B8C),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFE3E9F2),
    onSecondaryContainer = Color(0xFF142036),
    tertiary = Color(0xFF5B6B8C),
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFE3E9F2),
    onTertiaryContainer = Color(0xFF142036),
    background = Color.White,
    onBackground = Color(0xFF142036),
    surface = Color.White,
    onSurface = Color(0xFF142036),
    surfaceVariant = Color(0xFFF2F5FA),
    onSurfaceVariant = Color(0xFF76829A),
    surfaceTint = Color.Transparent,
    surfaceBright = Color.White,
    surfaceDim = Color(0xFFE3E9F2),
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color(0xFFF7F9FC),
    surfaceContainer = Color(0xFFF2F5FA),
    surfaceContainerHigh = Color.White,
    surfaceContainerHighest = Color(0xFFE3E9F2),
    inverseSurface = Color(0xFF1B2639),
    inverseOnSurface = Color(0xFFF2F5FA),
    outline = Color(0xFFAEB9CB),
    outlineVariant = Color(0xFFE9EEF5),
    error = Color(0xFFDF5260),
    onError = Color.White,
    errorContainer = Color(0xFFFCE8EA),
    onErrorContainer = Color(0xFF6B1520),
    scrim = Color(0x570A1428),
)

// The night lake: blue-slate paper, the lighter water for actions.
private val Dark = darkColorScheme(
    primary = Color(0xFF6AA3F9),
    onPrimary = Color(0xFF0B1A33),
    primaryContainer = Color(0xFF1B2F55),
    onPrimaryContainer = Color(0xFFDCE8FF),
    inversePrimary = Color(0xFF1F5CCD),
    secondary = Color(0xFF8D9BB8),
    onSecondary = Color(0xFF0F1626),
    secondaryContainer = Color(0xFF2B3343),
    onSecondaryContainer = Color(0xFFE5E9F1),
    tertiary = Color(0xFF8D9BB8),
    onTertiary = Color(0xFF0F1626),
    tertiaryContainer = Color(0xFF2B3343),
    onTertiaryContainer = Color(0xFFE5E9F1),
    background = Color(0xFF141923),
    onBackground = Color(0xFFE5E9F1),
    surface = Color(0xFF141923),
    onSurface = Color(0xFFE5E9F1),
    surfaceVariant = Color(0xFF1F2532),
    onSurfaceVariant = Color(0xFF8792A7),
    surfaceTint = Color.Transparent,
    surfaceBright = Color(0xFF2B3343),
    surfaceDim = Color(0xFF141923),
    surfaceContainerLowest = Color(0xFF10141C),
    surfaceContainerLow = Color(0xFF181D28),
    surfaceContainer = Color(0xFF1A202C),
    surfaceContainerHigh = Color(0xFF1F2633),
    surfaceContainerHighest = Color(0xFF2B3343),
    inverseSurface = Color(0xFFE5E9F1),
    inverseOnSurface = Color(0xFF141923),
    outline = Color(0xFF4B566C),
    outlineVariant = Color(0xFF242B39),
    error = Color(0xFFEE6672),
    onError = Color(0xFF2A0A0F),
    errorContainer = Color(0xFF4A1D24),
    onErrorContainer = Color(0xFFFCE8EA),
    scrim = Color(0x8C000000),
)

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

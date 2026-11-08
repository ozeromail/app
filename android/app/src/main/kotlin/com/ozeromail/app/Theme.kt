package com.ozeromail.app

import androidx.annotation.DrawableRes
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ColorScheme
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

// White paper and the icon's lake: warm sand cards, navy ink, the lake's deep blue for actions and Done, colour only
// where it means something. [accent] fills; `primary` is its darker tone, for text and Material's controls.
// All roles are set, so none of Material's default lilac shows. No dynamic colour.
private val Light = lightColorScheme(
    primary = Color(0xFF2456D6),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE9F0FF),
    onPrimaryContainer = Color(0xFF1B3F7A),
    inversePrimary = Color(0xFF7FA9FF),
    secondary = Color(0xFF5B6B8C),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFE3E9F2),
    onSecondaryContainer = Color(0xFF12192A),
    // The late sun of Snooze.
    tertiary = Color(0xFFE0922B),
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFE3E9F2),
    onTertiaryContainer = Color(0xFF12192A),
    background = Color.White,
    onBackground = Color(0xFF12192A),
    surface = Color.White,
    onSurface = Color(0xFF12192A),
    surfaceVariant = Color(0xFFF6F2EC),
    // Dark enough for small text on white and on the paper cards (4.8:1 on white).
    onSurfaceVariant = Color(0xFF6B7280),
    surfaceTint = Color.Transparent,
    surfaceBright = Color.White,
    surfaceDim = Color(0xFFE3E9F2),
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color(0xFFFAF8F5),
    surfaceContainer = Color(0xFFF6F2EC),
    surfaceContainerHigh = Color.White,
    surfaceContainerHighest = Color(0xFFE3E9F2),
    inverseSurface = Color(0xFF1A2234),
    inverseOnSurface = Color(0xFFF2F5FA),
    outline = Color(0xFFAEB9CB),
    outlineVariant = Color(0xFFE9EEF5),
    // The text tone, readable on white; swipes fill with the brighter [errorFill].
    error = Color(LIGHT_ERROR),
    onError = Color.White,
    errorContainer = Color(0xFFFCE8EA),
    onErrorContainer = Color(0xFF6B1520),
    scrim = Color(0x570A1428),
)

// The night lake: navy paper, the lighter water for text accents.
private val Dark = darkColorScheme(
    primary = Color(0xFF7FA9FF),
    onPrimary = Color(0xFF12192A),
    primaryContainer = Color(0xFF1B2D5C),
    onPrimaryContainer = Color(0xFFE9F0FF),
    inversePrimary = Color(0xFF2456D6),
    secondary = Color(0xFF8D9BB8),
    onSecondary = Color(0xFF0F1626),
    secondaryContainer = Color(0xFF2B3343),
    onSecondaryContainer = Color(0xFFE9EDF4),
    tertiary = Color(0xFFF0B25A),
    onTertiary = Color(0xFF2A1A05),
    tertiaryContainer = Color(0xFF2B3343),
    onTertiaryContainer = Color(0xFFE9EDF4),
    background = Color(0xFF12192A),
    onBackground = Color(0xFFE9EDF4),
    surface = Color(0xFF12192A),
    onSurface = Color(0xFFE9EDF4),
    surfaceVariant = Color(0xFF1F2532),
    onSurfaceVariant = Color(0xFF94A3B8),
    surfaceTint = Color.Transparent,
    surfaceBright = Color(0xFF2B3343),
    surfaceDim = Color(0xFF12192A),
    surfaceContainerLowest = Color(0xFF0E1422),
    surfaceContainerLow = Color(0xFF161D2C),
    surfaceContainer = Color(0xFF1A2234),
    surfaceContainerHigh = Color(0xFF1F2633),
    surfaceContainerHighest = Color(0xFF2B3343),
    inverseSurface = Color(0xFFE9EDF4),
    inverseOnSurface = Color(0xFF12192A),
    outline = Color(0xFF4B566C),
    outlineVariant = Color(0xFF242B39),
    error = Color(0xFFEE6672),
    onError = Color(0xFF2A0A0F),
    errorContainer = Color(0xFF4A1D24),
    onErrorContainer = Color(0xFFFCE8EA),
    scrim = Color(0x8C000000),
)

internal val Accent = Color(0xFF2F6BFF)

/** The lake blue of buttons, Done, marks and water, the same in both themes; white on it is 4.5:1. */
val ColorScheme.accent: Color
    get() = Accent

/** The brighter red a Delete swipe fills with; [ColorScheme.error] is the text tone. */
val ColorScheme.errorFill: Color
    get() = if (error == Color(LIGHT_ERROR)) Color(0xFFDF5260) else error

private const val LIGHT_ERROR = 0xFFC94452

/** What sits on [accent]. */
val ColorScheme.onAccent: Color
    get() = Color.White

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

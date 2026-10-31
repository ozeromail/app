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

// White paper and the icon's lake: cool greys, one bright morning blue for actions and Done, colour only
// where it means something. [accent] fills; `primary` is its darker tone, for text and Material's controls.
// All roles are set, so none of Material's default lilac shows. No dynamic colour.
private val Light = lightColorScheme(
    primary = Color(0xFF1A6FE0),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE6F1FF),
    onPrimaryContainer = Color(0xFF0B2E5C),
    inversePrimary = Color(0xFF8CC2FF),
    secondary = Color(0xFF5B6B8C),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFE3E9F2),
    onSecondaryContainer = Color(0xFF142036),
    // The late sun of Snooze.
    tertiary = Color(0xFFE0922B),
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFE3E9F2),
    onTertiaryContainer = Color(0xFF142036),
    background = Color.White,
    onBackground = Color(0xFF142036),
    surface = Color.White,
    onSurface = Color(0xFF142036),
    surfaceVariant = Color(0xFFF2F5FA),
    // Dark enough for small text on white and on the paper cards (4.8:1 and 4.4:1).
    onSurfaceVariant = Color(0xFF66728A),
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
    // The text tone, readable on white; swipes fill with the brighter [errorFill].
    error = Color(0xFFC94452),
    onError = Color.White,
    errorContainer = Color(0xFFFCE8EA),
    onErrorContainer = Color(0xFF6B1520),
    scrim = Color(0x570A1428),
)

// The night lake: blue-slate paper, the lighter water for actions.
private val Dark = darkColorScheme(
    primary = Color(0xFF6AB2FF),
    onPrimary = Color(0xFF06203A),
    primaryContainer = Color(0xFF15325A),
    onPrimaryContainer = Color(0xFFDCEBFF),
    inversePrimary = Color(0xFF1A6FE0),
    secondary = Color(0xFF8D9BB8),
    onSecondary = Color(0xFF0F1626),
    secondaryContainer = Color(0xFF2B3343),
    onSecondaryContainer = Color(0xFFE5E9F1),
    tertiary = Color(0xFFF0B25A),
    onTertiary = Color(0xFF2A1A05),
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

internal val Accent = Color(0xFF2F8CFF)

/** The bright blue of buttons, Done, marks and water, the same in both themes. */
val ColorScheme.accent: Color
    get() = Accent

/** The brighter red a Delete swipe fills with; [ColorScheme.error] is the text tone. */
val ColorScheme.errorFill: Color
    get() = if (error == LightError) Color(0xFFDF5260) else error

private val LightError = Color(0xFFC94452)

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

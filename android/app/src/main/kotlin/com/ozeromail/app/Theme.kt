package com.ozeromail.app

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

// Quiet neutrals and one blue accent; colour only where it means something. No dynamic colour.
private val Light = lightColorScheme(
    primary = Color(0xFF2F6FDE),
    onPrimary = Color.White,
    background = Color.White,
    onBackground = Color(0xFF1C1C1E),
    surface = Color.White,
    onSurface = Color(0xFF1C1C1E),
    surfaceVariant = Color(0xFFF2F2F4),
    onSurfaceVariant = Color(0xFF8A8A8E),
    outlineVariant = Color(0xFFE5E5EA),
    error = Color(0xFFD93A2B),
)

private val Dark = darkColorScheme(
    primary = Color(0xFF5C93F0),
    onPrimary = Color.White,
    background = Color(0xFF1C1C1E),
    onBackground = Color(0xFFF2F2F4),
    surface = Color(0xFF1C1C1E),
    onSurface = Color(0xFFF2F2F4),
    surfaceVariant = Color(0xFF2C2C2E),
    onSurfaceVariant = Color(0xFF98989D),
    outlineVariant = Color(0xFF38383A),
    error = Color(0xFFFF6B5E),
)

@Composable
fun OzeroTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) Dark else Light, content = content)
}

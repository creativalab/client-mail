package com.ferrowl.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

// Colors of the desktop themes (design/themes.json): Ruggine (light) and
// Notte (dark), mapped onto Material 3 roles.

private val Ruggine = lightColorScheme(
    primary = Color(0xFFA63A0C),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFFBEDE4),
    onPrimaryContainer = Color(0xFF7C2A07),
    secondaryContainer = Color(0xFFEADFD5),
    onSecondaryContainer = Color(0xFF1F1B18),
    background = Color(0xFFFFFFFF),
    onBackground = Color(0xFF1F1B18),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF1F1B18),
    surfaceVariant = Color(0xFFF3EEE9),
    onSurfaceVariant = Color(0xFF5B5650),
    surfaceContainer = Color(0xFFFBF9F7),
    surfaceContainerHigh = Color(0xFFF6F2EE),
    outline = Color(0xFFD6CDC3),
    outlineVariant = Color(0xFFE7E1DA),
    error = Color(0xFFB3261E),
)

private val Notte = darkColorScheme(
    primary = Color(0xFFE07A4A),
    onPrimary = Color(0xFF1A0D06),
    primaryContainer = Color(0xFF33231B),
    onPrimaryContainer = Color(0xFFF3B08E),
    secondaryContainer = Color(0xFF2C2F35),
    onSecondaryContainer = Color(0xFFECECEE),
    background = Color(0xFF17181B),
    onBackground = Color(0xFFECECEE),
    surface = Color(0xFF17181B),
    onSurface = Color(0xFFECECEE),
    surfaceVariant = Color(0xFF26292E),
    onSurfaceVariant = Color(0xFFA0A4AC),
    surfaceContainer = Color(0xFF1D1F23),
    surfaceContainerHigh = Color(0xFF1B1D21),
    outline = Color(0xFF41454D),
    outlineVariant = Color(0xFF2C2F35),
    error = Color(0xFFF28B82),
)

/** App theme: follows the system light/dark setting. */
@Composable
fun FerrowlTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) Notte else Ruggine, content = content)
}

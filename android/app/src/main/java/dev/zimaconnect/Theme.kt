package dev.zimaconnect

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.shape.RoundedCornerShape

val Accent = Color(0xFF4A7FD6)
val Ok = Color(0xFF30A46C)
val Warn = Color(0xFFE5484D)

// Same system as the server dashboard and the desktop app: achromatic chrome, one data hue.
private val Light = lightColorScheme(
    primary = Color(0xFF141414),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE8EEF9),
    onPrimaryContainer = Color(0xFF0A0A0A),
    background = Color(0xFFF2F4F7),
    onBackground = Color(0xFF0A0A0A),
    surface = Color.White,
    onSurface = Color(0xFF0A0A0A),
    surfaceVariant = Color(0xFFE5E7EA),
    onSurfaceVariant = Color(0xFF5F646B),
    outlineVariant = Color(0xFFE4E7EB),
    error = Color(0xFFC4242A),
)

private val Dark = darkColorScheme(
    primary = Color(0xFFF4F4F5),
    onPrimary = Color(0xFF0A0A0A),
    primaryContainer = Color(0xFF1A2433),
    onPrimaryContainer = Color(0xFFF4F4F5),
    background = Color(0xFF0B0B0C),
    onBackground = Color(0xFFF4F4F5),
    surface = Color(0xFF141416),
    onSurface = Color(0xFFF4F4F5),
    surfaceVariant = Color(0xFF26262A),
    onSurfaceVariant = Color(0xFFB0B3B8),
    outlineVariant = Color(0xFF232327),
    error = Color(0xFFEF6B6B),
)

/** Geist (SIL OFL 1.1, Vercel), bundled in res/font. */
val Geist = FontFamily(
    Font(R.font.geist_regular, FontWeight.Normal),
    Font(R.font.geist_medium, FontWeight.Medium),
    Font(R.font.geist_semibold, FontWeight.SemiBold),
)

private val Type = Typography(
    titleLarge = TextStyle(fontFamily = Geist, fontSize = 22.sp, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.2).sp),
    titleMedium = TextStyle(fontFamily = Geist, fontSize = 16.sp, fontWeight = FontWeight.SemiBold),
    titleSmall = TextStyle(fontFamily = Geist, fontSize = 14.sp, fontWeight = FontWeight.Medium),
    bodyLarge = TextStyle(fontFamily = Geist, fontSize = 15.sp),
    bodyMedium = TextStyle(fontFamily = Geist, fontSize = 14.sp),
    bodySmall = TextStyle(fontFamily = Geist, fontSize = 12.5.sp),
    labelLarge = TextStyle(fontFamily = Geist, fontSize = 14.sp, fontWeight = FontWeight.Medium),
    labelMedium = TextStyle(fontFamily = Geist, fontSize = 12.sp, fontWeight = FontWeight.Medium),
)

private val Round = Shapes(
    small = RoundedCornerShape(6.dp),
    medium = RoundedCornerShape(8.dp),
    large = RoundedCornerShape(8.dp),
)

@Composable
fun ZimaTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (isSystemInDarkTheme()) Dark else Light,
        typography = Type,
        shapes = Round,
        content = content,
    )
}

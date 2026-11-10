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

val Accent = Color(0xFF0A84FF)
val Ok = Color(0xFF30A46C)
val Warn = Color(0xFFE5484D)

private val Light = lightColorScheme(
    primary = Accent,
    onPrimary = Color.White,
    primaryContainer = Color(0x1A0A84FF),
    onPrimaryContainer = Accent,
    background = Color(0xFFF5F5F7),
    onBackground = Color(0xFF1D1D1F),
    surface = Color.White,
    onSurface = Color(0xFF1D1D1F),
    surfaceVariant = Color(0xFFECECF0),
    onSurfaceVariant = Color(0xFF6E6E73),
    outlineVariant = Color(0x14000000),
    error = Warn,
)

private val Dark = darkColorScheme(
    primary = Accent,
    onPrimary = Color.White,
    primaryContainer = Color(0x2E0A84FF),
    onPrimaryContainer = Color(0xFF64B5FF),
    background = Color(0xFF1C1C1E),
    onBackground = Color(0xFFF5F5F7),
    surface = Color(0xFF2C2C2E),
    onSurface = Color(0xFFF5F5F7),
    surfaceVariant = Color(0xFF3A3A3C),
    onSurfaceVariant = Color(0xFFA1A1A6),
    outlineVariant = Color(0x17FFFFFF),
    error = Color(0xFFFF6369),
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
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(14.dp),
    large = RoundedCornerShape(20.dp),
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

package dev.zimaconnect

import android.graphics.Paint
import android.graphics.Typeface
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.nativeCanvas

/** What a device looks like, guessed from its platform and name (same rule as the desktop app). */
fun kindOf(platform: String, name: String): String = when {
    platform == "server" -> "server"
    platform == "android" || platform == "ios" -> if (Regex("tab|pad|slate", RegexOption.IGNORE_CASE).containsMatchIn(name)) "tablet" else "phone"
    Regex("desktop|tower|pc$|^pc|nuc|mini|imac|studio|server|workstation", RegexOption.IGNORE_CASE).containsMatchIn(name) -> "desktop"
    else -> "laptop"
}

/** The person's colour, the same on every screen (falls back to a stable guess for old servers). */
fun hueOf(c: Contact): Float = (c.userHue ?: run {
    var h = 0
    for (ch in c.userName ?: c.id) h = (h * 31 + ch.code) % 360
    (h * 137) % 360
}).toFloat()

private val Body1 = Color(0xFF3B3C42)
private val Body2 = Color(0xFF1D1D21)
private val Alu1 = Color(0xFFECEEF2)
private val Alu2 = Color(0xFFB9BCC6)

private fun wall(h: Float) = Brush.linearGradient(
    listOf(Color.hsl(h, .85f, .72f), Color.hsl((h + 28) % 360, .75f, .6f), Color.hsl((h + 62) % 360, .7f, .46f)),
    start = Offset(0f, 0f), end = Offset(120f, 120f),
)

private val Off = Brush.linearGradient(listOf(Color(0xFF2B2C31), Color(0xFF18181B)), Offset.Zero, Offset(120f, 120f))
private val BodyBrush = Brush.linearGradient(listOf(Body1, Body2, Color(0xFF2A2B30)), Offset.Zero, Offset(120f, 120f))

/** Illustrations of the kinds of devices: dark bezels, a wallpaper in the owner's colour, a lock screen with the battery. */
@Composable
fun DeviceArt(kind: String, hue: Float, online: Boolean, battery: Pair<Int, Boolean>?, modifier: Modifier = Modifier) {
    val t = rememberInfiniteTransition(label = "art")
    val pulse by t.animateFloat(1f, 1.22f, infiniteRepeatable(tween(800, easing = LinearEasing), RepeatMode.Reverse), label = "pulse")
    val halo by t.animateFloat(.35f, 1f, infiniteRepeatable(tween(1100, easing = LinearEasing), RepeatMode.Reverse), label = "halo")
    val blink by t.animateFloat(.25f, 1f, infiniteRepeatable(tween(950, easing = LinearEasing), RepeatMode.Reverse), label = "blink")
    val charging = battery?.second == true

    Canvas(modifier) {
        val s = size.minDimension / 120f
        scale(s, pivot = Offset.Zero) {
            // soft shadow
            drawOval(Color.Black.copy(alpha = .14f), Offset(if (kind == "phone") 38f else 26f, 112f), Size(if (kind == "phone") 44f else 68f, 6f))
            when (kind) {
                "phone" -> phone(hue, online, battery, charging, pulse, halo)
                "tablet" -> tablet(hue, online, battery, charging, pulse)
                "laptop" -> laptop(hue, online)
                "desktop" -> desktop(hue, online)
                else -> server(online, blink)
            }
        }
    }
}

private fun DrawScope.screenText(text: String, x: Float, y: Float, sizeU: Float, weight: Int = 600, alpha: Float = 1f) {
    val p = Paint().apply {
        isAntiAlias = true
        color = android.graphics.Color.argb((alpha * 255).toInt(), 255, 255, 255)
        textSize = sizeU
        textAlign = Paint.Align.CENTER
        typeface = Typeface.create("sans-serif", if (weight >= 600) Typeface.BOLD else Typeface.NORMAL)
    }
    drawContext.canvas.nativeCanvas.drawText(text, x, y, p)
}

private fun DrawScope.phone(h: Float, online: Boolean, battery: Pair<Int, Boolean>?, charging: Boolean, pulse: Float, halo: Float) {
    drawRoundRect(BodyBrush, Offset(40f, 6f), Size(40f, 108f), CornerRadius(10.5f))
    drawRoundRect(Color.White.copy(alpha = .18f), Offset(40.4f, 6.4f), Size(39.2f, 107.2f), CornerRadius(10.1f), style = Stroke(.8f))
    drawRoundRect(if (online) wall(h) else Off, Offset(42.5f, 8.5f), Size(35f, 103f), CornerRadius(8f))
    if (online) {
        drawCircle(Color.White.copy(alpha = .12f), 22f, Offset(76f, 26f))
        screenText("9:41", 60f, 38f, 11f)
        screenText("Tue 10 Nov", 60f, 46f, 3.6f, 500, .85f)
        if (battery != null) {
            val pct = battery.first.coerceIn(3, 100)
            if (charging) drawCircle(Brush.radialGradient(listOf(Color(0xFF3DDC84).copy(alpha = .55f * halo), Color.Transparent), Offset(60f, 84f), 24f), 24f, Offset(60f, 84f))
            drawRoundRect(Color.Black.copy(alpha = .28f), Offset(47f, 78f), Size(26f, 12f), CornerRadius(6f))
            drawRoundRect(if (charging || pct > 20) Color.White.copy(alpha = .92f) else Color(0xFFFF6B6B), Offset(48f, 79f), Size(maxOf(8f, 24f * pct / 100f), 10f), CornerRadius(5f))
            if (charging) bolt(Offset(60f, 84f), pulse)
        } else {
            for (i in 0 until 4) drawRoundRect(Color.White.copy(alpha = .4f), Offset(48f + (i % 2) * 13f, 70f + (i / 2) * 13f), Size(10f, 10f), CornerRadius(3f))
        }
        drawRoundRect(Color.White.copy(alpha = .8f), Offset(53f, 106f), Size(14f, 1.6f), CornerRadius(.8f))
    }
    drawRoundRect(Color(0xFF050506), Offset(55f, 11.5f), Size(10f, 3f), CornerRadius(1.5f))
}

private fun DrawScope.bolt(c: Offset, k: Float) {
    val p = Path().apply {
        moveTo(c.x + 1.5f * k, c.y - 3.8f * k); lineTo(c.x - 3.4f * k, c.y + 1.4f * k); lineTo(c.x + 0f, c.y + 1.4f * k)
        lineTo(c.x - 1.2f * k, c.y + 4.6f * k); lineTo(c.x + 3.6f * k, c.y - 1f * k); lineTo(c.x + .3f * k, c.y - 1f * k); close()
    }
    drawPath(p, Color(0xFF14B85A))
}

private fun DrawScope.tablet(h: Float, online: Boolean, battery: Pair<Int, Boolean>?, charging: Boolean, pulse: Float) {
    drawRoundRect(BodyBrush, Offset(16f, 16f), Size(88f, 88f), CornerRadius(11f))
    drawRoundRect(if (online) wall(h) else Off, Offset(19.5f, 19.5f), Size(81f, 81f), CornerRadius(7f))
    if (online) {
        drawCircle(Color.White.copy(alpha = .12f), 26f, Offset(86f, 34f))
        screenText("9:41", 60f, 52f, 15f)
        if (battery != null) {
            val pct = battery.first.coerceIn(3, 100)
            drawRoundRect(Color.Black.copy(alpha = .28f), Offset(45f, 68f), Size(30f, 12f), CornerRadius(6f))
            drawRoundRect(Color.White.copy(alpha = .92f), Offset(46f, 69f), Size(maxOf(8f, 28f * pct / 100f), 10f), CornerRadius(5f))
            if (charging) bolt(Offset(60f, 74f), pulse)
        }
    }
}

private fun DrawScope.windows(h: Float, x: Float, y: Float, w: Float, hh: Float) {
    drawCircle(Color.White.copy(alpha = .12f), 16f, Offset(x + w - 12f, y + 10f))
    drawRoundRect(Color.White.copy(alpha = .88f), Offset(x + 6f, y + 8f), Size(w * .38f, hh * .42f), CornerRadius(2.5f))
    drawRoundRect(Color.Black.copy(alpha = .1f), Offset(x + 6f, y + 8f), Size(w * .38f, 4f), CornerRadius(2f))
    drawRoundRect(Color.White.copy(alpha = .6f), Offset(x + w * .52f, y + hh * .35f), Size(w * .34f, hh * .36f), CornerRadius(2.5f))
    drawRoundRect(Color.White.copy(alpha = .35f), Offset(x + w * .3f, y + hh - 8f), Size(w * .4f, 4f), CornerRadius(2f))
}

private fun DrawScope.laptop(h: Float, online: Boolean) {
    drawRoundRect(BodyBrush, Offset(18f, 20f), Size(84f, 57f), CornerRadius(5f))
    drawRoundRect(if (online) wall(h) else Off, Offset(20.4f, 22.4f), Size(79.2f, 52f), CornerRadius(2.6f))
    if (online) windows(h, 20.4f, 22.4f, 79.2f, 52f)
    val base = Path().apply {
        moveTo(8f, 79f); lineTo(112f, 79f); lineTo(109f, 87.5f)
        cubicTo(108.5f, 89.5f, 106.8f, 91f, 104.7f, 91f); lineTo(15.3f, 91f)
        cubicTo(13.2f, 91f, 11.5f, 89.5f, 11f, 87.5f); close()
    }
    drawPath(base, Brush.verticalGradient(listOf(Alu1, Alu2), 79f, 91f))
    drawRoundRect(Color(0xFF8F929C).copy(alpha = .55f), Offset(50f, 79f), Size(20f, 4f), CornerRadius(2f))
}

private fun DrawScope.desktop(h: Float, online: Boolean) {
    drawRoundRect(BodyBrush, Offset(14f, 14f), Size(92f, 62f), CornerRadius(4.5f))
    drawRoundRect(if (online) wall(h) else Off, Offset(16.6f, 16.6f), Size(86.8f, 52f), CornerRadius(2.2f))
    if (online) windows(h, 16.6f, 16.6f, 86.8f, 52f)
    val neck = Path().apply { moveTo(53f, 76f); lineTo(67f, 76f); lineTo(69f, 97f); lineTo(51f, 97f); close() }
    drawPath(neck, Brush.verticalGradient(listOf(Alu1, Alu2), 76f, 97f))
    drawRoundRect(Brush.verticalGradient(listOf(Alu1, Alu2), 97f, 102f), Offset(38f, 97f), Size(44f, 5f), CornerRadius(2.5f))
}

private fun DrawScope.server(online: Boolean, blink: Float) {
    drawRoundRect(BodyBrush, Offset(24f, 10f), Size(72f, 98f), CornerRadius(9f))
    drawRoundRect(Color.White.copy(alpha = .2f), Offset(24.4f, 10.4f), Size(71.2f, 97.2f), CornerRadius(8.6f), style = Stroke(.8f))
    val led = if (online) Color(0xFF3DDC84) else Color(0xFF55565C)
    for (i in 0 until 4) {
        val y = 18f + i * 18f
        drawRoundRect(Color(0xFF0B0B0D), Offset(31f, y), Size(58f, 13f), CornerRadius(3.2f))
        drawRoundRect(Color.White.copy(alpha = .14f), Offset(33f, y + 3.5f), Size(34f, 1.8f), CornerRadius(.9f))
        drawRoundRect(Color.White.copy(alpha = .09f), Offset(33f, y + 7f), Size(22f, 1.4f), CornerRadius(.7f))
        drawCircle(led.copy(alpha = if (online) (if ((i % 2) == 0) blink else 1.25f - blink).coerceIn(.2f, 1f) else 1f), 2f, Offset(81f, y + 6.5f))
    }
    drawCircle(Color(0xFF0B0B0D), 4.2f, Offset(60f, 97f))
    drawCircle(led, 2.1f, Offset(60f, 97f))
}

package dev.zimaconnect

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.app.Activity
import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.Path
import android.graphics.PixelFormat
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.ImageReader
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.provider.Settings
import android.util.Base64
import android.util.DisplayMetrics
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationCompat
import org.json.JSONObject
import java.io.ByteArrayOutputStream

/** This phone's screen for the owner's other devices: they see it and can touch it (both opt-in). */
object ScreenBridge {
    @Volatile var frame: ByteArray? = null
    @Volatile var width = 0
    @Volatile var height = 0

    /** When a viewer last asked for a picture: we only encode while somebody is looking. */
    @Volatile var lastAsked = 0L

    fun answer(ctx: Context, op: JSONObject): JSONObject {
        val kind = op.optString("op")
        val running = ScreenService.running
        val input = ScreenInputService.instance
        return try {
            when (kind) {
                "status" -> JSONObject().put("sharing", running).put("control", input != null).put("width", width).put("height", height)
                "frame" -> {
                    if (!running) return err("Screen sharing is off on the phone. Turn it on in the app's settings.")
                    lastAsked = System.currentTimeMillis()
                    val f = frame
                    if (f == null) JSONObject().put("wait", true) else JSONObject().put("jpg", Base64.encodeToString(f, Base64.NO_WRAP)).put("width", width).put("height", height)
                }
                "tap", "hold", "swipe", "key" -> {
                    val i = input ?: return err("Allow control: turn on Zima Connect in the phone's Accessibility settings.")
                    val (w, h) = realSize(ctx)
                    fun px(v: Double) = v.coerceIn(0.0, 1.0)
                    when (kind) {
                        "tap" -> i.stroke(px(op.getDouble("x")) * w, px(op.getDouble("y")) * h, null, null, 50)
                        "hold" -> i.stroke(px(op.getDouble("x")) * w, px(op.getDouble("y")) * h, null, null, 700)
                        "swipe" -> i.stroke(
                            px(op.getDouble("x1")) * w, px(op.getDouble("y1")) * h,
                            px(op.getDouble("x2")) * w, px(op.getDouble("y2")) * h,
                            op.optLong("ms", 250).coerceIn(50, 2000),
                        )
                        else -> {
                            val action = when (op.optString("name")) {
                                "back" -> AccessibilityService.GLOBAL_ACTION_BACK
                                "home" -> AccessibilityService.GLOBAL_ACTION_HOME
                                "recents" -> AccessibilityService.GLOBAL_ACTION_RECENTS
                                "notifications" -> AccessibilityService.GLOBAL_ACTION_NOTIFICATIONS
                                else -> return err("unknown key")
                            }
                            i.performGlobalAction(action)
                        }
                    }
                    JSONObject().put("ok", true)
                }
                else -> err("unknown request")
            }
        } catch (e: Throwable) {
            err(e.message ?: "failed")
        }
    }

    private fun err(m: String) = JSONObject().put("error", m)

    /** The whole screen in pixels, as touches are given. */
    @Suppress("DEPRECATION")
    fun realSize(ctx: Context): Pair<Int, Int> {
        val wm = ctx.getSystemService(WindowManager::class.java)
        if (Build.VERSION.SDK_INT >= 30) {
            val b = wm.maximumWindowMetrics.bounds
            return b.width() to b.height()
        }
        val m = DisplayMetrics()
        wm.defaultDisplay.getRealMetrics(m)
        return m.widthPixels to m.heightPixels
    }
}

/** Captures the screen while sharing is on. Needs the person's yes (the system asks) every time. */
class ScreenService : Service() {
    private var projection: MediaProjection? = null
    private var display: VirtualDisplay? = null
    private var reader: ImageReader? = null
    private var thread: HandlerThread? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        val code = intent?.getIntExtra(EXTRA_CODE, Activity.RESULT_CANCELED) ?: Activity.RESULT_CANCELED
        val data = intent?.getParcelableExtra<Intent>(EXTRA_DATA)
        if (code != Activity.RESULT_OK || data == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        val stop = PendingIntent.getService(this, 7, Intent(this, ScreenService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE)
        val note: Notification = NotificationCompat.Builder(this, ZimaConnectApp.CH_SERVICE)
            .setSmallIcon(android.R.drawable.ic_menu_view)
            .setContentTitle("Sharing this screen")
            .setContentText("Your other devices can see it. Tap Stop to end.")
            .setOngoing(true)
            .addAction(0, "Stop", stop)
            .build()
        if (Build.VERSION.SDK_INT >= 29) startForeground(NOTE_ID, note, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION) else startForeground(NOTE_ID, note)

        val mgr = getSystemService(MediaProjectionManager::class.java)
        val p = mgr.getMediaProjection(code, data)
        projection = p
        p.registerCallback(object : MediaProjection.Callback() {
            override fun onStop() {
                stopSelf()
            }
        }, Handler(mainLooper))

        val (rw, rh) = ScreenBridge.realSize(this)
        val scale = minOf(1f, 720f / rw)
        val w = ((rw * scale).toInt() / 2) * 2
        val h = ((rh * scale).toInt() / 2) * 2
        ScreenBridge.width = w
        ScreenBridge.height = h
        val t = HandlerThread("zc-screen").also { it.start() }
        thread = t
        val handler = Handler(t.looper)
        val r = ImageReader.newInstance(w, h, PixelFormat.RGBA_8888, 2)
        reader = r
        var last = 0L
        r.setOnImageAvailableListener({ rd ->
            val img = rd.acquireLatestImage() ?: return@setOnImageAvailableListener
            try {
                val now = System.currentTimeMillis()
                // Only while somebody looks, about eight pictures a second.
                if (now - last < 120 || now - ScreenBridge.lastAsked > 4000) return@setOnImageAvailableListener
                last = now
                val plane = img.planes[0]
                val pad = plane.rowStride - plane.pixelStride * w
                val bmp = Bitmap.createBitmap(w + pad / plane.pixelStride, h, Bitmap.Config.ARGB_8888)
                bmp.copyPixelsFromBuffer(plane.buffer)
                val out = ByteArrayOutputStream(64 * 1024)
                Bitmap.createBitmap(bmp, 0, 0, w, h).compress(Bitmap.CompressFormat.JPEG, 60, out)
                bmp.recycle()
                ScreenBridge.frame = out.toByteArray()
            } finally {
                img.close()
            }
        }, handler)
        display = p.createVirtualDisplay("zima-connect", w, h, resources.displayMetrics.densityDpi, DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, r.surface, null, handler)
        running = true
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        running = false
        ScreenBridge.frame = null
        display?.release()
        reader?.close()
        projection?.stop()
        thread?.quitSafely()
        super.onDestroy()
    }

    companion object {
        const val ACTION_STOP = "dev.zimaconnect.SCREEN_STOP"
        const val EXTRA_CODE = "code"
        const val EXTRA_DATA = "data"
        private const val NOTE_ID = 4711

        /** Observed by the settings card. */
        var running by mutableStateOf(false)
            private set

        fun start(ctx: Context, code: Int, data: Intent) {
            ctx.startForegroundService(Intent(ctx, ScreenService::class.java).putExtra(EXTRA_CODE, code).putExtra(EXTRA_DATA, data))
        }

        fun stop(ctx: Context) {
            ctx.stopService(Intent(ctx, ScreenService::class.java))
        }
    }
}

/** Lets other devices touch this screen. The person turns it on in the system's Accessibility settings. */
class ScreenInputService : AccessibilityService() {
    override fun onServiceConnected() {
        instance = this
        control = true
    }

    override fun onUnbind(intent: Intent?): Boolean {
        instance = null
        control = false
        return super.onUnbind(intent)
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}

    override fun onInterrupt() {}

    /** A tap, press or swipe. [x2]/[y2] null means a stroke that stays in place. */
    fun stroke(x: Double, y: Double, x2: Double?, y2: Double?, ms: Long) {
        val path = Path().apply {
            moveTo(x.toFloat(), y.toFloat())
            if (x2 != null && y2 != null) lineTo(x2.toFloat(), y2.toFloat())
        }
        dispatchGesture(GestureDescription.Builder().addStroke(GestureDescription.StrokeDescription(path, 0, ms)).build(), null, null)
    }

    companion object {
        @Volatile var instance: ScreenInputService? = null
        var control by mutableStateOf(false)
            private set
    }
}

/** Settings: let my other devices see and use this phone's screen. */
@Composable
fun ScreenCard(say: (String) -> Unit) {
    val ctx = LocalContext.current
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val data = r.data
        if (r.resultCode == Activity.RESULT_OK && data != null) {
            ScreenService.start(ctx, r.resultCode, data)
            say("Your other devices can now see this screen")
        } else {
            say("Screen sharing was not allowed")
        }
    }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Show this screen on my other devices", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "See and use this phone from your computer. Only your own devices, only where your household rules allow it, and only while this is on. Android asks every time.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(checked = ScreenService.running, onCheckedChange = {
                    if (it) ask.launch(ctx.getSystemService(MediaProjectionManager::class.java).createScreenCaptureIntent()) else ScreenService.stop(ctx)
                })
            }
            Spacer(Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Touch from my other devices", style = MaterialTheme.typography.titleSmall)
                    Text(
                        if (ScreenInputService.control) "Allowed. Turn it off in Accessibility settings." else "Off. Needs Zima Connect in Accessibility settings.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                OutlinedButton(onClick = { ctx.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }) { Text("Settings") }
            }
        }
    }
}

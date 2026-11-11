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
import android.graphics.Path
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.os.Bundle
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
import kotlin.math.roundToInt

/** This phone's screen for the owner's other devices: they see it and can touch it (both opt-in). */
object ScreenBridge {
    class Chunk(val seq: Long, val key: Boolean, val ts: Long, val data: ByteArray)

    private val lock = Object()
    private val chunks = ArrayList<Chunk>() // from the last key frame on
    private var seq = 0L

    @Volatile var width = 0

    @Volatile var height = 0

    @Volatile var codec = "avc1.42001f"

    @Volatile var wantKey: (() -> Unit)? = null

    fun reset() = synchronized(lock) {
        chunks.clear()
        lock.notifyAll()
    }

    fun push(data: ByteArray, key: Boolean, ts: Long) = synchronized(lock) {
        if (key) chunks.clear()
        if (chunks.isEmpty() && !key) return
        chunks.add(Chunk(++seq, key, ts, data))
        lock.notifyAll()
    }

    /** Everything after [after]; from the last key frame when the viewer is new or fell behind. */
    private fun video(after: Long): JSONObject {
        val deadline = System.currentTimeMillis() + 250
        var asked = false
        synchronized(lock) {
            while (true) {
                val first = chunks.firstOrNull()
                val out = when {
                    first == null -> emptyList()
                    after < 0 || first.seq > after + 1 -> chunks.toList()
                    else -> chunks.filter { it.seq > after }
                }
                if (out.isNotEmpty() || !ScreenService.running) {
                    val arr = org.json.JSONArray()
                    out.forEach { c ->
                        arr.put(JSONObject().put("seq", c.seq).put("key", c.key).put("ts", c.ts).put("data", Base64.encodeToString(c.data, Base64.NO_WRAP)))
                    }
                    return JSONObject().put("frames", arr).put("codec", codec).put("width", width).put("height", height)
                }
                if (first == null && !asked) {
                    asked = true
                    wantKey?.invoke()
                }
                val left = deadline - System.currentTimeMillis()
                if (left <= 0) return JSONObject().put("frames", org.json.JSONArray()).put("codec", codec).put("width", width).put("height", height)
                lock.wait(left)
            }
        }
    }

    fun answer(ctx: Context, op: JSONObject): JSONObject {
        val kind = op.optString("op")
        val running = ScreenService.running
        val input = ScreenInputService.instance
        return try {
            when (kind) {
                "status" -> JSONObject().put("sharing", running).put("control", input != null).put("width", width).put("height", height)
                "video" -> {
                    if (!running) return err("Screen sharing is off on the phone. Turn it on in the app's settings.")
                    video(op.optLong("after", -1))
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

/** Captures the screen as H.264 while sharing is on. Needs the person's yes (the system asks) every time. */
class ScreenService : Service() {
    private var projection: MediaProjection? = null
    private var display: VirtualDisplay? = null
    private var encoder: MediaCodec? = null
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

        try {
            val mgr = getSystemService(MediaProjectionManager::class.java)
            val p = mgr.getMediaProjection(code, data)
            projection = p
            p.registerCallback(object : MediaProjection.Callback() {
                override fun onStop() {
                    stopSelf()
                }
            }, Handler(mainLooper))

            // 720 wide, sizes in steps of 16 (what hardware encoders like).
            val (rw, rh) = ScreenBridge.realSize(this)
            val w = 720
            val h = ((rh * 720f / rw) / 16f).roundToInt() * 16
            ScreenBridge.width = w
            ScreenBridge.height = h
            ScreenBridge.reset()

            val t = HandlerThread("zc-screen").also { it.start() }
            thread = t
            val handler = Handler(t.looper)
            val fmt = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, w, h).apply {
                setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
                setInteger(MediaFormat.KEY_BIT_RATE, 5_000_000)
                setInteger(MediaFormat.KEY_FRAME_RATE, 30)
                setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 2)
                // A still screen still sends a picture now and then, so a new viewer is never stuck.
                setLong(MediaFormat.KEY_REPEAT_PREVIOUS_FRAME_AFTER, 200_000)
                setInteger(MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline)
                if (Build.VERSION.SDK_INT >= 30) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
            }
            val enc = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
            encoder = enc
            var csd = ByteArray(0)
            enc.setCallback(object : MediaCodec.Callback() {
                override fun onInputBufferAvailable(codec: MediaCodec, index: Int) {}

                override fun onOutputBufferAvailable(codec: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
                    try {
                        val buf = codec.getOutputBuffer(index) ?: return
                        val bytes = ByteArray(info.size)
                        buf.position(info.offset)
                        buf.get(bytes)
                        when {
                            info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0 -> {
                                csd = bytes
                                // avc1.PPCCLL from the SPS (the three bytes after the NAL header)
                                val i = (0 until bytes.size - 7).firstOrNull { bytes[it].toInt() == 0 && bytes[it + 1].toInt() == 0 && bytes[it + 2].toInt() == 1 && bytes[it + 3].toInt() and 0x1f == 7 }
                                if (i != null) ScreenBridge.codec = "avc1." + (4..6).joinToString("") { "%02x".format(bytes[i + it]) }
                            }
                            info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME != 0 -> ScreenBridge.push(csd + bytes, true, info.presentationTimeUs)
                            else -> ScreenBridge.push(bytes, false, info.presentationTimeUs)
                        }
                    } finally {
                        codec.releaseOutputBuffer(index, false)
                    }
                }

                override fun onError(codec: MediaCodec, e: MediaCodec.CodecException) {
                    stopSelf()
                }

                override fun onOutputFormatChanged(codec: MediaCodec, format: MediaFormat) {}
            }, handler)
            enc.configure(fmt, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
            val surface = enc.createInputSurface()
            enc.start()
            ScreenBridge.wantKey = {
                runCatching { enc.setParameters(Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0) }) }
            }
            display = p.createVirtualDisplay("zima-connect", w, h, resources.displayMetrics.densityDpi, DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, surface, null, handler)
            running = true
        } catch (e: Throwable) {
            stopSelf()
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        running = false
        ScreenBridge.wantKey = null
        ScreenBridge.reset()
        display?.release()
        runCatching { encoder?.stop() }
        runCatching { encoder?.release() }
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

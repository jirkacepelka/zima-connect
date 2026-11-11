package dev.zimaconnect

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.Uri
import android.media.AudioManager
import android.os.BatteryManager
import android.view.KeyEvent
import android.media.AudioAttributes
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.os.Handler
import android.os.Looper
import android.os.Build
import android.os.Environment
import android.os.IBinder
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.provider.MediaStore
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

/**
 * Foreground service: keeps the Rust node alive so other devices can reach this phone,
 * and turns incoming events into clipboard writes, files in Downloads and notifications.
 */
class ConnectService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var loop: Job? = null
    private var nextId = 1000

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(1, ongoing())
        if (loop?.isActive != true) {
            loop = scope.launch {
                if (!AppModel.ready.await()) {
                    stopSelf()
                    return@launch
                }
                // Collect anything the server kept for us while we were offline.
                launch {
                    runCatching { Api.call("pull_inbox") }
                    AppModel.refresh()
                }
                launch {
                    delay(8_000)
                    while (isActive) {
                        runCatching { pushBattery() }
                        delay(5 * 60_000L)
                    }
                }
                // LocalSend: be found by, and send to, LocalSend users on the same network.
                launch {
                    runCatching {
                        getSystemService(android.net.wifi.WifiManager::class.java)
                            .createMulticastLock("zc-localsend").apply { setReferenceCounted(false); acquire() }
                    }
                    val base = File(filesDir, "zc").apply { mkdirs() }
                    val dl = (getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: File(filesDir, "downloads")).apply { mkdirs() }
                    runCatching {
                        Api.call("ls_start", Api.args("data_dir" to base.absolutePath, "download_dir" to dl.absolutePath, "device_name" to AppModel.deviceName))
                    }.onFailure { return@launch }
                    while (isActive) {
                        val ev = runCatching { Api.call("ls_event") as? JSONObject }.onFailure { delay(3_000) }.getOrNull() ?: continue
                        runCatching { handleLs(ev) }
                    }
                }
                while (isActive) {
                    val raw = Native.nextEvent(30_000)
                    if (raw.isNotEmpty()) runCatching { handle(JSONObject(raw)) }
                }
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        scope.coroutineContext[Job]?.cancel()
        super.onDestroy()
    }

    private fun ongoing(): Notification {
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, ZimaConnectApp.CH_SERVICE)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Zima Connect")
            .setContentText("Ready to receive from your devices")
            .setContentIntent(open)
            .setOngoing(true)
            .setSilent(true)
            .build()
    }

    private fun handleLs(ev: JSONObject) {
        when (ev.optString("kind")) {
            "incoming" -> LocalSendBridge.ask(this, ev, nextId++)
            "received" -> {
                val name = ev.getString("name")
                val from = ev.getString("from")
                val shown = saveToDownloads(File(ev.getString("path")), name)
                AppModel.log("file", "Received $name", "from $from with LocalSend · $shown")
                notify("File from $from", "$name saved in $shown")
            }
        }
    }

    private suspend fun handle(ev: JSONObject) {
        when (ev.optString("kind")) {
            "clipboard" -> {
                val text = ev.getString("text")
                val from = ev.getString("from_name")
                withContext(Dispatchers.Main) {
                    getSystemService(ClipboardManager::class.java)
                        .setPrimaryClip(ClipData.newPlainText("Zima Connect", text))
                }
                AppModel.log("clipboard", "Clipboard received", "${text.length} characters from $from")
                notify("Clipboard from $from", "Copied. Paste it anywhere.")
            }
            "file_received" -> {
                val name = ev.getString("name")
                val from = ev.getString("from_name")
                val shown = saveToDownloads(File(ev.getString("path")), name)
                AppModel.log("file", "Received $name", "from $from · $shown")
                notify("File from $from", "$name saved in $shown")
            }
            "notification" -> {
                val from = ev.getString("from_name")
                AppModel.log("notification", "${ev.getString("app")}: ${ev.getString("title")}", "from $from")
                notify("${ev.getString("title")} · $from", ev.getString("body"))
            }
            "ring" -> {
                val from = ev.getString("from_name")
                AppModel.log("ring", "Found by $from", "")
                Ringer.start(this, from)
            }
            "ring_stop" -> Ringer.stop(this)
            "sms_request" -> {
                val reply = SmsBridge.answer(applicationContext, ev.getJSONObject("op"))
                runCatching { Api.call("sms_reply", Api.args("call_id" to ev.getLong("req_id"), "value" to reply)) }
            }
            "files_request" -> {
                val op = ev.getJSONObject("op")
                val id = ev.getLong("req_id")
                scope.launch(kotlinx.coroutines.Dispatchers.IO) {
                    val reply = FilesBridge.answer(applicationContext, op)
                    runCatching { Api.call("files_reply", Api.args("call_id" to id, "value" to reply)) }
                }
            }
            "screen_request" -> {
                // Video requests wait for the next picture: do not hold up the other events.
                val op = ev.getJSONObject("op")
                val id = ev.getLong("req_id")
                scope.launch(kotlinx.coroutines.Dispatchers.IO) {
                    val reply = ScreenBridge.answer(applicationContext, op)
                    runCatching { Api.call("screen_reply", Api.args("call_id" to id, "value" to reply)) }
                }
            }
            "phone_request" -> {
                val reply = PhoneBridge.answer(applicationContext, ev.getJSONObject("op"))
                runCatching { Api.call("phone_reply", Api.args("call_id" to ev.getLong("req_id"), "value" to reply)) }
            }
            "open_url" -> {
                val from = ev.getString("from_name")
                val url = ev.getString("url")
                AppModel.log("link", "Link from $from", url)
                val view = PendingIntent.getActivity(
                    this, nextId, Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                    PendingIntent.FLAG_IMMUTABLE,
                )
                notify("Link from $from", url, view)
            }
            "battery" -> {
                val from = ev.getString("from")
                AppModel.batteries = AppModel.batteries + (from to (ev.getInt("percent") to ev.getBoolean("charging")))
                AppModel.refresh()
            }
            "remote" -> remote(ev.getJSONObject("input"))
            "paired", "joined", "household_updated" -> AppModel.refresh()
        }
    }

    /** Media keys and volume from another device (a touchpad would need an accessibility service). */
    private fun remote(input: JSONObject) {
        if (input.optString("k") != "media") return
        val audio = getSystemService(AudioManager::class.java)
        fun key(code: Int) {
            audio.dispatchMediaKeyEvent(KeyEvent(KeyEvent.ACTION_DOWN, code))
            audio.dispatchMediaKeyEvent(KeyEvent(KeyEvent.ACTION_UP, code))
        }
        when (input.optString("action")) {
            "play_pause" -> key(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE)
            "next" -> key(KeyEvent.KEYCODE_MEDIA_NEXT)
            "prev" -> key(KeyEvent.KEYCODE_MEDIA_PREVIOUS)
            "vol_up" -> audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, AudioManager.ADJUST_RAISE, AudioManager.FLAG_SHOW_UI)
            "vol_down" -> audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, AudioManager.ADJUST_LOWER, AudioManager.FLAG_SHOW_UI)
            "mute" -> audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, AudioManager.ADJUST_TOGGLE_MUTE, AudioManager.FLAG_SHOW_UI)
        }
    }

    /** Tell my other devices how full the battery is, every few minutes. */
    private suspend fun pushBattery() {
        val i = registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED)) ?: return
        val level = i.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
        val scale = i.getIntExtra(BatteryManager.EXTRA_SCALE, 100)
        if (level < 0) return
        val percent = level * 100 / scale
        val status = i.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
        val charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL
        for (c in AppModel.contacts.filter { it.mine && it.online && "status" in it.caps }) {
            runCatching {
                Api.call("send_battery", Api.args("id" to c.id, "percent" to percent, "charging" to charging))
            }
        }
    }

    private fun notify(title: String, text: String, tap: PendingIntent? = null) {
        val open = tap ?: PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(this, ZimaConnectApp.CH_EVENTS)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(title)
            .setContentText(text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        runCatching {
            NotificationManagerCompat.from(this).notify(nextId++, n)
        } // fails silently when the notification permission was not granted
    }

    /** Move a received file into the public Downloads/Zima Connect folder. Returns where it ended up. */
    private fun saveToDownloads(file: File, name: String): String {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return "the app's folder"
        return runCatching {
            val values = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, name)
                put(MediaStore.Downloads.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/Zima Connect")
            }
            val uri = contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                ?: error("no uri")
            contentResolver.openOutputStream(uri)!!.use { out -> file.inputStream().use { it.copyTo(out) } }
            file.delete()
            "Downloads/Zima Connect"
        }.getOrDefault("the app's folder")
    }
}

/**
 * "Find my phone": loud alarm sound and vibration.
 * Stops on: the Stop button (notification or in the app), opening the app, the sender's
 * "Stop ringing", or after 30 seconds at the latest (a main-thread timer that always fires).
 */
object Ringer {
    private const val RING_ID = 999
    private const val MAX_MS = 30_000L
    private val main = Handler(Looper.getMainLooper())
    private var player: MediaPlayer? = null
    private var appCtx: Context? = null
    private val autoStop = Runnable { appCtx?.let { stop(it) } }

    fun start(ctx: Context, from: String) {
        val app = ctx.applicationContext
        main.post {
            stopNow(app)
            appCtx = app
            AppModel.ringingFrom = from
            val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
                ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
            player = runCatching {
                MediaPlayer().apply {
                    setAudioAttributes(
                        AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_ALARM)
                            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                            .build(),
                    )
                    setDataSource(app, uri)
                    isLooping = true
                    prepare()
                    start()
                }
            }.getOrNull()
            vibrator(app).vibrate(VibrationEffect.createWaveform(longArrayOf(0, 600, 400), 0))
            showNotification(app, from)
            main.postDelayed(autoStop, MAX_MS)
        }
    }

    /** Safe to call from any thread, any number of times. */
    fun stop(ctx: Context) {
        val app = ctx.applicationContext
        if (Looper.myLooper() == Looper.getMainLooper()) stopNow(app) else main.post { stopNow(app) }
    }

    private fun stopNow(app: Context) {
        main.removeCallbacks(autoStop)
        player?.let { p ->
            runCatching { if (p.isPlaying) p.stop() }
            runCatching { p.release() }
        }
        player = null
        runCatching { vibrator(app).cancel() }
        runCatching { NotificationManagerCompat.from(app).cancel(RING_ID) }
        AppModel.ringingFrom = null
    }

    private fun vibrator(ctx: Context): Vibrator =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            ctx.getSystemService(VibratorManager::class.java).defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            ctx.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
        }

    private fun showNotification(ctx: Context, from: String) {
        val stopIntent = PendingIntent.getBroadcast(
            ctx, 1, Intent(ctx, StopRingReceiver::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val open = PendingIntent.getActivity(
            ctx, 2, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(ctx, ZimaConnectApp.CH_RING)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("$from is looking for this phone")
            .setContentText("Tap Stop to silence it")
            .setContentIntent(open)
            .setDeleteIntent(stopIntent) // swiping it away also stops the sound
            .addAction(0, "Stop", stopIntent)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_ALARM)
            .setAutoCancel(true)
            .build()
        runCatching { NotificationManagerCompat.from(ctx).notify(RING_ID, n) }
    }
}

class StopRingReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Ringer.stop(context)
}

/** Start the service again after a reboot so the phone stays reachable. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        // After a reboot or an in-app update the node must come back by itself.
        if (intent.action == Intent.ACTION_BOOT_COMPLETED || intent.action == Intent.ACTION_MY_PACKAGE_REPLACED) {
            runCatching {
                ContextCompat.startForegroundService(context, Intent(context, ConnectService::class.java))
            }
        }
    }
}

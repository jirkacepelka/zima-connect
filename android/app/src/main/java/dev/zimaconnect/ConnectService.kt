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
import android.media.AudioAttributes
import android.media.Ringtone
import android.media.RingtoneManager
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
                Ringer.start(this)
            }
            "paired", "joined", "household_updated" -> AppModel.refresh()
        }
    }

    private fun notify(title: String, text: String) {
        val open = PendingIntent.getActivity(
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

/** "Find my phone": loud alarm sound and vibration until stopped (or 30 s). */
object Ringer {
    private var ringtone: Ringtone? = null
    private var stopper: Job? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    fun start(ctx: Context) {
        stop()
        val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
        ringtone = RingtoneManager.getRingtone(ctx, uri)?.apply {
            audioAttributes = AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_ALARM)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build()
            isLooping = true
            play()
        }
        val pattern = longArrayOf(0, 600, 400)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            ctx.getSystemService(VibratorManager::class.java).defaultVibrator
                .vibrate(VibrationEffect.createWaveform(pattern, 0))
        } else {
            @Suppress("DEPRECATION")
            (ctx.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator)
                .vibrate(VibrationEffect.createWaveform(pattern, 0))
        }

        val stopIntent = PendingIntent.getBroadcast(
            ctx, 1, Intent(ctx, StopRingReceiver::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val open = PendingIntent.getActivity(
            ctx, 2, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(ctx, ZimaConnectApp.CH_RING)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Someone is looking for this phone")
            .setContentText("Tap Stop to silence it")
            .setContentIntent(open)
            .addAction(0, "Stop", stopIntent)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(NotificationCompat.CATEGORY_ALARM)
            .build()
        runCatching { NotificationManagerCompat.from(ctx).notify(RING_ID, n) }

        stopper = scope.launch {
            delay(30_000)
            stop(ctx)
        }
    }

    fun stop(ctx: Context? = null) {
        stopper?.cancel()
        runCatching { ringtone?.stop() }
        ringtone = null
        if (ctx != null) {
            runCatching { NotificationManagerCompat.from(ctx).cancel(RING_ID) }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                ctx.getSystemService(VibratorManager::class.java).defaultVibrator.cancel()
            } else {
                @Suppress("DEPRECATION")
                (ctx.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator).cancel()
            }
        }
    }

    private const val RING_ID = 999
}

class StopRingReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) = Ringer.stop(context)
}

/** Start the service again after a reboot so the phone stays reachable. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED) {
            runCatching {
                ContextCompat.startForegroundService(context, Intent(context, ConnectService::class.java))
            }
        }
    }
}

package dev.zimaconnect

import android.Manifest
import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.media.RingtoneManager
import android.media.Ringtone
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.NoiseSuppressor
import android.os.Handler
import android.os.Looper
import android.util.Base64
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Call
import androidx.compose.material.icons.outlined.CallEnd
import androidx.compose.material.icons.outlined.Mic
import androidx.compose.material.icons.outlined.MicOff
import androidx.compose.material.icons.outlined.VolumeUp
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONObject

/** Voice calls on the phone: ring, answer, talk. Audio is PCM16 mono 16 kHz in 20 ms frames. */
object CallEngine {
    private const val RATE = 16000
    private const val FRAME = 640 // bytes = 320 samples = 20 ms
    private const val NOTIF = 4242
    private val main = Handler(Looper.getMainLooper())

    var callId by mutableStateOf(0L)
    var peer by mutableStateOf("")
    var state by mutableStateOf("idle") // idle | incoming | ringing | connected | ended
    var outgoing by mutableStateOf(false)
    var muted by mutableStateOf(false)
    var speaker by mutableStateOf(false)
    var since by mutableStateOf(0L)
    var note by mutableStateOf("")

    private var ringtone: Ringtone? = null
    @Volatile private var running = false
    private var rec: AudioRecord? = null
    private var track: AudioTrack? = null

    fun hasMic(ctx: Context) = ContextCompat.checkSelfPermission(ctx, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    // -- events from the node (ConnectService) -------------------------------------------------

    fun onIncoming(ctx: Context, id: Long, from: String) {
        main.post {
            callId = id; peer = from; state = "incoming"; outgoing = false; muted = false; speaker = false; note = ""
            startRing(ctx)
            notifyIncoming(ctx, from)
        }
    }

    fun onState(ctx: Context, id: Long, st: String, name: String) {
        main.post {
            if (id != callId && state != "idle" && state != "ended") return@post
            when (st) {
                "ringing" -> { callId = id; peer = name; state = "ringing"; outgoing = true }
                "connected" -> { stopRing(ctx); clearNotif(ctx); state = "connected"; since = System.currentTimeMillis(); startAudio(ctx, id) }
                "declined" -> finish(ctx, if (outgoing) "$name declined" else "Call declined")
                "busy" -> finish(ctx, "$name is busy")
                "unreachable" -> finish(ctx, "Could not reach $name")
                "ended" -> finish(ctx, "Call ended")
            }
        }
    }

    private fun finish(ctx: Context, text: String) {
        stopRing(ctx); clearNotif(ctx); stopAudio(ctx)
        state = "ended"; note = text
        main.postDelayed({ if (state == "ended") { state = "idle"; callId = 0 } }, 2600)
    }

    // -- actions -------------------------------------------------------------------------------

    suspend fun start(c: Contact) {
        val id = Api.call("call_start", Api.args("id" to c.id)) as Number
        main.post { callId = id.toLong(); peer = c.name; state = "ringing"; outgoing = true; muted = false; speaker = false; note = "" }
    }

    suspend fun accept() { Api.call("call_accept", Api.args("call_id" to callId)) }
    suspend fun decline() { Api.call("call_decline", Api.args("call_id" to callId)) }
    suspend fun hangUp() { Api.call("call_hangup", Api.args("call_id" to callId)) }

    fun toggleSpeaker(ctx: Context) {
        speaker = !speaker
        ctx.getSystemService(AudioManager::class.java).isSpeakerphoneOn = speaker
    }

    // -- ringing ---------------------------------------------------------------------------------

    private fun startRing(ctx: Context) {
        stopRing(ctx)
        ringtone = runCatching {
            RingtoneManager.getRingtone(ctx, RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)).apply {
                audioAttributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE).setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build()
                isLooping = true
                play()
            }
        }.getOrNull()
    }

    private fun stopRing(ctx: Context) {
        runCatching { ringtone?.stop() }
        ringtone = null
    }

    private fun notifyIncoming(ctx: Context, from: String) {
        val open = PendingIntent.getActivity(ctx, 0, Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(ctx, ZimaConnectApp.CH_CALL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("$from is calling")
            .setContentText("Zima Connect")
            .setCategory(NotificationCompat.CATEGORY_CALL)
            .setPriority(NotificationCompat.PRIORITY_MAX)
            .setOngoing(true)
            .setContentIntent(open)
            .setFullScreenIntent(open, true)
            .build()
        runCatching { NotificationManagerCompat.from(ctx).notify(NOTIF, n) }
    }

    private fun clearNotif(ctx: Context) {
        runCatching { NotificationManagerCompat.from(ctx).cancel(NOTIF) }
    }

    // -- audio -----------------------------------------------------------------------------------

    @SuppressLint("MissingPermission")
    private fun startAudio(ctx: Context, id: Long) {
        if (running) return
        running = true
        val am = ctx.getSystemService(AudioManager::class.java)
        am.mode = AudioManager.MODE_IN_COMMUNICATION
        am.isSpeakerphoneOn = speaker

        val minRec = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val r = AudioRecord(MediaRecorder.AudioSource.VOICE_COMMUNICATION, RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(minRec, FRAME * 8))
        rec = r
        if (AcousticEchoCanceler.isAvailable()) runCatching { AcousticEchoCanceler.create(r.audioSessionId)?.enabled = true }
        if (NoiseSuppressor.isAvailable()) runCatching { NoiseSuppressor.create(r.audioSessionId)?.enabled = true }
        val minPlay = AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val t = AudioTrack.Builder()
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setAudioFormat(AudioFormat.Builder().setSampleRate(RATE).setChannelMask(AudioFormat.CHANNEL_OUT_MONO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
            .setBufferSizeInBytes(maxOf(minPlay, FRAME * 8))
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()
        track = t
        r.startRecording()
        t.play()

        // microphone -> other side
        Thread({
            val buf = ByteArray(FRAME)
            while (running) {
                var got = 0
                while (got < FRAME && running) {
                    val n = r.read(buf, got, FRAME - got)
                    if (n <= 0) break
                    got += n
                }
                if (got == FRAME && !muted) {
                    val b64 = Base64.encodeToString(buf, Base64.NO_WRAP)
                    runCatching { Native.call("call_send", JSONObject().put("call_id", id).put("text", b64).toString()) }
                }
            }
        }, "zc-call-mic").start()

        // other side -> speaker
        Thread({
            while (running) {
                val res = runCatching { JSONObject(Native.call("call_recv", JSONObject().put("call_id", id).toString())) }.getOrNull() ?: break
                if (res.has("err")) break
                val v = res.opt("ok")
                if (v == JSONObject.NULL || v == null) continue
                val s = v as String
                if (s == "end") break
                val pcm = Base64.decode(s, Base64.NO_WRAP)
                t.write(pcm, 0, pcm.size)
            }
        }, "zc-call-speaker").start()
    }

    private fun stopAudio(ctx: Context) {
        running = false
        runCatching { rec?.stop() }
        runCatching { rec?.release() }
        runCatching { track?.stop() }
        runCatching { track?.release() }
        rec = null
        track = null
        val am = ctx.getSystemService(AudioManager::class.java)
        am.isSpeakerphoneOn = false
        am.mode = AudioManager.MODE_NORMAL
    }
}

/** The call card: incoming (answer or decline), calling, and in a call (mute, speaker, hang up). */
@Composable
fun CallOverlay() {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var now by remember { mutableStateOf(System.currentTimeMillis()) }
    var pendingAnswer by remember { mutableStateOf(false) }
    LaunchedEffect(CallEngine.state) {
        while (CallEngine.state == "connected") {
            now = System.currentTimeMillis()
            delay(500)
        }
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        if (ok && pendingAnswer) scope.launch { runCatching { CallEngine.accept() } }
        if (!ok && pendingAnswer) scope.launch { runCatching { CallEngine.decline() } }
        pendingAnswer = false
    }
    if (CallEngine.state == "idle") return

    Card(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(44.dp).clip(CircleShape).background(MaterialTheme.colorScheme.surfaceVariant), contentAlignment = Alignment.Center) {
                Text(CallEngine.peer.take(1).uppercase(), style = MaterialTheme.typography.titleMedium)
            }
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f)) {
                Text(CallEngine.peer, style = MaterialTheme.typography.titleMedium)
                val s = ((now - CallEngine.since) / 1000).coerceAtLeast(0)
                Text(
                    when (CallEngine.state) {
                        "incoming" -> "Incoming call"
                        "ringing" -> "Calling…"
                        "connected" -> "%d:%02d".format(s / 60, s % 60)
                        else -> CallEngine.note
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            when (CallEngine.state) {
                "incoming" -> {
                    RoundButton(Icons.Outlined.CallEnd, "Decline", Color(0xFFD63B3B)) { scope.launch { runCatching { CallEngine.decline() } } }
                    Spacer(Modifier.width(10.dp))
                    RoundButton(Icons.Outlined.Call, "Answer", Color(0xFF1FA857)) {
                        if (CallEngine.hasMic(ctx)) scope.launch { runCatching { CallEngine.accept() } } else { pendingAnswer = true; ask.launch(Manifest.permission.RECORD_AUDIO) }
                    }
                }
                "ringing" -> RoundButton(Icons.Outlined.CallEnd, "Cancel call", Color(0xFFD63B3B)) { scope.launch { runCatching { CallEngine.hangUp() } } }
                "connected" -> {
                    IconButton(onClick = { CallEngine.muted = !CallEngine.muted }) { Icon(if (CallEngine.muted) Icons.Outlined.MicOff else Icons.Outlined.Mic, if (CallEngine.muted) "Unmute" else "Mute") }
                    IconButton(onClick = { CallEngine.toggleSpeaker(ctx) }) { Icon(Icons.Outlined.VolumeUp, "Speaker", tint = if (CallEngine.speaker) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant) }
                    RoundButton(Icons.Outlined.CallEnd, "Hang up", Color(0xFFD63B3B)) { scope.launch { runCatching { CallEngine.hangUp() } } }
                }
            }
        }
    }
}

@Composable
private fun RoundButton(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, bg: Color, onClick: () -> Unit) {
    IconButton(onClick = onClick, modifier = Modifier.size(46.dp).clip(CircleShape).background(bg)) { Icon(icon, label, tint = Color.White) }
}

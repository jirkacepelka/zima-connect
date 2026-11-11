package dev.zimaconnect

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.json.JSONObject

/** A device nearby that speaks the LocalSend protocol. */
data class Nearby(val fingerprint: String, val alias: String, val model: String?, val type: String, val ip: String)

object LocalSendBridge {
    fun parse(v: Any?): List<Nearby> {
        val a = v as? org.json.JSONArray ?: return emptyList()
        return (0 until a.length()).map { a.getJSONObject(it) }.map { o ->
            Nearby(
                o.getString("fingerprint"), o.getString("alias"),
                if (o.isNull("model")) null else o.getString("model"),
                o.optString("device_type", "desktop"), o.optString("ip"),
            )
        }
    }

    /** Somebody wants to send files: ask with Accept and Decline right in the notification. */
    fun ask(ctx: Context, ev: JSONObject, id: Int) {
        val sid = ev.getString("session")
        val files = ev.getJSONArray("files")
        val first = files.optJSONObject(0)?.optString("name") ?: "a file"
        val text = if (files.length() > 1) "$first and ${files.length() - 1} more" else first
        fun action(accept: Boolean) = PendingIntent.getBroadcast(
            ctx, id * 2 + if (accept) 1 else 0,
            Intent(ctx, LocalSendReceiver::class.java).putExtra("session", sid).putExtra("accept", accept).putExtra("nid", id),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val open = PendingIntent.getActivity(ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(ctx, ZimaConnectApp.CH_EVENTS)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("${ev.getString("from")} wants to send you files")
            .setContentText(text)
            .setContentIntent(open)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setTimeoutAfter(60_000)
            .addAction(0, "Decline", action(false))
            .addAction(0, "Accept", action(true))
            .build()
        runCatching { NotificationManagerCompat.from(ctx).notify(id, n) }
    }
}

class LocalSendReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val sid = intent.getStringExtra("session") ?: return
        val accept = intent.getBooleanExtra("accept", false)
        runCatching { NotificationManagerCompat.from(context).cancel(intent.getIntExtra("nid", 0)) }
        val pending = goAsync()
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
            runCatching { Api.call("ls_decide", Api.args("id" to sid, "accept" to accept)) }
            pending.finish()
        }
    }
}

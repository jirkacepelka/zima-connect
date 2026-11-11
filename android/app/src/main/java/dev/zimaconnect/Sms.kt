package dev.zimaconnect

import android.Manifest
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.ContactsContract
import android.provider.Telephony
import android.telephony.SmsManager
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
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject

private val SMS_PERMISSIONS = arrayOf(
    Manifest.permission.READ_SMS,
    Manifest.permission.SEND_SMS,
    Manifest.permission.RECEIVE_SMS,
    Manifest.permission.READ_CONTACTS,
)

/** Text messages of this phone for the owner's other devices (opt-in). */
object SmsBridge {
    private val names = HashMap<String, String?>()

    fun enabled(ctx: Context) = ctx.getSharedPreferences("zc", Context.MODE_PRIVATE).getBoolean("sms_share", false)

    fun granted(ctx: Context) = SMS_PERMISSIONS.all { ContextCompat.checkSelfPermission(ctx, it) == PackageManager.PERMISSION_GRANTED }

    private fun nameOf(ctx: Context, address: String): String? = names.getOrPut(address) {
        runCatching {
            val uri = Uri.withAppendedPath(ContactsContract.PhoneLookup.CONTENT_FILTER_URI, Uri.encode(address))
            ctx.contentResolver.query(uri, arrayOf(ContactsContract.PhoneLookup.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }
        }.getOrNull()
    }

    /** Another device asked about the messages: answer with JSON (or an {"error": …}). */
    fun answer(ctx: Context, op: JSONObject): Any {
        if (!enabled(ctx) || !granted(ctx)) return JSONObject().put("error", "Text messages are not shared from this phone. Turn them on in its settings.")
        return runCatching {
            when (op.getString("op")) {
                "threads" -> threads(ctx)
                "thread" -> thread(ctx, op.getString("id"))
                "send" -> {
                    send(ctx, op.getString("to"), op.getString("body"))
                    JSONObject().put("ok", true)
                }
                else -> JSONObject().put("error", "unknown request")
            }
        }.getOrElse { JSONObject().put("error", it.message ?: "could not read messages") }
    }

    private fun threads(ctx: Context): JSONArray {
        val out = JSONArray()
        val seen = HashSet<String>()
        ctx.contentResolver.query(
            Telephony.Sms.CONTENT_URI,
            arrayOf(Telephony.Sms.THREAD_ID, Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE, Telephony.Sms.READ),
            null, null, "${Telephony.Sms.DATE} DESC",
        )?.use { c ->
            while (c.moveToNext() && out.length() < 60) {
                val id = c.getString(0) ?: continue
                if (!seen.add(id)) continue
                val addr = c.getString(1) ?: ""
                out.put(
                    JSONObject().put("id", id).put("address", addr).put("name", nameOf(ctx, addr) ?: JSONObject.NULL)
                        .put("snippet", (c.getString(2) ?: "").take(120)).put("date", c.getLong(3)).put("unread", c.getInt(4) == 0),
                )
            }
        }
        return out
    }

    private fun thread(ctx: Context, id: String): JSONArray {
        val rows = ArrayList<JSONObject>()
        ctx.contentResolver.query(
            Telephony.Sms.CONTENT_URI,
            arrayOf(Telephony.Sms._ID, Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE, Telephony.Sms.TYPE),
            "${Telephony.Sms.THREAD_ID} = ?", arrayOf(id), "${Telephony.Sms.DATE} DESC",
        )?.use { c ->
            while (c.moveToNext() && rows.size < 200) {
                rows.add(
                    JSONObject().put("id", c.getString(0)).put("address", c.getString(1) ?: "").put("body", c.getString(2) ?: "")
                        .put("date", c.getLong(3)).put("outgoing", c.getInt(4) != Telephony.Sms.MESSAGE_TYPE_INBOX),
                )
            }
        }
        return JSONArray(rows.reversed())
    }

    private fun send(ctx: Context, to: String, body: String) {
        require(to.length in 3..32 && to.all { it.isDigit() || it in "+ -()" }) { "that is not a phone number" }
        require(body.isNotBlank()) { "the message is empty" }
        val sms = ctx.getSystemService(SmsManager::class.java)
        sms.sendMultipartTextMessage(to.filter { it.isDigit() || it == '+' }, null, sms.divideMessage(body), null, null)
    }

    /** A message arrived: tell my other devices that may see it. */
    fun pushIncoming(ctx: Context, address: String, body: String) {
        if (!enabled(ctx) || !granted(ctx)) return
        val name = nameOf(ctx, address)
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
            for (c in AppModel.contacts.filter { it.mine && it.online && "sms" in it.caps }) {
                runCatching {
                    Api.call("sms_push", Api.args("id" to c.id, "path" to address, "user_name" to name, "text" to body))
                }
            }
        }
    }
}

class SmsReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Telephony.Sms.Intents.SMS_RECEIVED_ACTION) return
        val parts = Telephony.Sms.Intents.getMessagesFromIntent(intent) ?: return
        if (parts.isEmpty()) return
        val address = parts[0].originatingAddress ?: return
        SmsBridge.pushIncoming(context, address, parts.joinToString("") { it.messageBody ?: "" })
    }
}

/** Settings: show this phone's text messages on my other devices. */
@Composable
fun SmsCard(say: (String) -> Unit) {
    val ctx = LocalContext.current
    val prefs = remember { ctx.getSharedPreferences("zc", Context.MODE_PRIVATE) }
    var on by remember { mutableStateOf(prefs.getBoolean("sms_share", false) && SmsBridge.granted(ctx)) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { r ->
        if (r.values.all { it }) {
            on = true
            prefs.edit().putBoolean("sms_share", true).apply()
            say("Text messages are shared with your other devices")
        } else {
            on = false
            say("Allow text messages and contacts to share them")
        }
    }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Text messages on my other devices", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "Read and answer this phone's SMS from your computer. Only your own devices, and only where your household rules allow it.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(checked = on, onCheckedChange = {
                    if (it) ask.launch(SMS_PERMISSIONS) else {
                        on = false
                        prefs.edit().putBoolean("sms_share", false).apply()
                    }
                })
            }
            Spacer(Modifier.height(2.dp))
        }
    }
}

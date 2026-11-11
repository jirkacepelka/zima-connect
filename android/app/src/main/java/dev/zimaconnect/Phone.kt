package dev.zimaconnect

import android.Manifest
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioManager
import android.net.Uri
import android.os.Bundle
import android.provider.ContactsContract
import android.telecom.TelecomManager
import android.telephony.TelephonyManager
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
import org.json.JSONObject

private val PHONE_PERMISSIONS = arrayOf(
    Manifest.permission.CALL_PHONE,
    Manifest.permission.ANSWER_PHONE_CALLS,
    Manifest.permission.READ_PHONE_STATE,
    Manifest.permission.READ_CALL_LOG, // gives the number of an incoming call
    Manifest.permission.READ_CONTACTS, // gives the name
)

/** This phone's line for the owner's other devices: dial, answer, decline, hang up (opt-in). */
object PhoneBridge {
    private var state = "idle"
    private var number = ""
    private var name: String? = null

    fun enabled(ctx: Context) = ctx.getSharedPreferences("zc", Context.MODE_PRIVATE).getBoolean("phone_share", false)

    fun granted(ctx: Context) = PHONE_PERMISSIONS.all { ContextCompat.checkSelfPermission(ctx, it) == PackageManager.PERMISSION_GRANTED }

    private fun nameOf(ctx: Context, address: String): String? {
        if (address.isEmpty()) return null
        return runCatching {
            val uri = Uri.withAppendedPath(ContactsContract.PhoneLookup.CONTENT_FILTER_URI, Uri.encode(address))
            ctx.contentResolver.query(uri, arrayOf(ContactsContract.PhoneLookup.DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }
        }.getOrNull()
    }

    private fun err(m: String) = JSONObject().put("error", m)

    /** Another device asked something of the line: answer with JSON (or an {"error": …}). */
    fun answer(ctx: Context, op: JSONObject): Any {
        if (!enabled(ctx) || !granted(ctx)) return err("Phone calls are not shared from this phone. Turn them on in its settings.")
        val tm = ctx.getSystemService(TelecomManager::class.java)
        return runCatching {
            when (op.getString("op")) {
                "dial" -> {
                    val n = op.getString("number")
                    require(n.length in 3..32 && n.all { it.isDigit() || it in "+ -()*#" }) { "that is not a phone number" }
                    check(state == "idle") { "the phone is already in a call" }
                    tm.placeCall(Uri.fromParts("tel", n.filter { it.isDigit() || it in "+*#" }, null), Bundle())
                    JSONObject().put("dialing", n)
                }
                "answer" -> {
                    check(state == "ringing") { "nobody is calling" }
                    tm.acceptRingingCall()
                    JSONObject().put("ok", true)
                }
                "decline", "hang_up" -> {
                    check(state != "idle") { "there is no call" }
                    check(tm.endCall()) { "the phone would not end the call" }
                    JSONObject().put("ok", true)
                }
                "mute" -> {
                    ctx.getSystemService(AudioManager::class.java).isMicrophoneMute = op.getBoolean("on")
                    JSONObject().put("ok", true)
                }
                "state" -> JSONObject().put("state", state).put("number", number)
                else -> err("unknown request")
            }
        }.getOrElse { err(it.message ?: "the phone could not do that") }
    }

    /** The system says the line changed: remember it and tell my other devices. */
    fun onLine(ctx: Context, system: String, incoming: String?) {
        if (!enabled(ctx) || !granted(ctx)) return
        val next = when (system) {
            TelephonyManager.EXTRA_STATE_RINGING -> "ringing"
            TelephonyManager.EXTRA_STATE_OFFHOOK -> "talking"
            else -> "idle"
        }
        if (!incoming.isNullOrEmpty()) {
            number = incoming
            name = nameOf(ctx, incoming)
        }
        // The first ringing broadcast often has no number yet; wait for the one that has.
        if (next == state && incoming.isNullOrEmpty()) return
        if (next == "ringing" && number.isEmpty()) {
            state = next
            return
        }
        state = next
        val shownNumber = number
        val shownName = name
        if (next == "idle") {
            number = ""
            name = null
        }
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
            for (c in AppModel.contacts.filter { it.mine && it.online && "calls" in it.caps }) {
                runCatching {
                    Api.call("phone_push", Api.args("id" to c.id, "title" to next, "path" to shownNumber, "user_name" to shownName))
                }
            }
        }
    }
}

class PhoneStateReceiver : BroadcastReceiver() {
    @Suppress("DEPRECATION")
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != TelephonyManager.ACTION_PHONE_STATE_CHANGED) return
        val st = intent.getStringExtra(TelephonyManager.EXTRA_STATE) ?: return
        PhoneBridge.onLine(context, st, intent.getStringExtra(TelephonyManager.EXTRA_INCOMING_NUMBER))
    }
}

/** Settings: let my other devices dial and answer through this phone. */
@Composable
fun PhoneCard(say: (String) -> Unit) {
    val ctx = LocalContext.current
    val prefs = remember { ctx.getSharedPreferences("zc", Context.MODE_PRIVATE) }
    var on by remember { mutableStateOf(prefs.getBoolean("phone_share", false) && PhoneBridge.granted(ctx)) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { r ->
        if (r.values.all { it }) {
            on = true
            prefs.edit().putBoolean("phone_share", true).apply()
            say("Your other devices can now call through this phone")
        } else {
            on = false
            say("Allow phone and contacts to use this phone's line")
        }
    }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Calls from my other devices", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "Dial from your computer and see who is calling, answer or decline there. The call itself stays on this phone. Only your own devices, where your household rules allow it.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(checked = on, onCheckedChange = {
                    if (it) ask.launch(PHONE_PERMISSIONS) else {
                        on = false
                        prefs.edit().putBoolean("phone_share", false).apply()
                    }
                })
            }
            Spacer(Modifier.height(2.dp))
        }
    }
}

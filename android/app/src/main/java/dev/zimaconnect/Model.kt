package dev.zimaconnect

import android.content.Context
import android.os.Build
import android.os.Environment
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

data class Contact(
    val id: String,
    val name: String,
    val platform: String,
    val userId: String?,
    val userName: String?,
    val mine: Boolean,
    val direct: Boolean,
    val caps: Set<String>,
    val online: Boolean = false,
    val rttMs: Long? = null,
    /** Battery (percent, charging) as last reported by that device. */
    val battery: Pair<Int, Boolean>? = null,
)

data class User(val id: String, val name: String, val owner: Boolean)
data class Device(val id: String, val userId: String, val name: String, val platform: String)
data class Rule(val from: String, val to: String, val cap: String, val allow: Boolean)

data class Household(
    val name: String,
    val me: User,
    val canAdmin: Boolean,
    val users: List<User>,
    val devices: List<Device>,
    val rules: List<Rule>,
) {
    fun allowed(from: String, to: String, cap: String): Boolean {
        rules.firstOrNull { it.from == from && it.to == to && it.cap == cap }?.let { return it.allow }
        return defaultAllowed(from, to, cap)
    }
}

/** Own devices: everything. Between people: things done on purpose (files, clipboard, find my device). */
fun defaultAllowed(from: String, to: String, cap: String) =
    from == to || cap == "files" || cap == "clipboard" || cap == "ring"

data class Activity(val ts: Long, val kind: String, val title: String, val detail: String, val ok: Boolean = true)

/** What a send did: handed over directly, or kept on the server for an offline device. */
enum class Delivery { Delivered, Queued }

val CAPS = listOf(
    "files" to "Files and photos",
    "clipboard" to "Clipboard",
    "notifications" to "Notifications",
    "ring" to "Find my device",
    "calls" to "Voice calls",
    "remote" to "Remote control",
    "status" to "Battery and status",
)

data class ShelfItem(
    val id: String,
    val name: String,
    val size: Long,
    val ownerName: String,
    val everyone: Boolean,
    val toUsers: List<String>,
    val mine: Boolean,
)

fun parseShelf(v: Any?, users: List<User>): List<ShelfItem> {
    val arr = v as? JSONArray ?: return emptyList()
    return arr.objects().map { o ->
        val a = o.getJSONObject("audience")
        val ids = if (a.getString("to") == "users") a.getJSONArray("ids").let { x -> (0 until x.length()).map { x.getString(it) } } else emptyList()
        ShelfItem(
            id = o.getString("id"),
            name = o.getString("name"),
            size = o.getLong("size"),
            ownerName = o.getString("owner_user_name"),
            everyone = a.getString("to") == "everyone",
            toUsers = ids.map { id -> users.firstOrNull { it.id == id }?.name ?: "someone" },
            mine = o.optBoolean("mine"),
        )
    }
}

private fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }
private fun JSONObject.strOrNull(k: String): String? = if (isNull(k)) null else getString(k)

private fun parseUser(o: JSONObject) = User(o.getString("id"), o.getString("name"), o.getString("role") == "owner")

fun parseHousehold(v: Any?): Household? {
    val o = v as? JSONObject ?: return null
    val info = o.getJSONObject("info")
    val snap = o.getJSONObject("snapshot")
    return Household(
        name = info.getString("household"),
        me = parseUser(info.getJSONObject("me")),
        canAdmin = info.getBoolean("can_admin"),
        users = snap.getJSONArray("users").objects().map(::parseUser),
        devices = snap.getJSONArray("devices").objects().map {
            Device(it.getString("id"), it.getString("user_id"), it.getString("name"), it.getString("platform"))
        },
        rules = snap.getJSONArray("rules").objects().map {
            Rule(it.getString("from_user"), it.getString("to_user"), it.getString("cap"), it.getBoolean("allow"))
        },
    )
}

fun parseContacts(v: Any?, previous: List<Contact>): List<Contact> {
    val arr = v as? JSONArray ?: return emptyList()
    return arr.objects().map { o ->
        val caps = o.getJSONArray("caps").let { a -> (0 until a.length()).map { a.getString(it) }.toSet() }
        val id = o.getString("id")
        val old = previous.firstOrNull { it.id == id }
        Contact(
            id = id,
            name = o.getString("name"),
            platform = o.getString("platform"),
            userId = o.strOrNull("user_id"),
            userName = o.strOrNull("user_name"),
            mine = o.getBoolean("mine"),
            direct = o.getBoolean("direct"),
            caps = caps,
            online = old?.online ?: false,
            rttMs = old?.rttMs,
            battery = AppModel.batteries[id],
        )
    }
}

/** Everything the UI shows. Compose reads these directly. */
object AppModel {
    var contacts by mutableStateOf(listOf<Contact>())
    var household by mutableStateOf<Household?>(null)
    var activities by mutableStateOf(listOf<Activity>())
    var myId by mutableStateOf("")
    var deviceName by mutableStateOf("")
    var startError by mutableStateOf<String?>(null)

    /** Battery levels other devices told us about: device id to (percent, charging). */
    var batteries by mutableStateOf(mapOf<String, Pair<Int, Boolean>>())
    var shelf by mutableStateOf(listOf<ShelfItem>())

    /** Who is making this phone ring right now (null = silent). */
    var ringingFrom by mutableStateOf<String?>(null)

    /** Completes once the Rust node is running. */
    val ready = CompletableDeferred<Boolean>()

    suspend fun start(ctx: Context) {
        if (ready.isCompleted) return
        try {
            val base = File(ctx.filesDir, "zc").apply { mkdirs() }
            val downloads = (ctx.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: File(ctx.filesDir, "downloads"))
                .apply { mkdirs() }
            deviceName = Build.MODEL ?: "Android"
            val r = Api.call(
                "init",
                Api.args(
                    "data_dir" to base.absolutePath,
                    "download_dir" to downloads.absolutePath,
                    "device_name" to deviceName,
                ),
            ) as JSONObject
            myId = r.getString("id")
            ready.complete(true)
        } catch (e: Throwable) {
            startError = e.message ?: e.toString()
            ready.complete(false)
        }
    }

    fun log(kind: String, title: String, detail: String, ok: Boolean = true) {
        activities = (listOf(Activity(System.currentTimeMillis(), kind, title, detail, ok)) + activities).take(50)
    }

    /** Reload people and contacts, then check who is reachable (in parallel). */
    suspend fun refresh() {
        if (ready.await().not()) return
        household = parseHousehold(Api.call("household"))
        contacts = parseContacts(Api.call("contacts"), contacts)
        val pinged = coroutineScope {
            contacts.map { c ->
                async {
                    try {
                        val r = Api.call("ping", Api.args("id" to c.id)) as JSONObject
                        c.copy(online = true, rttMs = r.getLong("rtt_ms"))
                    } catch (_: Throwable) {
                        c.copy(online = false, rttMs = null)
                    }
                }
            }.awaitAll()
        }
        // Keep whatever changed meanwhile (e.g. a contact that just joined).
        contacts = contacts.map { c -> (pinged.firstOrNull { it.id == c.id } ?: c).copy(battery = batteries[c.id]) }
    }

    suspend fun renameDevice(name: String) {
        Api.call("rename_device", Api.args("device_name" to name))
        deviceName = name.trim()
        refresh()
    }

    suspend fun sendUrl(peer: Contact, url: String) {
        Api.call("send_url", Api.args("id" to peer.id, "text" to url))
        log("link", "Link sent", "to ${peer.name}")
    }

    suspend fun sendRemote(peer: Contact, input: JSONObject) {
        Api.call("send_remote", Api.args("id" to peer.id, "input" to input))
    }

    suspend fun refreshShelf() {
        shelf = parseShelf(Api.call("shelf_list"), household?.users ?: emptyList())
    }

    suspend fun shelfPut(file: File, label: String) {
        Api.call("shelf_put", Api.args("path" to file.absolutePath))
        log("shelf", "Put $label on your shelf", "reachable from all your devices")
        refreshShelf()
    }

    suspend fun shelfGet(item: ShelfItem): String {
        val path = Api.call("shelf_get", Api.args("id" to item.id)) as String
        log("file", "Took ${item.name} from the shelf", "")
        return path
    }

    suspend fun shelfDelete(item: ShelfItem) {
        Api.call("shelf_delete", Api.args("id" to item.id))
        refreshShelf()
    }

    suspend fun sendClipboard(peer: Contact, text: String): Delivery {
        val d = Api.call("send_clipboard", Api.args("id" to peer.id, "text" to text)) as String
        val delivery = if (d == "queued") Delivery.Queued else Delivery.Delivered
        log(
            "clipboard",
            if (delivery == Delivery.Queued) "Clipboard queued" else "Clipboard sent",
            if (delivery == Delivery.Queued) "${peer.name} is offline, the server will deliver it" else "${text.length} characters to ${peer.name}",
        )
        return delivery
    }

    suspend fun sendFile(peer: Contact, file: File, label: String): Delivery {
        val d = Api.call("send_file", Api.args("id" to peer.id, "path" to file.absolutePath)) as String
        val delivery = if (d == "queued") Delivery.Queued else Delivery.Delivered
        log(
            "file",
            if (delivery == Delivery.Queued) "Queued $label" else "Sent $label",
            if (delivery == Delivery.Queued) "${peer.name} is offline, the server will deliver it" else "to ${peer.name}",
        )
        return delivery
    }
}

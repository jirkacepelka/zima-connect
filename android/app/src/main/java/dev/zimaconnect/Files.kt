package dev.zimaconnect

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
import android.util.Base64
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
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
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.RandomAccessFile

/** The files of this phone for the owner's other devices: browse and copy, read-only (opt-in). */
object FilesBridge {
    private const val MAX_READ = 768 * 1024

    fun enabled(ctx: Context) = ctx.getSharedPreferences("zc", Context.MODE_PRIVATE).getBoolean("files_share", false)

    /** Reading everything on the phone needs the "all files" permission the person grants in Settings. */
    fun allowed(): Boolean = if (Build.VERSION.SDK_INT >= 30) Environment.isExternalStorageManager() else true

    private fun root(): File = Environment.getExternalStorageDirectory()

    private fun err(m: String) = JSONObject().put("error", m)

    /** The real file for a shared path, never outside the phone's shared storage. */
    private fun resolve(path: String): File? {
        val parts = path.split('/', '\\').filter { it.isNotEmpty() && it != "." }
        if (parts.any { it == ".." || it.contains(':') || it.contains('\u0000') }) return null
        var f = root()
        for (p in parts) f = File(f, p)
        val real = runCatching { f.canonicalFile }.getOrNull() ?: return null
        val base = runCatching { root().canonicalFile }.getOrNull() ?: return null
        return if (real.path == base.path || real.path.startsWith(base.path + File.separator)) real else null
    }

    fun answer(ctx: Context, op: JSONObject): JSONObject {
        if (!enabled(ctx)) return err("File sharing is off on the phone. Turn it on in the app's settings.")
        if (!allowed()) return err("The phone has not allowed access to all files yet (app settings).")
        return try {
            when (op.optString("op")) {
                "list" -> {
                    val dir = resolve(op.optString("path", "/")) ?: return err("that location is not available")
                    val kids = dir.listFiles() ?: return err("cannot open the folder")
                    val arr = JSONArray()
                    kids.take(5000).forEach { f ->
                        arr.put(JSONObject().put("name", f.name).put("dir", f.isDirectory).put("size", if (f.isDirectory) 0 else f.length()).put("mtime", f.lastModified() / 1000))
                    }
                    JSONObject().put("entries", arr)
                }
                "stat" -> {
                    val f = resolve(op.optString("path", "/")) ?: return err("that location is not available")
                    if (!f.exists()) return err("not found")
                    JSONObject().put("dir", f.isDirectory).put("size", if (f.isDirectory) 0 else f.length()).put("mtime", f.lastModified() / 1000)
                }
                "read" -> {
                    val f = resolve(op.optString("path")) ?: return err("that location is not available")
                    if (!f.isFile) return err("that is not a file")
                    val offset = op.optLong("offset", 0)
                    val want = minOf(op.optLong("len", MAX_READ.toLong()), MAX_READ.toLong()).toInt()
                    RandomAccessFile(f, "r").use { r ->
                        r.seek(offset)
                        val buf = ByteArray(want)
                        var got = 0
                        while (got < want) {
                            val n = r.read(buf, got, want - got)
                            if (n <= 0) break
                            got += n
                        }
                        JSONObject()
                            .put("data", Base64.encodeToString(buf, 0, got, Base64.NO_WRAP))
                            .put("size", f.length())
                            .put("eof", offset + got >= f.length())
                    }
                }
                else -> err("unknown request")
            }
        } catch (e: Throwable) {
            err(e.message ?: "failed")
        }
    }
}

/** Settings: let my other devices browse and copy this phone's files. */
@Composable
fun FilesCard(say: (String) -> Unit) {
    val ctx = LocalContext.current
    val prefs = remember { ctx.getSharedPreferences("zc", Context.MODE_PRIVATE) }
    var on by remember { mutableStateOf(prefs.getBoolean("files_share", false)) }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Files for my other devices", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "Browse this phone's storage from your computer and copy files off it. Read-only, only your own devices, only where your household rules allow it. Android asks you to allow access to all files.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Switch(checked = on, onCheckedChange = {
                    on = it
                    prefs.edit().putBoolean("files_share", it).apply()
                    if (it && !FilesBridge.allowed()) {
                        say("Allow access to all files for Zima Connect, then come back")
                        val i = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + ctx.packageName)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                        runCatching { ctx.startActivity(i) }
                    }
                })
            }
        }
    }
}

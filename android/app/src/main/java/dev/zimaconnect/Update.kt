package dev.zimaconnect

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.provider.Settings
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/** In-app updates from GitHub Releases: check, show what is new, download, verify, install. */
object Updater {
    private const val LATEST = "https://api.github.com/repos/jirkacepelka/zima-connect/releases/latest"
    private const val DOWNLOAD_PREFIX = "https://github.com/jirkacepelka/zima-connect/"

    data class Info(
        val latest: String,
        val notes: String,
        val size: Long,
        val url: String,
        val sha256: String?,
        val page: String,
    )

    var available by mutableStateOf<Info?>(null)

    /** 0f..1f while downloading, null otherwise. */
    var progress by mutableStateOf<Float?>(null)

    fun currentVersion(ctx: Context): String =
        runCatching { ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName }.getOrNull() ?: "0"

    private fun parse(v: String) = v.trimStart('v').split('.', '-', '+').take(3).map { it.toIntOrNull() ?: 0 }

    fun newer(latest: String, current: String): Boolean {
        val a = parse(latest)
        val b = parse(current)
        for (i in 0 until 3) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x > y
        }
        return false
    }

    /** Looks up the latest release. Sets [available] when it is newer than this install. */
    suspend fun check(ctx: Context): Info? = withContext(Dispatchers.IO) {
        val conn = URL(LATEST).openConnection() as HttpURLConnection
        try {
            conn.connectTimeout = 15_000
            conn.readTimeout = 20_000
            conn.setRequestProperty("Accept", "application/vnd.github+json")
            conn.setRequestProperty("User-Agent", "zima-connect-updater")
            if (conn.responseCode != 200) throw ApiException("GitHub answered ${conn.responseCode}")
            val rel = JSONObject(conn.inputStream.bufferedReader().readText())
            val tag = rel.getString("tag_name").trimStart('v')
            val info = if (newer(tag, currentVersion(ctx))) {
                val assets = rel.getJSONArray("assets")
                var apk: JSONObject? = null
                for (i in 0 until assets.length()) {
                    val a = assets.getJSONObject(i)
                    if (a.getString("name").endsWith("-android.apk")) apk = a
                }
                apk?.let {
                    Info(
                        latest = tag,
                        notes = rel.optString("body"),
                        size = it.getLong("size"),
                        url = it.getString("browser_download_url"),
                        sha256 = it.optString("digest").removePrefix("sha256:").ifEmpty { null },
                        page = rel.getString("html_url"),
                    )
                }
            } else null
            available = info
            info
        } finally {
            conn.disconnect()
        }
    }

    /** Download, verify and hand the APK to the system installer (it asks once to confirm). */
    suspend fun install(ctx: Context, info: Info) {
        if (!info.url.startsWith(DOWNLOAD_PREFIX)) throw ApiException("unexpected download address")
        if (!ctx.packageManager.canRequestPackageInstalls()) {
            ctx.startActivity(
                Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${ctx.packageName}"))
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            )
            throw ApiException("Allow Zima Connect to install apps, then press Update again")
        }
        val file = File(ctx.cacheDir, "update.apk")
        withContext(Dispatchers.IO) {
            progress = 0f
            try {
                val conn = URL(info.url).openConnection() as HttpURLConnection
                conn.connectTimeout = 15_000
                conn.readTimeout = 30_000
                conn.setRequestProperty("User-Agent", "zima-connect-updater")
                val total = conn.contentLengthLong.takeIf { it > 0 } ?: info.size
                val md = MessageDigest.getInstance("SHA-256")
                var done = 0L
                conn.inputStream.use { input ->
                    file.outputStream().use { out ->
                        val buf = ByteArray(64 * 1024)
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            out.write(buf, 0, n)
                            md.update(buf, 0, n)
                            done += n
                            progress = (done.toFloat() / total.coerceAtLeast(1)).coerceIn(0f, 1f)
                        }
                    }
                }
                val got = md.digest().joinToString("") { "%02x".format(it) }
                if (info.sha256 != null && !got.equals(info.sha256, ignoreCase = true)) {
                    file.delete()
                    throw ApiException("The download does not match its checksum, update cancelled")
                }
                commit(ctx, file)
            } finally {
                progress = null
            }
        }
    }

    private fun commit(ctx: Context, apk: File) {
        val installer = ctx.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
            session.openWrite("zima-connect", 0, apk.length()).use { out ->
                apk.inputStream().use { it.copyTo(out) }
                session.fsync(out)
            }
            val pending = PendingIntent.getBroadcast(
                ctx, id, Intent(ctx, InstallReceiver::class.java).setPackage(ctx.packageName),
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
            )
            session.commit(pending.intentSender)
        }
    }
}

/** The system installer reports back here; it may need the user's confirmation. */
class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, -1)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION")
                val confirm = intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
                confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                if (confirm != null) context.startActivity(confirm)
            }
        }
    }
}

/** "Version X is available" with what is new; shown above the content. */
@Composable
fun UpdateBanner(say: (String) -> Unit) {
    val info = Updater.available ?: return
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var open by remember { mutableStateOf(false) }

    Card(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
    ) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("Version ${info.latest} is available", style = MaterialTheme.typography.titleMedium)
                Text(
                    "You have ${Updater.currentVersion(ctx)}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Button(onClick = { open = true }) { Text("Update") }
        }
    }

    if (open) {
        val busy = Updater.progress != null
        AlertDialog(
            onDismissRequest = { if (!busy) open = false },
            title = { Text("Zima Connect ${info.latest}") },
            text = {
                Column {
                    Text(
                        "Your devices, people and settings stay as they are.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Spacer(Modifier.height(10.dp))
                    Column(Modifier.heightIn(max = 260.dp).verticalScroll(rememberScrollState())) {
                        ReleaseNotes(info.notes)
                    }
                    Updater.progress?.let {
                        Spacer(Modifier.height(12.dp))
                        LinearProgressIndicator(progress = { it }, modifier = Modifier.fillMaxWidth())
                        Text("Downloading… ${(it * 100).toInt()} %", style = MaterialTheme.typography.bodySmall)
                    }
                }
            },
            confirmButton = {
                Button(
                    enabled = !busy,
                    onClick = {
                        scope.launch {
                            try {
                                Updater.install(ctx, info)
                                open = false
                            } catch (e: Throwable) {
                                say(e.message ?: "Could not update")
                            }
                        }
                    },
                ) { Text(if (busy) "Downloading…" else "Update") }
            },
            dismissButton = { TextButton(enabled = !busy, onClick = { open = false }) { Text("Later") } },
        )
    }
}

@Composable
private fun ReleaseNotes(md: String) {
    val lines = md.lines().map { it.trim() }.filter { it.isNotEmpty() && !it.lowercase().trimStart('*').startsWith("full changelog") }
    if (lines.isEmpty()) Text("No release notes.")
    for (l in lines) {
        val clean = { s: String -> s.replace("**", "").replace("`", "") }
        when {
            l.startsWith("- ") || l.startsWith("* ") ->
                Text("•  " + clean(l.drop(2)), style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(bottom = 4.dp))
            l.startsWith("#") ->
                Text(clean(l.trimStart('#', ' ')), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 4.dp, bottom = 4.dp))
            else -> Text(clean(l), style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(bottom = 4.dp))
        }
    }
}

/** Settings row: current version and a manual check. */
@Composable
fun UpdateCard(say: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var checking by remember { mutableStateOf(false) }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text("Zima Connect ${Updater.currentVersion(ctx)}", style = MaterialTheme.typography.titleMedium)
            Text(
                "New versions are found automatically and installed with one tap, no reinstalling.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(8.dp))
            androidx.compose.material3.OutlinedButton(
                enabled = !checking,
                onClick = {
                    scope.launch {
                        checking = true
                        try {
                            val i = Updater.check(ctx)
                            say(if (i != null) "Version ${i.latest} is available" else "You are up to date")
                        } catch (e: Throwable) {
                            say(e.message ?: "Could not check for updates")
                        } finally {
                            checking = false
                        }
                    }
                },
            ) { Text(if (checking) "Checking…" else "Check for updates") }
        }
    }
}

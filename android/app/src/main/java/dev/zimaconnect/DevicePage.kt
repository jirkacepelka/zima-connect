package dev.zimaconnect

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Call
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.ContentPaste
import androidx.compose.material.icons.outlined.InsertDriveFile
import androidx.compose.material.icons.outlined.Language
import androidx.compose.material.icons.outlined.NotificationsActive
import androidx.compose.material.icons.outlined.TextSnippet
import androidx.compose.material.icons.outlined.Upload
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private class Staged(
    val kind: String, // file | text | link
    val label: String,
    val detail: String,
    val uri: Uri? = null,
    val value: String? = null,
) {
    var state by mutableStateOf("ready") // ready | sending | done | failed
}

/** One device: what it looks like, one place to send things to it, and a few tools. */
@Composable
fun DevicePage(c: Contact, say: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val dark = isSystemInDarkTheme()
    val hue = hueOf(c)
    val staged = remember(c.id) { mutableStateListOf<Staged>() }
    var draft by remember(c.id) { mutableStateOf("") }
    var sending by remember(c.id) { mutableStateOf(false) }
    var ringing by remember(c.id) { mutableStateOf(false) }
    LaunchedEffect(ringing) {
        if (ringing) {
            delay(30_000)
            ringing = false
        }
    }

    val live = setOf("ring", "remote", "status", "calls")
    fun can(cap: String) = cap in c.caps && (c.online || (!c.direct && cap !in live))
    val canSend = can("files") || can("clipboard")

    val askMic = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        if (ok) scope.launch { try { CallEngine.start(c) } catch (e: Throwable) { say(e.message ?: "Could not call") } } else say("Allow the microphone to make calls")
    }
    val pickFiles = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        for (u in uris) staged.add(Staged("file", queryName(ctx, u) ?: "file", "ready to send", uri = u))
    }

    fun addText(raw: String) {
        val t = raw.trim()
        if (t.isEmpty()) return
        val isUrl = Regex("^https?://\\S+$", RegexOption.IGNORE_CASE).matches(t) || Regex("^[\\w-]+(\\.[\\w-]+)+(/\\S*)?$").matches(t)
        if (isUrl) {
            val url = if (t.startsWith("http", true)) t else "https://$t"
            staged.add(Staged("link", url.removePrefix("https://").removePrefix("http://"), "opens in the browser", value = url))
        } else {
            staged.add(Staged("text", if (t.length > 48) t.take(48) + "…" else t, "${t.length} characters", value = t))
        }
        draft = ""
    }

    fun sendAll() {
        if (sending) return
        scope.launch {
            sending = true
            var queued = 0
            var failed = 0
            for (s in staged.toList().filter { it.state == "ready" || it.state == "failed" }) {
                s.state = "sending"
                try {
                    when (s.kind) {
                        "file" -> {
                            val (file, name) = copyToCache(ctx, s.uri!!)
                            try {
                                if (AppModel.sendFile(c, file, name) == Delivery.Queued) queued++
                            } finally {
                                file.delete()
                            }
                        }
                        "text" -> if (AppModel.sendClipboard(c, s.value!!) == Delivery.Queued) queued++
                        else -> AppModel.sendUrl(c, s.value!!)
                    }
                    s.state = "done"
                } catch (e: Throwable) {
                    s.state = "failed"
                    failed++
                }
            }
            sending = false
            if (failed == 0) say(if (queued > 0) "Sent. $queued will arrive when ${c.name} is back" else "Sent to ${c.name}") else say("$failed could not be sent")
            delay(900)
            staged.removeAll { it.state == "done" }
        }
    }

    val ready = staged.count { it.state == "ready" || it.state == "failed" }

    LazyColumn(contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 40.dp)) {
        // ── the device
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier.size(132.dp).clip(RoundedCornerShape(20.dp)).background(if (c.online) (if (dark) Color.hsl(hue, .22f, .12f) else Color.hsl(hue, .4f, .95f)) else MaterialTheme.colorScheme.surfaceVariant),
                    contentAlignment = Alignment.Center,
                ) { DeviceArt(kindOf(c.platform, c.name), hue, c.online, c.battery, Modifier.size(112.dp)) }
                Spacer(Modifier.width(18.dp))
                Column(Modifier.weight(1f)) {
                    Text(c.name, style = MaterialTheme.typography.titleLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    Spacer(Modifier.height(4.dp))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(8.dp).clip(CircleShape).background(if (c.online) Ok else MaterialTheme.colorScheme.outlineVariant))
                        Spacer(Modifier.width(6.dp))
                        Text(
                            if (c.online) "Connected${c.rttMs?.let { ", $it ms" } ?: ""}" else "Not reachable right now",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(14.dp).clip(CircleShape).background(Color.hsl(hue, .6f, if (dark) .35f else .62f)))
                        Spacer(Modifier.width(6.dp))
                        Text(
                            "${if (c.mine) "You" else c.userName ?: "Paired directly"} · ${platformLabel(c.platform)}",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    c.battery?.let {
                        Spacer(Modifier.height(8.dp))
                        LinearProgressIndicator(progress = { it.first / 100f }, modifier = Modifier.fillMaxWidth().height(6.dp).clip(CircleShape), color = Ok, trackColor = MaterialTheme.colorScheme.surfaceVariant)
                        Text("${it.first} %${if (it.second) " · charging" else ""}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
            Spacer(Modifier.height(28.dp))
        }

        // ── send
        item {
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).border(1.5.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(16.dp)).padding(16.dp),
            ) {
                if (!canSend) {
                    Text("Sending is not available", style = MaterialTheme.typography.titleSmall)
                    Text("Not allowed for ${c.userName ?: "this device"} by your household rules, or the device is not reachable.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                } else {
                    if (staged.isEmpty()) {
                        Column(Modifier.fillMaxWidth().padding(vertical = 12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                            Icon(Icons.Outlined.Upload, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                            Spacer(Modifier.height(6.dp))
                            Text("Send to ${c.name}", style = MaterialTheme.typography.titleSmall)
                            Text(
                                "Choose files, or type text or a link. Nothing is sent until you press Send.",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    } else {
                        staged.forEach { s ->
                            Row(
                                Modifier.fillMaxWidth().padding(bottom = 8.dp).clip(RoundedCornerShape(10.dp)).background(MaterialTheme.colorScheme.surfaceVariant).padding(horizontal = 12.dp, vertical = 8.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Icon(
                                    when (s.kind) { "file" -> Icons.Outlined.InsertDriveFile; "link" -> Icons.Outlined.Language; else -> Icons.Outlined.TextSnippet },
                                    null, Modifier.size(20.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                                Spacer(Modifier.width(12.dp))
                                Column(Modifier.weight(1f)) {
                                    Text(s.label, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    Text(s.detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                                when (s.state) {
                                    "sending" -> CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                                    "done" -> Text("✓", color = Ok)
                                    else -> IconButton(onClick = { staged.remove(s) }, Modifier.size(28.dp)) { Icon(Icons.Outlined.Close, "Remove ${s.label}", Modifier.size(16.dp)) }
                                }
                            }
                        }
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        OutlinedTextField(
                            value = draft,
                            onValueChange = { draft = it },
                            placeholder = { Text("Text or a link") },
                            singleLine = true,
                            enabled = can("clipboard"),
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                            modifier = Modifier.weight(1f),
                        )
                        IconButton(onClick = {
                            val clip = ctx.getSystemService(android.content.ClipboardManager::class.java).primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(ctx)?.toString()
                            if (clip.isNullOrBlank()) say("Clipboard is empty") else addText(clip)
                        }, enabled = can("clipboard")) { Icon(Icons.Outlined.ContentPaste, "Paste from clipboard") }
                    }
                    Spacer(Modifier.height(10.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
                        OutlinedButton(onClick = { pickFiles.launch(arrayOf("*/*")) }, enabled = can("files"), modifier = Modifier.weight(1f)) { Text("Files…") }
                        Button(
                            onClick = { if (draft.isNotBlank()) addText(draft) else sendAll() },
                            enabled = !sending && (ready > 0 || draft.isNotBlank()),
                            modifier = Modifier.weight(1f),
                        ) { Text(if (draft.isNotBlank()) "Add" else if (sending) "Sending…" else if (ready > 0) "Send $ready" else "Send") }
                    }
                }
            }
            Spacer(Modifier.height(28.dp))
            Text("Tools", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
        }

        // ── tools
        item {
            Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.Call, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) {
                    Text("Voice call", style = MaterialTheme.typography.titleSmall)
                    Text(if (can("calls")) "Talk to the person at this device" else "Not allowed or not reachable", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                OutlinedButton(enabled = can("calls") && CallEngine.state == "idle", onClick = {
                    if (CallEngine.hasMic(ctx)) scope.launch { try { CallEngine.start(c) } catch (e: Throwable) { say(e.message ?: "Could not call") } } else askMic.launch(android.Manifest.permission.RECORD_AUDIO)
                }) { Text("Call") }
            }
            Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.NotificationsActive, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) {
                    Text("Find device", style = MaterialTheme.typography.titleSmall)
                    Text(
                        if (can("ring")) (if (ringing) "Ringing. Stops by itself after 30 s." else "Make it ring, even when silent") else "Not allowed or not reachable",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (ringing) {
                    Button(onClick = {
                        scope.launch {
                            ringing = false
                            runCatching { Api.call("ring_stop", Api.args("id" to c.id)) }
                                .onSuccess { say("Stopped ringing") }
                                .onFailure { say(it.message ?: "Could not reach the device") }
                        }
                    }) { Text("Stop") }
                } else {
                    OutlinedButton(enabled = can("ring"), onClick = {
                        scope.launch {
                            try {
                                Api.call("ring", Api.args("id" to c.id))
                                ringing = true
                                say("Device is ringing")
                            } catch (e: Throwable) {
                                say(e.message ?: "Could not reach the device")
                            }
                        }
                    }) { Text("Ring") }
                }
            }
            RemoteControl(c, say)
            if (c.direct) {
                TextButton(onClick = {
                    scope.launch {
                        runCatching { Api.call("unpair", Api.args("id" to c.id)) }
                        say("${c.name} removed")
                        AppModel.refresh()
                    }
                }) { Text("Remove this device", color = MaterialTheme.colorScheme.error) }
            }
        }
    }
}

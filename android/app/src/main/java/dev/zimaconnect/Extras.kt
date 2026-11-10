package dev.zimaconnect

import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.PlayArrow
import androidx.compose.material.icons.outlined.SkipNext
import androidx.compose.material.icons.outlined.SkipPrevious
import androidx.compose.material.icons.outlined.VolumeDown
import androidx.compose.material.icons.outlined.VolumeOff
import androidx.compose.material.icons.outlined.VolumeUp
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
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
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

private fun media(a: String) = JSONObject().put("k", "media").put("action", a)

/** The phone as a touchpad, keyboard and media remote for another device. */
@Composable
fun RemoteControl(c: Contact, say: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var open by remember { mutableStateOf(false) }
    var text by remember { mutableStateOf("") }
    val inflight = remember { AtomicBoolean(false) }

    fun send(input: JSONObject, quiet: Boolean = false) {
        scope.launch {
            try {
                AppModel.sendRemote(c, input)
            } catch (e: Throwable) {
                if (!quiet) say(e.message ?: "Could not reach the device")
            }
        }
    }

    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
    ) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Remote control", style = MaterialTheme.typography.titleMedium)
                    Text(
                        if ("remote" in c.caps) "Touchpad, keyboard and media keys" else "Not allowed",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                OutlinedButton(onClick = { open = !open }, enabled = "remote" in c.caps && c.online) {
                    Text(if (open) "Hide" else "Open")
                }
            }
            if (open) {
                Spacer(Modifier.height(12.dp))
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    IconButton(onClick = { send(media("prev")) }) { Icon(Icons.Outlined.SkipPrevious, "Previous track") }
                    IconButton(onClick = { send(media("play_pause")) }) { Icon(Icons.Outlined.PlayArrow, "Play or pause") }
                    IconButton(onClick = { send(media("next")) }) { Icon(Icons.Outlined.SkipNext, "Next track") }
                    IconButton(onClick = { send(media("vol_down")) }) { Icon(Icons.Outlined.VolumeDown, "Volume down") }
                    IconButton(onClick = { send(media("mute")) }) { Icon(Icons.Outlined.VolumeOff, "Mute") }
                    IconButton(onClick = { send(media("vol_up")) }) { Icon(Icons.Outlined.VolumeUp, "Volume up") }
                }
                Spacer(Modifier.height(8.dp))
                Row(Modifier.fillMaxWidth().height(220.dp)) {
                    Box(
                        Modifier
                            .weight(1f)
                            .fillMaxHeight()
                            .clip(MaterialTheme.shapes.medium)
                            .background(MaterialTheme.colorScheme.surfaceVariant)
                            .pointerInput(c.id) {
                                detectTapGestures(
                                    onTap = { send(JSONObject().put("k", "click").put("button", "left")) },
                                    onDoubleTap = { send(JSONObject().put("k", "click").put("button", "double")) },
                                    onLongPress = { send(JSONObject().put("k", "click").put("button", "right")) },
                                )
                            }
                            .pointerInput(c.id) {
                                var ax = 0f
                                var ay = 0f
                                var last = 0L
                                detectDragGestures { change, drag ->
                                    change.consume()
                                    ax += drag.x * 1.4f
                                    ay += drag.y * 1.4f
                                    val now = System.currentTimeMillis()
                                    if (now - last > 40 && !inflight.get()) {
                                        val dx = ax.toInt()
                                        val dy = ay.toInt()
                                        if (dx != 0 || dy != 0) {
                                            ax -= dx
                                            ay -= dy
                                            last = now
                                            inflight.set(true)
                                            scope.launch {
                                                try {
                                                    AppModel.sendRemote(c, JSONObject().put("k", "move").put("dx", dx).put("dy", dy))
                                                } catch (_: Throwable) {
                                                } finally {
                                                    inflight.set(false)
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            "Drag to move · tap to click\nlong press for right click",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    Spacer(Modifier.width(8.dp))
                    Box(
                        Modifier
                            .width(40.dp)
                            .fillMaxHeight()
                            .clip(MaterialTheme.shapes.medium)
                            .background(MaterialTheme.colorScheme.surfaceVariant)
                            .pointerInput(c.id) {
                                var acc = 0f
                                detectVerticalDragGestures { change, drag ->
                                    change.consume()
                                    acc += drag / 6f
                                    val n = acc.toInt()
                                    if (n != 0 && !inflight.get()) {
                                        acc -= n
                                        inflight.set(true)
                                        scope.launch {
                                            try {
                                                AppModel.sendRemote(c, JSONObject().put("k", "scroll").put("dy", n))
                                            } catch (_: Throwable) {
                                            } finally {
                                                inflight.set(false)
                                            }
                                        }
                                    }
                                }
                            },
                        contentAlignment = Alignment.Center,
                    ) { Text("↕", color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                Spacer(Modifier.height(8.dp))
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = { send(JSONObject().put("k", "key").put("key", "left")) }) { Text("←") }
                    OutlinedButton(onClick = { send(JSONObject().put("k", "key").put("key", "right")) }) { Text("→") }
                    OutlinedButton(onClick = { send(JSONObject().put("k", "key").put("key", "esc")) }) { Text("Esc") }
                    OutlinedButton(onClick = { send(JSONObject().put("k", "key").put("key", "enter")) }) { Text("Enter") }
                }
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = text,
                        onValueChange = { text = it.take(500) },
                        placeholder = { Text("Type on ${c.name}") },
                        singleLine = true,
                        modifier = Modifier.weight(1f),
                    )
                    Spacer(Modifier.width(8.dp))
                    Button(
                        enabled = text.isNotEmpty(),
                        onClick = {
                            val t = text
                            text = ""
                            send(JSONObject().put("k", "text").put("text", t))
                        },
                    ) { Text("Type") }
                }
            }
        }
    }
}

/** Send a link that opens in the browser of the other device. */
@Composable
fun LinkCard(c: Contact, say: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var url by remember { mutableStateOf("") }
    val ok = "clipboard" in c.caps && c.online
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
    ) {
        Column(Modifier.padding(16.dp)) {
            Text("Open a link there", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = url,
                    onValueChange = { url = it },
                    placeholder = { Text("example.org") },
                    singleLine = true,
                    enabled = ok,
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = KeyboardType.Uri),
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Button(
                    enabled = ok && url.isNotBlank(),
                    onClick = {
                        val u = url.trim().let { if (it.startsWith("http://") || it.startsWith("https://")) it else "https://$it" }
                        scope.launch {
                            try {
                                AppModel.sendUrl(c, u)
                                url = ""
                                say("Link opened on ${c.name}")
                            } catch (e: Throwable) {
                                say(e.message ?: "Could not send the link")
                            }
                        }
                    },
                ) { Text("Open") }
            }
        }
    }
}

/** Rename this phone: other devices and the server show the new name. */
@Composable
fun RenameCard(say: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf(AppModel.deviceName) }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text("Name of this phone", style = MaterialTheme.typography.titleMedium)
            Text(
                "Other devices and the server show this name.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(value = name, onValueChange = { name = it.take(40) }, singleLine = true, modifier = Modifier.weight(1f))
                Spacer(Modifier.width(8.dp))
                Button(
                    enabled = name.isNotBlank() && name.trim() != AppModel.deviceName,
                    onClick = {
                        scope.launch {
                            try {
                                AppModel.renameDevice(name)
                                say("Renamed")
                            } catch (e: Throwable) {
                                say(e.message ?: "Could not rename")
                            }
                        }
                    },
                ) { Text("Save") }
            }
        }
    }
}

/** Files left on the server for others (or everybody) to pick up. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShelfScreen(say: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val household = AppModel.household
    var everyone by remember { mutableStateOf(true) }
    val picked = remember { mutableStateListOf<String>() }
    var busy by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        while (true) {
            runCatching { AppModel.refreshShelf() }
            delay(8000)
        }
    }

    val others = household?.users?.filter { it.id != household.me.id } ?: emptyList()

    val pick = rememberLauncherForFile { uri ->
        scope.launch {
            busy = true
            try {
                val (file, label) = copyToCache(ctx, uri)
                try {
                    val aud = if (everyone) JSONObject().put("to", "everyone")
                    else JSONObject().put("to", "users").put("ids", JSONArray(picked.toList()))
                    AppModel.shelfPut(file, label, aud)
                    say("Put on the shelf")
                } finally {
                    file.delete()
                }
            } catch (e: Throwable) {
                say(e.message ?: "Could not use the shelf")
            } finally {
                busy = false
            }
        }
    }

    if (household == null) {
        Column(Modifier.padding(24.dp)) {
            Text("The shelf lives on your Zima Connect server. Join the server first.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        return
    }

    LazyColumn(contentPadding = PaddingValues(16.dp)) {
        item {
            Text(
                "Leave files on the server for others to pick up, even when you are not online together.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(12.dp))
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    Text("Who can take it", style = MaterialTheme.typography.titleMedium)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(selected = everyone, onClick = { everyone = true })
                        Text("Everyone")
                        Spacer(Modifier.width(16.dp))
                        RadioButton(selected = !everyone, onClick = { everyone = false })
                        Text("Only…")
                    }
                    if (!everyone) {
                        if (others.isEmpty()) {
                            Text("No other people in the household yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                        } else {
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                others.forEach { u ->
                                    FilterChip(
                                        selected = u.id in picked,
                                        onClick = { if (u.id in picked) picked.remove(u.id) else picked.add(u.id) },
                                        label = { Text(u.name) },
                                    )
                                }
                            }
                        }
                    }
                    Spacer(Modifier.height(8.dp))
                    Button(onClick = { pick() }, enabled = !busy && (everyone || picked.isNotEmpty())) {
                        Text(if (busy) "Uploading…" else "Put a file on the shelf")
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
            Text("On the shelf", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
        }
        if (AppModel.shelf.isEmpty()) {
            item { Text("Nothing on the shelf.", color = MaterialTheme.colorScheme.onSurfaceVariant) }
        } else {
            items(AppModel.shelf) { i ->
                Card(
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                    modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
                ) {
                    Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(i.name, style = MaterialTheme.typography.titleSmall)
                            Text(
                                "${if (i.mine) "You" else i.ownerName} for ${if (i.everyone) "everyone" else i.toUsers.joinToString(", ").ifEmpty { "only you" }} · ${sizeText(i.size)}",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        IconButton(onClick = {
                            scope.launch {
                                try {
                                    val path = AppModel.shelfGet(i)
                                    val shown = saveToPublicDownloads(ctx, File(path), i.name)
                                    say("Saved in $shown")
                                } catch (e: Throwable) {
                                    say(e.message ?: "Could not download")
                                }
                            }
                        }) { Icon(Icons.Outlined.Download, "Take ${i.name}") }
                        if (i.mine) {
                            TextButton(onClick = {
                                scope.launch {
                                    runCatching { AppModel.shelfDelete(i) }.onFailure { say(it.message ?: "Could not remove") }
                                }
                            }) { Text("Remove", color = MaterialTheme.colorScheme.error) }
                        }
                    }
                }
            }
        }
        item {
            Spacer(Modifier.height(12.dp))
            Text(
                "Files on the shelf are stored on your server so it can show them to the right people. Direct sends and the offline queue stay end-to-end encrypted.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

private fun sizeText(n: Long) = when {
    n > 1_000_000_000 -> "%.1f GB".format(n / 1e9)
    n > 1_000_000 -> "%.1f MB".format(n / 1e6)
    else -> "${maxOf(1, n / 1000)} kB"
}

/** Move a downloaded file into the public Downloads/Zima Connect folder. Returns where it ended up. */
fun saveToPublicDownloads(ctx: Context, file: File, name: String): String {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return "the app's folder"
    return runCatching {
        val values = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, name)
            put(MediaStore.Downloads.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/Zima Connect")
        }
        val uri: Uri = ctx.contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values) ?: error("no uri")
        ctx.contentResolver.openOutputStream(uri)!!.use { out -> file.inputStream().use { it.copyTo(out) } }
        file.delete()
        "Downloads/Zima Connect"
    }.getOrDefault("the app's folder")
}

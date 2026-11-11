package dev.zimaconnect

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.InsertDriveFile
import androidx.compose.material.icons.outlined.Smartphone
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

private fun tintBg(h: Float, dark: Boolean) = if (dark) Color.hsl(h, .22f, .12f) else Color.hsl(h, .4f, .95f)

private val IMAGE_EXT = setOf("jpg", "jpeg", "png", "gif", "webp", "bmp", "avif")

private fun ext(name: String) = name.substringAfterLast('.', "").lowercase()

private fun sizeLabel(n: Long) = when {
    n > 1_000_000_000 -> "%.1f GB".format(n / 1e9)
    n > 1_000_000 -> "%.1f MB".format(n / 1e6)
    else -> "${maxOf(1, n / 1000)} kB"
}

/** Preview of a shelf file: images and the first page of a PDF, otherwise an icon with the extension. */
@Composable
fun ShelfThumb(item: ShelfItem, modifier: Modifier = Modifier, big: Boolean = false) {
    val ctx = LocalContext.current
    var bmp by remember(item.id) { mutableStateOf<Bitmap?>(null) }
    val e = ext(item.name)
    val previewable = (e in IMAGE_EXT && item.size <= 30_000_000) || (e == "pdf" && item.size <= 60_000_000)

    LaunchedEffect(item.id) {
        if (!previewable) return@LaunchedEffect
        bmp = runCatching {
            val f = AppModel.shelfCache(ctx, item)
            withContext(Dispatchers.IO) { if (e == "pdf") renderPdf(f, if (big) 1000 else 420) else decode(f, if (big) 1400 else 420) }
        }.getOrNull()
    }

    Box(
        modifier.clip(RoundedCornerShape(10.dp)).background(MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) {
        val b = bmp
        if (b != null) {
            Image(b.asImageBitmap(), item.name, Modifier.fillMaxSize(), contentScale = if (big) ContentScale.Fit else ContentScale.Crop)
        } else {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Icon(Icons.Outlined.InsertDriveFile, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(e.take(4).uppercase().ifEmpty { "FILE" }, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

private fun decode(f: File, target: Int): Bitmap? {
    val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(f.path, o)
    var sample = 1
    while (o.outWidth / (sample * 2) >= target) sample *= 2
    return BitmapFactory.decodeFile(f.path, BitmapFactory.Options().apply { inSampleSize = sample })
}

private fun renderPdf(f: File, width: Int): Bitmap? = runCatching {
    ParcelFileDescriptor.open(f, ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
        PdfRenderer(fd).use { r ->
            r.openPage(0).use { p ->
                val h = width * p.height / p.width
                val b = Bitmap.createBitmap(width, h, Bitmap.Config.ARGB_8888)
                b.eraseColor(android.graphics.Color.WHITE)
                p.render(b, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                b
            }
        }
    }
}.getOrNull()

@Composable
private fun SectionTitle(text: String, action: (@Composable () -> Unit)? = null) {
    Row(Modifier.fillMaxWidth().padding(top = 28.dp, bottom = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(text, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
        action?.invoke()
    }
}

/** The home page: your shelf, your devices and what happened lately. */
@Composable
fun HomePage(onOpen: (String) -> Unit, onAdd: () -> Unit, onHousehold: () -> Unit, say: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val contacts = AppModel.contacts
    val household = AppModel.household
    var filter by remember { mutableStateOf("all") }
    var preview by remember { mutableStateOf<ShelfItem?>(null) }
    var uploading by remember { mutableStateOf(false) }
    val dark = isSystemInDarkTheme()

    LaunchedEffect(household != null) {
        while (household != null) {
            runCatching { AppModel.refreshShelf() }
            kotlinx.coroutines.delay(8000)
        }
    }

    if (AppModel.startError != null) {
        Column(Modifier.fillMaxSize().padding(32.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
            Text("Could not start", style = MaterialTheme.typography.titleMedium)
            Text(AppModel.startError ?: "", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        return
    }

    val pick = rememberLauncherForFile { uri ->
        scope.launch {
            uploading = true
            try {
                val (file, label) = copyToCache(ctx, uri)
                try {
                    AppModel.shelfPut(file, label)
                    say("Put on your shelf")
                } finally {
                    file.delete()
                }
            } catch (e: Throwable) {
                say(e.message ?: "Could not use the shelf")
            } finally {
                uploading = false
            }
        }
    }

    val people = contacts.filter { !it.mine && it.userName != null }.mapNotNull { it.userName }.distinct()
    val shown = contacts.filter {
        when (filter) {
            "all" -> true
            "mine" -> it.mine
            "online" -> it.online
            else -> it.userName == filter
        }
    }
    val online = contacts.count { it.online }

    LazyColumn(contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 110.dp)) {
        item {
            Text(
                if (contacts.isEmpty()) "No other devices yet" else "$online of ${contacts.size} ${if (contacts.size == 1) "device" else "devices"} online",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        // ── shelf
        item {
            SectionTitle("Your shelf") {
                TextButton(onClick = { pick() }, enabled = household != null && !uploading) {
                    Icon(Icons.Outlined.Add, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(if (uploading) "Uploading…" else "Add files")
                }
            }
            if (household == null) {
                Text("The shelf lives on your Zima Connect server. Join the server first.", color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else if (AppModel.shelf.isEmpty()) {
                Column(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).border(1.5.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(18.dp)).clickable { pick() }.padding(24.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("Keep a file here", style = MaterialTheme.typography.titleSmall)
                    Text(
                        "Put a file on the shelf on one device and pick it up on another. Only you see it.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            } else {
                LazyRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    items(AppModel.shelf, key = { it.id }) { i ->
                        Column(Modifier.width(128.dp).clickable { preview = i }) {
                            ShelfThumb(i, Modifier.fillMaxWidth().aspectRatio(4f / 3f))
                            Spacer(Modifier.height(6.dp))
                            Text(i.name, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(sizeLabel(i.size), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
        }

        // ── devices
        item {
            SectionTitle("Devices")
            if (contacts.size > 1) {
                LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(bottom = 12.dp)) {
                    val chips = buildList { add("all" to "All"); add("mine" to "Mine"); people.forEach { add(it to it) }; add("online" to "Online") }
                    items(chips) { (k, label) ->
                        FilterChip(selected = filter == k, onClick = { filter = k }, label = { Text(label) })
                    }
                }
            }
        }
        if (contacts.isEmpty()) {
            item {
                Column(Modifier.fillMaxWidth().padding(vertical = 24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Box(Modifier.size(64.dp).clip(CircleShape).background(MaterialTheme.colorScheme.surfaceVariant), contentAlignment = Alignment.Center) {
                        Icon(Icons.Outlined.Smartphone, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Spacer(Modifier.height(16.dp))
                    if (household == null) {
                        Text("Connect your devices", style = MaterialTheme.typography.titleLarge)
                        Spacer(Modifier.height(6.dp))
                        Text(
                            "Join your Zima Connect server once and your devices find each other automatically. No account or domain.",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.height(20.dp))
                        Button(onClick = onAdd) { Text("Join server") }
                    } else {
                        Text("You are in ${household.name}", style = MaterialTheme.typography.titleLarge)
                        Spacer(Modifier.height(6.dp))
                        Text("No other devices yet.", color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Spacer(Modifier.height(20.dp))
                        Button(onClick = onHousehold) { Text("Open household") }
                    }
                }
            }
        } else {
            items(shown.chunked(2), key = { row -> row.joinToString { it.id } }) { row ->
                Row(Modifier.fillMaxWidth().padding(bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    row.forEach { c -> DeviceCard(c, dark, Modifier.weight(1f)) { onOpen(c.id) } }
                    if (row.size == 1) Spacer(Modifier.weight(1f))
                }
            }
        }

    }

    preview?.let { i ->
        AlertDialog(
            onDismissRequest = { preview = null },
            title = { Text(i.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
            text = {
                Column {
                    ShelfThumb(i, Modifier.fillMaxWidth().height(300.dp), big = true)
                    Spacer(Modifier.height(8.dp))
                    Text(sizeLabel(i.size), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            },
            confirmButton = {
                Button(onClick = {
                    preview = null
                    scope.launch {
                        try {
                            val path = AppModel.shelfGet(i)
                            say("Saved in ${saveToPublicDownloads(ctx, File(path), i.name)}")
                        } catch (e: Throwable) {
                            say(e.message ?: "Could not download")
                        }
                    }
                }) {
                    Icon(Icons.Outlined.Download, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("Save")
                }
            },
            dismissButton = {
                Row {
                    TextButton(onClick = {
                        preview = null
                        scope.launch { runCatching { AppModel.shelfDelete(i) }.onFailure { say(it.message ?: "Could not remove") } }
                    }) { Text("Remove", color = MaterialTheme.colorScheme.error) }
                    TextButton(onClick = { preview = null }) { Text("Close") }
                }
            },
        )
    }
}

@Composable
private fun DeviceCard(c: Contact, dark: Boolean, modifier: Modifier, onClick: () -> Unit) {
    val hue = hueOf(c)
    Column(
        modifier.clip(RoundedCornerShape(22.dp)).clickable(onClick = onClick).padding(horizontal = 6.dp, vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.fillMaxWidth().height(128.dp), contentAlignment = Alignment.Center) {
            DeviceArt(kindOf(c.platform, c.name), hue, c.online, c.battery, Modifier.size(112.dp).then(if (c.online) Modifier else Modifier.alpha(0.5f)))
        }
        Spacer(Modifier.height(6.dp))
        Text(c.name, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(14.dp).clip(CircleShape).background(Color.hsl(hue, .6f, if (dark) .35f else .62f)))
            Spacer(Modifier.width(6.dp))
            Text(
                (if (c.mine) "You" else c.userName ?: "Paired directly"),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(7.dp).clip(CircleShape).background(if (c.online) Ok else MaterialTheme.colorScheme.outlineVariant))
            Spacer(Modifier.width(6.dp))
            Text(
                if (c.online) "Connected${c.rttMs?.let { ", $it ms" } ?: ""}" else "Offline",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.weight(1f),
                maxLines = 1,
            )
            c.battery?.let { Text("${it.first} %${if (it.second) " ⚡" else ""}", style = MaterialTheme.typography.bodySmall) }
        }
    }
}

/** Returns a function that opens the system file picker and calls [onPicked] with the result. */
@Composable
internal fun rememberLauncherForFile(onPicked: (android.net.Uri) -> Unit): () -> Unit {
    val launcher = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) onPicked(uri)
    }
    return { launcher.launch(arrayOf("*/*")) }
}

/** The Send tab: pick a file, then pick who gets it. */
@Composable
fun SendPage(say: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val contacts = AppModel.contacts
    var outgoing by remember { mutableStateOf<Pair<File, String>?>(null) }
    val pickSend = rememberLauncherForFile { uri ->
        scope.launch {
            try {
                outgoing = copyToCache(ctx, uri)
            } catch (e: Throwable) {
                say(e.message ?: "Could not read the file")
            }
        }
    }
    outgoing?.let { (file, label) ->
        AlertDialog(
            onDismissRequest = { file.delete(); outgoing = null },
            title = { Text("Send $label to…") },
            text = {
                Column {
                    contacts.forEach { c ->
                        TextButton(onClick = {
                            outgoing = null
                            scope.launch {
                                try {
                                    AppModel.sendFile(c, file, label)
                                    say("Sent to ${c.name}")
                                } catch (e: Throwable) {
                                    say(e.message ?: "Could not send")
                                } finally {
                                    file.delete()
                                }
                            }
                        }) { Text(c.name) }
                    }
                    if (AppModel.nearby.isNotEmpty()) {
                        Text("Nearby, with LocalSend", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        AppModel.nearby.forEach { p ->
                            TextButton(onClick = {
                                outgoing = null
                                scope.launch {
                                    say("Waiting for ${p.alias} to accept…")
                                    try {
                                        AppModel.sendNearby(p, file, label)
                                        say("Sent to ${p.alias}")
                                    } catch (e: Throwable) {
                                        say(e.message ?: "Could not send")
                                    } finally {
                                        file.delete()
                                    }
                                }
                            }) { Text(p.alias) }
                        }
                    }
                }
            },
            confirmButton = {},
            dismissButton = { TextButton(onClick = { file.delete(); outgoing = null }) { Text("Cancel") } },
        )
    }


    Column(Modifier.fillMaxSize().padding(horizontal = 20.dp, vertical = 8.dp)) {
        Text(
            "Goes straight to the device you pick. Nothing is kept.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(16.dp))
        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).border(1.5.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(22.dp)).clickable { pickSend() }.padding(vertical = 56.dp, horizontal = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(Icons.Outlined.Add, null, Modifier.size(28.dp))
            Spacer(Modifier.height(8.dp))
            Text("Choose a file to send", style = MaterialTheme.typography.titleMedium)
            Text(
                "Then pick which device gets it. Devices nearby with LocalSend or Quick Share are listed too.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** What was sent and received lately. */
@Composable
fun ActivityScreen() {
    LazyColumn(contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 24.dp)) {
        if (AppModel.activities.isEmpty()) {
            item { Text("Nothing yet. Things you send and receive show up here.", color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        items(AppModel.activities) { a ->
            Column(Modifier.fillMaxWidth().padding(vertical = 10.dp)) {
                Row {
                    Text(a.title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                    Text(ago(a.ts), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                if (a.detail.isNotEmpty()) Text(a.detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

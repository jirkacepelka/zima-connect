package dev.zimaconnect

import android.content.ClipboardManager
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Computer
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.ContentPaste
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Dns
import androidx.compose.material.icons.outlined.Groups
import androidx.compose.material.icons.outlined.InsertDriveFile
import androidx.compose.material.icons.outlined.Inventory2
import androidx.compose.material.icons.outlined.Laptop
import androidx.compose.material.icons.outlined.NotificationsActive
import androidx.compose.material.icons.outlined.QrCodeScanner
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Smartphone
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.LocalContentColor
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.io.File

sealed interface Screen {
    data object Home : Screen
    data class Device(val id: String) : Screen
    data object Household : Screen
    data object Shelf : Screen
    data object Send : Screen
    data object Activity : Screen
    data object Settings : Screen
}

private fun iconFor(platform: String): ImageVector = when (platform) {
    "android", "ios" -> Icons.Outlined.Smartphone
    "server" -> Icons.Outlined.Dns
    "windows", "linux", "macos" -> Icons.Outlined.Laptop
    else -> Icons.Outlined.Computer
}

internal fun platformLabel(p: String) = mapOf(
    "android" to "Android", "windows" to "Windows", "linux" to "Linux",
    "macos" to "macOS", "ios" to "iOS", "server" to "ZimaOS",
)[p] ?: p

internal fun ago(ts: Long): String {
    val s = ((System.currentTimeMillis() - ts) / 1000).coerceAtLeast(0)
    return when {
        s < 45 -> "just now"
        s < 3600 -> "${(s / 60).coerceAtLeast(1)} min ago"
        s < 86400 -> "${s / 3600} h ago"
        else -> "${s / 86400} d ago"
    }
}

fun qrBitmap(text: String, size: Int = 720): Bitmap {
    val hints = mapOf(EncodeHintType.MARGIN to 2, EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.L)
    val m = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size, hints)
    val bmp = Bitmap.createBitmap(size, size, Bitmap.Config.RGB_565)
    for (x in 0 until size) for (y in 0 until size) bmp.setPixel(x, y, if (m[x, y]) 0xFF000000.toInt() else 0xFFFFFFFF.toInt())
    return bmp
}

private fun copyText(ctx: Context, text: String) {
    ctx.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Zima Connect", text))
}

internal fun queryName(ctx: Context, uri: Uri): String? =
    ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
        if (c.moveToFirst()) c.getString(0) else null
    }

/** Copy a picked/shared content Uri into our cache so the Rust node can read it by path. */
internal fun copyToCache(ctx: Context, uri: Uri): Pair<File, String> {
    val name = (queryName(ctx, uri) ?: "file").replace(Regex("[\\\\/:*?\"<>|]"), "_")
    val dir = File(ctx.cacheDir, "outgoing").apply { mkdirs() }
    val f = File(dir, name)
    ctx.contentResolver.openInputStream(uri)!!.use { i -> f.outputStream().use { i.copyTo(it) } }
    return f to name
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun App(shared: List<Uri>, clearShared: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val snack = remember { SnackbarHostState() }
    var screen by remember { mutableStateOf<Screen>(Screen.Home) }
    var addOpen by remember { mutableStateOf(false) }
    var invite by remember { mutableStateOf<Pair<String, String>?>(null) }

    fun say(msg: String) {
        scope.launch {
            snack.currentSnackbarData?.dismiss()
            snack.showSnackbar(msg)
        }
    }

    // Keep the node reachable in the background and the UI fresh.
    LaunchedEffect(Unit) {
        if (AppModel.ready.await()) {
            runCatching {
                ContextCompat.startForegroundService(ctx, Intent(ctx, ConnectService::class.java))
            }
        }
        launch {
            delay(5000)
            while (true) {
                runCatching { Updater.check(ctx) }
                delay(6 * 3600_000L)
            }
        }
        while (true) {
            runCatching { AppModel.refresh() }
            delay(6000)
        }
    }

    BackHandler(enabled = screen !is Screen.Home) { screen = Screen.Home }

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        snackbarHost = { SnackbarHost(snack) },
        topBar = {
            TopAppBar(
                title = { Text("Zima Connect", style = MaterialTheme.typography.titleLarge) },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background),
                actions = {
                    IconButton(onClick = { screen = Screen.Activity }) {
                        Icon(Icons.Outlined.History, contentDescription = "Activity", tint = if (screen is Screen.Activity) Accent else LocalContentColor.current)
                    }
                    if (AppModel.household != null) {
                        IconButton(onClick = { screen = Screen.Household }) {
                            Icon(Icons.Outlined.Groups, contentDescription = "People", tint = if (screen is Screen.Household) Accent else LocalContentColor.current)
                        }
                    }
                    IconButton(onClick = { screen = Screen.Settings }) {
                        Icon(Icons.Outlined.Settings, contentDescription = "Settings", tint = if (screen is Screen.Settings) Accent else LocalContentColor.current)
                    }
                },
            )
        },
        bottomBar = {
            BottomSwitch(
                sendSelected = screen is Screen.Send,
                onHome = { screen = Screen.Home },
                onSend = { screen = Screen.Send },
                onAdd = { addOpen = true },
            )
        },
    ) { pad ->
        Column(Modifier.padding(pad).fillMaxSize()) {
        UpdateBanner(::say)
        AppModel.ringingFrom?.let { from ->
            Card(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer),
            ) {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.NotificationsActive, null, tint = Accent)
                    Spacer(Modifier.width(12.dp))
                    Text("$from is looking for this phone", modifier = Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                    Button(onClick = { Ringer.stop(ctx) }) { Text("Stop") }
                }
            }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when (val s = screen) {
                Screen.Home -> HomePage(
                    onOpen = { screen = Screen.Device(it) },
                    onAdd = { addOpen = true },
                    onHousehold = { screen = Screen.Household },
                    say = ::say,
                )
                is Screen.Device -> {
                    val c = AppModel.contacts.firstOrNull { it.id == s.id }
                    if (c == null) screen = Screen.Home else DevicePage(c, ::say)
                }
                Screen.Household -> HouseholdScreen(::say) { code, label -> invite = code to label }
                Screen.Shelf -> ShelfScreen(::say)
                Screen.Send -> SendPage(::say)
                Screen.Activity -> ActivityScreen()
                Screen.Settings -> SettingsScreen(::say) { screen = Screen.Home }
            }
        }
        }
    }

    if (addOpen) AddDeviceDialog(onDismiss = { addOpen = false }, say = ::say)
    invite?.let { (code, label) -> InviteDialog(code, label) { invite = null } }

    if (shared.isNotEmpty()) {
        SharePicker(
            uris = shared,
            onDone = clearShared,
            say = ::say,
        )
    }
}

/** Home | Send, and the way to add a device. Shown on every screen. */
@Composable
private fun BottomSwitch(sendSelected: Boolean, onHome: () -> Unit, onSend: () -> Unit, onAdd: () -> Unit) {
    @Composable
    fun Seg(label: String, icon: androidx.compose.ui.graphics.vector.ImageVector, on: Boolean, click: () -> Unit) {
        Row(
            Modifier.clip(CircleShape)
                .background(if (on) MaterialTheme.colorScheme.surface else Color.Transparent)
                .clickable(onClick = click)
                .padding(horizontal = 18.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(icon, null, Modifier.size(18.dp), tint = if (on) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.width(8.dp))
            Text(label, style = MaterialTheme.typography.labelLarge, color = if (on) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    Row(
        Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.background).navigationBarsPadding().padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(Modifier.clip(CircleShape).background(MaterialTheme.colorScheme.surfaceVariant).padding(4.dp)) {
            Seg("Home", Icons.Outlined.Home, !sendSelected, onHome)
            Seg("Send", Icons.AutoMirrored.Outlined.Send, sendSelected, onSend)
        }
        Spacer(Modifier.weight(1f))
        Button(onClick = onAdd) {
            Icon(Icons.Outlined.Add, null, Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
            Text("Add device")
        }
    }
}

// ───────────────────────── Household ─────────────────────────

@Composable
private fun HouseholdScreen(say: (String) -> Unit, onInvite: (String, String) -> Unit) {
    val h = AppModel.household
    val scope = rememberCoroutineScope()
    var newPerson by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }

    if (h == null) {
        Text("You are not in a household.", modifier = Modifier.padding(24.dp))
        return
    }

    fun run(block: suspend () -> Unit) {
        if (busy) return
        scope.launch {
            busy = true
            try {
                block()
                AppModel.refresh()
            } catch (e: Throwable) {
                say(e.message ?: "Failed")
            } finally {
                busy = false
            }
        }
    }

    fun admin(op: JSONObject) = run { Api.call("admin", Api.args("op" to op)) }

    fun createInvite(userId: String?, name: String?, label: String) = run {
        val op = JSONObject().put("op", "create_invite")
            .put("user_id", userId ?: JSONObject.NULL).put("new_user_name", name ?: JSONObject.NULL)
        val code = Api.call("admin", Api.args("op" to op)) as? String
        if (code != null) onInvite(code, label)
        newPerson = ""
    }

    val me = h.me
    val pairs = if (h.canAdmin) {
        h.users.flatMap { a -> h.users.filter { it.id != a.id }.map { b -> a to b } }
    } else {
        h.users.filter { it.id != me.id }.flatMap { o -> listOf(me to o, o to me) }
    }
    fun who(u: User) = if (u.id == me.id) "You" else u.name

    LazyColumn(contentPadding = PaddingValues(16.dp)) {
        item {
            Text("You are ${me.name}${if (h.canAdmin) " · Owner" else ""}", color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(16.dp))
            Text("People", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
        }
        items(h.users, key = { it.id }) { u ->
            Card(
                modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            ) {
                Column(Modifier.padding(14.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            if (u.id == me.id) "${u.name} (you)" else u.name,
                            style = MaterialTheme.typography.titleMedium,
                            modifier = Modifier.weight(1f),
                        )
                        Text(if (u.owner) "Owner" else "Member", color = Accent, style = MaterialTheme.typography.bodySmall)
                    }
                    h.devices.filter { it.userId == u.id }.forEach { d ->
                        Row(Modifier.padding(top = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(iconFor(d.platform), null, modifier = Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                            Spacer(Modifier.width(8.dp))
                            Text(
                                d.name + if (d.id == AppModel.myId) " · this phone" else "",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.weight(1f),
                            )
                            if (h.canAdmin && d.id != AppModel.myId) {
                                IconButton(onClick = {
                                    admin(JSONObject().put("op", "remove_device").put("id", d.id))
                                }, modifier = Modifier.size(28.dp)) { Icon(Icons.Outlined.Close, "Remove ${d.name}", modifier = Modifier.size(16.dp)) }
                            }
                        }
                    }
                    if (h.canAdmin) {
                        Row {
                            TextButton(onClick = { createInvite(u.id, null, "New device for ${u.name}") }) { Text("Add device") }
                            if (!u.owner) {
                                TextButton(onClick = { admin(JSONObject().put("op", "remove_user").put("id", u.id)) }) {
                                    Text("Remove person", color = MaterialTheme.colorScheme.error)
                                }
                            }
                        }
                    }
                }
            }
        }
        if (h.canAdmin) {
            item {
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(
                    value = newPerson,
                    onValueChange = { newPerson = it },
                    label = { Text("Add a person, e.g. Mom") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                Button(
                    onClick = { createInvite(null, newPerson.trim(), "Invitation for ${newPerson.trim()}") },
                    enabled = newPerson.isNotBlank() && !busy,
                ) { Text("Create invitation") }
            }
        }
        if (pairs.isNotEmpty()) {
            item {
                Spacer(Modifier.height(24.dp))
                Text("What people can do for each other", style = MaterialTheme.typography.titleMedium)
                Text(
                    if (h.canAdmin) "Rules apply between people. Their own devices always work together." else "These rules are set by the owner.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(8.dp))
            }
            items(pairs, key = { (a, b) -> a.id + b.id }) { (from, to) ->
                Card(
                    modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                ) {
                    Column(Modifier.padding(14.dp)) {
                        Text("${who(from)} → ${who(to)}", style = MaterialTheme.typography.titleMedium)
                        Text(
                            "What ${if (from.id == me.id) "you" else from.name} can do on ${if (to.id == me.id) "your" else "${to.name}’s"} devices",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        CAPS.forEach { (cap, label) ->
                            HorizontalDivider(Modifier.padding(top = 8.dp), color = MaterialTheme.colorScheme.outlineVariant)
                            Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text(label, modifier = Modifier.weight(1f))
                                Switch(
                                    checked = h.allowed(from.id, to.id, cap),
                                    enabled = h.canAdmin && !busy,
                                    onCheckedChange = { v ->
                                        val op = if (v == defaultAllowed(from.id, to.id, cap)) {
                                            JSONObject().put("op", "clear_rule")
                                                .put("from_user", from.id).put("to_user", to.id).put("cap", cap)
                                        } else {
                                            JSONObject().put("op", "set_rule")
                                                .put("from_user", from.id).put("to_user", to.id).put("cap", cap).put("allow", v)
                                        }
                                        admin(op)
                                    },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

// ───────────────────────── Settings ─────────────────────────

@Composable
private fun SettingsScreen(say: (String) -> Unit, onLeft: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val prefs = remember { ctx.getSharedPreferences("zc", Context.MODE_PRIVATE) }
    var mirror by remember { mutableStateOf(prefs.getBoolean("mirror", false)) }

    LazyColumn(contentPadding = PaddingValues(16.dp)) {
        item {
            UpdateCard(say)
            Spacer(Modifier.height(16.dp))
            SmsCard(say)
            Spacer(Modifier.height(12.dp))
            PhoneCard(say)
            Spacer(Modifier.height(12.dp))
            ScreenCard(say)
            Spacer(Modifier.height(16.dp))
            RenameCard(say)
            if (AppModel.household != null) {
                Spacer(Modifier.height(16.dp))
                ColourCard(say)
            }
            Spacer(Modifier.height(16.dp))
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text("Mirror my notifications", style = MaterialTheme.typography.titleMedium)
                            Text(
                                "Show this phone’s notifications on your other devices. Never sent to other people.",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Switch(checked = mirror, onCheckedChange = {
                            mirror = it
                            prefs.edit().putBoolean("mirror", it).apply()
                        })
                    }
                    if (mirror) {
                        Spacer(Modifier.height(8.dp))
                        OutlinedButton(onClick = {
                            ctx.startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        }) { Text("Allow notification access") }
                    }
                }
            }
            Spacer(Modifier.height(16.dp))
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    Text("This phone", style = MaterialTheme.typography.titleMedium)
                    Text(AppModel.deviceName, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Spacer(Modifier.height(8.dp))
                    Text("Device ID", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    SelectionContainer {
                        Text(AppModel.myId, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                    }
                }
            }
            if (AppModel.household != null) {
                Spacer(Modifier.height(16.dp))
                OutlinedButton(onClick = {
                    scope.launch {
                        runCatching { Api.call("leave") }
                        AppModel.refresh()
                        say("Left the household")
                        onLeft()
                    }
                }) { Text("Leave household", color = MaterialTheme.colorScheme.error) }
            }
        }
    }
}

// ───────────────────────── Dialogs ─────────────────────────

@Composable
private fun scanner(onResult: (String) -> Unit): () -> Unit {
    val launcher = androidx.activity.compose.rememberLauncherForActivityResult(ScanContract()) { r ->
        r.contents?.let(onResult)
    }
    return {
        launcher.launch(
            ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("Point the camera at the QR code")
                .setBeepEnabled(false)
                .setOrientationLocked(false),
        )
    }
}

@Composable
private fun AddDeviceDialog(onDismiss: () -> Unit, say: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var tab by remember { mutableStateOf(if (AppModel.household != null) 1 else 0) }
    var serverCode by remember { mutableStateOf("") }
    var yourName by remember { mutableStateOf("") }
    var enterCode by remember { mutableStateOf("") }
    var myCode by remember { mutableStateOf("") }
    var working by remember { mutableStateOf(false) }

    val scanServer = scanner { serverCode = it }
    val scanPair = scanner { enterCode = it }

    LaunchedEffect(tab) {
        if (tab == 1 && myCode.isEmpty()) {
            myCode = runCatching { Api.call("pairing_code") as String }.getOrDefault("")
        }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            when (tab) {
                0 -> Button(enabled = serverCode.isNotBlank() && !working, onClick = {
                    scope.launch {
                        working = true
                        try {
                            val name = Api.call("join", Api.args("code" to serverCode.trim(), "user_name" to yourName.trim().ifEmpty { null })) as String
                            say("Joined $name")
                            AppModel.refresh()
                            onDismiss()
                        } catch (e: Throwable) {
                            say(e.message ?: "Could not join")
                        } finally {
                            working = false
                        }
                    }
                }) { Text(if (working) "Connecting…" else "Join") }
                2 -> Button(enabled = enterCode.isNotBlank() && !working, onClick = {
                    scope.launch {
                        working = true
                        try {
                            val name = Api.call("pair", Api.args("code" to enterCode.trim())) as String
                            say("Paired with $name")
                            AppModel.refresh()
                            onDismiss()
                        } catch (e: Throwable) {
                            say(e.message ?: "Could not pair")
                        } finally {
                            working = false
                        }
                    }
                }) { Text(if (working) "Connecting…" else "Pair") }
                else -> Button(onClick = {
                    copyText(ctx, myCode)
                    say("Code copied")
                }, enabled = myCode.isNotEmpty()) { Text("Copy code") }
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Close") } },
        title = { Text("Add device") },
        text = {
            Column {
                TabRow(selectedTabIndex = tab, containerColor = Color.Transparent) {
                    listOf("Join server", "Show code", "Enter code").forEachIndexed { i, t ->
                        Tab(selected = tab == i, onClick = { tab = i }, text = { Text(t, fontSize = 13.sp) })
                    }
                }
                Spacer(Modifier.height(12.dp))
                when (tab) {
                    0 -> Column {
                        Text(
                            "Open your Zima Connect server’s page (or ask the household owner for an invitation) and scan or paste the code.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.height(8.dp))
                        OutlinedTextField(
                            value = serverCode, onValueChange = { serverCode = it },
                            label = { Text("Server code") }, minLines = 2, maxLines = 4,
                            modifier = Modifier.fillMaxWidth(),
                            keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = KeyboardType.Ascii),
                        )
                        TextButton(onClick = scanServer) {
                            Icon(Icons.Outlined.QrCodeScanner, null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(6.dp)); Text("Scan QR code")
                        }
                        OutlinedTextField(
                            value = yourName, onValueChange = { yourName = it },
                            label = { Text("Your name (if you are the first or a new person)") },
                            singleLine = true, modifier = Modifier.fillMaxWidth(),
                        )
                    }
                    1 -> Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                        Text(
                            "Direct pairing without a server: on the other device choose Enter code and scan or paste this. Works once, 15 minutes.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.height(10.dp))
                        if (myCode.isNotEmpty()) {
                            val bmp = remember(myCode) { qrBitmap(myCode) }
                            Image(bmp.asImageBitmap(), "QR code", Modifier.size(220.dp).clip(MaterialTheme.shapes.small))
                        } else {
                            Text("Preparing…")
                        }
                    }
                    else -> Column {
                        Text(
                            "Direct pairing without a server: scan or paste the code shown on the other device.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.height(8.dp))
                        OutlinedTextField(
                            value = enterCode, onValueChange = { enterCode = it },
                            label = { Text("Pairing code") }, minLines = 2, maxLines = 4,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        TextButton(onClick = scanPair) {
                            Icon(Icons.Outlined.QrCodeScanner, null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(6.dp)); Text("Scan QR code")
                        }
                    }
                }
            }
        },
    )
}

@Composable
private fun InviteDialog(code: String, label: String, onDismiss: () -> Unit) {
    val ctx = LocalContext.current
    var copied by remember { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            Button(onClick = { copyText(ctx, code); copied = true }) {
                Icon(if (copied) Icons.Outlined.Check else Icons.Outlined.ContentCopy, null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(6.dp)); Text(if (copied) "Copied" else "Copy code")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Close") } },
        title = { Text(label) },
        text = {
            Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                Text(
                    "On the new device install Zima Connect, choose Add device → Join server and scan or paste this code. Works once, 24 hours.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(10.dp))
                val bmp = remember(code) { qrBitmap(code) }
                Image(bmp.asImageBitmap(), "QR code", Modifier.size(220.dp).clip(MaterialTheme.shapes.small))
            }
        },
    )
}

/** Files shared into the app from the gallery or file manager: pick who gets them. */
@Composable
private fun SharePicker(uris: List<Uri>, onDone: () -> Unit, say: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val targets = AppModel.contacts.filter { "files" in it.caps }

    AlertDialog(
        onDismissRequest = onDone,
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDone) { Text("Cancel") } },
        title = { Text("Send ${if (uris.size == 1) "file" else "${uris.size} files"} to…") },
        text = {
            if (targets.isEmpty()) {
                Text("No device can receive files yet. Join a household first.")
            } else {
                Column {
                    targets.forEach { c ->
                        Row(
                            Modifier.fillMaxWidth().clip(MaterialTheme.shapes.small).clickable {
                                onDone()
                                scope.launch {
                                    var queued = false
                                    try {
                                        for (u in uris) {
                                            val (file, name) = copyToCache(ctx, u)
                                            try {
                                                if (AppModel.sendFile(c, file, name) == Delivery.Queued) queued = true
                                            } finally {
                                                file.delete()
                                            }
                                        }
                                        say(if (queued) "Queued: ${c.name} will get it when it is back" else "Sent to ${c.name}")
                                    } catch (e: Throwable) {
                                        say(e.message ?: "Could not send")
                                    }
                                }
                            }.padding(vertical = 12.dp, horizontal = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(iconFor(c.platform), null, tint = Accent)
                            Spacer(Modifier.width(12.dp))
                            Column(Modifier.weight(1f)) {
                                Text(c.name)
                                Text(
                                    if (c.mine) "Your device" else (c.userName ?: "Paired directly"),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            Box(Modifier.size(9.dp).clip(CircleShape).background(if (c.online) Ok else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f)))
                        }
                    }
                }
            }
        },
    )
}

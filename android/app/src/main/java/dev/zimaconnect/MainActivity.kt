package dev.zimaconnect

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// Zima Brand Colors
val ZimaDarkest = Color(0xFF09090B)
val ZimaDarker = Color(0xFF121215)
val ZimaCard = Color(0xFF18181B)
val ZimaBorder = Color(0xFF27272A)
val ZimaAccent = Color(0xFFE94560)
val ZimaEmerald = Color(0xFF10B981)
val ZimaAmber = Color(0xFFF59E0B)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            ZimaConnectTheme {
                HomeScreen()
            }
        }
    }
}

@Composable
fun ZimaConnectTheme(content: @Composable () -> Unit) {
    val darkColorScheme = darkColorScheme(
        primary = ZimaAccent,
        background = ZimaDarkest,
        surface = ZimaCard,
        onPrimary = Color.White,
        onBackground = Color(0xFFF4F4F5),
        onSurface = Color(0xFFF4F4F5)
    )

    MaterialTheme(
        colorScheme = darkColorScheme,
        content = content
    )
}

data class DeviceItem(
    val id: String,
    val name: String,
    val os: String,
    val isOnline: Boolean,
    val pendingQueueCount: Int
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen() {
    val devices = remember {
        mutableStateListOf(
            DeviceItem("1", "Workstation PC", "Windows 11", true, 0),
            DeviceItem("2", "ThinkPad Laptop", "Linux (Fedora)", false, 1)
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(
                            modifier = Modifier
                                .size(28.dp)
                                .clip(RoundedCornerShape(6.dp))
                                .background(ZimaAccent),
                            contentAlignment = Alignment.Center
                        ) {
                            Text(
                                text = "Z",
                                color = Color.White,
                                fontWeight = FontWeight.Bold,
                                fontSize = 16.sp
                            )
                        }
                        Spacer(modifier = Modifier.width(10.dp))
                        Text(
                            text = "Zima Connect",
                            fontWeight = FontWeight.SemiBold,
                            fontSize = 18.sp
                        )
                    }
                },
                actions = {
                    IconButton(onClick = {}) {
                        Icon(Icons.Default.Settings, contentDescription = "Settings", tint = Color.Gray)
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = ZimaDarker
                )
            )
        },
        containerColor = ZimaDarkest
    ) { innerPadding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // Server Status Header Card
            item {
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp),
                    colors = CardDefaults.cardColors(containerColor = ZimaCard)
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Column {
                            Text("ZimaOS Hub", fontWeight = FontWeight.Bold, color = Color.White, fontSize = 15.sp)
                            Text("192.168.1.100:9120", fontFamily = FontFamily.Monospace, color = Color.Gray, fontSize = 12.sp)
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Box(
                                modifier = Modifier
                                    .size(8.dp)
                                    .clip(CircleShape)
                                    .background(ZimaEmerald)
                            )
                            Spacer(modifier = Modifier.width(6.dp))
                            Text("Connected", color = ZimaEmerald, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                        }
                    }
                }
            }

            // Quick Actions Section
            item {
                Text(
                    text = "QUICK ACTIONS",
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Bold,
                    color = Color.Gray,
                    letterSpacing = 1.sp
                )
                Spacer(modifier = Modifier.height(8.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    QuickActionButton(
                        modifier = Modifier.weight(1f),
                        icon = Icons.Default.ContentPaste,
                        label = "Clipboard",
                        color = Color(0xFF3B82F6)
                    )
                    QuickActionButton(
                        modifier = Modifier.weight(1f),
                        icon = Icons.Default.UploadFile,
                        label = "Send File",
                        color = ZimaEmerald
                    )
                    QuickActionButton(
                        modifier = Modifier.weight(1f),
                        icon = Icons.Default.QrCodeScanner,
                        label = "Pair PIN",
                        color = ZimaAccent
                    )
                }
            }

            // Paired Devices Section
            item {
                Text(
                    text = "PAIRED DEVICES",
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Bold,
                    color = Color.Gray,
                    letterSpacing = 1.sp
                )
            }

            items(devices.size) { index ->
                val device = devices[index]
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp),
                    colors = CardDefaults.cardColors(containerColor = ZimaCard)
                ) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Icon(
                                    imageVector = Icons.Default.Laptop,
                                    contentDescription = null,
                                    tint = Color.White,
                                    modifier = Modifier.size(24.dp)
                                )
                                Spacer(modifier = Modifier.width(12.dp))
                                Column {
                                    Text(device.name, fontWeight = FontWeight.SemiBold, color = Color.White, fontSize = 15.sp)
                                    Text(device.os, color = Color.Gray, fontSize = 12.sp)
                                }
                            }

                            Box(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(4.dp))
                                    .background(if (device.isOnline) ZimaEmerald.copy(alpha = 0.15f) else ZimaDarker)
                                    .padding(horizontal = 8.dp, vertical = 4.dp)
                            ) {
                                Text(
                                    text = if (device.isOnline) "ONLINE" else "OFFLINE",
                                    color = if (device.isOnline) ZimaEmerald else Color.Gray,
                                    fontSize = 10.sp,
                                    fontWeight = FontWeight.Bold,
                                    fontFamily = FontFamily.Monospace
                                )
                            }
                        }

                        if (!device.isOnline && device.pendingQueueCount > 0) {
                            Spacer(modifier = Modifier.height(12.dp))
                            Surface(
                                shape = RoundedCornerShape(8.dp),
                                color = ZimaAmber.copy(alpha = 0.12f),
                                modifier = Modifier.fillMaxWidth()
                            ) {
                                Row(
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    Icon(Icons.Default.HourglassTop, contentDescription = null, tint = ZimaAmber, modifier = Modifier.size(14.dp))
                                    Spacer(modifier = Modifier.width(6.dp))
                                    Text(
                                        text = "${device.pendingQueueCount} file staged on server (auto-delivers when online)",
                                        color = ZimaAmber,
                                        fontSize = 11.sp
                                    )
                                }
                            }
                        }

                        Spacer(modifier = Modifier.height(14.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            OutlinedButton(
                                onClick = {},
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(8.dp),
                                colors = ButtonDefaults.outlinedButtonColors(contentColor = Color.White)
                            ) {
                                Text("Send File", fontSize = 12.sp)
                            }
                            Button(
                                onClick = {},
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(8.dp),
                                colors = ButtonDefaults.buttonColors(containerColor = ZimaAccent)
                            ) {
                                Text("Push Text", fontSize = 12.sp)
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun QuickActionButton(
    modifier: Modifier = Modifier,
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    label: String,
    color: Color
) {
    Card(
        modifier = modifier.height(84.dp),
        shape = RoundedCornerShape(12.dp),
        colors = CardDefaults.cardColors(containerColor = ZimaCard)
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(12.dp),
            verticalArrangement = Arrangement.SpaceBetween
        ) {
            Icon(icon, contentDescription = label, tint = color, modifier = Modifier.size(20.dp))
            Text(label, fontSize = 12.sp, fontWeight = FontWeight.Medium, color = Color.White)
        }
    }
}

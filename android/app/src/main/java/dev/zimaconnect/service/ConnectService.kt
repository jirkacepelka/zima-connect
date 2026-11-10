package dev.zimaconnect.service

import android.app.Notification
import android.app.Service
import android.content.Intent
import android.os.IBinder
import androidx.core.app.NotificationCompat
import dev.zimaconnect.R
import dev.zimaconnect.ZimaConnectApp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.TimeUnit

class ConnectService : Service() {

    private val job = SupervisorJob()
    private val scope = CoroutineScope(Dispatchers.IO + job)

    private var client: OkHttpClient? = null
    private var webSocket: WebSocket? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        startForeground(NOTIFICATION_ID, buildForegroundNotification("Connecting to ZimaOS..."))
        initWebSocketClient()
    }

    private fun initWebSocketClient() {
        client = OkHttpClient.Builder()
            .readTimeout(0, TimeUnit.MILLISECONDS)
            .pingInterval(30, TimeUnit.SECONDS)
            .build()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val serverUrl = intent?.getStringExtra(EXTRA_SERVER_URL) ?: "ws://192.168.1.100:9120/api/v1/ws"
        val token = intent?.getStringExtra(EXTRA_TOKEN) ?: ""

        connectWebSocket(serverUrl, token)
        return START_STICKY
    }

    private fun connectWebSocket(serverUrl: String, token: String) {
        scope.launch {
            val urlWithToken = if (serverUrl.contains("?")) "$serverUrl&token=$token" else "$serverUrl?token=$token"
            val request = Request.Builder().url(urlWithToken).build()

            webSocket = client?.newWebSocket(request, object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    updateNotification("Connected to ZimaOS Server")
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    // Handle incoming message envelope from desktop or other peers
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    updateNotification("Disconnected. Reconnecting...")
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    updateNotification("Connection failed: ${t.localizedMessage}")
                }
            })
        }
    }

    private fun updateNotification(status: String) {
        val manager = getSystemService(android.app.NotificationManager::class.java)
        manager.notify(NOTIFICATION_ID, buildForegroundNotification(status))
    }

    private fun buildForegroundNotification(statusText: String): Notification {
        return NotificationCompat.Builder(this, ZimaConnectApp.CHANNEL_SERVICE)
            .setContentTitle("Zima Connect")
            .setContentText(statusText)
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()
    }

    override fun onDestroy() {
        super.onDestroy()
        webSocket?.close(1000, "Service destroyed")
        client?.dispatcher?.executorService?.shutdown()
        scope.cancel()
    }

    companion object {
        const val NOTIFICATION_ID = 9120
        const val EXTRA_SERVER_URL = "server_url"
        const val EXTRA_TOKEN = "token"
    }
}

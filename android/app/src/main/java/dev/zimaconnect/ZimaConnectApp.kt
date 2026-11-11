package dev.zimaconnect

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

class ZimaConnectApp : Application() {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onCreate() {
        super.onCreate()
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CH_SERVICE, "Connection", NotificationManager.IMPORTANCE_MIN).apply {
                description = "Keeps Zima Connect reachable in the background"
                setShowBadge(false)
            },
        )
        nm.createNotificationChannel(
            NotificationChannel(CH_EVENTS, "Received items", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "Files, clipboard and notifications from your other devices"
            },
        )
        nm.createNotificationChannel(
            NotificationChannel(CH_RING, "Find my phone", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Shown while this phone is ringing because you asked to find it"
            },
        )
        // Start the Rust node as early as possible; the UI and service wait for it.
        scope.launch { AppModel.start(applicationContext) }
    }

    companion object {
        const val CH_SERVICE = "service"
        const val CH_EVENTS = "events"
        const val CH_RING = "ring"
    }
}

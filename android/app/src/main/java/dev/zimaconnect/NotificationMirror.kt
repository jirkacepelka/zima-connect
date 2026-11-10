package dev.zimaconnect

import android.app.Notification
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/**
 * Mirrors this phone's notifications to the user's OTHER devices (never to other people),
 * and only where the household policy allows it. Needs "Notification access" in system settings.
 */
class NotificationMirror : NotificationListenerService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        val prefs = getSharedPreferences("zc", MODE_PRIVATE)
        if (!prefs.getBoolean("mirror", false)) return
        if (sbn.packageName == packageName) return
        val n = sbn.notification
        if ((n.flags and Notification.FLAG_ONGOING_EVENT) != 0) return
        if ((n.flags and Notification.FLAG_GROUP_SUMMARY) != 0) return
        val extras = n.extras
        val title = extras.getCharSequence(Notification.EXTRA_TITLE)?.toString().orEmpty()
        val text = extras.getCharSequence(Notification.EXTRA_TEXT)?.toString().orEmpty()
        if (title.isEmpty() && text.isEmpty()) return
        val app = runCatching {
            packageManager.getApplicationLabel(packageManager.getApplicationInfo(sbn.packageName, 0)).toString()
        }.getOrDefault(sbn.packageName)

        scope.launch {
            if (!AppModel.ready.await()) return@launch
            AppModel.contacts
                .filter { it.mine && "notifications" in it.caps }
                .forEach { c ->
                    runCatching {
                        Api.call(
                            "send_notification",
                            Api.args("id" to c.id, "app" to app, "title" to title, "body" to text),
                        )
                    }
                }
        }
    }
}

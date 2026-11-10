package dev.zimaconnect

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.os.Build

class ZimaConnectApp : Application() {

    override fun onCreate() {
        super.onCreate()
        createNotificationChannels()
    }

    private fun createNotificationChannels() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val serviceChannel = NotificationChannel(
                CHANNEL_SERVICE,
                "Zima Connect Service",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Keeps persistent connection with ZimaOS server"
            }

            val transferChannel = NotificationChannel(
                CHANNEL_TRANSFERS,
                "File Transfers",
                NotificationManager.IMPORTANCE_DEFAULT
            ).apply {
                description = "Alerts for incoming and offline queued files"
            }

            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(serviceChannel)
            manager.createNotificationChannel(transferChannel)
        }
    }

    companion object {
        const val CHANNEL_SERVICE = "zima_connect_service"
        const val CHANNEL_TRANSFERS = "zima_connect_transfers"
    }
}

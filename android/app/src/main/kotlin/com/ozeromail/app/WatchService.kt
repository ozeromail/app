package com.ozeromail.app

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.os.IBinder
import android.os.PowerManager
import android.os.SystemClock
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.NewMail

/**
 * Keeps a connection open to each account set to Instantly while the app is away, so its mail,
 * and the notification for it, come the moment the server has it. With the app on screen the
 * inbox watches instead. Android shows the service as a quiet line in the notifications.
 */
class WatchService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var watching: Job? = null
    private var network: ConnectivityManager.NetworkCallback? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        running = true
        val manager = NotificationManagerCompat.from(this)
        // The lowest importance: no sound, no icon in the status bar, folded at the bottom.
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.notify_connected_channel), NotificationManager.IMPORTANCE_MIN)
                .apply { setShowBadge(false) },
        )
        val notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(getString(R.string.notify_connected))
            .setContentIntent(openInbox(this))
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setOngoing(true)
            .setSilent(true)
            .build()
        startForeground(ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        // A new network leaves the old connections dead without a word: they open again on it.
        val callback = object : ConnectivityManager.NetworkCallback() {
            private var last: Network? = null
            override fun onAvailable(network: Network) {
                if (last != null && last != network) renew()
                last = network
            }
        }
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(callback)
        network = callback
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        renew()
        return START_STICKY
    }

    override fun onDestroy() {
        running = false
        network?.let { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) }
        getSystemService(AlarmManager::class.java).cancel(alarm(this))
        scope.cancel()
        super.onDestroy()
    }

    /**
     * Opens every connection again: on a start, on a new network and at each alarm, as a quiet
     * connection may have been dropped by the network without the phone hearing of it.
     */
    private fun renew() {
        // Long enough to open the connections and sync what came meanwhile, with the screen off.
        awake(RENEW_AWAKE_MS)
        getSystemService(AlarmManager::class.java).setAndAllowWhileIdle(
            AlarmManager.ELAPSED_REALTIME_WAKEUP,
            SystemClock.elapsedRealtime() + RENEW_MS,
            alarm(this),
        )
        watching?.cancel()
        watching = scope.launch {
            val app = application as OzeroApp
            val accounts = try {
                app.mail.accounts().filter { it.newMail == NewMail.Push && it.canPush && !it.needsSignIn }
            } catch (e: CoreException) {
                Log.w("Ozero", "accounts not read", e)
                return@launch
            }
            if (accounts.isEmpty()) {
                stopSelf()
                return@launch
            }
            // The inbox on screen watches by itself; two connections would only cost more.
            app.visible.collectLatest { visible ->
                if (visible) return@collectLatest
                app.mail.watch(accounts) { account ->
                    awake(SYNC_AWAKE_MS)
                    checkMail(app) { it.id == account.id }
                }
            }
        }
    }

    /** Keeps the CPU on for up to [ms] even with the screen off, so a sync started gets done. */
    private fun awake(ms: Long) {
        getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Ozero:watch")
            .acquire(ms)
    }

    companion object {
        private const val CHANNEL = "connected"
        /** Apart from the messages' ids, which are positive, and sign-in ones, which are negative. */
        private const val ID = Int.MAX_VALUE
        /** How often the connections open again; Android lets an alarm wake an idle phone about this rarely. */
        private const val RENEW_MS = 10 * 60_000L
        private const val RENEW_AWAKE_MS = 30_000L
        private const val SYNC_AWAKE_MS = 60_000L

        /** Whether the service runs, so the periodic check leaves its accounts to it. */
        @Volatile
        var running = false
            private set

        /** Starts the service if one of [accounts] is set to Instantly, else stops it. */
        fun update(context: Context, accounts: List<AccountItem>) {
            val intent = Intent(context, WatchService::class.java)
            if (accounts.any { it.newMail == NewMail.Push && it.canPush && !it.needsSignIn }) {
                try {
                    context.startForegroundService(intent)
                } catch (e: IllegalStateException) {
                    // Not allowed from the background; it starts when the app is opened next.
                    Log.w("Ozero", "watch not started", e)
                }
            } else {
                context.stopService(intent)
            }
        }

        private fun alarm(context: Context): PendingIntent = PendingIntent.getService(
            context,
            0,
            Intent(context, WatchService::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }

    /** Starts the service again once the phone has started, or the app was updated. */
    class Restart : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action != Intent.ACTION_BOOT_COMPLETED && intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
            val app = context.applicationContext as OzeroApp
            val pending = goAsync()
            CoroutineScope(Dispatchers.Default).launch {
                try {
                    update(context, app.mail.accounts())
                } catch (e: CoreException) {
                    Log.w("Ozero", "accounts not read", e)
                } finally {
                    pending.finish()
                }
            }
        }
    }
}

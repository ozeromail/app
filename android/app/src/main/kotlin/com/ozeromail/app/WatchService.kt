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
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
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
    /** The accounts watched, by id: a start that changes nothing keeps their connections. */
    private var watched: Set<Long>? = null
    private val renewing = Mutex()
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
                if (last != null && last != network) renew(force = true)
                last = network
            }
        }
        getSystemService(ConnectivityManager::class.java).registerDefaultNetworkCallback(callback)
        network = callback
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        renew(force = intent?.action == RENEW)
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
     * Watches the accounts set to Instantly, opening every connection again when [force]d: on a
     * new network and at each alarm, as the network may drop a quiet connection without the
     * phone hearing of it.
     */
    private fun renew(force: Boolean) {
        // Long enough to open the connections and sync what came meanwhile, with the screen off.
        if (force) getSystemService(PowerManager::class.java).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, TAG).acquire(RENEW_AWAKE_MS)
        scope.launch { renewing.withLock { renewNow(force) } }
    }

    private suspend fun renewNow(force: Boolean) {
        val app = application as OzeroApp
        val accounts = try {
            app.mail.accounts().filter { it.watched }
        } catch (e: CoreException) {
            Log.w("Ozero", "accounts not read", e)
            return
        }
        if (accounts.isEmpty()) {
            stopSelf()
            return
        }
        val ids = accounts.map { it.id }.toSet()
        if (!force && ids == watched && watching?.isActive == true) return
        watched = ids
        getSystemService(AlarmManager::class.java).setAndAllowWhileIdle(
            AlarmManager.ELAPSED_REALTIME_WAKEUP,
            SystemClock.elapsedRealtime() + RENEW_MS,
            alarm(this),
        )
        watching?.cancel()
        watching = scope.launch {
            // The inbox on screen watches by itself; two connections would only cost more.
            app.visible.collectLatest { visible ->
                if (visible) return@collectLatest
                // Not for a moment away, as when the app hands over to the file picker.
                delay(AWAY_MS)
                app.mail.watch(accounts) { account ->
                    // The CPU stays on for the sync even with the screen off.
                    awake { checkMail(app) { it.id == account.id } }
                }
            }
        }
    }

    private suspend fun <T> awake(work: suspend () -> T): T {
        val lock = getSystemService(PowerManager::class.java).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, TAG)
        lock.acquire(AWAKE_MAX_MS)
        try {
            return work()
        } finally {
            if (lock.isHeld) lock.release()
        }
    }

    companion object {
        private const val CHANNEL = "connected"
        /** Apart from the messages' ids, which are positive, and sign-in ones, which are negative. */
        private const val ID = Int.MAX_VALUE
        private const val RENEW = "renew"
        /** How often the connections open again; Android lets an alarm wake an idle phone about this rarely. */
        private const val RENEW_MS = 10 * 60_000L
        private const val TAG = "Ozero:watch"
        private const val RENEW_AWAKE_MS = 30_000L
        private const val AWAKE_MAX_MS = 60_000L
        private const val AWAY_MS = 3_000L

        /** Whether the account's connection stays open with the app closed. */
        private val AccountItem.watched get() = newMail == NewMail.Push && !needsSignIn

        /** Whether the service runs, so the periodic check leaves its accounts to it. */
        @Volatile
        var running = false
            private set

        /** Starts the service if one of [accounts] is set to Instantly, else stops it. */
        fun update(context: Context, accounts: List<AccountItem>) {
            val intent = Intent(context, WatchService::class.java)
            if (accounts.any { it.watched }) {
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
            Intent(context, WatchService::class.java).setAction(RENEW),
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

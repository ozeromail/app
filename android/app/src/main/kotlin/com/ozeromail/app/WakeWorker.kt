package com.ozeromail.app

import android.content.Context
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit
import uniffi.ozero_api.CoreException

/** Brings snoozed mail back to the inbox on time, the app open or not, and tells about it. */
class WakeWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as OzeroApp
        try {
            val woken = app.mail.wake()
            // With the app open, the inbox shows it on top.
            if (!app.inForeground) notifyMail(applicationContext, woken)
            // Last: it replaces this run.
            app.mail.scheduleWake()
        } catch (e: CoreException) {
            // The mail check wakes it later.
            Log.w("Ozero", "snoozed mail not woken", e)
        }
        return Result.success()
    }

    companion object {
        private const val NAME = "wake-snoozed"

        /**
         * Runs at [at], in epoch seconds, in place of the run set before; none with null. Offline
         * too: the mail comes back on the phone, and the server hears once there is a network.
         */
        fun schedule(context: Context, at: Long?) {
            val manager = WorkManager.getInstance(context)
            if (at == null) {
                manager.cancelUniqueWork(NAME)
                return
            }
            val request = OneTimeWorkRequestBuilder<WakeWorker>()
                .setInitialDelay((at * 1000 - System.currentTimeMillis()).coerceAtLeast(0), TimeUnit.MILLISECONDS)
                .build()
            manager.enqueueUniqueWork(NAME, ExistingWorkPolicy.REPLACE, request)
        }
    }
}

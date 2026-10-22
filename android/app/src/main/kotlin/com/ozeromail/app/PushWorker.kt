package com.ozeromail.app

import android.content.Context
import android.util.Log
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf

/** Sends what the person did on the phone to an account's server once there is a network, the app open or not. */
class PushWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val account = inputData.getLong(ACCOUNT, 0)
        return try {
            (applicationContext as OzeroApp).mail.pushNow(account)
            Result.success()
        } catch (e: Exception) {
            // Still queued: the next sync sends it, so a server that keeps failing is not asked forever.
            Log.w("Ozero", "push failed", e)
            if (runAttemptCount < MAX_ATTEMPTS) Result.retry() else Result.success()
        }
    }

    companion object {
        private const val ACCOUNT = "account"
        private const val MAX_ATTEMPTS = 5

        /**
         * One account's runs follow each other: a run that waits out a held action sends what
         * comes due meanwhile, and the next one finds what came after it.
         */
        fun enqueue(context: Context, account: Long) {
            val request = OneTimeWorkRequestBuilder<PushWorker>()
                .setInputData(workDataOf(ACCOUNT to account))
                .setConstraints(Constraints(requiredNetworkType = NetworkType.CONNECTED))
                .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                .build()
            WorkManager.getInstance(context).enqueueUniqueWork("push-$account", ExistingWorkPolicy.APPEND_OR_REPLACE, request)
        }
    }
}

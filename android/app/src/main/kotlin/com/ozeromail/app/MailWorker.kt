package com.ozeromail.app

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem

/** Checks every account for new mail while the app is closed and tells about mail from people. */
class MailWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val mail = (applicationContext as OzeroApp).mail
        try {
            mail.refresh()
        } catch (e: CoreException) {
            // Other accounts may still have synced; the next run tries again.
            Log.w("Ozero", "background sync failed", e)
        }
        try {
            notify(applicationContext, mail.newMail())
        } catch (e: CoreException) {
            Log.w("Ozero", "new mail not read", e)
        }
        return Result.success()
    }

    companion object {
        private const val NAME = "check-mail"

        /** Every 15 minutes, the shortest Android allows, whenever there is a network. */
        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<MailWorker>(15, TimeUnit.MINUTES)
                .setConstraints(Constraints(requiredNetworkType = NetworkType.CONNECTED))
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }
    }
}

private const val CHANNEL = "new_mail"
private const val GROUP = "new_mail"
private const val SUMMARY_ID = 0
/** More at once is noise; the summary still counts them all. */
private const val MAX_SHOWN = 5

/** One notification per message, newest first, grouped under a summary; a tap opens the inbox. */
private fun notify(context: Context, messages: List<MessageItem>) {
    if (messages.isEmpty()) return
    if (context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
    val manager = NotificationManagerCompat.from(context)
    manager.createNotificationChannel(
        NotificationChannel(CHANNEL, context.getString(R.string.notify_channel), NotificationManager.IMPORTANCE_DEFAULT),
    )
    val open = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
        PendingIntent.FLAG_IMMUTABLE,
    )
    fun builder() = NotificationCompat.Builder(context, CHANNEL)
        .setSmallIcon(R.drawable.ic_notification)
        .setContentIntent(open)
        .setAutoCancel(true)
        .setGroup(GROUP)
    try {
        messages.take(MAX_SHOWN).forEach { message ->
            val text = message.snippet.ifBlank { message.subject }
            manager.notify(
                message.id.toInt(),
                builder()
                    .setContentTitle(message.sender)
                    .setContentText(message.subject)
                    .setStyle(NotificationCompat.BigTextStyle().bigText("${message.subject}\n$text"))
                    .setWhen(message.date * 1000)
                    .setShowWhen(true)
                    .build(),
            )
        }
        manager.notify(
            SUMMARY_ID,
            builder()
                .setGroupSummary(true)
                .setNumber(messages.size)
                .setStyle(NotificationCompat.InboxStyle().also { style -> messages.take(MAX_SHOWN).forEach { style.addLine("${it.sender}  ${it.subject}") } })
                .build(),
        )
    } catch (e: SecurityException) {
        // The permission was taken back between the check and the post.
        Log.w("Ozero", "notifications not allowed", e)
    }
}

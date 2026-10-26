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
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem

/** Checks every account for new mail while the app is closed and tells about mail from people. */
class MailWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as OzeroApp
        val mail = app.mail
        val signedOut = try {
            mail.accounts().filter { it.needsSignIn }.map { it.id }
        } catch (e: CoreException) {
            // The mail still comes; only a sign-out may go unannounced.
            Log.w("Ozero", "accounts not read", e)
            emptyList()
        }
        // Snoozed mail the exact run missed, say after a restart, comes back now.
        val woken = try {
            mail.wake().also { mail.scheduleWake() }
        } catch (e: CoreException) {
            Log.w("Ozero", "snoozed mail not woken", e)
            emptyList()
        }
        try {
            mail.refresh()
        } catch (e: CoreException) {
            // Other accounts may still have synced; the next run tries again.
            Log.w("Ozero", "background sync failed", e)
        }
        // Once, when the server signs an account out; the inbox shows it from then on.
        if (!app.inForeground) try {
            notifySignIn(applicationContext, mail.accounts().filter { it.needsSignIn && it.id !in signedOut })
        } catch (e: CoreException) {
            Log.w("Ozero", "accounts not read", e)
        }
        try {
            // With the app open, the inbox shows the mail; a notification would only repeat it.
            if (app.inForeground) mail.forgetNewMail() else notifyMail(applicationContext, woken + mail.newMail())
        } catch (e: CoreException) {
            Log.w("Ozero", "new mail not read", e)
        }
        return Result.success()
    }

    companion object {
        private const val NAME = "check-mail"

        /** Removes the notifications: the inbox on screen shows that mail now. */
        fun clearNotifications(context: Context) = NotificationManagerCompat.from(context).cancelAll()

        /** Every 15 minutes, the shortest Android allows, with a network and enough battery. */
        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<MailWorker>(15, TimeUnit.MINUTES)
                .setConstraints(Constraints(requiredNetworkType = NetworkType.CONNECTED, requiresBatteryNotLow = true))
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }
    }
}

private const val CHANNEL = "new_mail"
private const val SIGN_IN_CHANNEL = "sign_in"
private const val GROUP = "new_mail"
private const val SUMMARY_ID = 0
/** More at once is noise; the summary still counts them all. */
private const val MAX_SHOWN = 5

private fun openInbox(context: Context) = PendingIntent.getActivity(
    context,
    0,
    Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
    PendingIntent.FLAG_IMMUTABLE,
)

/** Asks to sign in to each account again; a tap opens the inbox, where its card is. */
private fun notifySignIn(context: Context, accounts: List<AccountItem>) {
    if (accounts.isEmpty()) return
    if (context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
    val manager = NotificationManagerCompat.from(context)
    manager.createNotificationChannel(
        NotificationChannel(SIGN_IN_CHANNEL, context.getString(R.string.notify_sign_in_channel), NotificationManager.IMPORTANCE_DEFAULT),
    )
    accounts.forEach { account ->
        manager.notify(
            // Apart from the messages' ids, which are positive.
            -account.id.toInt(),
            NotificationCompat.Builder(context, SIGN_IN_CHANNEL)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentIntent(openInbox(context))
                .setAutoCancel(true)
                .setContentTitle(context.getString(R.string.inbox_signed_out, account.name))
                .setContentText(context.getString(R.string.inbox_signed_out_text))
                .build(),
        )
    }
}

/** One notification per message, newest first, grouped under a summary; a tap opens the inbox. */
internal fun notifyMail(context: Context, messages: List<MessageItem>) {
    if (messages.isEmpty()) return
    if (context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
    val manager = NotificationManagerCompat.from(context)
    manager.createNotificationChannel(
        NotificationChannel(CHANNEL, context.getString(R.string.notify_channel), NotificationManager.IMPORTANCE_DEFAULT),
    )
    val open = openInbox(context)
    fun builder() = NotificationCompat.Builder(context, CHANNEL)
        .setSmallIcon(R.drawable.ic_notification)
        .setContentIntent(open)
        .setAutoCancel(true)
        .setGroup(GROUP)
    val shown = messages.take(MAX_SHOWN)
    val subject = { m: MessageItem -> m.subject.ifBlank { context.getString(R.string.inbox_no_subject) } }
    shown.forEach { message ->
        manager.notify(
            message.id.toInt(),
            builder()
                .setContentTitle(message.sender)
                .setContentText(subject(message))
                .setStyle(NotificationCompat.BigTextStyle().bigText(listOf(subject(message), message.snippet).filter { it.isNotBlank() }.joinToString("\n")))
                .setWhen(message.date * 1000)
                .setShowWhen(true)
                .build(),
        )
    }
    // Counts what earlier runs left in the shade too, and what this one did not show.
    val showing = manager.activeNotifications.count { it.notification.group == GROUP && it.id != SUMMARY_ID }
    manager.notify(SUMMARY_ID, builder().setGroupSummary(true).setNumber(showing + messages.size - shown.size).build())
}

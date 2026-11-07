package com.ozeromail.app

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.RemoteInput
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.richEscape

/**
 * Done, Read and Reply on a new mail notification. Each changes the database first, as a tap in
 * the app does, and the server hears in the background; the notification then goes.
 */
class NotificationActions : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getLongExtra(MESSAGE, 0)
        val reply = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(REPLY)?.toString()
        val mail = (context.applicationContext as OzeroApp).mail
        val pending = goAsync()
        CoroutineScope(Dispatchers.Default).launch {
            try {
                val message = mail.message(id) ?: return@launch
                when (intent.action) {
                    DONE -> mail.archive(listOf(message))
                    READ -> mail.markRead(listOf(message))
                    REPLY -> if (!reply.isNullOrBlank()) {
                        mail.send(mail.reply(message, all = false).copy(text = richEscape(reply.trim())))
                        mail.markRead(listOf(message))
                    }
                }
            } catch (e: CoreException) {
                Log.w("Ozero", "notification action not done", e)
            } finally {
                NotificationManagerCompat.from(context).cancel(id.toInt())
                pending.finish()
            }
        }
    }

    companion object {
        private const val DONE = "com.ozeromail.app.DONE"
        private const val READ = "com.ozeromail.app.READ"
        private const val REPLY = "com.ozeromail.app.REPLY"
        private const val MESSAGE = "message"

        /** Done when it can be archived, Read, and Reply typed in the notification. */
        fun actions(context: Context, message: MessageItem): List<NotificationCompat.Action> {
            fun intent(action: String, mutable: Boolean = false) = PendingIntent.getBroadcast(
                context,
                // One for each message and action, so none replaces another's.
                message.id.toInt() * 3 + listOf(DONE, READ, REPLY).indexOf(action),
                Intent(context, NotificationActions::class.java).setAction(action).putExtra(MESSAGE, message.id),
                PendingIntent.FLAG_UPDATE_CURRENT or if (mutable) PendingIntent.FLAG_MUTABLE else PendingIntent.FLAG_IMMUTABLE,
            )
            val done = NotificationCompat.Action.Builder(0, context.getString(R.string.action_done), intent(DONE))
                .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_ARCHIVE)
                .setShowsUserInterface(false)
                .build()
            val read = NotificationCompat.Action.Builder(0, context.getString(R.string.action_read), intent(READ))
                .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_MARK_AS_READ)
                .setShowsUserInterface(false)
                .build()
            // Mutable: the system writes the typed answer into it.
            val reply = NotificationCompat.Action.Builder(0, context.getString(R.string.message_reply), intent(REPLY, mutable = true))
                .addRemoteInput(RemoteInput.Builder(REPLY).setLabel(context.getString(R.string.message_reply_prompt)).build())
                .setSemanticAction(NotificationCompat.Action.SEMANTIC_ACTION_REPLY)
                .setAllowGeneratedReplies(false)
                .setShowsUserInterface(false)
                .build()
            return listOfNotNull(done.takeIf { message.canArchive }, read, reply)
        }
    }
}

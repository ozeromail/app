package com.ozeromail.app

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.app.NotificationCompat.Action
import androidx.core.app.RemoteInput
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem

/**
 * Done, Read and Reply on a new mail notification. Each changes the database first, as a tap in
 * the app does, and the server hears in the background; the notification then goes.
 */
class NotificationActions : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getLongExtra(MESSAGE, 0)
        val reply = RemoteInput.getResultsFromIntent(intent)?.getCharSequence(Kind.REPLY.name)?.toString()
        val mail = (context.applicationContext as OzeroApp).mail
        val pending = goAsync()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                val message = mail.message(id) ?: return@launch
                when (intent.action?.let(Kind::valueOf)) {
                    Kind.DONE -> mail.archive(listOf(message))
                    Kind.READ -> mail.markRead(listOf(message))
                    Kind.REPLY -> if (!reply.isNullOrBlank()) mail.quickReply(message, reply)
                    null -> Unit
                }
            } catch (e: CoreException) {
                Log.w("Ozero", "notification action not done", e)
            } finally {
                dismissMail(context, id)
                pending.finish()
            }
        }
    }

    private enum class Kind { DONE, READ, REPLY }

    companion object {
        private const val MESSAGE = "message"

        /** Done when it can be archived, Read, and Reply typed in the notification. */
        fun actions(context: Context, message: MessageItem): List<Action> {
            fun action(kind: Kind, label: Int, semantic: Int, mutable: Boolean = false) = Action.Builder(
                0,
                context.getString(label),
                PendingIntent.getBroadcast(
                    context,
                    // One for each message and action, so none replaces another's.
                    message.id.toInt() * Kind.entries.size + kind.ordinal,
                    Intent(context, NotificationActions::class.java).setAction(kind.name).putExtra(MESSAGE, message.id),
                    PendingIntent.FLAG_UPDATE_CURRENT or if (mutable) PendingIntent.FLAG_MUTABLE else PendingIntent.FLAG_IMMUTABLE,
                ),
            ).setSemanticAction(semantic).setShowsUserInterface(false)
            return buildList {
                if (message.canArchive) add(action(Kind.DONE, R.string.action_done, Action.SEMANTIC_ACTION_ARCHIVE).build())
                add(action(Kind.READ, R.string.action_read, Action.SEMANTIC_ACTION_MARK_AS_READ).build())
                // Mutable: the system writes the typed answer into it.
                add(
                    action(Kind.REPLY, R.string.message_reply, Action.SEMANTIC_ACTION_REPLY, mutable = true)
                        .addRemoteInput(RemoteInput.Builder(Kind.REPLY.name).setLabel(context.getString(R.string.message_reply_prompt)).build())
                        .setAllowGeneratedReplies(false)
                        .build(),
                )
            }
        }
    }
}

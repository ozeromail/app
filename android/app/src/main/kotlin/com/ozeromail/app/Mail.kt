package com.ozeromail.app

import android.content.Context
import android.util.Log
import androidx.annotation.StringRes
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.Category
import uniffi.ozero_api.Core
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftItem
import uniffi.ozero_api.Endpoint
import uniffi.ozero_api.ImagesPage
import uniffi.ozero_api.InboxTab
import uniffi.ozero_api.MessageBody
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.UnsentItem
import uniffi.ozero_api.googleSignIn

/** What the screens need from the core; every call runs off the main thread. */
class Mail(private val context: Context) {
    private val vault = Vault(context)

    private val core by lazy {
        Core.open(File(context.noBackupFilesDir, "mail.db").path, vault.databaseKey())
    }

    /** Lives as long as the app, so an action still reaches the server after its screen closes. */
    private val background = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var prefetching: Job? = null

    suspend fun hasAccounts(): Boolean = io { core.accounts().isNotEmpty() }

    /** The accounts mail can be sent from, in the order they were added. */
    suspend fun accounts(): List<AccountItem> = io { core.accounts() }

    /** [color] is a place in [accountColors]. */
    suspend fun setAccountColor(account: AccountItem, color: Int) = io { core.setAccountColor(account.id, color.toUInt()) }

    /** Removes the account and its mail from the phone, then its secret. */
    suspend fun removeAccount(account: AccountItem) = io {
        core.removeAccount(account.id)
        vault.forgetPassword(account.email)
    }

    suspend fun inbox(tab: Category): List<MessageItem> = io { core.unifiedInbox(tab, INBOX_SIZE) }

    /** Unread mail in People that came since the last call, for a notification. */
    suspend fun newMail(): List<MessageItem> = io { core.takeNewMail() }

    /** The mail synced so far is on screen, so it is not announced later. */
    suspend fun forgetNewMail() = io { core.forgetNewMail() }

    /** The inbox tabs in order, with their unread counts. */
    suspend fun tabs(): List<InboxTab> = io { core.inboxTabs() }

    /**
     * The core saves the account only after the server has accepted the login.
     * The password goes first, so a saved account always has one; never over a saved account's.
     */
    suspend fun signIn(email: String, password: String, imap: Endpoint, smtp: Endpoint) = io {
        if (core.accounts().any { it.email.equals(email, ignoreCase = true) }) throw CoreException.AccountExists()
        vault.setPassword(email, password)
        core.addImapAccount(email, email, imap, smtp, password)
    }

    /** Addresses the browser came back to from Google, for the sign-in screen to finish. */
    val googleRedirects = Channel<String>(Channel.CONFLATED)

    /**
     * Starts Google sign-in and returns the page to open in the browser. A second tap opens the
     * same sign-in, so whichever browser tab finishes it, the answer is one we expect.
     */
    suspend fun startGoogle(): String = io {
        (vault.googleSignIn() ?: googleSignIn().also { vault.setGoogleSignIn(it) }).url
    }

    /**
     * Finishes Google sign-in with the address the browser came back to. Like [signIn], the
     * secret is kept before the account is saved.
     */
    suspend fun finishGoogle(redirect: String) = io {
        // No sign-in under way: an old answer, say from a link opened again.
        val start = vault.googleSignIn() ?: throw CoreException.Auth()
        val account = core.googleAccount(start, redirect)
        vault.setGoogleSignIn(null)
        vault.setPassword(account.email, account.secret)
        core.addGmailAccount(account.email)
    }

    /**
     * The message body, downloaded once by the core.
     * A missing password only matters for a download, which then fails to log in.
     */
    suspend fun body(message: MessageItem): MessageBody = io {
        core.messageBody(message.id, password(message.accountId).orEmpty())
    }

    /** Messages matching every word of [text], newest first. */
    suspend fun search(text: String): List<MessageItem> = io { core.search(text, SEARCH_SIZE) }

    /** The message's conversation, oldest first. */
    suspend fun thread(message: MessageItem): List<MessageItem> = io { core.thread(message.id) }

    /**
     * Marks the message's conversation read, or unread, on the device now, so the list shows it on return;
     * the server hears about it in the background.
     */
    suspend fun markRead(message: MessageItem, seen: Boolean = true) {
        io { core.markRead(message.id, seen) }
        push(message.accountId)
    }

    /**
     * Takes the message's conversation out of the inbox now and returns the actions for [undo].
     * The server gets the move once the undo time is over.
     */
    suspend fun archive(message: MessageItem): List<Long> = move(message) { core.archive(it) }

    /** As [archive], into the trash. */
    suspend fun trash(message: MessageItem): List<Long> = move(message) { core.trash(it) }

    private suspend fun move(message: MessageItem, queue: (Long) -> List<Long>): List<Long> {
        val actions = io { queue(message.id) }
        push(message.accountId)
        return actions
    }

    /** [message]'s page with its remote images loaded; null when it has none. */
    suspend fun images(message: MessageItem): ImagesPage? = io { core.messageImages(message.id) }

    /** From now on the remote images of mail from [message]'s sender load on their own. */
    suspend fun allowImages(message: MessageItem) = io { core.allowImages(message.id) }

    /** An answer to [message]: to its sender, or with [all] to everyone it went to as well. */
    suspend fun reply(message: MessageItem, all: Boolean): DraftItem = io { core.reply(message, all) }

    /** The saved text of [message] quoted under [header]; empty while its body is not saved. */
    suspend fun quote(message: MessageItem, header: String): String = io { core.quote(message.id, header) }

    /** Takes back an archive that has not gone to the server yet; false if too late. */
    suspend fun undo(actions: List<Long>): Boolean = io { core.undo(actions) }

    /**
     * Puts the message in the outbox and returns it for [undoSend].
     * It goes to the server once the undo time is over.
     */
    suspend fun send(draft: DraftItem): Long {
        val id = io { core.send(draft.accountId, draft.to, draft.subject, draft.text, draft.replyTo, draft.quote) }
        push(draft.accountId)
        return id
    }

    /** The messages the server refused, newest first; [undoSend] takes one back to fix it. */
    suspend fun unsent(): List<UnsentItem> = io { core.unsent() }

    /** Takes back a message that has not gone out yet, to edit it again; null if too late. */
    suspend fun undoSend(id: Long): DraftItem? = io { core.undoSend(id) }

    /** Sends the account's queued actions in the background, waiting for held ones. */
    private fun push(accountId: Long) {
        background.launch {
            try {
                val password = password(accountId) ?: return@launch
                core.pushActions(accountId, password)
            } catch (e: Exception) {
                // Still queued: the next push or sync sends it. Nothing here may crash the app.
                Log.w("Ozero", "push failed", e)
            }
        }
    }

    /** Syncs all accounts at once; the first failure is thrown once every one has finished. */
    suspend fun refresh() = io {
        val failures = coroutineScope {
            core.accounts().mapNotNull { account ->
                val password = vault.password(account.email) ?: return@mapNotNull null
                async {
                    try {
                        core.sync(account.id, password)
                        null
                    } catch (e: CoreException) {
                        e
                    }
                }
            }.awaitAll()
        }
        failures.firstNotNullOfOrNull { it }?.let { throw it }
    }

    /**
     * Downloads the newest inbox mail whole in the background, so it opens at once. Only for
     * the app on screen: background checks stay light.
     */
    fun prefetch() {
        // One at a time: a second would download the same mail.
        if (prefetching?.isActive == true) return
        prefetching = background.launch {
            for (account in core.accounts()) {
                try {
                    val password = vault.password(account.email) ?: continue
                    core.prefetch(account.id, password)
                } catch (e: CoreException) {
                    // The reader downloads what is left when a message opens.
                    Log.w("Ozero", "prefetch failed", e)
                }
            }
        }
    }

    private fun password(accountId: Long): String? =
        core.accounts().firstOrNull { it.id == accountId }?.let { vault.password(it.email) }

    private suspend fun <T> io(block: suspend () -> T): T = withContext(Dispatchers.IO) { block() }

    companion object {
        const val IMAPS_PORT = 993
        const val SMTPS_PORT = 465
        private const val INBOX_SIZE = 200u
        private const val SEARCH_SIZE = 100u
    }
}

/** The string to show for a core error; the details only go to the log. */
@StringRes
fun CoreException.text(): Int {
    Log.w("Ozero", "core error", this)
    return when (this) {
        is CoreException.Auth -> R.string.error_auth
        is CoreException.AccountExists -> R.string.error_account_exists
        is CoreException.Network -> R.string.error_network
        is CoreException.Tls -> R.string.error_tls
        is CoreException.Server -> R.string.error_server
        is CoreException.Gone -> R.string.error_gone
        is CoreException.NoArchive -> R.string.error_no_archive
        is CoreException.NoTrash -> R.string.error_no_trash
        is CoreException.BadRecipients -> R.string.error_bad_recipients
        is CoreException.NoSmtp -> R.string.error_no_smtp
        else -> R.string.error_storage
    }
}

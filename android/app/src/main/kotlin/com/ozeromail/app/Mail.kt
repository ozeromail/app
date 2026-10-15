package com.ozeromail.app

import android.content.Context
import android.util.Log
import androidx.annotation.StringRes
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import uniffi.ozero_api.Core
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.MessageItem

/** What the screens need from the core; every call runs off the main thread. */
class Mail(private val context: Context) {
    private val vault = Vault(context)

    private val core by lazy {
        Core.open(File(context.noBackupFilesDir, "mail.db").path, vault.databaseKey())
    }

    suspend fun hasAccounts(): Boolean = io { core.accounts().isNotEmpty() }

    suspend fun inbox(): List<MessageItem> = io { core.unifiedInbox(INBOX_SIZE) }

    /**
     * The core saves the account only after the server has accepted the login.
     * The password goes first, so a saved account always has one.
     */
    suspend fun signIn(email: String, password: String, host: String, port: Int) = io {
        vault.setPassword(email, password)
        core.addImapAccount(email, email, host, port.toUShort(), password)
    }

    /** Syncs all accounts at once; the first failure is thrown once every one has finished. */
    suspend fun refresh() = io {
        val failures = coroutineScope {
            core.accounts().mapNotNull { account ->
                val password = vault.password(account.email) ?: return@mapNotNull null
                async {
                    try {
                        core.syncImap(account.id, password)
                        null
                    } catch (e: CoreException) {
                        e
                    }
                }
            }.awaitAll()
        }
        failures.firstNotNullOfOrNull { it }?.let { throw it }
    }

    private suspend fun <T> io(block: suspend () -> T): T = withContext(Dispatchers.IO) { block() }

    companion object {
        const val IMAPS_PORT = 993
        private const val INBOX_SIZE = 200u
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
        else -> R.string.error_storage
    }
}

package com.ozeromail.app

import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.util.Log
import androidx.annotation.StringRes
import java.io.File
import java.util.UUID
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.flow.retryWhen
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.ozero_api.AccountItem
import uniffi.ozero_api.AccountKind
import uniffi.ozero_api.Answered
import uniffi.ozero_api.AttachmentFile
import uniffi.ozero_api.AttachmentItem
import uniffi.ozero_api.Category
import uniffi.ozero_api.Core
import uniffi.ozero_api.CoreException
import uniffi.ozero_api.DraftFileItem
import uniffi.ozero_api.DraftItem
import uniffi.ozero_api.DraftRow
import uniffi.ozero_api.PeopleItem
import uniffi.ozero_api.Endpoint
import uniffi.ozero_api.FolderItem
import uniffi.ozero_api.FolderRole
import uniffi.ozero_api.ForwardLabels
import uniffi.ozero_api.ImagesPage
import uniffi.ozero_api.InboxTab
import uniffi.ozero_api.InviteAnswer
import uniffi.ozero_api.InviteItem
import uniffi.ozero_api.Mailbox
import uniffi.ozero_api.MessageBody
import uniffi.ozero_api.MessageItem
import uniffi.ozero_api.NewMail
import uniffi.ozero_api.PersonCard
import uniffi.ozero_api.PersonFound
import uniffi.ozero_api.RecentFileItem
import uniffi.ozero_api.Unsubscribed
import uniffi.ozero_api.UnsentItem
import uniffi.ozero_api.googleSignIn

/** What the screens need from the core; every call runs off the main thread. */
class Mail(private val context: Context) {
    private val vault = Vault(context)

    /**
     * Opened on first use, off the main thread: loading the library there stalls the start of a
     * process woken for a background job, and Android then takes the job's late answers for a hang.
     */
    private val core by lazy {
        try {
            CoreNative.init(context)
        } catch (e: RuntimeException) {
            // The core then refuses secure connections with a TLS error instead of crashing here.
            Log.e("Ozero", "certificate checks are not set up", e)
        }
        Core.open(File(context.noBackupFilesDir, "mail.db").path, vault.databaseKey()).also {
            it.useCertificates(KeyChainCertificates(context))
            it.usePlatformTls(AndroidTls(context))
        }
    }

    /** Lives as long as the app, so an action still reaches the server after its screen closes. */
    private val background = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var prefetching: Job? = null

    private val savedDrafts = MutableStateFlow(emptyList<DraftRow>())

    /** The drafts on the phone as their list shows them, last saved first; read again each time the database changes. */
    val drafts: StateFlow<List<DraftRow>> = savedDrafts

    /** The draft saved last, by id: the one the toast after writing opens again. */
    @Volatile
    var lastSaved = 0L
        private set

    /** This run's downloaded attachments; those of earlier runs go, as an id may name another message by now. */
    private val downloads = File(context.cacheDir, "attachments/${UUID.randomUUID()}")

    init {
        background.launch {
            downloads.parentFile?.listFiles()?.filter { it != downloads }?.forEach { it.deleteRecursively() }
        }
    }

    /** Each change to the database, here or in the background, at most every [CHANGE_PAUSE_MS]. */
    private val changed = flow {
        var seen = 0uL
        while (true) {
            seen = io { core.changed(seen) }
            emit(Unit)
            delay(CHANGE_PAUSE_MS)
        }
    }.retryWhen { e, _ ->
        // Say the database did not open: the screens show that error themselves.
        Log.w("Ozero", "changes not read", e)
        delay(CHANGE_RETRY_MS)
        true
    }.shareIn(background, SharingStarted.Eagerly)

    /**
     * Emits at once, then each time the database changes: screens read what they show then.
     * One that is busy reading gets one more emission for all the changes meanwhile.
     */
    val changes: Flow<Unit> = changed.onSubscription { emit(Unit) }.conflate()

    init {
        background.launch { changes.collect { loadDrafts() } }
    }

    /** Whether actions the server refused were undone on the phone since the last call. */
    suspend fun takeReverted(): Boolean = io { core.takeReverted() }

    suspend fun hasAccounts(): Boolean = io { core.accounts().isNotEmpty() }

    /** The accounts mail can be sent from, in the order they were added. */
    suspend fun accounts(): List<AccountItem> = io { core.accounts() }

    /** [color] is a place in [accountColors]. */
    suspend fun setAccountColor(account: AccountItem, color: Int) = io { core.setAccountColor(account.id, color.toUInt()) }

    /** The person's own name for [account]; empty goes back to the usual one. */
    suspend fun setAccountLabel(account: AccountItem, label: String) = io { core.setAccountLabel(account.id, label) }

    /** The signature of accounts without one of their own; empty for none. */
    suspend fun signature(): String = io { core.signature() }

    suspend fun setSignature(signature: String) = io { core.setSignature(signature) }

    /** Gives the account a signature of its own; empty goes back to the default one. */
    suspend fun setAccountSignature(account: AccountItem, signature: String) = io { core.setAccountSignature(account.id, signature) }

    /** Removes the account and its mail from the phone, then its secret. */
    suspend fun removeAccount(account: AccountItem) = io {
        core.removeAccount(account.id)
        vault.forgetPassword(account.email)
    }

    /**
     * The newest [limit] conversations of a tab of [account]'s inbox, or with null of the
     * unified inbox; fewer once the saved mail ends, see [older].
     */
    suspend fun inbox(account: Long?, tab: Category, limit: Int): List<MessageItem> = io { core.inbox(account, tab, limit.toUInt()) }

    /** A mailbox other than the unified inbox, which [inbox] shows by tab. */
    suspend fun mailbox(mailbox: Mailbox, limit: Int): List<MessageItem> = io { core.mailbox(mailbox, limit.toUInt()) }

    /**
     * Brings older mail of [mailbox] from every account's server, for a list scrolled to its end;
     * false once none of them has more. The first failure is thrown once every one has finished.
     */
    suspend fun older(mailbox: Mailbox): Boolean = eachAccount(mailbox) { account, password ->
        core.olderMail(account.id, password, mailbox)
    }.any { it }

    /** Keeps [account]'s mail of the last [days] days on the phone, or all of it with 0. */
    suspend fun setKeepDays(account: AccountItem, days: Int) = io { core.setAccountKeep(account.id, days.toUInt()) }

    /** Sets when the phone looks for the account's new mail with the app closed. */
    suspend fun setNewMail(account: AccountItem, check: NewMail) = io { core.setAccountNewMail(account.id, check) }

    /** Every account's folders; they show once the account's first sync has listed them. */
    suspend fun folders(): List<FolderItem> = io { core.folders() }

    /** Unread mail in People that came since the last call, for a notification. */
    suspend fun newMail(): List<MessageItem> = io { core.takeNewMail() }

    /** The mail synced so far is on screen, so it is not announced later. */
    suspend fun forgetNewMail() = io { core.forgetNewMail() }

    /** The tabs of [account]'s inbox, or of the unified one, in order, with their unread counts. */
    suspend fun tabs(account: Long?): List<InboxTab> = io { core.inboxTabs(account) }

    /** Whether every tab of [account]'s inbox, or of the unified one, is empty. */
    suspend fun inboxEmpty(account: Long?): Boolean = io { core.inboxIsEmpty(account) }

    /** Unread People mail in each account's inbox, by account id; none for an account without. */
    suspend fun peopleUnread(): Map<Long, Int> = io { core.peopleUnread().associate { it.account to it.unread.toInt() } }

    /** Puts the accounts in this order. */
    suspend fun setAccountOrder(accounts: List<AccountItem>) = io { core.setAccountOrder(accounts.map { it.id }) }

    /** Takes [account]'s inbox into the unified one, or leaves it out. */
    suspend fun setInAll(account: AccountItem, inAll: Boolean) = io { core.setAccountInAll(account.id, inAll) }

    /** Shows notifications for the account's new mail from people, or stops them. */
    suspend fun setNotify(account: AccountItem, notify: Boolean) = io { core.setAccountNotify(account.id, notify) }

    /** The name the account's mail goes out under; empty leaves only the address. */
    suspend fun setSender(account: AccountItem, name: String) = io { core.setAccountSender(account.id, name) }

    /** Pins [folder] among its account's usual folders, or unpins it. */
    suspend fun setPinned(folder: FolderItem, pinned: Boolean) = io { core.setFolderPinned(folder.id, pinned) }

    /**
     * The core saves the account only after the server has accepted the login.
     * The password goes first, so a saved account always has one; never over a saved account's.
     */
    suspend fun signIn(email: String, password: String, imap: Endpoint, smtp: Endpoint) = io {
        if (core.accounts().any { it.email.equals(email, ignoreCase = true) }) throw CoreException.AccountExists()
        vault.setPassword(email, password)
        core.addImapAccount(email, email, imap, smtp, password)
    }

    /**
     * Like [signIn], with the servers found from the email: IMAP at its domain's usual hosts,
     * else Exchange. Fails with `NotFound` when neither answers.
     */
    suspend fun signInFound(email: String, password: String) = io {
        if (core.accounts().any { it.email.equals(email, ignoreCase = true) }) throw CoreException.AccountExists()
        vault.setPassword(email, password)
        core.addFoundAccount(email, password)
    }

    /**
     * Like [signIn], for Exchange: the core finds the server from the email unless [server]
     * is given, and signs in as the email unless [login] is.
     */
    /** [trusted] is the fingerprint of a server certificate the person chose to trust. */
    suspend fun signInExchange(
        email: String,
        password: String,
        server: String?,
        login: String?,
        certificate: String?,
        trusted: String? = null,
    ) = io {
        if (core.accounts().any { it.email.equals(email, ignoreCase = true) }) throw CoreException.AccountExists()
        vault.setPassword(email, password)
        // No name: the core asks the server for the owner's.
        core.addExchangeAccount(email, "", server, login, certificate, trusted, password)
    }

    /** Addresses the browser came back to from Google, for the sign-in screen to finish. */
    val googleRedirects = Channel<String>(Channel.CONFLATED)

    /**
     * Starts Google sign-in and returns the page to open in the browser; [again] signs in to
     * that saved account, which Google then offers, as it does a typed [email]. A second tap opens the same sign-in, so
     * whichever browser tab finishes it, the answer is one we expect.
     */
    suspend fun startGoogle(again: AccountItem? = null, email: String? = null): String = io {
        val hint = again?.email ?: email
        val waiting = vault.googleSignIn()?.takeIf { Uri.parse(it.url).getQueryParameter("login_hint") == hint }
        (waiting ?: googleSignIn(hint).also { vault.setGoogleSignIn(it) }).url
    }

    /**
     * Finishes Google sign-in with the address the browser came back to. Like [signIn], the
     * secret is kept before the account is saved. With [again], the sign-in was for that saved
     * account: its new secret replaces the old one and its mail stays.
     */
    suspend fun finishGoogle(redirect: String, again: AccountItem? = null) = io {
        // No sign-in under way: an old answer, say from a link opened again.
        val start = vault.googleSignIn() ?: throw CoreException.Auth()
        val account = core.googleAccount(start, redirect, again?.id)
        vault.setGoogleSignIn(null)
        // Under the saved address, which Google may write in other letter case.
        vault.setPassword(again?.email ?: account.email, account.secret)
        if (again == null) core.addGmailAccount(account.email, account.name) else core.signedInAgain(again.id, account.name)
    }

    /** A new password for a saved IMAP or Exchange account, kept once its servers accept it. */
    suspend fun signInAgain(account: AccountItem, password: String) = io {
        core.checkPassword(account.id, password)
        vault.setPassword(account.email, password)
        core.signedInAgain(account.id, null)
    }

    /**
     * The message body, downloaded once by the core.
     * A missing password only matters for a download, which then fails to log in.
     */
    suspend fun body(message: MessageItem): MessageBody = io {
        core.messageBody(message.id, password(message.accountId).orEmpty())
    }

    /**
     * The attachments' files, those not here yet downloaded together, once a run, into the
     * app's cache, where other apps may be given them.
     */
    suspend fun attachments(message: MessageItem, items: List<AttachmentItem>): List<File> = io {
        val untitled = context.getString(R.string.attachment_untitled)
        val files = items.map { File(downloads, "${message.id}-${it.part}/${it.fileName(untitled)}") }
        val missing = items.zip(files).filter { (_, file) -> !file.exists() }
        if (missing.isNotEmpty()) {
            missing.forEach { (_, file) -> file.parentFile?.mkdirs() }
            val targets = missing.map { (item, file) -> AttachmentFile(item.part, file.path) }
            core.saveAttachments(message.id, targets, password(message.accountId).orEmpty())
        }
        files
    }

        /** Messages matching every word of [text], newest first, with what [searchServer] found for it. */
    suspend fun search(text: String): List<MessageItem> = io { core.search(text, SEARCH_SIZE) }

    /**
     * Searches every account's mail on its server for [text], older mail than the phone keeps
     * included; [search] then shows what was found. The first failure is thrown once all are done.
     */
    suspend fun searchServer(text: String) {
        // Back from a result, the same search has its answer saved already.
        val now = SystemClock.elapsedRealtime()
        if (searched?.let { (last, at) -> last == text && now - at < SEARCH_KEEP_MS } == true) return
        eachAccount(INBOX) { account, password -> core.searchServer(account.id, password, text) }
        searched = text to now
    }

    /** The last server search that went through, and when. */
    private var searched: Pair<String, Long>? = null

    /** Up to [PEOPLE_SIZE] people for what was typed in "To", those written to most and lately first. */
    suspend fun findPeople(text: String): List<PersonFound> = io { core.findPeople(text, PEOPLE_SIZE) }

    /**
     * People in the company directories of the Exchange accounts whose name or address starts with
     * [text]; nobody under three letters. The first failure is thrown once all are done.
     */
    suspend fun searchDirectory(text: String): List<PersonFound> =
        eachAccount(INBOX, which = { it.kind == AccountKind.EXCHANGE }) { account, password ->
            core.searchDirectory(account.id, password, text)
        }.flatten()

    /** The name to show for each of [emails] on its chip: the contacts', the mail's or the address. */
    suspend fun recipientNames(emails: List<String>): List<String> = io { core.recipientNames(emails) }

    /** What a tap on someone's name shows, [name] as the mail gives it. */
    suspend fun person(email: String, name: String?): PersonCard = io { core.person(email, name) }

    /** All mail from or to [emails], newest first. */
    suspend fun mailWith(emails: List<String>): List<MessageItem> = io { core.mailWith(emails, SEARCH_SIZE) }

    /**
     * Hands the phone's contacts to the core, which names people with them, or forgets them once
     * the access is gone. Only when they changed: each write makes the screens read again.
     */
    fun readContacts() {
        background.launch { giveContacts() }
    }

    /** As [readContacts], returning once they are given. */
    suspend fun giveContacts() = io {
        try {
            core.setContacts(PhoneContacts.read(context))
        } catch (e: CoreException) {
            Log.w("Ozero", "contacts not kept", e)
        }
    }

    /** The message's conversation, oldest first. */
    suspend fun thread(message: MessageItem): List<MessageItem> = io { core.thread(message.id) }

    /**
     * Marks the messages' conversations read, or the messages unread, on the device now and in one go,
     * so the list shows it on return; the server hears about it in the background.
     */
    suspend fun markRead(messages: List<MessageItem>, seen: Boolean = true) {
        io { core.markRead(messages.map { it.id }, seen) }
        push(messages)
    }

    /**
     * Takes the messages' conversations out of the inbox now, all or none, and returns the actions for
     * [undo]. The server gets the move once the undo time is over.
     */
    suspend fun archive(messages: List<MessageItem>): List<Long> = move(messages) { core.archive(it) }

    /** As [archive], into the account's spam. */
    suspend fun spam(messages: List<MessageItem>): List<Long> = move(messages) { core.spam(it) }

    /** Flags the messages, or takes their flags off; only they, not the rest of their conversations. */
    suspend fun flag(messages: List<MessageItem>, flagged: Boolean) {
        io { core.flag(messages.map { it.id }, flagged) }
        push(messages)
    }

    /** As [archive], into [folder], another folder of the messages' account. */
    suspend fun moveToFolder(messages: List<MessageItem>, folder: FolderItem): List<Long> =
        move(messages) { core.moveToFolder(it, folder.id) }

    /** Where [moveToFolder] can take [message]: its account's inbox, archive and own folders, but its own. */
    suspend fun moveTargets(message: MessageItem): List<FolderItem> = io { core.moveTargets(message.id) }

    /** Leaves [message]'s list; a request or message asking to leave goes out in the background. */
    suspend fun unsubscribe(message: MessageItem): Unsubscribed {
        val done = io { core.unsubscribe(message.id) }
        push(message.accountId)
        return done
    }

    /** The meeting [message] invites to, and the day around it, once its body is saved; null if it is no invitation. */
    suspend fun invite(message: MessageItem): InviteItem? = io { core.invite(message.id) }

    /**
     * Answers the invitation [message] is, held like [archive]; the organizer hears in the
     * background, by mail with [subject] and [comment] where the server does not tell them.
     */
    suspend fun answerInvite(message: MessageItem, answer: InviteAnswer, comment: String, subject: String): Answered {
        val answered = io { core.answerInvite(message.id, answer, comment, subject) }
        push(message.accountId)
        return answered
    }

    /** Takes back an answer not sent yet; false if too late. */
    suspend fun undoAnswer(answered: Answered): Boolean = io { core.undoAnswer(answered) }

    /** As [archive], back into the inbox: for mail in the archive, trash or spam. */
    suspend fun moveToInbox(messages: List<MessageItem>): List<Long> = move(messages) { core.moveToInbox(it) }

    /**
     * As [archive], into the account's snoozed folder; the conversations come back to the top of
     * the inbox, unread, at [until], in epoch seconds.
     */
    suspend fun snooze(messages: List<MessageItem>, until: Long): List<Long> {
        val actions = move(messages) { core.snooze(it, until) }
        scheduleWake()
        return actions
    }

    /**
     * Brings back the snoozed mail whose time has come and returns it, to tell about; the server
     * hears about it in the background. Then [scheduleWake] for the rest.
     */
    suspend fun wake(): List<MessageItem> {
        val woken = io { core.wake() }
        push(woken)
        return woken
    }

    /** Wakes the next snoozed mail on time, the app open or not; see [WakeWorker]. */
    suspend fun scheduleWake() = WakeWorker.schedule(context, io { core.nextWake() })

    /** As [archive], into the trash, or gone from the server for good for mail already there. */
    suspend fun delete(messages: List<MessageItem>): List<Long> {
        val (toTrash, forever) = messages.partition { it.canTrash }
        return move(messages) { core.delete(toTrash.map { it.id }, forever.map { it.id }) }
    }

    /** As [delete], [message] alone: the rest of its conversation stays. */
    suspend fun deleteAlone(message: MessageItem): List<Long> = move(listOf(message)) {
        if (message.canTrash) core.deleteAlone(it, emptyList()) else core.deleteAlone(emptyList(), it)
    }

    /** Empties a trash or spam [mailbox] for good, mail not loaded included, held like [archive]. */
    suspend fun emptyMailbox(mailbox: Mailbox): List<Long> {
        val actions = io { core.emptyMailbox(mailbox) }
        pushAll()
        return actions
    }

    private suspend fun move(messages: List<MessageItem>, queue: (List<Long>) -> List<Long>): List<Long> {
        val actions = io { queue(messages.map { it.id }) }
        push(messages)
        return actions
    }

    private fun push(messages: List<MessageItem>) = messages.map { it.accountId }.distinct().forEach(::push)

    /** [message]'s page with its remote images loaded; null when it has none. */
    suspend fun images(message: MessageItem): ImagesPage? = io { core.messageImages(message.id) }

    /** From now on the remote images of mail from [message]'s sender load on their own. */
    suspend fun allowImages(message: MessageItem) = io { core.allowImages(message.id) }

    /** The senders whose images load on their own because the person chose so. */
    suspend fun imageSenders(): List<String> = io { core.imageSenders() }

    /** The sender's images wait to be asked for again. */
    suspend fun forbidImages(email: String) = io { core.forbidImages(email) }

    /** Whether all mail's images load on their own, but in spam. */
    suspend fun imagesAlways(): Boolean = io { core.imagesAlways() }

    suspend fun setImagesAlways(always: Boolean) = io { core.setImagesAlways(always) }

    /** A blank message from the first account, with its signature. */
    suspend fun newDraft(): DraftItem = io { core.newDraft() }

    /** An answer to [message]: to its sender, or with [all] to everyone it went to as well. */
    suspend fun reply(message: MessageItem, all: Boolean): DraftItem = io { core.reply(message, all) }

    /** Who [message] went to, and who else a reply to all would copy. */
    suspend fun people(message: MessageItem): PeopleItem = io { core.people(message) }

    /** [message] to send on, its text under [labels] and [date]; loads the text if not saved. */
    suspend fun forward(message: MessageItem, labels: ForwardLabels, date: String): DraftItem = io {
        core.forward(message, labels, date, password(message.accountId).orEmpty())
    }

    /** The text of [message] quoted under [header]; downloaded if it is not saved. */
    suspend fun quote(message: MessageItem, header: String): String = io {
        core.quote(message.id, header, password(message.accountId).orEmpty())
    }

    /** Takes back an archive that has not gone to the server yet; false if too late. */
    suspend fun undo(actions: List<Long>): Boolean = io { core.undo(actions) }

    /**
     * Puts the message in the outbox and returns it for [undoSend].
     * It goes to the server once the undo time is over.
     */
    suspend fun send(draft: DraftItem): Long {
        val id = io { core.send(draft) }
        push(draft.accountId)
        return id
    }

    /** Keeps [draft] on the phone; a new one if its id is 0. Returns its id. */
    suspend fun saveDraft(draft: DraftItem): Long = io { core.saveDraft(draft) }.also { lastSaved = it }

    /** Saves the drafts into each account's drafts folder too, once there is a network. */
    suspend fun pushDrafts() = pushAll()

    /** Reads [drafts] again; on failure they stay as they were. */
    private suspend fun loadDrafts() {
        try {
            savedDrafts.value = io { core.drafts() }
        } catch (e: CoreException) {
            Log.w("Ozero", "drafts not read", e)
        }
    }

    suspend fun deleteDraft(id: Long) = io { core.deleteDraft(id) }

    /** Brings back [draft] as it was thrown away, saved: with its files if [deleteDraft] was a moment ago. */
    suspend fun restoreDraft(draft: DraftItem): DraftItem = io { core.restoreDraft(draft) }

    /** Adds a file to the saved draft [draft]; it goes with the message. */
    suspend fun attachFile(draft: Long, file: PickedFile): DraftFileItem = io { core.attachFile(draft, file.name, file.mime, file.data) }

    /** Files of saved mail to send on, newest first. */
    suspend fun recentFiles(): List<RecentFileItem> = io { core.recentFiles() }

    /** [file] of saved mail, downloaded now to send on. */
    suspend fun downloadFile(file: RecentFileItem): PickedFile = io {
        PickedFile(file.name, file.mime, core.downloadFile(file.message, file.part, password(file.accountId).orEmpty()))
    }

    /** Takes a file off its draft. */
    suspend fun detachFile(file: Long) = io { core.detachFile(file) }

    /** The contents of a file added to a draft, to show it in the text. */
    suspend fun draftFileData(file: Long): ByteArray = io { core.draftFileData(file) }

    /** Saves a message being written as it changes, starting from the saved draft [id], or 0. */
    fun keeper(id: Long) = DraftKeeper(this, background, id)

    /** The message [id], if it is still saved: say the one a draft answers. */
    suspend fun message(id: Long): MessageItem? = io { core.thread(id).find { it.id == id } }

    /** The messages the server refused, newest first; [undoSend] takes one back to fix it. */
    suspend fun unsent(): List<UnsentItem> = io { core.unsent() }

    /** Takes back a message that has not gone out yet, to edit it again as a draft; null if too late. */
    suspend fun undoSend(id: Long): DraftItem? = io { core.undoSend(id) }

    /**
     * Sends the account's queued actions and mail once there is a network, waiting for held
     * ones, also if the app is closed meanwhile.
     */
    private fun push(accountId: Long) = PushWorker.enqueue(context, accountId)

    /** [push] for every account; one with nothing to send does not connect. */
    private suspend fun pushAll() = io { core.accounts() }.forEach { push(it.id) }

    /** Sends the account's queued actions and mail now, waiting for held ones; see [push]. */
    suspend fun pushNow(accountId: Long) {
        val password = io { password(accountId) } ?: return
        io { core.pushActions(accountId, password) }
    }

    /**
     * Syncs [mailbox] in all accounts at once; the first failure is thrown once every one has
     * finished. A refused login is not one: it marks its account. True if an account needs
     * signing in again. Before a failure is thrown, [onFailing] gets the names of the accounts
     * it is about if another account got through; else the whole phone is offline. Only the
     * accounts [which] picks are synced.
     */
    suspend fun refresh(
        mailbox: Mailbox = INBOX,
        which: (AccountItem) -> Boolean = { true },
        onFailing: (List<String>) -> Unit = {},
    ): Boolean {
        val failed = ConcurrentLinkedQueue<Pair<String, CoreException>>()
        val done = AtomicInteger()
        try {
            eachAccount(mailbox, which) { account, password ->
                try {
                    core.syncMailbox(account.id, password, mailbox)
                    done.incrementAndGet()
                } catch (e: CoreException) {
                    if (e !is CoreException.Auth) failed += account.name to e
                    throw e
                }
            }
        } catch (e: CoreException) {
            // Only those that failed the way the error shown says.
            if (done.get() > 0) onFailing(failed.filter { it.second.javaClass == e.javaClass }.map { it.first })
            throw e
        }
        return io { core.accounts().any { it.needsSignIn } }
    }

    /**
     * Runs [call] at once for every account with mail in [mailbox], but those that need signing
     * in again, and returns what they gave; the first failure but a refused login is thrown once
     * every one has finished.
     */
    private suspend fun <T> eachAccount(
        mailbox: Mailbox,
        which: (AccountItem) -> Boolean = { true },
        call: suspend (AccountItem, String) -> T,
    ): List<T> = io {
        // A folder is one account's, so only that account is asked.
        val owner = (mailbox as? Mailbox.Folder)?.let { box -> core.folders().find { it.id == box.id }?.accountId }
        val signedIn = core.accounts().filter { (owner == null || it.id == owner) && !it.needsSignIn && which(it) }.mapNotNull { account ->
            vault.password(account.email)?.let { account to it }
        }
        val results: List<Result<T>> = coroutineScope {
            signedIn.map { (account, password) ->
                async {
                    try {
                        Result.success(call(account, password))
                    } catch (e: CoreException) {
                        Result.failure(e)
                    }
                }
            }.awaitAll()
        }
        results.firstNotNullOfOrNull { it.exceptionOrNull()?.takeIf { e -> e !is CoreException.Auth } }?.let { throw it }
        results.mapNotNull { it.getOrNull() }
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

    /**
     * Calls [onMail] each time an account's inbox may have changed, until cancelled: for the
     * app on screen. [onMail] says whether its sync went through.
     */
    suspend fun watch(accounts: List<AccountItem>, onMail: suspend (AccountItem) -> Boolean) = coroutineScope {
        // One signed out would only be refused again and again.
        for (account in accounts.filterNot { it.needsSignIn }) {
            launch {
                val password = io { vault.password(account.email) } ?: return@launch
                var pause = WATCH_PAUSE_MS
                var failed = false
                while (true) {
                    // After a failed wait a sync comes first, until one goes through: mail may
                    // have come meanwhile, and the error on screen goes.
                    if (failed) failed = !onMail(account)
                    val started = SystemClock.elapsedRealtime()
                    val fine = try {
                        !io { core.waitForMail(account.id, password) } || onMail(account)
                    } catch (e: CoreException) {
                        Log.w("Ozero", "wait for mail failed", e)
                        failed = true
                        false
                    }
                    // A failure, or a server that keeps waking at once, waits longer each time.
                    val quick = SystemClock.elapsedRealtime() - started < WATCH_PAUSE_MS
                    pause = if (fine && !quick) WATCH_PAUSE_MS else (pause * 2).coerceAtMost(WATCH_PAUSE_MAX_MS)
                    delay(pause)
                }
            }
        }
    }

    private fun password(accountId: Long): String? =
        core.accounts().firstOrNull { it.id == accountId }?.let { vault.password(it.email) }

    private suspend fun <T> io(block: suspend () -> T): T = withContext(Dispatchers.IO) { block() }

    companion object {
        val INBOX: Mailbox = Mailbox.Unified(FolderRole.INBOX)
        const val IMAPS_PORT = 993
        const val SMTPS_PORT = 465
        private const val SEARCH_SIZE = 100u
        private const val PEOPLE_SIZE = 6u
        private const val WATCH_PAUSE_MS = 5_000L
        /** A sync writes many times; the screens read again at most this often. */
        private const val CHANGE_PAUSE_MS = 250L
        private const val CHANGE_RETRY_MS = 5_000L
        private const val WATCH_PAUSE_MAX_MS = 5 * 60_000L

        /** How long a server search answer stays good enough to not ask again. */
        private const val SEARCH_KEEP_MS = 60_000L
    }
}

/** What a connection or server error said, for a failed sign-in; null for the others. */
fun CoreException.detail(): String? = when (this) {
    is CoreException.Network -> this.detail
    is CoreException.Tls -> this.detail
    is CoreException.Server -> this.detail
    // Who refused it and why, and what to compare with the IT team's.
    is CoreException.UntrustedCertificate -> "${this.host}: ${this.reason}\nSHA-256 ${this.fingerprint}"
    else -> null
}

/** The string to show for a core error; the details only go to the log. */
@StringRes
fun CoreException.text(): Int {
    Log.w("Ozero", "core error", this)
    return when (this) {
        is CoreException.Auth -> R.string.error_auth
        is CoreException.AccountExists -> R.string.error_account_exists
        is CoreException.WrongAccount -> R.string.error_wrong_account
        is CoreException.Network -> R.string.error_network
        is CoreException.Tls -> R.string.error_tls
        is CoreException.Server -> R.string.error_server
        is CoreException.Gone -> R.string.error_gone
        is CoreException.NoArchive -> R.string.error_no_archive
        is CoreException.NoTrash -> R.string.error_no_trash
        is CoreException.NoSpam -> R.string.error_no_spam
        is CoreException.NoSnoozed -> R.string.error_no_snoozed
        is CoreException.BadRecipients -> R.string.error_bad_recipients
        is CoreException.BadCopies -> R.string.error_bad_copies
        is CoreException.NoSmtp -> R.string.error_no_smtp
        is CoreException.NoServer -> R.string.error_no_server
        is CoreException.NotFound -> R.string.error_server_not_found
        is CoreException.NoEws -> R.string.error_no_ews
        is CoreException.CertificateNeeded -> R.string.error_certificate_needed
        is CoreException.UntrustedCertificate -> R.string.error_untrusted_certificate
        is CoreException.DeviceBlocked -> R.string.error_device_blocked
        is CoreException.TooLarge -> R.string.compose_files_too_large
        else -> R.string.error_storage
    }
}

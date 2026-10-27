import Foundation
import UniformTypeIdentifiers
import SwiftUI
import os

/// How one account's check went.
private enum Checked: Sendable {
    case done
    case failed(String, any Error)
}

/// A check that failed for `accounts`, by name, while the others got through.
struct PartlyFailed: Error {
    let error: any Error
    let accounts: [String]
}

/// What the screens need from the core. The async calls run off the main actor.
final class Mail: Sendable {
    static let imapsPort: UInt16 = 993
    static let smtpsPort: UInt16 = 465
    static let inbox = Mailbox.unified(role: .inbox)
    private static let searchSize: UInt32 = 100
    /// A sync writes many times; the screens read again at most this often.
    private static let changePause = Duration.milliseconds(250)
    private static let changeRetry = Duration.seconds(5)
    private static let watchPause = Duration.seconds(5)
    private static let watchPauseMax = Duration.seconds(5 * 60)
    /// How long a server search answer stays good enough to not ask again.
    private static let searchKeep: TimeInterval = 60

    private let core: Core
    private let vault = Vault()
    private let state = MailState()

    private init(core: Core) {
        self.core = core
    }

    /// The one instance the screens and background checks share, opened on first use.
    static func shared() async throws -> Mail {
        try await opener.mail()
    }

    private static let opener = Opener()

    fileprivate static func open() throws -> Mail {
        let dir = try FileManager.default
            .url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appending(path: "Mail", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        // The key lives in this device's keychain only, so a backup of the database could never be opened.
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var excluded = dir
        try excluded.setResourceValues(values)
        let path = dir.appending(path: "mail.db").path(percentEncoded: false)
        let core = try Core.open(path: path, key: Vault().databaseKey())
        core.useCertificates(certificates: KeychainCertificates())
        let mail = Mail(core: core)
        mail.forgetOldDownloads()
        return mail
    }

    /// Emits at once, then each time the database changes, here or in the background, at most
    /// every `changePause`: screens read what they show then. A screen busy reading gets one
    /// more emission for all the changes meanwhile.
    func changes() -> AsyncStream<Void> {
        AsyncStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let task = Task {
                continuation.yield()
                var seen: UInt64 = 0
                while !Task.isCancelled {
                    seen = await core.changed(seen: seen)
                    continuation.yield()
                    try? await Task.sleep(for: Self.changePause)
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// Whether actions the server refused were undone on the phone since the last call.
    func takeReverted() async throws -> Bool {
        try core.takeReverted()
    }

    func hasAccounts() async throws -> Bool {
        try !core.accounts().isEmpty
    }

    /// The accounts mail can be sent from, in the order they were added.
    func accounts() async throws -> [AccountItem] {
        try core.accounts()
    }

    /// `color` is a place in `accountColors()`.
    func setAccountColor(_ account: AccountItem, color: Int) async throws {
        try core.setAccountColor(account: account.id, color: UInt32(color))
    }

    /// The person's own name for `account`; empty goes back to the usual one.
    func setAccountLabel(_ account: AccountItem, label: String) async throws {
        try core.setAccountLabel(account: account.id, label: label)
    }

    /// When the phone looks for `account`'s new mail with the app closed.
    func setNewMail(_ account: AccountItem, _ check: NewMail) async throws {
        try core.setAccountNewMail(account: account.id, when: check)
    }

    /// The signature of accounts without one of their own; empty for none.
    func signature() async throws -> String {
        try core.signature()
    }

    func setSignature(_ signature: String) async throws {
        try core.setSignature(signature: signature)
    }

    /// Gives `account` a signature of its own; empty goes back to the default one.
    func setAccountSignature(_ account: AccountItem, signature: String) async throws {
        try core.setAccountSignature(account: account.id, signature: signature)
    }

    /// Keeps `account`'s mail of the last `days` days on the phone, or all of it with 0.
    func setKeepDays(_ account: AccountItem, days: Int) async throws {
        try await core.setAccountKeep(account: account.id, days: UInt32(days))
    }

    /// Removes the account and its mail from the phone, then its secret.
    func removeAccount(_ account: AccountItem) async throws {
        try core.removeAccount(account: account.id)
        vault.forgetPassword(email: account.email)
    }

    /// The newest `limit` conversations of a tab; fewer once the saved mail ends, see `older`.
    func inbox(_ tab: Category, limit: Int) async throws -> [MessageItem] {
        try core.unifiedInbox(tab: tab, limit: UInt32(limit))
    }

    /// A mailbox other than the unified inbox, which `inbox` shows by tab.
    func mailbox(_ mailbox: Mailbox, limit: Int) async throws -> [MessageItem] {
        try core.mailbox(mailbox: mailbox, limit: UInt32(limit))
    }

    /// Brings older mail of `mailbox` from every account's server, for a list scrolled to its
    /// end; false once none of them has more. The first failure is thrown once every one has
    /// finished.
    func older(_ mailbox: Mailbox) async throws -> Bool {
        try await eachAccount(mailbox) { core, account, password in
            try await core.olderMail(account: account.id, secret: password, mailbox: mailbox)
        }.contains(true)
    }

    /// Every account's folders; they show once the account's first sync has listed them.
    func folders() async throws -> [FolderItem] {
        try core.folders()
    }

    /// Unread mail in People that came since the last call, for a notification.
    func newMail() async throws -> [MessageItem] {
        try core.takeNewMail()
    }

    /// The mail synced so far is on screen, so it is not announced later.
    func forgetNewMail() async throws {
        try core.forgetNewMail()
    }

    /// The inbox tabs in order, with their unread counts.
    func tabs() async throws -> [InboxTab] {
        try core.inboxTabs()
    }

    /// Whether every inbox tab is empty.
    func inboxEmpty() async throws -> Bool {
        try core.inboxIsEmpty()
    }

    /// The core saves the account only after the server has accepted the login.
    /// The password goes first, so a saved account always has one; never over a saved account's.
    func signIn(email: String, password: String, imap: Endpoint, smtp: Endpoint) async throws {
        if try core.accounts().contains(where: { $0.email.lowercased() == email.lowercased() }) {
            throw CoreError.AccountExists
        }
        try vault.setPassword(password, email: email)
        _ = try await core.addImapAccount(
            email: email, displayName: email, imap: imap, smtp: smtp, password: password)
    }

    /// Like `signIn`, with the servers found from the email: IMAP at its domain's usual hosts,
    /// else Exchange. Fails with `NotFound` when neither answers.
    func signInFound(email: String, password: String) async throws {
        if try core.accounts().contains(where: { $0.email.lowercased() == email.lowercased() }) {
            throw CoreError.AccountExists
        }
        try vault.setPassword(password, email: email)
        _ = try await core.addFoundAccount(email: email, password: password)
    }

    /// Like `signIn`, for Exchange: the core finds the server from the email unless `server`
    /// is given, and signs in as the email unless `login` is.
    func signInExchange(
        email: String, password: String, server: String?, login: String?, certificate: String?,
        trusted: String? = nil
    ) async throws {
        if try core.accounts().contains(where: { $0.email.lowercased() == email.lowercased() }) {
            throw CoreError.AccountExists
        }
        try vault.setPassword(password, email: email)
        // No name: the core asks the server for the owner's.
        _ = try await core.addExchangeAccount(
            email: email, displayName: "", server: server, login: login, certificate: certificate,
            trusted: trusted, password: password)
    }

    /// Starts Google sign-in and returns the page to open in the browser; `again` signs in to
    /// that saved account, which Google then offers, as it does a typed `email`. A second tap opens the same sign-in, so
    /// whichever browser finishes it, the answer is one we expect.
    func startGoogle(again: AccountItem? = nil, email: String? = nil) async throws -> URL {
        let hint = again?.email ?? email
        let waiting = vault.googleSignIn().flatMap { start in
            URLComponents(string: start.url)?.queryItems?.first { $0.name == "login_hint" }?.value == hint ? start : nil
        }
        let start: GoogleSignIn
        if let waiting {
            start = waiting
        } else {
            start = try googleSignIn(email: hint)
            try vault.setGoogleSignIn(start)
        }
        guard let url = URL(string: start.url) else { throw CoreError.Auth }
        return url
    }

    /// Finishes Google sign-in with the address the browser came back to. Like `signIn`, the
    /// secret is kept before the account is saved. With `again`, the sign-in was for that saved
    /// account: its new secret replaces the old one and its mail stays.
    func finishGoogle(redirect: URL, again: AccountItem? = nil) async throws {
        // No sign-in under way: an old answer, say from a link opened again.
        guard let start = vault.googleSignIn() else { throw CoreError.Auth }
        let account = try await core.googleAccount(signIn: start, redirect: redirect.absoluteString, again: again?.id)
        try vault.setGoogleSignIn(nil)
        // Under the saved address, which Google may write in other letter case.
        try vault.setPassword(account.secret, email: again?.email ?? account.email)
        if let again {
            try core.signedInAgain(account: again.id, name: account.name)
        } else {
            _ = try core.addGmailAccount(email: account.email, name: account.name)
        }
    }

    /// A new password for a saved IMAP or Exchange account, kept once its servers accept it.
    func signInAgain(_ account: AccountItem, password: String) async throws {
        try await core.checkPassword(account: account.id, password: password)
        try vault.setPassword(password, email: account.email)
        try core.signedInAgain(account: account.id, name: nil)
    }

    /// The message body, downloaded once by the core.
    /// A missing password only matters for a download, which then fails to log in.
    func body(of message: MessageItem) async throws -> MessageBody {
        try await core.messageBody(message: message.id, secret: password(of: message.accountId) ?? "")
    }

    /// The attachments' files, those not here yet downloaded together, once a run, into the
    /// app's caches, where other apps may be given them.
    func attachments(of message: MessageItem, _ items: [AttachmentItem]) async throws -> [URL] {
        let untitled = String(localized: "attachment_untitled")
        let files = items.map { item in
            downloads.appending(path: "\(message.id)-\(item.part)", directoryHint: .isDirectory)
                .appending(path: item.fileName(untitled: untitled))
        }
        let missing = zip(items, files).filter { !FileManager.default.fileExists(atPath: $0.1.path(percentEncoded: false)) }
        if !missing.isEmpty {
            for (_, file) in missing {
                try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            }
            let targets = missing.map { AttachmentFile(part: $0.0.part, path: $0.1.path(percentEncoded: false)) }
            try await core.saveAttachments(message: message.id, files: targets, secret: password(of: message.accountId) ?? "")
        }
        return files
    }

    /// This run's downloaded attachments; those of earlier runs go, as an id may name another
    /// message by now.
    private let downloads = FileManager.default.temporaryDirectory
        .appending(path: "attachments", directoryHint: .isDirectory)
        .appending(path: UUID().uuidString, directoryHint: .isDirectory)

    private func forgetOldDownloads() {
        let parent = downloads.deletingLastPathComponent()
        let old = (try? FileManager.default.contentsOfDirectory(at: parent, includingPropertiesForKeys: nil)) ?? []
        for dir in old where dir.lastPathComponent != downloads.lastPathComponent {
            try? FileManager.default.removeItem(at: dir)
        }
    }

    /// Messages matching every word of `text`, newest first, with what `searchServer` found for it.
    func search(_ text: String) async throws -> [MessageItem] {
        try core.search(text: text, limit: Self.searchSize)
    }

    /// Searches every account's mail on its server for `text`, older mail than the phone keeps
    /// included; `search` then shows what was found. The first failure is thrown once all are done.
    func searchServer(_ text: String) async throws {
        // Back from a result, the same search has its answer saved already.
        if let last = state.searched, last.text == text, Date.now.timeIntervalSince(last.at) < Self.searchKeep {
            return
        }
        _ = try await eachAccount(Self.inbox) { core, account, password in
            try await core.searchServer(account: account.id, secret: password, text: text)
        }
        state.searched = (text, .now)
    }

    /// The message's conversation, oldest first.
    func thread(of message: MessageItem) async throws -> [MessageItem] {
        try core.thread(message: message.id)
    }

    /// The message `id`, if it is still saved: say the one a draft answers.
    func message(_ id: Int64) async throws -> MessageItem? {
        try core.thread(message: id).first { $0.id == id }
    }

    /// Marks the messages' conversations read, or the messages unread, on the device now and in
    /// one go, so the list shows it on return; the server hears about it in the background.
    func markRead(_ messages: [MessageItem], seen: Bool = true) async throws {
        try core.markRead(messages: messages.map(\.id), seen: seen)
        push(messages)
    }

    /// Takes the messages' conversations out of the inbox now, all or none, and returns the
    /// actions for `undo`. The server gets the move once the undo time is over.
    func archive(_ messages: [MessageItem]) async throws -> [Int64] {
        try move(messages) { try core.archive(messages: $0) }
    }

    /// As `archive`, back into the inbox: for mail in the archive, trash or spam.
    func moveToInbox(_ messages: [MessageItem]) async throws -> [Int64] {
        try move(messages) { try core.moveToInbox(messages: $0) }
    }

    /// As `archive`, into the account's snoozed folder; the conversations come back to the top of
    /// the inbox, unread, at `until`.
    func snooze(_ messages: [MessageItem], until: Date) async throws -> [Int64] {
        try move(messages) { try core.snooze(messages: $0, until: Int64(until.timeIntervalSince1970)) }
    }

    /// Brings back the snoozed mail whose time has come and returns it, to tell about; the server
    /// hears about it in the background.
    func wake() async throws -> [MessageItem] {
        let woken = try core.wake()
        push(woken)
        return woken
    }

    /// When the next snoozed message comes back; nil if none waits.
    func nextWake() async throws -> Date? {
        try core.nextWake().map { Date(timeIntervalSince1970: TimeInterval($0)) }
    }

    /// As `archive`, into the trash, or gone from the server for good for mail already there.
    func delete(_ messages: [MessageItem]) async throws -> [Int64] {
        let toTrash = messages.filter(\.canTrash).map(\.id)
        let forever = messages.filter { !$0.canTrash }.map(\.id)
        return try move(messages) { _ in try core.delete(messages: toTrash, forever: forever) }
    }

    /// Empties a trash or spam `mailbox` for good, mail not loaded included, held like `archive`.
    func emptyMailbox(_ mailbox: Mailbox) async throws -> [Int64] {
        let actions = try core.emptyMailbox(mailbox: mailbox)
        try pushAll()
        return actions
    }

    private func move(_ messages: [MessageItem], _ queue: ([Int64]) throws -> [Int64]) throws -> [Int64] {
        let actions = try queue(messages.map(\.id))
        push(messages)
        return actions
    }

    private func push(_ messages: [MessageItem]) {
        for account in Set(messages.map(\.accountId)) {
            push(account: account)
        }
    }

    /// `message`'s page with its remote images loaded; nil when it has none.
    func images(of message: MessageItem) async throws -> ImagesPage? {
        try await core.messageImages(message: message.id)
    }

    /// From now on the remote images of mail from `message`'s sender load on their own.
    func allowImages(of message: MessageItem) async throws {
        try core.allowImages(message: message.id)
    }

    /// A blank message from the first account, with its signature.
    func newDraft() async throws -> DraftItem {
        try core.newDraft()
    }

    /// An answer to `message`: to its sender, or with `all` to everyone it went to as well.
    func reply(to message: MessageItem, all: Bool) async throws -> DraftItem {
        try core.reply(message: message, all: all)
    }

    /// `message` to send on, its text under `labels` and `date`; loads the text if not saved.
    func forward(_ message: MessageItem, labels: ForwardLabels, date: String) async throws -> DraftItem {
        try await core.forward(message: message, labels: labels, date: date, secret: password(of: message.accountId) ?? "")
    }

    /// The text of `message` quoted under `header`; downloaded if it is not saved.
    func quote(_ message: MessageItem, header: String) async throws -> String {
        try await core.quote(message: message.id, header: header, secret: password(of: message.accountId) ?? "")
    }

    /// Takes back an archive that has not gone to the server yet; false if too late.
    func undo(_ actions: [Int64]) async throws -> Bool {
        try core.undo(actions: actions)
    }

    /// Puts the message in the outbox and returns it for `undoSend`.
    /// It goes to the server once the undo time is over.
    func send(_ draft: DraftItem) async throws -> Int64 {
        let id = try core.send(draft: draft)
        push(account: draft.accountId)
        return id
    }

    /// Keeps `draft` on the phone; a new one if its id is 0. Returns its id.
    func saveDraft(_ draft: DraftItem) async throws -> Int64 {
        try core.saveDraft(draft: draft)
    }

    /// Saves the drafts into each account's drafts folder too. On failure the next sync
    /// takes them.
    func pushDrafts() {
        try? pushAll()
    }

    /// The drafts on the phone, last saved first.
    func drafts() async throws -> [DraftItem] {
        try core.drafts()
    }

    func deleteDraft(_ id: Int64) async throws {
        try core.deleteDraft(id: id)
    }

    /// Brings back `draft` as it was thrown away, saved: with its files if `deleteDraft` was a
    /// moment ago.
    func restoreDraft(_ draft: DraftItem) async throws -> DraftItem {
        try core.restoreDraft(draft: draft)
    }

    /// Adds a file to the saved draft `draft`; it goes with the message.
    func attachFile(draft: Int64, _ file: PickedFile) async throws -> DraftFileItem {
        try core.attachFile(draft: draft, name: file.name, mime: file.mime, data: file.data)
    }

    /// Files of saved mail to send on, newest first.
    func recentFiles() async throws -> [RecentFileItem] {
        try core.recentFiles()
    }

    /// `file` of saved mail, downloaded now to send on.
    func downloadFile(_ file: RecentFileItem) async throws -> PickedFile {
        let data = try await core.downloadFile(message: file.message, part: file.part,
                                               secret: password(of: file.accountId) ?? "")
        return PickedFile(name: file.name, mime: file.mime, data: data)
    }

    /// Takes a file off its draft.
    func detachFile(_ file: Int64) async throws {
        try core.detachFile(file: file)
    }

    /// The messages the server refused, newest first; `undoSend` takes one back to fix it.
    func unsent() async throws -> [UnsentItem] {
        try core.unsent()
    }

    /// Takes back a message that has not gone out yet, to edit it again as a draft; nil if too
    /// late.
    func undoSend(_ id: Int64) async throws -> DraftItem? {
        try core.undoSend(id: id)
    }

    /// `push` for every account; one with nothing to send does not connect.
    private func pushAll() throws {
        for account in try core.accounts() {
            push(account: account.id)
        }
    }

    /// Sends the account's queued actions and mail, waiting for held ones, in a task no screen
    /// owns; iOS gives it time to finish if the app goes to the background meanwhile.

    private func push(account: Int64) {
        Task.detached {
            let finish = await Background.begin("push")
            defer { Task { await finish.end() } }
            do {
                guard let password = try self.password(of: account) else { return }
                try await self.core.pushActions(account: account, secret: password)
            } catch {
                // Still queued: the next push or sync sends it.
                log.error("push failed: \(String(describing: error), privacy: .public)")
            }
        }
    }

    /// Syncs `mailbox` in all accounts at once; the first failure is thrown once every one has
    /// finished. A refused login is not one: it marks its account. True if an account needs
    /// signing in again. Only the accounts `which` picks are synced.
    @discardableResult
    func refresh(
        _ mailbox: Mailbox = Mail.inbox, which: (AccountItem) -> Bool = { _ in true }
    ) async throws -> Bool {
        let checked = try await eachAccount(mailbox, which) { core, account, password -> Checked in
            do {
                try await core.syncMailbox(account: account.id, secret: password, mailbox: mailbox)
                return .done
            } catch {
                // A refused login marks its account, as `eachAccount` lets it; leaving the app
                // ends the check quietly.
                if error is CancellationError { throw error }
                if case CoreError.Auth? = error as? CoreError { throw error }
                return .failed(account.name, error)
            }
        }
        let failed = checked.compactMap { checked -> (name: String, error: any Error)? in
            guard case .failed(let name, let error) = checked else { return nil }
            return (name, error)
        }
        if let first = failed.first {
            // Named only while another account got through, and only those that failed the way
            // the error shown says: else the whole phone is offline.
            guard failed.count < checked.count else { throw first.error }
            let key = first.error.textKey()
            throw PartlyFailed(error: first.error, accounts: failed.filter { $0.error.textKey() == key }.map(\.name))
        }
        return try core.accounts().contains(where: \.needsSignIn)
    }

    /// Runs `call` at once for every account with mail in `mailbox`, but those that need signing
    /// in again, and returns what they gave; the first failure but a refused login is thrown once
    /// every one has finished.
    private func eachAccount<T: Sendable>(
        _ mailbox: Mailbox, _ which: (AccountItem) -> Bool = { _ in true },
        _ call: @escaping @Sendable (Core, AccountItem, String) async throws -> T
    ) async throws -> [T] {
        // A folder is one account's, so only that account is asked.
        var owner: Int64?
        if case .folder(let id) = mailbox {
            owner = try core.folders().first { $0.id == id }?.accountId
        }
        let signedIn = try core.accounts().filter { (owner == nil || $0.id == owner) && !$0.needsSignIn && which($0) }
            .compactMap { account in vault.passwordIfAny(email: account.email).map { (account, $0) } }
        let core = self.core
        let results = await withTaskGroup(of: Result<T, Error>.self) { group in
            for (account, password) in signedIn {
                group.addTask {
                    do {
                        return .success(try await call(core, account, password))
                    } catch {
                        return .failure(error)
                    }
                }
            }
            var all: [Result<T, Error>] = []
            for await result in group {
                all.append(result)
            }
            return all
        }
        for case .failure(let error) in results {
            if case CoreError.Auth? = error as? CoreError { continue }
            throw error
        }
        return results.compactMap { try? $0.get() }
    }

    /// Downloads the newest inbox mail whole in the background, so it opens at once. Only for
    /// the app on screen: background checks stay light.
    func prefetch() {
        // One at a time: a second would download the same mail.
        guard state.startPrefetch() else { return }
        Task.detached {
            defer { self.state.endPrefetch() }
            for account in (try? self.core.accounts()) ?? [] {
                guard let password = self.vault.passwordIfAny(email: account.email) else { continue }
                do {
                    try await self.core.prefetch(account: account.id, secret: password)
                } catch {
                    // The reader downloads what is left when a message opens.
                    log.error("prefetch failed: \(String(describing: error), privacy: .public)")
                }
            }
        }
    }

    /// Calls `onMail` each time an account's inbox may have changed, until cancelled: for the
    /// app on screen. `onMail` says whether its sync went through.
    func watch(_ accounts: [AccountItem], onMail: @escaping @Sendable () async -> Bool) async {
        await withTaskGroup(of: Void.self) { group in
            // One signed out would only be refused again and again.
            for account in accounts where !account.needsSignIn {
                group.addTask {
                    guard let password = self.vault.passwordIfAny(email: account.email) else { return }
                    var pause = Self.watchPause
                    var failed = false
                    while !Task.isCancelled {
                        // After a failed wait a sync comes first, until one goes through: mail may
                        // have come meanwhile, and the error on screen goes.
                        if failed { failed = !(await onMail()) }
                        let started = ContinuousClock.now
                        var fine: Bool
                        do {
                            fine = try await !self.core.waitForMail(account: account.id, secret: password)
                            if !fine { fine = await onMail() }
                        } catch {
                            log.error("wait for mail failed: \(String(describing: error), privacy: .public)")
                            failed = true
                            fine = false
                        }
                        // A failure, or a server that keeps waking at once, waits longer each time.
                        let quick = ContinuousClock.now - started < Self.watchPause
                        pause = fine && !quick ? Self.watchPause : min(pause * 2, Self.watchPauseMax)
                        try? await Task.sleep(for: pause)
                    }
                }
            }
        }
    }

    private func password(of account: Int64) throws -> String? {
        try core.accounts().first { $0.id == account }.flatMap { vault.passwordIfAny(email: $0.email) }
    }
}

/// What a `Mail` keeps between calls, behind a lock, as `Mail` is shared across tasks.
private final class MailState: @unchecked Sendable {
    private let lock = NSLock()
    private var _searched: (text: String, at: Date)?
    private var prefetching = false

    /// The last server search that went through, and when.
    var searched: (text: String, at: Date)? {
        get { lock.withLock { _searched } }
        set { lock.withLock { _searched = newValue } }
    }

    /// False if one runs already.
    func startPrefetch() -> Bool {
        lock.withLock {
            if prefetching { return false }
            prefetching = true
            return true
        }
    }

    func endPrefetch() {
        lock.withLock { prefetching = false }
    }
}

/// Opens the database once, however many callers ask at the same time.
private actor Opener {
    private var opened: Mail?

    func mail() throws -> Mail {
        if let opened { return opened }
        let mail = try Mail.open()
        opened = mail
        return mail
    }
}

private let log = Logger(subsystem: "com.ozeromail.app", category: "core")

extension Error {
    /// The string to show; the details only go to the log.
    func text() -> LocalizedStringKey {
        LocalizedStringKey(textKey())
    }

    /// The key of the string to show; the details only go to the log.
    func textKey() -> String {
        log.error("core error: \(String(describing: self), privacy: .public)")
        switch self as? CoreError {
        case .Auth?: return "error_auth"
        case .AccountExists?: return "error_account_exists"
        case .WrongAccount?: return "error_wrong_account"
        case .Network?: return "error_network"
        case .Tls?: return "error_tls"
        case .Server?: return "error_server"
        case .Gone?: return "error_gone"
        case .NoArchive?: return "error_no_archive"
        case .NoTrash?: return "error_no_trash"
        case .NoSnoozed?: return "error_no_snoozed"
        case .BadRecipients?: return "error_bad_recipients"
        case .BadCopies?: return "error_bad_copies"
        case .NoSmtp?: return "error_no_smtp"
        case .NoServer?: return "error_no_server"
        case .NotFound?: return "error_server_not_found"
        case .NoEws?: return "error_no_ews"
        case .CertificateNeeded?: return "error_certificate_needed"
        case .UntrustedCertificate?: return "error_untrusted_certificate"
        case .DeviceBlocked?: return "error_device_blocked"
        case .TooLarge?: return "compose_files_too_large"
        default: return "error_storage"
        }
    }

    /// What a connection or server error said, for a failed sign-in; nil for the others.
    var detail: String? {
        switch self as? CoreError {
        case .Network(let detail)?, .Tls(let detail)?, .Server(let detail)?: return detail
        // Who refused it and why, and what to compare with the IT team's.
        case .UntrustedCertificate(let host, let fingerprint, let reason)?:
            return "\(host): \(reason)\nSHA-256 \(fingerprint)"
        default: return nil
        }
    }

    /// Whether the network was not there, as opposed to a server that answered.
    var isNetwork: Bool {
        if case .Network? = self as? CoreError { return true }
        return false
    }
}

extension AttachmentItem {
    /// A name safe to save under, with an extension when the sender gave none.
    func fileName(untitled: String) -> String {
        let unsafe = CharacterSet(charactersIn: "\\/:*?\"<>|").union(.controlCharacters)
        let clean = String(String.UnicodeScalarView(name.unicodeScalars.map { unsafe.contains($0) ? "_" : $0 }))
            .trimmingCharacters(in: CharacterSet(charactersIn: ". "))
        if clean.isEmpty {
            guard let ext = UTType(mimeType: mime)?.preferredFilenameExtension else { return untitled }
            return "\(untitled).\(ext)"
        }
        let maxBytes = 200
        if clean.utf8.count <= maxBytes { return clean }
        // File systems cap a name at 255 bytes; keep the extension so apps still know the type.
        let ext = (clean as NSString).pathExtension
        let suffix = !ext.isEmpty && ext.count < 10 ? ".\(ext)" : ""
        var stem = String(clean.dropLast(suffix.count))
        while (stem + suffix).utf8.count > maxBytes {
            stem.removeLast()
        }
        return stem.trimmingCharacters(in: CharacterSet(charactersIn: ". ")) + suffix
    }
}

extension Category {
    /// Every tab, in the order the core lists them; shown before the counts arrive.
    static let all: [Category] = [.people, .notifications, .newsletters]

    var label: LocalizedStringKey {
        switch self {
        case .people: "tab_people"
        case .notifications: "tab_notifications"
        case .newsletters: "tab_newsletters"
        }
    }
}

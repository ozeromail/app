import Foundation
import SwiftUI
import os

/// What the screens need from the core. The async calls run off the main actor.
final class Mail: Sendable {
    static let imapsPort: UInt16 = 993
    static let smtpsPort: UInt16 = 465
    private static let inboxSize: UInt32 = 200

    private let core: Core
    private let vault = Vault()

    private init(core: Core) {
        self.core = core
    }

    /// The one instance the screens and background checks share, opened on first use.
    static func shared() async throws -> Mail {
        try await opened.mail()
    }

    private static let opened = Opener()

    fileprivate static func open() throws -> Mail {
        var dir = try FileManager.default
            .url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appending(path: "Mail", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        // The key lives in this device's keychain only, so a backup of the database could never be opened.
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try dir.setResourceValues(values)
        let path = dir.appending(path: "mail.db").path(percentEncoded: false)
        return Mail(core: try Core.open(path: path, key: Vault().databaseKey()))
    }

    func hasAccounts() async throws -> Bool {
        try !core.accounts().isEmpty
    }

    /// The accounts mail can be sent from, in the order they were added.
    func accounts() async throws -> [AccountItem] {
        try core.accounts()
    }

    func inbox(_ tab: Category) async throws -> [MessageItem] {
        try core.unifiedInbox(tab: tab, limit: Self.inboxSize)
    }

    /// Unread mail in People that came since the last call, for a notification. The inbox calls
    /// it too, so mail already seen there is not announced later.
    func newMail() async throws -> [MessageItem] {
        try core.takeNewMail()
    }

    /// The inbox tabs in order, with their unread counts.
    func tabs() async throws -> [InboxTab] {
        try core.inboxTabs()
    }

    /// The core saves the account only after the server has accepted the login.
    /// The password goes first, so a saved account always has one.
    func signIn(email: String, password: String, imap: Endpoint, smtp: Endpoint) async throws {
        try vault.setPassword(password, email: email)
        _ = try await core.addImapAccount(
            email: email, displayName: email, imap: imap, smtp: smtp, password: password)
    }

    /// The message body, downloaded once by the core.
    /// A missing password only matters for a download, which then fails to log in.
    func body(of message: MessageItem) async throws -> MessageBody {
        let password = try password(of: message.accountId) ?? ""
        return try await core.messageBody(message: message.id, password: password)
    }

    /// Marks the message read on the device now, so the list shows it on return;
    /// the server hears about it in the background.
    func markRead(_ message: MessageItem) throws {
        try core.markRead(message: message.id, seen: true)
        push(account: message.accountId)
    }

    /// Takes the message out of the inbox now and returns the action for `undo`.
    /// The server gets the move once the undo time is over.
    func archive(_ message: MessageItem) throws -> Int64 {
        let action = try core.archive(message: message.id)
        push(account: message.accountId)
        return action
    }

    /// Takes back an archive that has not gone to the server yet; false if too late.
    func undo(_ action: Int64) throws -> Bool {
        try core.undo(action: action)
    }

    /// Puts the message in the outbox and returns it for `undoSend`.
    /// It goes to the server once the undo time is over.
    func send(_ draft: DraftItem) async throws -> Int64 {
        let id = try core.send(
            account: draft.accountId, to: draft.to, subject: draft.subject, text: draft.text,
            replyTo: draft.replyTo)
        push(account: draft.accountId)
        return id
    }

    /// The messages the server refused, newest first; `undoSend` takes one back to fix it.
    func unsent() async throws -> [UnsentItem] {
        try core.unsent()
    }

    /// Takes back a message that has not gone out yet, to edit it again; nil if too late.
    func undoSend(_ id: Int64) async throws -> DraftItem? {
        try core.undoSend(id: id)
    }

    /// Sends the account's queued actions, waiting for held ones, in a task no screen owns.
    private func push(account: Int64) {
        Task.detached {
            do {
                guard let password = try self.password(of: account) else { return }
                try await self.core.pushActions(account: account, password: password)
            } catch {
                // Still queued: the next push or sync sends it.
                log.error("push failed: \(String(describing: error), privacy: .public)")
            }
        }
    }

    private func password(of account: Int64) throws -> String? {
        let account = try core.accounts().first { $0.id == account }
        return account.flatMap { try? vault.password(email: $0.email) }
    }

    /// Syncs all accounts at once; the first failure is thrown once every one has finished.
    func refresh() async throws {
        let accounts = try core.accounts()
        // The group waits for every sync before it returns, failed or not.
        let failure = await withTaskGroup(of: Error?.self) { group in
            for account in accounts {
                group.addTask {
                    do {
                        let password = try self.vault.password(email: account.email)
                        try await self.core.syncImap(account: account.id, password: password)
                        return nil
                    } catch {
                        return error
                    }
                }
            }
            var first: Error?
            for await case let failure? in group where first == nil {
                first = failure
            }
            return first
        }
        if let failure { throw failure }
    }
}

/// Opens the database once, however many callers ask at the same time.
private actor Opener {
    private var mail: Mail?

    func mail() throws -> Mail {
        if let mail { return mail }
        let opened = try Mail.open()
        mail = opened
        return opened
    }
}

private let log = Logger(subsystem: "com.ozeromail.app", category: "core")

extension Error {
    /// The string to show; the details only go to the log.
    func text() -> LocalizedStringKey {
        log.error("core error: \(String(describing: self), privacy: .public)")
        switch self as? CoreError {
        case .AccountExists?: return "error_account_exists"
        case .Auth?: return "error_auth"
        case .Network?: return "error_network"
        case .Tls?: return "error_tls"
        case .Server?: return "error_server"
        case .Gone?: return "error_gone"
        case .NoArchive?: return "error_no_archive"
        case .BadRecipients?: return "error_bad_recipients"
        case .NoSmtp?: return "error_no_smtp"
        default: return "error_storage"
        }
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

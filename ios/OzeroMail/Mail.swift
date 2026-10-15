import Foundation
import SwiftUI
import os

/// What the screens need from the core. The async calls run off the main actor.
final class Mail: Sendable {
    static let imapsPort: UInt16 = 993
    private static let inboxSize: UInt32 = 200

    private let core: Core
    private let vault = Vault()

    private init(core: Core) {
        self.core = core
    }

    static func open() async throws -> Mail {
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

    func inbox() async throws -> [MessageItem] {
        try core.unifiedInbox(limit: Self.inboxSize)
    }

    /// The core saves the account only after the server has accepted the login.
    /// The password goes first, so a saved account always has one.
    func signIn(email: String, password: String, host: String, port: UInt16) async throws {
        try vault.setPassword(password, email: email)
        _ = try await core.addImapAccount(
            email: email, displayName: email, host: host, port: port, password: password)
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
        default: return "error_storage"
        }
    }
}

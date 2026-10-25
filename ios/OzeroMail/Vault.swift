import Foundation
import Security

/// Secrets in the keychain, readable after the first unlock and never synced or moved to another device.
struct Vault: Sendable {
    struct Failure: Error {
        let status: OSStatus
    }

    /// The database key, made on first run.
    func databaseKey() throws -> Data {
        if let key = try read("database") { return key }
        var bytes = [UInt8](repeating: 0, count: 32)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        guard status == errSecSuccess else { throw Failure(status: status) }
        let key = Data(bytes)
        try write("database", key)
        return key
    }

    /// Keyed by email, which is unique per account and known before the account is saved.
    func password(email: String) throws -> String {
        guard let data = try read(passwordKey(email)) else {
            throw Failure(status: errSecItemNotFound)
        }
        return String(decoding: data, as: UTF8.self)
    }

    /// The password, or nil when there is none or the keychain cannot be read.
    func passwordIfAny(email: String) -> String? {
        try? password(email: email)
    }

    func setPassword(_ password: String, email: String) throws {
        try write(passwordKey(email), Data(password.utf8))
    }

    func forgetPassword(email: String) {
        SecItemDelete(item(passwordKey(email)) as CFDictionary)
    }

    private func passwordKey(_ email: String) -> String {
        "password.\(email.lowercased())"
    }

    /// The Google sign-in waiting for the browser, kept in case iOS closes the app meanwhile.
    func googleSignIn() -> GoogleSignIn? {
        guard let data = try? read(Self.google) else { return nil }
        let parts = String(decoding: data, as: UTF8.self).components(separatedBy: "\n")
        return parts.count == 3 ? GoogleSignIn(url: parts[0], verifier: parts[1], state: parts[2]) : nil
    }

    func setGoogleSignIn(_ signIn: GoogleSignIn?) throws {
        guard let signIn else {
            SecItemDelete(item(Self.google) as CFDictionary)
            return
        }
        try write(Self.google, Data([signIn.url, signIn.verifier, signIn.state].joined(separator: "\n").utf8))
    }

    private static let google = "google.signin"

    private func item(_ name: String) -> [CFString: Any] {
        [kSecClass: kSecClassGenericPassword, kSecAttrService: "com.ozeromail.app", kSecAttrAccount: name]
    }

    private func read(_ name: String) throws -> Data? {
        var query = item(name)
        query[kSecReturnData] = true
        query[kSecMatchLimit] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw Failure(status: status) }
        return result as? Data
    }

    private func write(_ name: String, _ value: Data) throws {
        SecItemDelete(item(name) as CFDictionary)
        var attributes = item(name)
        attributes[kSecValueData] = value
        attributes[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(attributes as CFDictionary, nil)
        guard status == errSecSuccess else { throw Failure(status: status) }
    }
}

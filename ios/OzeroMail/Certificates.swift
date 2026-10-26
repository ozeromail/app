import Foundation
import Security

/// Client certificates the person added from a .p12 file, kept in this app's keychain under
/// their certificate's name. The key stays there: the core only asks it to sign.
final class KeychainCertificates: ClientCertificates, @unchecked Sendable {
    struct Unreadable: Error {}

    func chain(name: String) -> [Data] {
        guard let leaf = Self.identity(name).flatMap(Self.certificate) else { return [] }
        return [SecCertificateCopyData(leaf) as Data] + Self.intermediates(name)
    }

    func sign(name: String, signing: Signing, data: Data) -> Data? {
        guard let identity = Self.identity(name) else { return nil }
        var key: SecKey?
        guard SecIdentityCopyPrivateKey(identity, &key) == errSecSuccess, let key else { return nil }
        return SecKeyCreateSignature(key, signing.algorithm, data as CFData, nil) as Data?
    }

    /// Keeps the identity in a .p12 file, with the certificates that vouch for it, and returns
    /// the name it goes by. One with the same name, such as the one it renews, goes.
    static func add(p12: Data, password: String) throws -> String {
        var items: CFArray?
        let options = [kSecImportExportPassphrase: password] as CFDictionary
        guard SecPKCS12Import(p12 as CFData, options, &items) == errSecSuccess,
              let item = (items as? [[String: Any]])?.first,
              let found = item[kSecImportItemIdentity as String],
              CFGetTypeID(found as CFTypeRef) == SecIdentityGetTypeID()
        else { throw Unreadable() }
        let identity = found as! SecIdentity
        guard let leaf = certificate(identity), let name = subject(leaf) else { throw Unreadable() }
        if let old = self.identity(name) {
            SecItemDelete([kSecValueRef: old] as CFDictionary)
        }
        // Readable in the background while the phone is locked, as the passwords in `Vault` are.
        let status = SecItemAdd(
            [kSecValueRef: identity, kSecAttrAccessible: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]
                as CFDictionary, nil)
        guard status == errSecSuccess || status == errSecDuplicateItem else {
            throw Vault.Failure(status: status)
        }
        let leafData = SecCertificateCopyData(leaf) as Data
        let chain = (item[kSecImportItemCertChain as String] as? [SecCertificate] ?? [])
            .map { SecCertificateCopyData($0) as Data }
            .filter { $0 != leafData }
        try keepIntermediates(chain, name: name)
        return name
    }

    private static let chains = "certificate-chain"

    private static func intermediates(_ name: String) -> [Data] {
        var result: CFTypeRef?
        let query = [
            kSecClass: kSecClassGenericPassword, kSecAttrService: chains, kSecAttrAccount: name,
            kSecReturnData: true,
        ] as CFDictionary
        guard SecItemCopyMatching(query, &result) == errSecSuccess, let data = result as? Data else { return [] }
        return (try? PropertyListDecoder().decode([Data].self, from: data)) ?? []
    }

    private static func keepIntermediates(_ chain: [Data], name: String) throws {
        let item = [kSecClass: kSecClassGenericPassword, kSecAttrService: chains, kSecAttrAccount: name] as CFDictionary
        SecItemDelete(item)
        var attributes = [
            kSecClass: kSecClassGenericPassword, kSecAttrService: chains, kSecAttrAccount: name,
            kSecAttrAccessible: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ] as [CFString: Any]
        attributes[kSecValueData] = try PropertyListEncoder().encode(chain)
        let status = SecItemAdd(attributes as CFDictionary, nil)
        guard status == errSecSuccess else { throw Vault.Failure(status: status) }
    }

    private static func identity(_ name: String) -> SecIdentity? {
        var result: CFTypeRef?
        let query = [kSecClass: kSecClassIdentity, kSecMatchLimit: kSecMatchLimitAll, kSecReturnRef: true] as CFDictionary
        guard SecItemCopyMatching(query, &result) == errSecSuccess,
              let all = result as? [SecIdentity] else { return nil }
        return all.first { certificate($0).flatMap(subject) == name }
    }

    private static func certificate(_ identity: SecIdentity) -> SecCertificate? {
        var certificate: SecCertificate?
        return SecIdentityCopyCertificate(identity, &certificate) == errSecSuccess ? certificate : nil
    }

    private static func subject(_ certificate: SecCertificate) -> String? {
        SecCertificateCopySubjectSummary(certificate) as String?
    }
}

private extension Signing {
    var algorithm: SecKeyAlgorithm {
        switch self {
        case .rsaPkcs1Sha256: .rsaSignatureMessagePKCS1v15SHA256
        case .rsaPkcs1Sha384: .rsaSignatureMessagePKCS1v15SHA384
        case .rsaPkcs1Sha512: .rsaSignatureMessagePKCS1v15SHA512
        case .rsaPssSha256: .rsaSignatureMessagePSSSHA256
        case .rsaPssSha384: .rsaSignatureMessagePSSSHA384
        case .rsaPssSha512: .rsaSignatureMessagePSSSHA512
        case .ecdsaSha256: .ecdsaSignatureMessageX962SHA256
        case .ecdsaSha384: .ecdsaSignatureMessageX962SHA384
        case .ecdsaSha512: .ecdsaSignatureMessageX962SHA512
        }
    }
}

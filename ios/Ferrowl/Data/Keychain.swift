import Foundation
import Security

/// Secrets (passwords, OpenPGP passphrases, sign-in tokens) in the iOS
/// Keychain. They stay on this device: not in backups, not synced through
/// iCloud, readable once the phone was unlocked after a restart (so the
/// background refresh can use them).
enum Keychain {
    private static func query(_ service: String, _ key: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
        ]
    }

    static func get(_ service: String, _ key: String) -> String? {
        var q = query(service, key)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let data = out as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    @discardableResult
    static func set(_ service: String, _ key: String, _ secret: String) -> Bool {
        let data = Data(secret.utf8)
        let q = query(service, key)
        let update: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemUpdate(q as CFDictionary, update as CFDictionary)
        if status == errSecSuccess { return true }
        guard status == errSecItemNotFound else { return false }
        var add = q
        add.merge(update) { $1 }
        return SecItemAdd(add as CFDictionary, nil) == errSecSuccess
    }

    static func delete(_ service: String, _ key: String) {
        SecItemDelete(query(service, key) as CFDictionary)
    }
}

/// Where the engine keeps sign-in (refresh) tokens of Microsoft accounts.
/// Called by the engine from background threads; the Keychain is
/// thread-safe.
final class KeychainTokenVault: TokenVault, @unchecked Sendable {
    private let service = "com.ferrowl.app.tokens"

    func get(key: String) -> String? { Keychain.get(service, key) }

    func set(key: String, secret: String) -> Bool { Keychain.set(service, key, secret) }

    func delete(key: String) { Keychain.delete(service, key) }
}

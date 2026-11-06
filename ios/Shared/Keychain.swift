import Foundation
import Security

/// Secrets (passwords, OpenPGP passphrases, sign-in tokens) in the iOS
/// Keychain. They stay on this device: not in backups, not synced through
/// iCloud, readable once the phone was unlocked after a restart (so the
/// background refresh and the notification extension can use them).
/// They belong to the App Group, shared with the notification extension.
enum Keychain {
    static let services = ["com.ferrowl.app.accounts", "com.ferrowl.app.tokens", "com.ferrowl.app.push"]

    private static func query(_ service: String, _ key: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key,
            kSecAttrAccessGroup as String: SharedStorage.group,
        ]
    }

    /// Items saved by versions before the App Group (the app's own group):
    /// moved to the shared one.
    static func moveToSharedGroup() {
        for service in services {
            let q: [String: Any] = [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: service,
                kSecMatchLimit as String: kSecMatchLimitAll,
                kSecReturnAttributes as String: true,
                kSecReturnData as String: true,
            ]
            var out: CFTypeRef?
            guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess,
                  let items = out as? [[String: Any]] else { continue }
            for item in items {
                guard (item[kSecAttrAccessGroup as String] as? String) != SharedStorage.group,
                      let key = item[kSecAttrAccount as String] as? String,
                      let data = item[kSecValueData as String] as? Data,
                      let secret = String(data: data, encoding: .utf8)
                else { continue }
                if set(service, key, secret) {
                    var old: [String: Any] = [
                        kSecClass as String: kSecClassGenericPassword,
                        kSecAttrService as String: service,
                        kSecAttrAccount as String: key,
                    ]
                    if let group = item[kSecAttrAccessGroup as String] { old[kSecAttrAccessGroup as String] = group }
                    SecItemDelete(old as CFDictionary)
                }
            }
        }
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

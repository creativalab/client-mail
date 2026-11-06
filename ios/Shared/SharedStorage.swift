import Foundation

/// What the app shares with its notification extension (which reads the
/// new message to show its sender and subject): the App Group
/// `group.com.ferrowl.app`, holding the accounts file, the mail cache and
/// the settings both need, and the Keychain items of the same group.
enum SharedStorage {
    static let group = "group.com.ferrowl.app"

    /// Settings read by both (push registration, newest uid notified).
    static let defaults = UserDefaults(suiteName: group) ?? .standard

    /// The shared folder (the app's own Application Support if the group is
    /// missing, e.g. a build signed without it).
    static let directory: URL = {
        let fm = FileManager.default
        let own = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let dir = fm.containerURL(forSecurityApplicationGroupIdentifier: group)?
            .appendingPathComponent("Library/Application Support", isDirectory: true) ?? own
        try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        // The cache is rebuilt from the server: no need to back it up.
        var url = dir
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? url.setResourceValues(values)
        return dir
    }()

    /// The engine's files (cache, POP3 mailbox, OpenPGP keys).
    static var mail: URL {
        let dir = directory.appendingPathComponent("mail", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Versions before the extension kept everything in the app's own
    /// folder, settings and Keychain: moved to the shared ones once.
    static func migrate() {
        guard !defaults.bool(forKey: "shared.migrated") else { return }
        let fm = FileManager.default
        let old = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        if old.standardizedFileURL != directory.standardizedFileURL {
            for name in ["accounts.json", "mail"] {
                let from = old.appendingPathComponent(name)
                let to = directory.appendingPathComponent(name)
                if fm.fileExists(atPath: from.path), !fm.fileExists(atPath: to.path) {
                    try? fm.moveItem(at: from, to: to)
                }
            }
        }
        let standard = UserDefaults.standard
        for key in ["push.enabled", "push.accounts"] {
            if let value = standard.object(forKey: key), defaults.object(forKey: key) == nil {
                defaults.set(value, forKey: key)
            }
        }
        Keychain.moveToSharedGroup()
        defaults.set(true, forKey: "shared.migrated")
    }
}

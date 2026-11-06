import Foundation
import UserNotifications

/// The number on the app icon: unread messages in the inboxes of every
/// account. Each account's count is kept in the shared settings, written
/// by the app when it loads an inbox and by the notification extension
/// when new mail arrives.
enum Badge {
    private static func key(_ email: String) -> String { "unread:\(email.lowercased())" }

    /// The total, after saving one account's count.
    @discardableResult
    static func set(_ unread: Int, for email: String) -> Int {
        let d = SharedStorage.defaults
        d.set(unread, forKey: key(email))
        return total()
    }

    static func total() -> Int {
        SharedStorage.defaults.dictionaryRepresentation()
            .filter { $0.key.hasPrefix("unread:") }
            .compactMap { $0.value as? Int }
            .reduce(0, +)
    }

    /// Forgets a removed account.
    static func forget(_ email: String) {
        SharedStorage.defaults.removeObject(forKey: key(email))
    }

    /// Shows the total on the icon (the app; the extension sets it in the
    /// notification it shows).
    static func show() {
        UNUserNotificationCenter.current().setBadgeCount(total())
    }
}

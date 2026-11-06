import Foundation
import UserNotifications

/// Runs when a push from Ferrowl's server arrives ("New mail" for an
/// account id): reads the account's inbox here on the phone and shows the
/// sender and subject of the new message (several: how many, and the
/// newest), with the unread count on the app icon. The server never sees
/// any of this. If reading takes too long or fails, the plain "New mail"
/// is shown.
final class NotificationService: UNNotificationServiceExtension {
    private var deliver: ((UNNotificationContent) -> Void)?
    private var content: UNMutableNotificationContent?
    private var work: Task<Void, Never>?

    override func didReceive(_ request: UNNotificationRequest,
                             withContentHandler handler: @escaping (UNNotificationContent) -> Void) {
        deliver = handler
        let content = (request.content.mutableCopy() as? UNMutableNotificationContent) ?? UNMutableNotificationContent()
        self.content = content
        let accountId = request.content.userInfo["account"] as? String
        work = Task {
            await Self.fill(content, accountId: accountId)
            self.finish()
        }
    }

    override func serviceExtensionTimeWillExpire() {
        work?.cancel()
        finish()
    }

    private func finish() {
        guard let deliver, let content else { return }
        self.deliver = nil
        deliver(content)
    }

    private static func fill(_ content: UNMutableNotificationContent, accountId: String?) async {
        // Which account: the server sends the id it gave the account.
        let ids = SharedStorage.defaults.data(forKey: "push.accounts")
            .flatMap { try? JSONDecoder().decode([String: String].self, from: $0) } ?? [:]
        guard let accountId, let email = ids.first(where: { $0.value == accountId })?.key else { return }
        let store = AccountStore(directory: SharedStorage.directory)
        guard let account = store.load().accounts.first(where: { $0.email.lowercased() == email }) else { return }
        content.subtitle = account.email
        content.threadIdentifier = account.email

        setTokenVault(vault: KeychainTokenVault())
        let engine = Engine(dataDir: SharedStorage.mail.path, language: Language.code)
        do {
            let folders = try await engine.connect(account: account)
            let inbox = folders.first { $0.kind == .inbox }?.id ?? "INBOX"
            let headers = try await engine.headers(folderId: inbox).headers
            let unread = try? await engine.unread(folderIds: [inbox])[inbox]
            if let n = Badge.unread(server: unread, headers: headers, gmail: Badge.isGmail(account)) {
                content.badge = NSNumber(value: Badge.set(n, for: account.email))
            }
            // The same memory as the app's background check: each message once.
            let key = "lastUid:\(account.email.lowercased())"
            let last = UInt32(SharedStorage.defaults.integer(forKey: key))
            let newest = headers.map(\.uid).max() ?? last
            SharedStorage.defaults.set(Int(max(newest, last)), forKey: key)
            // First time for this account: no memory of what was already
            // announced, so only the newest unread message is shown.
            let fresh = last == 0
                ? Array(headers.filter { !$0.seen }.sorted { $0.uid > $1.uid }.prefix(1))
                : headers.filter { $0.uid > last && !$0.seen }.sorted { $0.uid > $1.uid }
            guard let first = fresh.first else { return }
            // Tapping it opens this message.
            content.userInfo["email"] = account.email
            content.userInfo["uid"] = NSNumber(value: first.uid)
            let sender = first.from.isEmpty ? first.fromAddress : first.from
            let subject = first.subject.isEmpty ? t("no_subject") : first.subject
            if fresh.count == 1 {
                content.title = sender
                content.body = subject
            } else {
                content.title = t("notif_many", ["n": "\(fresh.count)"])
                content.body = "\(sender): \(subject)"
            }
        } catch {
            // The plain "New mail" stays.
        }
    }
}

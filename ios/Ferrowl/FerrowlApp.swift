import SwiftUI
import UserNotifications

/// Everything shared by the screens: one per process.
@MainActor
@Observable
final class AppModel {
    let engine: Engine
    let notices = Notices()
    let prefs = Preferences()
    let pro = ProStore()
    let push = PushRegistrar()
    let mail: MailRepository
    /// A second engine on the same files, to check the accounts that are
    /// not the active one without disconnecting it.
    let side: Engine
    private let banners = ForegroundBanners()

    init() {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let data = support.appendingPathComponent("mail", isDirectory: true)
        try? FileManager.default.createDirectory(at: data, withIntermediateDirectories: true)
        // The cache is rebuilt from the server: no need to back it up.
        var dir = support
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? dir.setResourceValues(values)

        // Sign-in tokens go to the Keychain.
        setTokenVault(vault: KeychainTokenVault())
        engine = Engine(dataDir: data.path, language: Language.code)
        side = Engine(dataDir: data.path, language: Language.code)
        UNUserNotificationCenter.current().delegate = banners
        mail = MailRepository(engine: engine, notices: notices, directory: support)
    }

    /// Whether an account may be used: the first is free, the others need
    /// Ferrowl Pro.
    func mayUse(_ email: String) -> Bool {
        pro.pro || mail.accounts.first?.email.caseInsensitiveCompare(email) == .orderedSame
    }

    /// Whether another account may be added (Ferrowl Pro after the first).
    var mayAddAccount: Bool { pro.pro || mail.accounts.isEmpty }
}

/// New-mail notifications also show while the app is open.
final class ForegroundBanners: NSObject, UNUserNotificationCenterDelegate {
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification,
                                withCompletionHandler done: @escaping (UNNotificationPresentationOptions) -> Void) {
        done([.banner, .list, .sound])
    }
}

@main
struct FerrowlApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var app: AppModel
    @Environment(\.scenePhase) private var phase

    init() {
        let app = AppModel()
        _app = State(initialValue: app)
        BackgroundRefresh.register(app)
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .onAppear { delegate.push = app.push }
                .environment(app)
                .environment(app.mail)
                .environment(app.notices)
                .environment(app.prefs)
                .environment(app.pro)
                .tint(Theme.current(app.prefs.theme).accent)
                .preferredColorScheme(Theme.current(app.prefs.theme).scheme)
        }
        .onChange(of: phase) { _, now in
            switch now {
            case .active:
                // Ferrowl Pro may have been bought or cancelled elsewhere meanwhile.
                app.pro.start()
            case .background:
                if app.prefs.notifications, app.mail.activeEmail != nil { BackgroundRefresh.schedule() }
            default:
                break
            }
        }
    }
}

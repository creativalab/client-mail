import SwiftUI

/// Account setup the first time, the mailbox after; notices on top of both.
struct RootView: View {
    @Environment(AppModel.self) private var app
    @Environment(MailRepository.self) private var mail
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        Group {
            if mail.accounts.isEmpty {
                NavigationStack { SetupView(first: true) }
            } else {
                MainView()
                    // A new active account starts the mailbox over.
                    .id(mail.activeEmail ?? "")
            }
        }
        .overlay(alignment: .top) { NoticeBanner() }
        .task { app.pro.start() }
        // While the app is open: new mail of every account every few minutes.
        .task(id: scenePhase == .active && !mail.accounts.isEmpty) {
            if scenePhase == .active, !mail.accounts.isEmpty { await BackgroundRefresh.watch(app) }
        }
        // Instant notifications: the push server follows the accounts (and
        // gets the key again after it restarted).
        .task(id: "\(app.pro.pro)|\(mail.accounts.map(\.email).joined(separator: ","))") {
            do {
                try await app.push.sync(app)
            } catch {
                app.notices.error(describePush(error))
            }
        }
        // Notifications are asked for once there is mail to announce.
        .task(id: mail.accounts.isEmpty) {
            if !mail.accounts.isEmpty { BackgroundRefresh.askPermission() }
        }
        // Without Ferrowl Pro only the first account may stay active (also
        // when the subscription ends while another one is in use).
        .task(id: "\(app.pro.pro)|\(mail.activeEmail ?? "")|\(mail.accounts.count)") {
            guard !app.pro.pro, let active = mail.activeEmail, !app.mayUse(active) else { return }
            // Give the App Store the time to answer at start-up.
            try? await Task.sleep(for: .seconds(3))
            guard !Task.isCancelled, !app.pro.pro else { return }
            await mail.backToFirst()
        }
    }
}

/// The notice at the top of the screen: a confirmation or an error, maybe
/// with a button (Undo).
struct NoticeBanner: View {
    @Environment(Notices.self) private var notices

    var body: some View {
        if let notice = notices.current {
            HStack(spacing: 12) {
                Image(systemName: notice.isError ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                    .foregroundStyle(notice.isError ? .red : .accentColor)
                Text(notice.text)
                    .font(.subheadline)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if let label = notice.actionLabel {
                    Button(label) { notices.act() }
                        .font(.subheadline.bold())
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
            .shadow(color: .black.opacity(0.12), radius: 8, y: 2)
            .padding(.horizontal, 12)
            .padding(.top, 4)
            .transition(.move(edge: .top).combined(with: .opacity))
            .onTapGesture { if notice.actionLabel == nil { notices.dismiss() } }
            .accessibilityAddTraits(.isStaticText)
            .id(notice.id)
            .animation(.snappy, value: notice.id)
        }
    }
}

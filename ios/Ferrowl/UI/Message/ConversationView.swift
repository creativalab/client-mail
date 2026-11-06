import SwiftUI

/// A conversation: its messages oldest first; the newest is opened.
struct ConversationView: View {
    let model: MailboxModel
    let row: Row
    let compose: (ComposeRequest) -> Void

    var body: some View {
        List {
            Section {
                ForEach(row.uids.reversed(), id: \.self) { uid in
                    NavigationLink(value: uid) {
                        if let h = model.header(uid) {
                            VStack(alignment: .leading, spacing: 2) {
                                HStack {
                                    Text(h.from.isEmpty ? h.fromAddress : h.from)
                                        .font(.body.weight(h.seen ? .regular : .semibold))
                                        .lineLimit(1)
                                    Spacer()
                                    Text(Dates.short(h.timestamp)).font(.footnote).foregroundStyle(.secondary)
                                }
                                Text(h.preview).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                            }
                        } else {
                            Text("#\(uid)")
                        }
                    }
                }
            } header: {
                VStack(alignment: .leading, spacing: 4) {
                    Text(row.header.subject.isEmpty ? t("no_subject") : row.header.subject)
                        .font(.title3.weight(.semibold))
                        .foregroundStyle(.primary)
                        .textCase(nil)
                    Text(t("conversation_count", ["n": "\(row.uids.count)"]))
                        .textCase(nil)
                }
                .padding(.bottom, 4)
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(for: UInt32.self) { uid in
            MessageView(model: model, folderId: model.folderId, uid: uid,
                        starred: model.header(uid)?.flagged ?? false, compose: compose)
        }
    }
}

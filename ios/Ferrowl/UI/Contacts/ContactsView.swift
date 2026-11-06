import SwiftUI

/// Contacts from the account's CardDAV server (a copy on the phone).
struct ContactsView: View {
    let write: (String) -> Void

    @Environment(MailRepository.self) private var mail
    @Environment(Notices.self) private var notices
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var contacts: [Contact] = []
    @State private var syncing = false

    var body: some View {
        NavigationStack {
            List(contacts, id: \.self) { contact in
                NavigationLink {
                    ContactDetail(contact: contact) { address in
                        dismiss()
                        write(address)
                    }
                } label: {
                    HStack(spacing: 12) {
                        Avatar(name: contact.name.isEmpty ? (contact.emails.first ?? "?") : contact.name, size: 36)
                        VStack(alignment: .leading) {
                            Text(contact.name.isEmpty ? (contact.emails.first ?? "") : contact.name)
                            if let email = contact.emails.first, !contact.name.isEmpty {
                                Text(email).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .overlay {
                if contacts.isEmpty {
                    if syncing {
                        ProgressView(t("contacts_syncing"))
                    } else {
                        ContentUnavailableView(t("contacts_empty"), systemImage: "person.2")
                    }
                }
            }
            .searchable(text: $query, prompt: t("contacts_search"))
            .refreshable { await sync() }
            .navigationTitle(t("contacts"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button(t("close")) { dismiss() } }
                ToolbarItem(placement: .primaryAction) {
                    Button { Task { await sync() } } label: { Image(systemName: "arrow.clockwise") }
                        .accessibilityLabel(t("contacts_sync"))
                        .disabled(syncing)
                }
            }
            .task(id: query) {
                contacts = await mail.contacts(query)
            }
        }
    }

    private func sync() async {
        syncing = true
        defer { syncing = false }
        do {
            let n = try await mail.syncContacts()
            notices.info(t("contacts_count", ["n": "\(n)"]))
        } catch {
            notices.error(describe(error))
        }
        contacts = await mail.contacts(query)
    }
}

struct ContactDetail: View {
    let contact: Contact
    let write: (String) -> Void

    var body: some View {
        List {
            Section {
                HStack(spacing: 16) {
                    Avatar(name: contact.name.isEmpty ? (contact.emails.first ?? "?") : contact.name, size: 56)
                    VStack(alignment: .leading) {
                        Text(contact.name).font(.title3.weight(.semibold))
                        if !contact.organization.isEmpty {
                            Text(contact.organization).foregroundStyle(.secondary)
                        }
                    }
                }
            }
            if !contact.emails.isEmpty {
                Section(t("email")) {
                    ForEach(contact.emails, id: \.self) { email in
                        Button {
                            write(email)
                        } label: {
                            Label(email, systemImage: "envelope")
                        }
                    }
                }
            }
            if !contact.phones.isEmpty {
                Section {
                    ForEach(contact.phones, id: \.self) { phone in
                        let digits = phone.filter { $0.isNumber || $0 == "+" }
                        if let url = URL(string: "tel:\(digits)") {
                            Link(destination: url) { Label(phone, systemImage: "phone") }
                        }
                    }
                }
            }
            if !contact.note.isEmpty {
                Section { Text(contact.note).textSelection(.enabled) }
            }
        }
        .navigationBarTitleDisplayMode(.inline)
    }
}

import SwiftUI

/// A meeting invitation, cancellation or reply found in a message, with
/// Accept / Maybe / Decline.
struct InvitationCard: View {
    let invitation: Invitation
    let model: MessageModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(title, systemImage: invitation.method == "CANCEL" ? "calendar.badge.minus" : "calendar")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.tint)
            Text(invitation.summary).font(.headline)
            Text(when).font(.subheadline)
            if !invitation.location.isEmpty {
                Label(invitation.location, systemImage: "mappin.and.ellipse").font(.subheadline)
            }
            if invitation.recurring {
                Label(t("event_repeats"), systemImage: "repeat").font(.footnote).foregroundStyle(.secondary)
            }
            if !invitation.organizer.isEmpty {
                Text("\(t("organizer")): \(invitation.organizerName.isEmpty ? invitation.organizer : invitation.organizerName)")
                    .font(.footnote).foregroundStyle(.secondary)
            }

            switch invitation.method {
            case "REQUEST":
                if let reply = model.inviteReply {
                    Text(t(answeredKey(reply))).font(.footnote.weight(.medium))
                }
                HStack {
                    answer(.accepted, "invite_accept")
                    answer(.tentative, "invite_tentative")
                    answer(.declined, "invite_decline")
                }
                .disabled(model.inviteBusy)
            case "CANCEL":
                if model.inCalendar {
                    Button(t("invite_remove"), role: .destructive) { model.removeCancelled() }
                        .buttonStyle(.bordered)
                        .disabled(model.inviteBusy)
                }
            default:
                ForEach(invitation.answers, id: \.email) { a in
                    Text(t(replyKey(a.status), ["who": a.name.isEmpty ? a.email : a.name])).font(.subheadline)
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 14))
    }

    private var title: String {
        switch invitation.method {
        case "CANCEL": t("invite_cancelled_title")
        case "REPLY": t("invite_reply_title")
        default: t("invite_title")
        }
    }

    private var when: String {
        if invitation.allDay {
            let first = invitation.firstDay ?? ""
            let last = invitation.lastDay ?? first
            return first == last ? "\(first) · \(t("all_day"))" : "\(first) – \(last) · \(t("all_day"))"
        }
        let start = Dates.full(invitation.start)
        let sameDay = Calendar.current.isDate(Date(timeIntervalSince1970: TimeInterval(invitation.start)),
                                              inSameDayAs: Date(timeIntervalSince1970: TimeInterval(invitation.end)))
        return "\(start) – \(sameDay ? Dates.time(invitation.end) : Dates.full(invitation.end))"
    }

    private func answer(_ reply: InviteReply, _ key: String) -> some View {
        Button(t(key)) { model.respond(reply) }
            .buttonStyle(.bordered)
            .tint(model.inviteReply == reply ? .accentColor : .secondary)
    }

    private func answeredKey(_ reply: InviteReply) -> String {
        switch reply {
        case .accepted: "invite_you_accepted"
        case .tentative: "invite_you_tentative"
        case .declined: "invite_you_declined"
        }
    }

    private func replyKey(_ status: String) -> String {
        switch status.uppercased() {
        case "ACCEPTED": "invite_reply_accepted"
        case "TENTATIVE": "invite_reply_tentative"
        default: "invite_reply_declined"
        }
    }
}

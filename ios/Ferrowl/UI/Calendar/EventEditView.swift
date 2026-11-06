import SwiftUI

/// The event form: a new event on a day, or a stored one (by its url).
struct EventEditView: View {
    let url: String?
    let day: Date

    @Environment(MailRepository.self) private var mail
    @Environment(Notices.self) private var notices
    @Environment(\.dismiss) private var dismiss
    @State private var loaded = false
    @State private var summary = ""
    @State private var location = ""
    @State private var notes = ""
    @State private var allDay = false
    @State private var start = Date()
    @State private var end = Date()
    @State private var repeatRule: Repeat = .never
    @State private var calendars: [CalendarInfo] = []
    @State private var calendar = ""
    @State private var busy = false
    @State private var confirmDelete = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(t("event_title"), text: $summary)
                    TextField(t("event_location"), text: $location)
                }
                Section {
                    Toggle(t("all_day"), isOn: $allDay)
                    DatePicker(t("event_starts"), selection: $start, displayedComponents: allDay ? .date : [.date, .hourAndMinute])
                    DatePicker(t("event_ends"), selection: $end, in: start..., displayedComponents: allDay ? .date : [.date, .hourAndMinute])
                    Picker(t("repeat"), selection: $repeatRule) {
                        Text(t("repeat_never")).tag(Repeat.never)
                        Text(t("repeat_daily")).tag(Repeat.daily)
                        Text(t("repeat_weekly")).tag(Repeat.weekly)
                        Text(t("repeat_monthly")).tag(Repeat.monthly)
                        Text(t("repeat_yearly")).tag(Repeat.yearly)
                        if repeatRule == .custom { Text(t("repeat_custom")).tag(Repeat.custom) }
                    }
                }
                if url == nil, calendars.count > 1 {
                    Picker(t("event_calendar"), selection: $calendar) {
                        ForEach(calendars, id: \.url) { c in Text(c.name).tag(c.url) }
                    }
                }
                Section {
                    TextField(t("event_notes"), text: $notes, axis: .vertical).lineLimit(3...10)
                }
                if url != nil {
                    Section {
                        Button(t("delete"), role: .destructive) { confirmDelete = true }
                    }
                }
            }
            .navigationTitle(t(url == nil ? "new_event" : "edit_event"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(t("cancel")) { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if busy { ProgressView() } else { Button(t("event_save")) { save() }.disabled(!loaded) }
                }
            }
            .onChange(of: start) { old, new in
                // Moving the start keeps the event's length.
                end = end.addingTimeInterval(new.timeIntervalSince(old))
            }
            .confirmationDialog(t("event_delete_confirm"), isPresented: $confirmDelete, titleVisibility: .visible) {
                Button(t("delete"), role: .destructive) { delete() }
            }
            .disabled(busy)
            .task { await load() }
        }
    }

    private static let format: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd'T'HH:mm"
        return f
    }()

    private func load() async {
        guard !loaded else { return }
        calendars = await mail.calendars()
        calendar = calendars.first?.url ?? ""
        let draft: EventDraft
        if let url, let stored = await mail.eventDraft(url) {
            draft = stored
        } else {
            draft = newEventDraft(day: CalendarView.key(day))
        }
        summary = draft.summary
        location = draft.location
        notes = draft.description
        allDay = draft.allDay
        repeatRule = draft.repeat
        let s = Self.format.date(from: draft.start) ?? day
        start = s
        end = Self.format.date(from: draft.end) ?? s.addingTimeInterval(3600)
        loaded = true
    }

    private func save() {
        let draft = EventDraft(
            summary: summary, location: location, description: notes, allDay: allDay,
            start: Self.format.string(from: start), end: Self.format.string(from: end), repeat: repeatRule
        )
        busy = true
        Task {
            defer { busy = false }
            do {
                notices.info(try await mail.saveEvent(calendarUrl: calendar, url: url, draft: draft))
                dismiss()
            } catch {
                notices.error(describe(error))
            }
        }
    }

    private func delete() {
        guard let url else { return }
        busy = true
        Task {
            defer { busy = false }
            do {
                notices.info(try await mail.deleteEvent(url))
                dismiss()
            } catch {
                notices.error(describe(error))
            }
        }
    }
}

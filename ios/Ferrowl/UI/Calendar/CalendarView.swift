import SwiftUI

/// The account's calendars (CalDAV, a copy on the phone): the events of a
/// month, day by day.
struct CalendarView: View {
    @Environment(MailRepository.self) private var mail
    @Environment(Notices.self) private var notices
    @Environment(\.dismiss) private var dismiss
    @State private var month = Calendar.current.dateInterval(of: .month, for: Date())!.start
    @State private var events: [CalendarEvent] = []
    @State private var syncing = false
    @State private var editing: EventEdit?

    struct EventEdit: Identifiable {
        let id = UUID()
        var url: String?
        var day: Date
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(days, id: \.0) { day, items in
                    Section(Dates.day(day)) {
                        ForEach(items, id: \.self) { event in
                            Button {
                                editing = EventEdit(url: event.url, day: day)
                            } label: {
                                EventRow(event: event)
                            }
                            .foregroundStyle(.primary)
                        }
                    }
                }
            }
            .overlay {
                if events.isEmpty {
                    if syncing {
                        ProgressView(t("calendar_syncing"))
                    } else {
                        ContentUnavailableView(t("calendar_empty"), systemImage: "calendar")
                    }
                }
            }
            .refreshable { await sync() }
            .navigationTitle(month.formatted(Date.FormatStyle().month(.wide).year().locale(Language.locale)))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(t("close")) { dismiss() } }
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        let today = Date()
                        let inMonth = Calendar.current.isDate(today, equalTo: month, toGranularity: .month)
                        editing = EventEdit(url: nil, day: inMonth ? today : month)
                    } label: { Image(systemName: "plus") }
                    .accessibilityLabel(t("new_event"))
                }
                ToolbarItemGroup(placement: .bottomBar) {
                    Button { shift(-1) } label: { Image(systemName: "chevron.backward") }
                        .accessibilityLabel(t("previous_month"))
                    Spacer()
                    Button(t("today")) { month = Calendar.current.dateInterval(of: .month, for: Date())!.start }
                    Spacer()
                    Button { Task { await sync() } } label: { Image(systemName: "arrow.clockwise") }
                        .accessibilityLabel(t("calendar_sync"))
                        .disabled(syncing)
                    Spacer()
                    Button { shift(1) } label: { Image(systemName: "chevron.forward") }
                        .accessibilityLabel(t("next_month"))
                }
            }
            .task(id: month) { await load() }
            .sheet(item: $editing, onDismiss: { Task { await load() } }) { edit in
                EventEditView(url: edit.url, day: edit.day)
                    .showsNotices()
            }
        }
    }

    /// Events by day (an event lasting several days is listed on each).
    private var days: [(Date, [CalendarEvent])] {
        let cal = Calendar.current
        guard let range = cal.dateInterval(of: .month, for: month) else { return [] }
        var out: [(Date, [CalendarEvent])] = []
        var day = range.start
        while day < range.end {
            let next = cal.date(byAdding: .day, value: 1, to: day)!
            let start = Int64(day.timeIntervalSince1970), end = Int64(next.timeIntervalSince1970)
            let items = events.filter { e in
                if e.allDay, let first = e.firstDay {
                    let key = Self.key(day)
                    return key >= first && key <= (e.lastDay ?? first)
                }
                return e.start < end && (e.end > start || e.start >= start)
            }
            if !items.isEmpty { out.append((day, items)) }
            day = next
        }
        return out
    }

    static func key(_ date: Date) -> String {
        let c = Calendar.current.dateComponents([.year, .month, .day], from: date)
        return String(format: "%04d-%02d-%02d", c.year!, c.month!, c.day!)
    }

    private func shift(_ months: Int) {
        month = Calendar.current.date(byAdding: .month, value: months, to: month)!
    }

    private func load() async {
        guard let range = Calendar.current.dateInterval(of: .month, for: month) else { return }
        events = await mail.agenda(from: Int64(range.start.timeIntervalSince1970), to: Int64(range.end.timeIntervalSince1970))
    }

    private func sync() async {
        syncing = true
        defer { syncing = false }
        do {
            try await mail.syncCalendar()
        } catch {
            notices.error(describe(error))
        }
        await load()
    }
}

struct EventRow: View {
    let event: CalendarEvent

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            RoundedRectangle(cornerRadius: 2)
                .fill(Theme.parse(event.color) ?? .accentColor)
                .frame(width: 4)
            VStack(alignment: .leading, spacing: 2) {
                Text(event.summary.isEmpty ? t("no_subject") : event.summary).font(.body.weight(.medium))
                Text(event.allDay ? t("all_day") : "\(Dates.time(event.start)) – \(Dates.time(event.end))")
                    .font(.subheadline).foregroundStyle(.secondary)
                if !event.location.isEmpty {
                    Text(event.location).font(.footnote).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer()
            if event.recurring {
                Image(systemName: "repeat").font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }
}

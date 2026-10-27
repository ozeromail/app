import SwiftUI

/// Messages waiting for a time to snooze to; `then` runs once one is picked.
struct Snoozing: Identifiable {
    let id = UUID()
    let messages: [MessageItem]
    var then: @MainActor () -> Void = {}
}

/// A swiped message waiting for a folder from `targets`.
struct Moving {
    let message: MessageItem
    let targets: [(folder: FolderItem, title: String)]
}

/// The times to snooze to, from `now` on: later today and this evening while there is time
/// left, tomorrow morning, the weekend from a weekday, and next week unless that is tomorrow.
func snoozeChoices(now: Date = .now, calendar: Calendar = .current) -> [(label: String, date: Date)] {
    let today = calendar.startOfDay(for: now)
    func at(_ day: Date, _ hour: Int) -> Date {
        calendar.date(bySettingHour: hour, minute: 0, second: 0, of: day) ?? day
    }
    let tomorrowDay = calendar.date(byAdding: .day, value: 1, to: today) ?? today
    let tomorrow = at(tomorrowDay, 9)
    // Up to the next quarter hour, so the time reads round.
    let inThree = now.addingTimeInterval(3 * 3600)
    let quarter = 15 * 60.0
    let later = Date(timeIntervalSince1970: (inThree.timeIntervalSince1970 / quarter).rounded(.up) * quarter)
    let evening = at(today, 19)
    var choices: [(label: String, date: Date)] = []
    if calendar.isDate(later, inSameDayAs: now), later <= at(today, 21) {
        choices.append(("snooze_later", later))
    }
    if now < evening.addingTimeInterval(-2 * 3600) {
        choices.append(("snooze_evening", evening))
    }
    choices.append(("snooze_tomorrow", tomorrow))
    let saturday = calendar.nextDate(after: today, matching: DateComponents(weekday: 7), matchingPolicy: .nextTime)
    if !calendar.isDateInWeekend(now), let saturday, !calendar.isDate(saturday, inSameDayAs: tomorrowDay) {
        choices.append(("snooze_weekend", at(saturday, 10)))
    }
    let monday = calendar.nextDate(after: today, matching: DateComponents(weekday: 2), matchingPolicy: .nextTime)
    if let monday, !calendar.isDate(monday, inSameDayAs: tomorrowDay) {
        choices.append(("snooze_next_week", at(monday, 9)))
    }
    return choices
}

/// When snoozed mail comes back: the time today, the weekday and time within a week, else the
/// date and time.
func snoozeTime(_ date: Date) -> String {
    let calendar = Calendar.current
    let days = calendar.dateComponents([.day], from: calendar.startOfDay(for: .now), to: calendar.startOfDay(for: date)).day ?? 0
    if days == 0 { return date.formatted(date: .omitted, time: .shortened) }
    if days < 7 { return date.formatted(.dateTime.weekday(.abbreviated).hour().minute()) }
    return longDate(Int64(date.timeIntervalSince1970))
}

/// Asks until when to snooze: the usual times, each with when it is, and any date and time.
struct SnoozeSheet: View {
    let onPick: (Date) -> Void

    @State private var now = Date.now
    @State private var picking = false
    @State private var picked = Calendar.current.date(byAdding: .day, value: 1, to: .now) ?? .now

    var body: some View {
        NavigationStack {
            List {
                if picking {
                    DatePicker("snooze_pick", selection: $picked, in: now..., displayedComponents: [.date, .hourAndMinute])
                        .datePickerStyle(.graphical)
                    Button("action_snooze") { onPick(picked) }
                        .disabled(picked <= .now)
                } else {
                    ForEach(snoozeChoices(now: now), id: \.label) { choice in
                        Button {
                            onPick(choice.date)
                        } label: {
                            LabeledContent(LocalizedStringKey(choice.label), value: snoozeTime(choice.date))
                        }
                        .tint(.primary)
                    }
                    Button {
                        picking = true
                    } label: {
                        LabeledContent("snooze_pick") { Image(systemName: "chevron.right") }
                    }
                    .tint(.primary)
                }
                Section {} footer: { Text("snooze_note") }
            }
            .navigationTitle("snooze_title")
            .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents(picking ? [.large] : [.medium, .large])
    }
}

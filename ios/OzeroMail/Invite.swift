import EventKit
import EventKitUI
import SwiftUI

/// The meeting a message invites to, over its text: when, where, who, how to join, and what else
/// is on around it. Once answered it says what, and `onChange` answers again; where the meeting
/// lands in no calendar, `onAddToCalendar` puts it in the phone's.
struct InviteCard: View {
    let invite: InviteItem
    let onChange: (() -> Void)?
    let onAddToCalendar: () -> Void
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                CalendarLeaf(start: invite.start, zone: invite.allDay ? .allDay : nil)
                VStack(alignment: .leading, spacing: 2) {
                    if invite.cancelled {
                        Badge(text: tr("invite_cancelled"), tint: .ozeroError)
                    } else if invite.before != nil && invite.answer == nil {
                        Badge(text: tr("invite_moved"), tint: .ozeroSnooze)
                    }
                    Text(verbatim: invite.summary.isBlank ? tr("inbox_no_subject") : invite.summary)
                        .font(.body.weight(.semibold))
                    if let before = invite.before {
                        Text(verbatim: whenText(before.start, before.end, allDay: invite.allDay))
                            .strikethrough()
                            .foregroundStyle(.secondary)
                    }
                    Text(verbatim: whenText(invite.start, invite.end, allDay: invite.allDay))
                        .fontWeight(invite.before != nil ? .semibold : .regular)
                        .foregroundStyle(invite.before != nil ? .primary : .secondary)
                    if let their = theirTime(invite) {
                        Text(verbatim: their).foregroundStyle(.secondary)
                    }
                }
                .font(.footnote)
            }
            .padding(.horizontal, 14)
            .padding(.top, 14)
            .padding(.bottom, 12)
            VStack(alignment: .leading, spacing: 6) {
                if !invite.location.isBlank {
                    Meta(icon: "mappin.and.ellipse") { Text(verbatim: invite.location) }
                }
                if let link = invite.link, let url = URL(string: link) {
                    Meta(icon: "video") {
                        Button("invite_join") { openURL(url) }
                            .fontWeight(.semibold)
                            .foregroundStyle(Color.ozeroAccent)
                    }
                }
                let organizer = invite.organizerName ?? invite.organizerEmail
                if organizer != nil || invite.others > 0 {
                    Meta(icon: "person.2") {
                        let parts = [organizer.map { tr("invite_organizer", $0) }, invite.others > 0 ? tr("invite_others", Int(invite.others)) : nil]
                        Text(verbatim: parts.compactMap { $0 }.joined(separator: " · "))
                    }
                }
                if let repeats = invite.repeats {
                    Meta(icon: "repeat") { Text(verbatim: repeatText(repeats)) }
                }
            }
            .font(.footnote)
            .padding(.horizontal, 14)
            .padding(.bottom, 12)
            if !invite.cancelled && !invite.allDay {
                DayStrip(invite: invite)
            }
            if !invite.cancelled, let answer = invite.answer {
                Divider()
                HStack(spacing: 8) {
                    Label(tr(answer.status), systemImage: answer.icon)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(answer.tint)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if invite.canAddToCalendar && answer != .no {
                        CardButton(label: "invite_add_to_calendar", action: onAddToCalendar)
                    }
                    if let onChange {
                        CardButton(label: "invite_change", action: onChange)
                    }
                }
                .padding(.leading, 14)
                .padding(.trailing, 10)
                .padding(.vertical, 8)
            }
        }
        .overlay(RoundedRectangle(cornerRadius: 16).stroke(Color.ozeroCard, lineWidth: 1))
        .clipShape(.rect(cornerRadius: 16))
        .padding(.horizontal, 16)
        .padding(.top, 4)
        .padding(.bottom, 12)
    }
}

/// The day of the month under its weekday, as on a calendar's leaf.
private struct CalendarLeaf: View {
    let start: Int64
    let zone: TimeZone?

    var body: some View {
        VStack(spacing: 0) {
            Text(verbatim: dateText(start, "EEE", zone: zone).uppercased())
                .font(.caption2.weight(.semibold))
                .tracking(0.8)
            Text(verbatim: dateText(start, "d", zone: zone))
                .font(.title2.weight(.bold).monospacedDigit())
        }
        .foregroundStyle(Color.ozeroAccent)
        .frame(width: 48)
        .padding(.top, 6)
        .padding(.bottom, 7)
        .background(Color.ozeroAccent.opacity(0.1), in: .rect(cornerRadius: 12))
    }
}

private struct Badge: View {
    let text: String
    let tint: Color

    var body: some View {
        Text(verbatim: text)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(tint)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(Color.ozeroCard, in: .capsule)
            .padding(.bottom, 4)
    }
}

/// A line of the card after its icon.
private struct Meta<Content: View>: View {
    let icon: String
    @ViewBuilder let content: Content

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: icon).foregroundStyle(.secondary).frame(width: 18)
            content
        }
    }
}

private struct CardButton: View {
    let label: LocalizedStringKey
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.primary)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .background(Color.ozeroCard, in: .rect(cornerRadius: 10))
        }
        .buttonStyle(.plain)
    }
}

/// The hours around the meeting the core picks, with the person's other meetings in them as
/// their invitations tell; one at the same time stands beside it, in red.
private struct DayStrip: View {
    let invite: InviteItem

    private static let hour: Int64 = 3_600
    private static let hourHeight: CGFloat = 23
    private static let label: CGFloat = 40
    /// Where a meeting at the same time starts beside this one, as a part of the width.
    private static let split: CGFloat = 0.62

    var body: some View {
        let hour = Self.hour
        let from = invite.dayFrom
        let to = invite.dayTo
        let hours = Int((to - from) / hour)
        let clash = invite.day.contains(where: \.clashes)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(verbatim: dateText(invite.start, "EEEEMMMMd"))
                    .font(.caption2.weight(.semibold))
                Spacer()
                Text("invite_from_mail").font(.caption2).lineLimit(1)
            }
            .foregroundStyle(.secondary)
            GeometryReader { geometry in
                let lane = geometry.size.width - Self.label
                ZStack(alignment: .topLeading) {
                    ForEach(0..<hours, id: \.self) { i in
                        HStack(spacing: 0) {
                            Text(verbatim: dateText(from + Int64(i) * hour, "jm"))
                                .font(.caption2.monospacedDigit())
                                .foregroundStyle(.secondary)
                                .frame(width: Self.label, alignment: .leading)
                            Rectangle().fill(Color.ozeroCard).frame(height: 1)
                        }
                        .offset(y: Self.hourHeight * CGFloat(i) - 7)
                    }
                    ForEach(invite.day, id: \.message) { e in
                        let beside = e.clashes
                        event(
                            "\(e.summary) · \(dateText(e.start, "jm"))", start: e.start, end: e.end, from: from, to: to,
                            x: Self.label + (beside ? lane * Self.split : 0), width: beside ? lane * (1 - Self.split) : lane,
                            ink: beside ? .ozeroError : .secondary, edge: beside ? .ozeroError : Color.secondary.opacity(0.4), mine: false)
                    }
                    event(
                        invite.summary, start: invite.start, end: invite.end, from: from, to: to,
                        x: Self.label, width: clash ? lane * Self.split - 4 : lane, ink: .ozeroAccent, edge: .ozeroAccent, mine: true)
                }
            }
            .frame(height: Self.hourHeight * CGFloat(hours))
        }
        .padding(.horizontal, 14)
        .padding(.top, 10)
        .padding(.bottom, 12)
        .overlay(alignment: .top) { Divider() }
    }

    private func event(
        _ text: String, start: Int64, end: Int64, from: Int64, to: Int64, x: CGFloat, width: CGFloat, ink: Color, edge: Color, mine: Bool
    ) -> some View {
        let top = Self.hourHeight * CGFloat(max(start, from) - from) / CGFloat(Self.hour)
        let height = Self.hourHeight * CGFloat(min(end, to) - max(start, from)) / CGFloat(Self.hour) - 2
        return Text(verbatim: text)
            .font(.caption2.weight(mine ? .semibold : .medium))
            .foregroundStyle(ink)
            .lineLimit(1)
            .padding(.leading, 9)
            .padding(.trailing, 6)
            .padding(.top, 1)
            .frame(width: max(width, 0), height: max(height, 14), alignment: .topLeading)
            .background(mine ? Color.ozeroAccent.opacity(0.1) : Color.ozeroCard)
            .overlay(alignment: .leading) { Rectangle().fill(edge).frame(width: 3) }
            .clipShape(.rect(cornerRadius: 7))
            .offset(x: x, y: top + 1)
    }
}

/// The bottom row while an invitation waits for an answer: No, Maybe and Yes, under the thumb,
/// Yes filled, and the message's other actions in `onMore`.
struct InviteBar: View {
    let onAnswer: (InviteAnswer) -> Void
    let onMore: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            answer(.no)
            answer(.maybe)
            answer(.yes)
            Button(action: onMore) {
                Image(systemName: "ellipsis")
                    .font(.body.weight(.medium))
                    .foregroundStyle(.primary)
                    .frame(width: 50, height: 50)
                    .background(Color.ozeroCard, in: .rect(cornerRadius: 14))
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("action_more"))
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 4)
        .background(.bar)
    }

    private func answer(_ answer: InviteAnswer) -> some View {
        let filled = answer == .yes
        return Button { onAnswer(answer) } label: {
            Text(LocalizedStringKey(answer.label))
                .font(.subheadline.weight(.semibold))
                .lineLimit(1)
                .foregroundStyle(filled ? .white : .primary)
                .frame(maxWidth: .infinity, minHeight: 50)
                .background(filled ? Color.ozeroFill : .clear, in: .rect(cornerRadius: 14))
                .overlay(RoundedRectangle(cornerRadius: 14).stroke(filled ? Color.ozeroFill : Color.ozeroCard, lineWidth: 1))
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

/// Asks why not, before No goes to `organizer`; the reason may stay empty.
struct ReasonSheet: View {
    let organizer: String
    let onSend: (String) -> Void
    @State private var reason = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(verbatim: tr("invite_reason_title", organizer)).font(.title3.weight(.semibold))
            Text("invite_reason_text").font(.subheadline).foregroundStyle(.secondary)
            TextField("invite_reason_prompt", text: $reason, axis: .vertical)
                .lineLimit(2...4)
                .padding(12)
                .background(Color.ozeroCard, in: .rect(cornerRadius: 12))
            HStack(spacing: 8) {
                Button { onSend("") } label: {
                    Text("invite_reason_skip").frame(maxWidth: .infinity, minHeight: 46)
                        .background(Color.ozeroCard, in: .rect(cornerRadius: 14))
                }
                Button { onSend(reason) } label: {
                    Text("compose_send").foregroundStyle(.white).frame(maxWidth: .infinity, minHeight: 46)
                        .background(Color.ozeroFill, in: .rect(cornerRadius: 14))
                }
            }
            .font(.body.weight(.semibold))
            .buttonStyle(.plain)
        }
        .padding(22)
        .presentationDetents([.height(300)])
    }
}

/// The phone's new-event screen, filled in from an invitation; it needs no calendar permission.
struct AddToCalendar: UIViewControllerRepresentable {
    let invite: InviteItem
    let onDone: () -> Void

    func makeUIViewController(context: Context) -> EKEventEditViewController {
        let store = EKEventStore()
        let event = EKEvent(eventStore: store)
        event.title = invite.summary
        if invite.allDay {
            // EventKit reads a day-long event's days on the phone's clock; the core gives them
            // in UTC, and its end is the day after the last.
            event.isAllDay = true
            event.startDate = sameDayHere(invite.start)
            event.endDate = sameDayHere(max(invite.start, invite.end - 86_400))
        } else {
            event.startDate = Date(timeIntervalSince1970: TimeInterval(invite.start))
            event.endDate = Date(timeIntervalSince1970: TimeInterval(invite.end))
        }
        event.location = invite.location.isBlank ? nil : invite.location
        event.url = invite.link.flatMap(URL.init(string:))
        let controller = EKEventEditViewController()
        controller.eventStore = store
        controller.event = event
        controller.editViewDelegate = context.coordinator
        return controller
    }

    func updateUIViewController(_ controller: EKEventEditViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onDone: onDone) }

    final class Coordinator: NSObject, EKEventEditViewDelegate {
        let onDone: () -> Void

        init(onDone: @escaping () -> Void) { self.onDone = onDone }

        func eventEditViewController(_ controller: EKEventEditViewController, didCompleteWith action: EKEventEditViewAction) {
            onDone()
        }
    }
}

/// The phone's midnight of the day that is `seconds` in UTC.
private func sameDayHere(_ seconds: Int64) -> Date {
    var utc = Calendar(identifier: .gregorian)
    utc.timeZone = .allDay
    let day = utc.dateComponents([.year, .month, .day], from: Date(timeIntervalSince1970: TimeInterval(seconds)))
    return Calendar.current.date(from: day) ?? Date(timeIntervalSince1970: TimeInterval(seconds))
}

/// What a list shows of an invitation in place of its text: "Wed, 29 Oct, 11:00 · Room 7".
@MainActor
func inviteLine(_ mark: InviteMark) -> String {
    let start = mark.allDay ? dateText(mark.start, daySkeleton(mark.start, long: false), zone: .allDay) : dateText(mark.start, daySkeleton(mark.start, long: false) + "jm")
    return [mark.cancelled ? tr("invite_cancelled") : nil, start, mark.location.isBlank ? nil : mark.location]
        .compactMap { $0 }
        .joined(separator: " · ")
}

extension InviteAnswer {
    /// The subject of the answer's mail, where it goes as one.
    var subject: String {
        switch self {
        case .yes: "invite_subject_yes"
        case .maybe: "invite_subject_maybe"
        case .no: "invite_subject_no"
        }
    }

    /// "You're going", and so on: the answer as a toast and the card say it.
    var status: String {
        switch self {
        case .yes: "invite_going"
        case .maybe: "invite_might_go"
        case .no: "invite_not_going"
        }
    }

    var label: String {
        switch self {
        case .yes: "invite_yes"
        case .maybe: "invite_maybe"
        case .no: "invite_no"
        }
    }

    var icon: String {
        switch self {
        case .yes: "checkmark"
        case .maybe: "questionmark"
        case .no: "xmark"
        }
    }

    var tint: Color {
        switch self {
        case .yes: .green
        case .maybe: .ozeroSnooze
        case .no: .ozeroError
        }
    }
}

/// "Wednesday, 29 October, 11:00–12:00"; across days both ends in full. A day-long meeting
/// shows its days in UTC, as the core gives them.
@MainActor
private func whenText(_ start: Int64, _ end: Int64, allDay: Bool) -> String {
    let skeleton = daySkeleton(start, long: true)
    if allDay {
        let last = max(start, end - 86_400)
        let first = dateText(start, skeleton, zone: .allDay)
        return last == start ? "\(first) · \(tr("invite_all_day"))" : "\(first) – \(dateText(last, skeleton, zone: .allDay))"
    }
    let calendar = Calendar.current
    let sameDay = calendar.isDate(moment(start), inSameDayAs: moment(end))
    return sameDay
        ? "\(dateText(start, skeleton)), \(dateText(start, "jm"))–\(dateText(end, "jm"))"
        : "\(dateText(start, skeleton + "jm")) – \(dateText(end, skeleton + "jm"))"
}

/// The organizer's clock, where it differs from the phone's: "16:00–17:00 their time".
@MainActor
private func theirTime(_ invite: InviteItem) -> String? {
    guard let offset = invite.offset, !invite.allDay,
          TimeZone.current.secondsFromGMT(for: moment(invite.start)) != Int(offset) * 60,
          let zone = TimeZone(secondsFromGMT: Int(offset) * 60) else { return nil }
    return tr("invite_their_time", "\(dateText(invite.start, "jm", zone: zone))–\(dateText(invite.end, "jm", zone: zone))")
}

/// "Every week · Mon, Wed", "Every 2 weeks".
private func repeatText(_ repeats: RepeatItem) -> String {
    let n = Int(repeats.interval)
    let every: String = switch repeats.every {
    case .day: n > 1 ? tr("invite_repeat_days", n) : tr("invite_repeat_day")
    case .week: n > 1 ? tr("invite_repeat_weeks", n) : tr("invite_repeat_week")
    case .month: n > 1 ? tr("invite_repeat_months", n) : tr("invite_repeat_month")
    case .year: n > 1 ? tr("invite_repeat_years", n) : tr("invite_repeat_year")
    }
    guard !repeats.days.isEmpty else { return every }
    // Monday is 0 in the core, Sunday first in the calendar's names.
    let names = Calendar.current.shortWeekdaySymbols
    let days = repeats.days.map { names[(Int($0) + 1) % 7] }.joined(separator: ", ")
    return "\(every) · \(days)"
}

/// The weekday, day and month, and the year when it is not this one.
private func daySkeleton(_ seconds: Int64, long: Bool) -> String {
    let thisYear = Calendar.current.isDate(moment(seconds), equalTo: .now, toGranularity: .year)
    return (thisYear ? "" : "y") + (long ? "EEEEMMMMd" : "EEEMMMd")
}

/// `seconds` by a localized template, on the phone's clock or in `zone`. Formatters are kept
/// by template and zone: building one is slow, and the list and the strip ask for many.
@MainActor
private func dateText(_ seconds: Int64, _ template: String, zone: TimeZone? = nil) -> String {
    let key = "\(template)|\(zone?.identifier ?? "")"
    let formatter = formatters[key] ?? {
        let made = DateFormatter()
        made.setLocalizedDateFormatFromTemplate(template)
        if let zone { made.timeZone = zone }
        formatters[key] = made
        return made
    }()
    return formatter.string(from: moment(seconds))
}

@MainActor private var formatters: [String: DateFormatter] = [:]

extension TimeZone {
    /// The zone a day-long meeting's days are given in.
    static let allDay = TimeZone(identifier: "UTC") ?? .gmt
}

private func moment(_ seconds: Int64) -> Date { Date(timeIntervalSince1970: TimeInterval(seconds)) }

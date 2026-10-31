import EventKit
import EventKitUI
import SwiftUI

/// The meeting a message invites to, over its text, in three lines: what, when, where with how to
/// join; then a line of the day around it, and the meeting at the same time if there is one. Once
/// answered it says what, and `onChange` answers again; where the meeting lands in no calendar,
/// `onAddToCalendar` puts it in the phone's.
struct InviteCard: View {
    let invite: InviteItem
    let onChange: (() -> Void)?
    let onAddToCalendar: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
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
                        .font(.footnote)
                        .strikethrough()
                        .foregroundStyle(.secondary)
                }
                let repeats = invite.repeats.map(repeatText)
                Text(verbatim: [whenText(invite.start, invite.end, allDay: invite.allDay), repeats].compactMap { $0 }.joined(separator: " · "))
                    .font(.subheadline)
                if let their = theirTime(invite) {
                    Text(verbatim: their).font(.footnote).foregroundStyle(.secondary)
                }
                if let place = placeText(invite) {
                    Text(place).font(.subheadline).foregroundStyle(.secondary).tint(.ozeroAccent)
                }
                if !invite.cancelled && !invite.allDay {
                    DayLine(invite: invite).padding(.top, 10)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 14)
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
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.ozeroCard, in: .rect(cornerRadius: 16))
        .padding(.horizontal, 16)
        .padding(.top, 4)
        .padding(.bottom, 12)
    }
}

/// "Room 4 · Join", the link opening the meeting.
@MainActor
private func placeText(_ invite: InviteItem) -> AttributedString? {
    let url = invite.link.flatMap(URL.init(string:))
    guard !invite.location.isBlank || url != nil else { return nil }
    var text = AttributedString(invite.location)
    if let url {
        if !invite.location.isBlank { text += AttributedString(" · ") }
        var join = AttributedString(tr("invite_join"))
        join.link = url
        join.font = .subheadline.weight(.semibold)
        text += join
    }
    return text
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
            .background(Color(.systemBackground), in: .capsule)
            .padding(.bottom, 4)
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
                .background(Color(.systemBackground), in: .rect(cornerRadius: 10))
        }
        .buttonStyle(.plain)
    }
}

/// The hours around the meeting the core picks, as one thin line: the meeting in accent, the
/// person's other meetings in grey as their invitations tell, one at the same time in red and
/// named under it.
private struct DayLine: View {
    let invite: InviteItem

    private static let hour: Int64 = 3_600
    private static let thickness: CGFloat = 6

    var body: some View {
        let from = invite.dayFrom
        let span = CGFloat(max(invite.dayTo - from, Self.hour))
        let clashes = invite.day.filter(\.clashes)
        let hours = Int(span) / Int(Self.hour)
        let step = max(1, (hours + 2) / 3)
        VStack(alignment: .leading, spacing: 6) {
            GeometryReader { geometry in
                let width = geometry.size.width
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.secondary.opacity(0.2))
                    ForEach(invite.day.filter { !$0.clashes }, id: \.message) { e in
                        busy(e.start, e.end, from: from, span: span, width: width, color: Color.secondary.opacity(0.55))
                    }
                    busy(invite.start, invite.end, from: from, span: span, width: width, color: .ozeroFill)
                    ForEach(clashes, id: \.message) { e in
                        busy(e.start, e.end, from: from, span: span, width: width, color: Color.ozeroError.opacity(0.85))
                    }
                }
            }
            .frame(height: Self.thickness)
            GeometryReader { geometry in
                ForEach(Array(stride(from: 0, through: hours, by: step)), id: \.self) { h in
                    // The first and last hours sit at the ends, inside the card.
                    let at = CGFloat(Int64(h) * Self.hour)
                    let last = at >= span
                    Text(verbatim: hourText(from + Int64(h) * Self.hour))
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: last ? .infinity : nil, alignment: .trailing)
                        .offset(x: last || h == 0 ? 0 : geometry.size.width * at / span - 6)
                }
            }
            .frame(height: 14)
            if let first = clashes.first {
                let what = "\(first.summary), \(dateText(first.start, "jm"))"
                let more = clashes.count > 1 ? tr("invite_others", clashes.count - 1) : nil
                Text(verbatim: tr("invite_clash", [what, more].compactMap { $0 }.joined(separator: " ")))
                    .font(.footnote)
                    .foregroundStyle(Color.ozeroError)
            }
        }
    }

    private func busy(_ start: Int64, _ end: Int64, from: Int64, span: CGFloat, width: CGFloat, color: Color) -> some View {
        let left = width * CGFloat(max(start, from) - from) / span
        let length = width * CGFloat(min(end, invite.dayTo) - max(start, from)) / span
        return Capsule().fill(color).frame(width: max(length, 0)).offset(x: left)
    }
}

/// "9", "15": an hour of the day, without minutes or AM and PM.
private func hourText(_ seconds: Int64) -> String {
    let hour = Calendar.current.component(.hour, from: moment(seconds))
    let twelve = DateFormatter.dateFormat(fromTemplate: "j", options: 0, locale: .current)?.contains("a") ?? false
    return twelve ? "\((hour + 11) % 12 + 1)" : "\(hour)"
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
    let start = mark.allDay ? dateText(mark.start, daySkeleton(mark.start), zone: .allDay) : dateText(mark.start, daySkeleton(mark.start) + "jm")
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

/// "Wed, 29 Oct · 11:00–12:00"; across days both ends in full. A day-long meeting
/// shows its days in UTC, as the core gives them.
@MainActor
private func whenText(_ start: Int64, _ end: Int64, allDay: Bool) -> String {
    let skeleton = daySkeleton(start)
    if allDay {
        let last = max(start, end - 86_400)
        let first = dateText(start, skeleton, zone: .allDay)
        return last == start ? "\(first) · \(tr("invite_all_day"))" : "\(first) – \(dateText(last, skeleton, zone: .allDay))"
    }
    let calendar = Calendar.current
    let sameDay = calendar.isDate(moment(start), inSameDayAs: moment(end))
    return sameDay
        ? "\(dateText(start, skeleton)) · \(dateText(start, "jm"))–\(dateText(end, "jm"))"
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
private func daySkeleton(_ seconds: Int64) -> String {
    let thisYear = Calendar.current.isDate(moment(seconds), equalTo: .now, toGranularity: .year)
    return (thisYear ? "" : "y") + "EEEMMMd"
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

import SwiftUI

/// The unified inbox: shows what is saved at once, then syncs; pull down to sync again.
struct InboxView: View {
    let mail: Mail

    /// Nil until the saved mail has been read once.
    @State private var messages: [MessageItem]?
    @State private var syncing = false
    @State private var error: LocalizedStringKey?

    var body: some View {
        NavigationStack {
            List {
                if messages?.isEmpty == true && !syncing && error == nil {
                    Text("inbox_empty")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity)
                        .listRowSeparator(.hidden)
                }
                ForEach(messages ?? [], id: \.id) { message in
                    NavigationLink(value: message) { MessageRow(message: message) }
                }
            }
            .listStyle(.plain)
            .navigationTitle("inbox_title")
            .navigationDestination(for: MessageItem.self) { MessageView(mail: mail, message: $0) }
            // Back from a message, which now has a snippet.
            .onAppear { if messages != nil { Task { await load() } } }
            .refreshable { await sync() }
            .safeAreaInset(edge: .bottom) {
                if let error {
                    Text(error).font(.footnote).foregroundStyle(.secondary).padding()
                }
            }
        }
        .task {
            await load()
            await sync()
        }
    }

    private func load() async {
        do {
            messages = try await mail.inbox()
        } catch {
            self.error = error.text()
        }
    }

    private func sync() async {
        guard !syncing else { return }
        syncing = true
        error = nil
        defer { syncing = false }
        do {
            try await mail.refresh()
        } catch {
            self.error = error.text()
        }
        await load()
    }
}

private struct MessageRow: View {
    let message: MessageItem

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Circle()
                .fill(message.seen ? AnyShapeStyle(.clear) : AnyShapeStyle(.tint))
                .frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline) {
                    Text(message.sender)
                        .fontWeight(message.seen ? .regular : .semibold)
                        .lineLimit(1)
                    Spacer()
                    Text(shortDate(message.date))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                message.subjectText
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if !message.snippet.isEmpty {
                    Text(message.snippet)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
        }
    }
}

extension MessageItem {
    /// The name, or the address when there is none.
    var sender: String {
        let name = fromName?.trimmingCharacters(in: .whitespaces) ?? ""
        return name.isEmpty ? fromEmail : name
    }

    var subjectText: Text {
        subject.trimmingCharacters(in: .whitespaces).isEmpty ? Text("inbox_no_subject") : Text(subject)
    }
}

/// Time for today, day and month for this year, the full date before that.
private func shortDate(_ seconds: Int64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(seconds))
    let calendar = Calendar.current
    if calendar.isDateInToday(date) {
        return date.formatted(date: .omitted, time: .shortened)
    }
    if calendar.isDate(date, equalTo: .now, toGranularity: .year) {
        return date.formatted(.dateTime.day().month(.abbreviated))
    }
    return date.formatted(date: .numeric, time: .omitted)
}

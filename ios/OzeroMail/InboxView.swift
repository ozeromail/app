import SwiftUI

/// The unified inbox: shows what is saved at once, then syncs; pull down to sync again.
struct InboxView: View {
    let mail: Mail

    /// Nil until the saved mail has been read once.
    @State private var messages: [MessageItem]?
    @State private var syncing = false
    @State private var error: LocalizedStringKey?
    /// The last archive, while its toast can still take it back.
    @State private var undoable: Int64?
    @State private var tab = Category.people
    @State private var tabs: [InboxTab] = []

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
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button("action_done") { archive(message) }
                                .tint(.accentColor)
                        }
                }
            }
            .listStyle(.plain)
            .navigationTitle("inbox_title")
            .navigationDestination(for: MessageItem.self) { MessageView(mail: mail, message: $0) }
            .refreshable { await sync() }
            .safeAreaInset(edge: .bottom) {
                VStack {
                    if let undoable {
                        UndoToast { undo(undoable) }
                            .task(id: undoable) {
                                // Closes before the core sends the move, so a tap is never too late.
                                try? await Task.sleep(for: .seconds(Int(undoSeconds())))
                                if self.undoable == undoable { self.undoable = nil }
                            }
                    }
                    if let error {
                        Text(error).font(.footnote).foregroundStyle(.secondary).padding()
                    }
                    Picker("inbox_title", selection: $tab) {
                        ForEach(Category.all, id: \.self) { category in
                            let unread = tabs.first { $0.category == category }?.unread ?? 0
                            (Text(category.label) + Text(unread > 0 ? " \(unread)" : ""))
                                .tag(category)
                        }
                    }
                    .pickerStyle(.segmented)
                    .padding(.horizontal)
                    .padding(.bottom, 8)
                }
                .background(.bar)
            }
            // Also runs on the way back from a message, which now has a snippet.
            .task(id: tab) { await load() }
        }
        .task { await sync() }
    }

    /// Done: the message leaves the list at once; the toast can take it back for a moment.
    private func archive(_ message: MessageItem) {
        do {
            undoable = try mail.archive(message)
            messages?.removeAll { $0.id == message.id }
            Task { await counts() }
        } catch {
            self.error = error.text()
        }
    }

    private func undo(_ action: Int64) {
        undoable = nil
        Task {
            do {
                if try mail.undo(action) { await load() }
            } catch {
                self.error = error.text()
            }
        }
    }

    private func loadList() async {
        let shown = tab
        do {
            let found = try await mail.inbox(shown)
            // A tab picked meanwhile loads its own list.
            if tab == shown { messages = found }
        } catch {
            // Never leave the last tab's messages under the new one.
            if tab == shown { messages = [] }
            self.error = error.text()
        }
    }

    private func counts() async {
        do {
            tabs = try await mail.tabs()
        } catch {
            self.error = error.text()
        }
    }

    private func load() async {
        await loadList()
        await counts()
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

private struct UndoToast: View {
    let undo: () -> Void

    var body: some View {
        HStack {
            Text("inbox_archived")
            Spacer()
            Button("action_undo", action: undo).fontWeight(.semibold)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(.regularMaterial, in: .rect(cornerRadius: 12))
        .padding(.horizontal)
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

import SwiftUI
import UserNotifications

/// The unified inbox: shows what is saved at once, then syncs; pull down to sync again.
struct InboxView: View {
    let mail: Mail

    /// Nil until the saved mail has been read once.
    @State private var messages: [MessageItem]?
    @State private var syncing = false
    @State private var error: LocalizedStringKey?
    /// The last archive or send, while its toast can still take it back.
    @State private var undoable: Undoable?
    @State private var composing: Composing?
    @State private var tab = Category.people
    @State private var tabs: [InboxTab] = []
    @State private var unsent: [UnsentItem] = []
    @State private var path: [MessageItem] = []

    var body: some View {
        NavigationStack(path: $path) {
            List {
                ForEach(unsent, id: \.id) { failed in
                    Button { fix(failed) } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("inbox_not_sent").fontWeight(.semibold).foregroundStyle(.red)
                            (failed.subject.isEmpty ? Text("inbox_no_subject") : Text(verbatim: failed.subject))
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                }
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
            .navigationDestination(for: MessageItem.self) { message in
                MessageView(mail: mail, message: message) { composing = .reply(to: message) }
            }
            .refreshable { await sync() }
            .safeAreaInset(edge: .bottom) {
                VStack {
                    if let undoable {
                        UndoToast(text: undoable.text) { undo(undoable) }
                            .task(id: undoable) {
                                // Closes before the core sends, so a tap is never too late.
                                try? await Task.sleep(for: .seconds(Int(undoSeconds())))
                                if self.undoable == undoable { self.undoable = nil }
                            }
                    }
                    if let error {
                        Text(error).font(.footnote).foregroundStyle(.secondary).padding()
                    }
                    HStack {
                        Picker("inbox_title", selection: $tab) {
                            ForEach(Category.all, id: \.self) { category in
                                let unread = tabs.first { $0.category == category }?.unread ?? 0
                                (Text(category.label) + Text(unread > 0 ? " \(unread)" : ""))
                                    .tag(category)
                            }
                        }
                        .pickerStyle(.segmented)
                        Button("compose_new") {
                            composing = .new
                        }
                    }
                    .padding(.horizontal)
                    .padding(.bottom, 8)
                }
                .background(.bar)
            }
            // Also runs on the way back from a message, which now has a snippet.
            .task(id: tab) { await load() }
        }
        .task { await sync() }
        .task {
            // Only once there is an account: without one a check has nothing to do.
            BackgroundCheck.schedule()
            // Asked once the inbox shows, so it is clear what the notifications are for.
            _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
        }
        .fullScreenCover(item: $composing) { composing in
            ComposeView(mail: mail, composing: composing) { id, sent in
                self.composing = nil
                // Back to the list, where the toast is.
                path = []
                undoable = .send(id, sent)
            }
        }
    }

    /// Done: the message leaves the list at once; the toast can take it back for a moment.
    private func archive(_ message: MessageItem) {
        do {
            undoable = .archive(try mail.archive(message))
            messages?.removeAll { $0.id == message.id }
            Task { await counts() }
        } catch {
            self.error = error.text()
        }
    }

    private func undo(_ action: Undoable) {
        undoable = nil
        Task {
            do {
                switch action {
                case .archive(let id):
                    if try mail.undo(id) { await load() }
                case .send(let id, let sent):
                    if let draft = try await mail.undoSend(id) {
                        var again = sent
                        again.draft = draft
                        composing = again
                    }
                }
            } catch {
                self.error = error.text()
            }
        }
    }

    /// A refused message, taken back to fix it.
    private func fix(_ failed: UnsentItem) {
        Task {
            do {
                if let draft = try await mail.undoSend(failed.id) {
                    composing = Composing(draft: draft)
                }
            } catch {
                self.error = error.text()
            }
            await counts()
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
            unsent = try await mail.unsent()
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
        // Whatever synced is on screen now, also when one account failed: no notification later.
        do {
            try await mail.forgetNewMail()
        } catch {
            self.error = error.text()
        }
        UNUserNotificationCenter.current().removeAllDeliveredNotifications()
        await load()
    }
}

/// An action the toast can still take back.
private enum Undoable: Equatable {
    case archive(Int64)
    case send(Int64, Composing)

    var text: LocalizedStringKey {
        switch self {
        case .archive: "inbox_archived"
        case .send: "inbox_sending"
        }
    }
}

private struct UndoToast: View {
    let text: LocalizedStringKey
    let undo: () -> Void

    var body: some View {
        HStack {
            Text(text)
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

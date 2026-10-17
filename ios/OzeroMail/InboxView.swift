import SwiftUI
import UserNotifications

/// The unified inbox: shows what is saved at once, then syncs; pull down to sync again.
struct InboxView: View {
    let mail: Mail

    /// Nil until the saved mail has been read once.
    @State private var messages: [MessageItem]?
    @State private var syncing = false
    /// A sync asked for while one runs, say for a new account, runs right after it.
    @State private var syncAgain = false
    @State private var error: LocalizedStringKey?
    /// The last archive or send, while its toast can still take it back.
    @State private var undoable: Undoable?
    @State private var composing: Composing?
    @State private var tab = Category.people
    @State private var tabs: [InboxTab] = []
    @State private var unsent: [UnsentItem] = []
    @State private var path: [MessageItem] = []
    @State private var query = ""
    @State private var addingAccount = false
    /// With the query they answer, so "No messages" never shows for a query still waiting.
    @State private var found: (query: String, messages: [MessageItem])?
    @State private var searchError: LocalizedStringKey?

    /// Search results while there is a query, otherwise nil: the inbox shows.
    private var results: [MessageItem]? {
        query.trimmingCharacters(in: .whitespaces).isEmpty ? nil : found?.messages ?? []
    }

    var body: some View {
        NavigationStack(path: $path) {
            List {
                if let results {
                    if found?.query == query && results.isEmpty {
                        EmptyRow()
                    }
                    ForEach(results, id: \.id) { message in
                        NavigationLink(value: message) { MessageRow(message: message) }
                            .listRowBackground(AccountStrip(account: message.account))
                    }
                } else {
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
                        EmptyRow()
                    }
                    ForEach(messages ?? [], id: \.id) { message in
                        NavigationLink(value: message) { MessageRow(message: message) }
                            .listRowBackground(AccountStrip(account: message.account))
                            .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                Button("action_done") { archive(message) }
                                    .tint(.accentColor)
                            }
                    }
                }
            }
            .listStyle(.plain)
            .navigationTitle("inbox_title")
            .searchable(text: $query, prompt: Text("search_prompt"))
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("inbox_add_account") { addingAccount = true }
                }
            }
            .sheet(isPresented: $addingAccount) {
                SignInView(mail: mail, onCancel: { addingAccount = false }) {
                    addingAccount = false
                    Task { await sync() }
                }
            }
            .task(id: query) {
                searchError = nil
                guard results != nil else { return }
                do {
                    // A pause in typing starts a search, so each key does not.
                    try await Task.sleep(for: .milliseconds(150))
                    found = (query, try await mail.search(query))
                } catch is CancellationError {
                    // The query changed meanwhile.
                } catch {
                    searchError = error.text()
                }
            }
            .navigationDestination(for: MessageItem.self) { message in
                MessageView(mail: mail, message: message) { composing = try? mail.reply(to: $0) }
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
                    if let shown = results == nil ? error : searchError {
                        Text(shown).font(.footnote).foregroundStyle(.secondary).padding()
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
                case .archive(let ids):
                    if try mail.undo(ids) { await load() }
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
        guard !syncing else {
            syncAgain = true
            return
        }
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
        if syncAgain {
            syncAgain = false
            Task { await sync() }
        }
    }
}

/// An action the toast can still take back.
private enum Undoable: Equatable {
    case archive([Int64])
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

/// A strip of the account's colour along the row's leading edge, when there is one.
private struct AccountStrip: View {
    let account: AccountMark?

    var body: some View {
        Rectangle()
            .fill(account?.tint ?? .clear)
            .frame(width: 3)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A row of the list; with several accounts, the account's name in its colour marks it.
private struct MessageRow: View {
    let message: MessageItem

    var body: some View {
        let unread = message.threadUnread > 0
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Circle()
                .fill(unread ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear))
                .frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline) {
                    Text(message.sender)
                        .fontWeight(unread ? .semibold : .regular)
                        .lineLimit(1)
                    if message.threadCount > 1 {
                        Text(verbatim: "\(message.threadCount)")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
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
                if let account = message.account {
                    Text(verbatim: account.name)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
        }
    }
}

extension AccountMark {
    /// The account's colour, from the core.
    var tint: Color {
        Color(
            red: Double((color >> 16) & 0xFF) / 255,
            green: Double((color >> 8) & 0xFF) / 255,
            blue: Double(color & 0xFF) / 255)
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
func shortDate(_ seconds: Int64) -> String {
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

/// "No messages", as a row of a list.
private struct EmptyRow: View {
    var body: some View {
        Text("inbox_empty")
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity)
            .listRowSeparator(.hidden)
    }
}

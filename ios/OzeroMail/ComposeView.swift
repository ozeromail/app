import SwiftUI

/// A message being written, and the one it answers or forwards.
struct Composing: Identifiable, Equatable {
    let id = UUID()
    var draft: DraftItem
    var original: MessageItem?
}

/// A new message, a reply or a forward, on the whole screen. The fields sit low, near the
/// keyboard; the message being answered or forwarded stays on top. `onSent` gets the outbox id
/// for undo.
///
/// What is written is kept as a draft on the phone a moment after typing stops, when the app
/// goes to the background and when the screen closes; `onKept` then says it is in the drafts.
/// `onDiscard` gets what was thrown away, for undo.
struct ComposeView: View {
    let mail: Mail
    let composing: Composing
    let onClose: () -> Void
    let onSent: (Int64, Composing) -> Void
    let onKept: () -> Void
    let onDiscard: (DraftItem) -> Void

    @State private var to = ""
    @State private var subject = ""
    @State private var text = ""
    @State private var accountId: Int64 = 0
    @State private var quote = ""
    @State private var quotable = true
    @State private var accounts: [AccountItem] = []
    @State private var busy = false
    @State private var error: LocalizedStringKey?
    @State private var keeper: DraftKeeper?
    /// Whether anything was written here; until then a saved draft stays as it was.
    @State private var changed = false
    /// Sent or thrown away: nothing more to keep.
    @State private var finished = false
    /// A forward sends the message's files too, from the account it came to.
    @State private var forwarded: MessageItem?
    /// The draft as it opened, to tell a change from the fields being filled in.
    @State private var firstSeen: DraftItem?
    @FocusState private var focus: Field?
    @Environment(\.scenePhase) private var scenePhase

    private enum Field { case to, subject, text }

    private var start: DraftItem { composing.draft }

    /// A new message starts from the first account.
    private var from: AccountItem? { accounts.first { $0.id == accountId } ?? accounts.first }

    /// Account 0 is kept as none, and opens with the first account again.
    private var current: DraftItem {
        DraftItem(
            id: 0, accountId: accountId, to: to, subject: subject, text: text, replyTo: start.replyTo, quote: quote,
            saved: 0, forward: start.forward)
    }

    /// Something written here or before, which Delete throws away.
    private var hasDraft: Bool { changed || start.id != 0 }

    var body: some View {
        VStack(spacing: 0) {
            if let original = composing.original {
                Original(mail: mail, message: original)
                    .frame(maxHeight: focus == nil ? .infinity : 112)
            } else {
                Spacer(minLength: 0)
            }
            VStack(alignment: .leading, spacing: 12) {
                TextField("compose_to", text: $to)
                    .keyboardType(.emailAddress)
                    .textContentType(.emailAddress)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .focused($focus, equals: .to)
                    .submitLabel(.next)
                    .onSubmit { focus = .subject }
                Divider()
                TextField("compose_subject", text: $subject)
                    .font(.title2.bold())
                    .focused($focus, equals: .subject)
                    .submitLabel(.next)
                    .onSubmit { focus = .text }
                TextField("compose_text", text: $text, axis: .vertical)
                    .lineLimit(focus == nil ? 7...14 : 3...7)
                    .focused($focus, equals: .text)
                // A forward is its quote, so it stays.
                let forwarding = start.forward != nil
                let files = forwarded.flatMap { $0.accountId == from?.id ? $0.attachments : nil } ?? []
                if !files.isEmpty { AttachmentChips(names: files) }
                if !quote.isEmpty {
                    QuoteLine(text: composing.original == nil ? quote : nil, forwarding: forwarding,
                              onRemove: forwarding ? nil : { quote = "" })
                } else if let original = composing.original, quotable {
                    Button("compose_add_quote") { addQuote(original) }
                        .font(.subheadline)
                        .buttonStyle(.bordered)
                        .buttonBorderShape(.capsule)
                }
            }
            .padding(.horizontal, 20)
            if let error {
                Text(error).foregroundStyle(Color.ozeroError).padding(.horizontal, 20).padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            Divider().padding(.top, 10)
            HStack(spacing: 8) {
                RoundButton(icon: "xmark", label: "action_close", size: 44, action: close)
                if hasDraft {
                    RoundButton(icon: "trash", label: "action_delete_draft", size: 44, action: discard)
                }
                if let from {
                    Menu {
                        ForEach(accounts, id: \.id) { account in
                            Button {
                                accountId = account.id
                            } label: {
                                if account.id == from.id { Label(account.email, systemImage: "checkmark") } else { Text(account.email) }
                            }
                        }
                    } label: {
                        HStack(spacing: 8) {
                            AccountInitial(name: from.name, color: from.tint, size: 22)
                            Text(verbatim: from.email).font(.subheadline).lineLimit(1).foregroundStyle(.primary)
                        }
                        .padding(.leading, 8)
                        .padding(.trailing, 12)
                        .frame(height: 44)
                        .background(Color.ozeroCard, in: .capsule)
                    }
                    .disabled(accounts.count < 2)
                }
                Spacer(minLength: 0)
                Button(action: send) {
                    Image(systemName: "paperplane.fill")
                        .foregroundStyle(.white)
                        .frame(width: 56, height: 44)
                        .background(Color.ozeroFill, in: .capsule)
                }
                .disabled(busy || from == nil)
                .accessibilityLabel(Text("compose_send"))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
        }
        .animation(.smooth, value: focus)
        .onAppear(perform: begin)
        .task {
            do {
                accounts = try await mail.accounts()
                if let id = start.forward { forwarded = try await mail.message(id) }
            } catch {
                self.error = error.text()
            }
        }
        // A pause in typing saves the draft, so each key does not.
        .task(id: current) {
            guard firstSeen != nil else { return }
            if current != firstSeen { changed = true }
            guard changed else { return }
            try? await Task.sleep(for: .seconds(1))
            if !finished { keeper?.save(current) }
        }
        // The app may be closed in the background at any time.
        .onChange(of: scenePhase) { _, phase in
            if phase != .active { keep() }
        }
    }

    private func begin() {
        guard firstSeen == nil else { return }
        to = start.to
        subject = start.subject
        text = start.text
        accountId = start.accountId
        quote = start.quote
        keeper = DraftKeeper(mail: mail, id: start.id)
        firstSeen = current
    }

    private func keep() {
        if changed && !finished { keeper?.save(current) }
    }

    private func close() {
        keep()
        if !finished && hasDraft && !current.isBlank { onKept() } else { onClose() }
    }

    private func discard() {
        finished = true
        keeper?.discard()
        var thrown = current
        thrown.accountId = from?.id ?? accountId
        onDiscard(thrown)
    }

    private func send() {
        guard !busy, let from, let keeper else { return }
        busy = true
        error = nil
        var draft = current
        draft.accountId = from.id
        Task {
            defer { busy = false }
            do {
                let id = try await keeper.send(draft)
                finished = true
                var sent = composing
                sent.draft = draft
                onSent(id, sent)
            } catch {
                self.error = error.text()
            }
        }
    }

    private func addQuote(_ original: MessageItem) {
        let header = tr("compose_quote_header", longDate(original.date), original.sender)
        Task {
            do {
                quote = try await mail.quote(original, header: header)
                // Nothing to quote.
                quotable = !quote.isEmpty
            } catch {
                self.error = error.text()
            }
        }
    }
}

/// The message being answered, whole, scrolling in the room it gets.
private struct Original: View {
    let mail: Mail
    let message: MessageItem
    @State private var text = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(verbatim: message.sender).font(.subheadline.weight(.semibold)).lineLimit(1)
                Spacer()
                Text(verbatim: shortDate(message.date)).font(.footnote).foregroundStyle(.secondary)
            }
            ScrollView {
                Text(verbatim: text.isEmpty ? message.snippet : text)
                    .font(.readingQuote)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(14)
        .background(Color.ozeroCard, in: .rect(cornerRadius: 16))
        .padding(.horizontal, 16)
        .padding(.top, 12)
        .padding(.bottom, 10)
        .task(id: message.id) {
            // The snippet stays if the text cannot be read.
            if let body = try? await mail.body(of: message), !body.text.isBlank {
                text = body.text
            }
        }
    }
}

/// The quoted original under the text, or with `text` nil only that it goes there: its text is
/// in the card above already. A way to take it out if `onRemove` is set.
private struct QuoteLine: View {
    let text: String?
    let forwarding: Bool
    let onRemove: (() -> Void)?

    var body: some View {
        HStack {
            Text(verbatim: text ?? tr(forwarding ? "compose_forwarded" : "compose_quoted"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(text == nil ? 1 : 6)
                .padding(.leading, 12)
                .overlay(alignment: .leading) { Rectangle().fill(.quaternary).frame(width: 3) }
            Spacer()
            if let onRemove {
                Button(action: onRemove) { Image(systemName: "xmark").foregroundStyle(.secondary) }
                    .accessibilityLabel(Text("action_remove"))
            }
        }
    }
}

/// Keeps a message being written on the phone as it changes. Saves, the send and a discard run
/// one after another, each with the newest text and the id the first save got, so they outlive
/// the screen and the last one still lands after it closes. Once the message is sent or thrown
/// away, nothing more is saved.
final class DraftKeeper: Sendable {
    private let mail: Mail
    private let state: State

    private actor State {
        var id: Int64
        var latest: DraftItem?
        var done = false
        /// What the last save wrote, so the same text is not written again.
        var written: DraftItem?
        /// The operation running or waiting last; the next one starts after it. An actor alone
        /// would let a send in while a save waits for the core, with the draft's id not yet known.
        private var last: Task<Void, Never>?

        init(id: Int64) {
            self.id = id
        }

        /// Runs `operation` after every one queued before it.
        private func queue(_ operation: @escaping @Sendable (isolated State) async -> Void) -> Task<Void, Never> {
            let before = last
            let task = Task {
                await before?.value
                await operation(self)
            }
            last = task
            return task
        }

        func save(_ draft: DraftItem, with mail: Mail) {
            latest = draft
            _ = queue { state in await state.write(with: mail) }
        }

        private func write(with mail: Mail) async {
            // A later call already saved it.
            guard let newest = latest else { return }
            latest = nil
            guard !done, newest != written else { return }
            do {
                if !newest.isBlank {
                    var draft = newest
                    draft.id = id
                    id = try await mail.saveDraft(draft)
                } else if id != 0 {
                    try await mail.deleteDraft(id)
                    id = 0
                }
                written = newest
            } catch {
                // Gone: sent or thrown away elsewhere; a later save tries again otherwise.
                if case CoreError.Gone? = error as? CoreError { done = true }
            }
        }

        func send(_ draft: DraftItem, with mail: Mail) async throws -> Int64 {
            latest = nil
            var result: Result<Int64, Error> = .failure(CancellationError())
            await queue { state in
                var draft = draft
                draft.id = state.id
                do {
                    result = .success(try await mail.send(draft))
                    state.done = true
                } catch {
                    result = .failure(error)
                }
            }.value
            return try result.get()
        }

        func discard(with mail: Mail) {
            latest = nil
            done = true
            _ = queue { state in
                if state.id != 0 { try? await mail.deleteDraft(state.id) }
            }
        }
    }

    init(mail: Mail, id: Int64) {
        self.mail = mail
        self.state = State(id: id)
    }

    /// Saves `draft` soon; one with nothing written throws the saved one away.
    func save(_ draft: DraftItem) {
        Task { await state.save(draft, with: mail) }
    }

    /// Sends `draft`, and the saved draft goes with it.
    func send(_ draft: DraftItem) async throws -> Int64 {
        try await state.send(draft, with: mail)
    }

    /// Throws the saved draft away, and stops saving.
    func discard() {
        Task { await state.discard(with: mail) }
    }
}

/// The drafts on the phone, last saved first; a tap opens one to go on writing.
struct DraftsView: View {
    let model: InboxModel

    var body: some View {
        List {
            if model.drafts.isEmpty {
                ContentUnavailableView("drafts_empty", systemImage: "square.and.pencil")
                    .listRowSeparator(.hidden)
            }
            ForEach(model.drafts, id: \.id) { draft in
                Button { model.openDraft(draft) } label: { DraftRow(draft: draft) }
                    .buttonStyle(.plain)
            }
        }
        .listStyle(.plain)
        .navigationTitle("folder_drafts")
        .task { await model.counts() }
    }
}

/// Who it goes to and when it was saved, then the subject and the start of the text.
private struct DraftRow: View {
    let draft: DraftItem

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(verbatim: draft.to.isBlank ? tr("drafts_no_recipients") : draft.to)
                    .font(.body.weight(.medium)).lineLimit(1)
                Spacer()
                Text(verbatim: shortDate(draft.saved)).font(.footnote).foregroundStyle(.secondary)
            }
            Text(verbatim: draft.subject.isBlank ? tr("inbox_no_subject") : draft.subject)
                .font(.subheadline).lineLimit(1)
            let text = draft.text.trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty {
                Text(verbatim: text).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
            }
        }
        .padding(.vertical, 6)
        .contentShape(.rect)
    }
}

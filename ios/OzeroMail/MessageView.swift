import SwiftUI

/// A conversation, oldest first, on one page that scrolls title and all. What was unread when it
/// opened is open, and the page starts at the first of it; the rest sit closed as a line each, and
/// a tap opens or closes any of them. Under each open one in a conversation, Reply and Forward
/// answer that very message.
///
/// Reply, Delete, Snooze, More and Done sit in one row at the bottom, in reach of the thumb; the
/// row steps aside while the page is read down and comes back on the way up and at the end. A
/// reply is written low on the same page, the message still in sight above it; one more tap takes
/// it to the whole screen.
struct MessageView: View {
    let model: InboxModel
    let message: MessageItem
    /// Beside the list, on its side: the actions fit one row.
    var pane = false

    @State private var thread: [MessageItem] = []
    @State private var error: LocalizedStringKey?
    /// What the user opened or closed.
    @State private var opened: [Int64: Bool] = [:]
    /// What was unread when it opened, and every message it had then; nil until read.
    @State private var unread: Set<Int64>?
    @State private var known: Set<Int64> = []
    @State private var showAll = false
    @State private var more = false
    /// The folders the message can move to, picked from More.
    @State private var targets: [(folder: FolderItem, title: String)]?
    @State private var files = Files()
    /// Loaded bodies and who each message went to, so a message opened again shows at once.
    @State private var bodies = Bodies()
    @State private var people: [Int64: PeopleItem] = [:]
    @State private var details: Set<Int64> = []
    @State private var person: PersonItem?
    @State private var replying: Replying?
    @State private var barShown = true
    /// A reply on its way to the page: a second tap meanwhile starts nothing more.
    @State private var answering = false
    /// The rest of the conversation downloading; it stops when the message closes.
    @State private var filling: Task<Void, Never>?

    /// How long a conversation waits for the bodies that open before it shows.
    private static let bodyWait = Duration.milliseconds(250)

    private var mail: Mail { model.mail }

    /// Unread then, new since, or the one from the list when nothing was unread.
    private func isOpen(_ m: MessageItem) -> Bool {
        if let opened = opened[m.id] { return opened }
        guard let unread else { return m.id == message.id }
        return unread.contains(m.id) || !known.contains(m.id) || (unread.isEmpty && m.id == message.id)
    }

    var body: some View {
        let shown = thread.isEmpty ? [message] : thread
        // The newest message not from the user.
        let answer = shown.last { !$0.sent } ?? message
        let many = shown.count > 1 || message.threadCount > 1
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    Title(message: message)
                    // Until the conversation is read only the title shows, so nothing jumps in above.
                    let ready = unread != nil || error != nil
                    // A long run of closed messages in the middle folds into one line.
                    let middle = 1..<max(1, shown.count - 2)
                    let folded = !showAll && middle.count >= 2 && !middle.contains { isOpen(shown[$0]) }
                    ForEach(Array((ready ? shown : []).enumerated()), id: \.element.id) { i, m in
                        if i == 1 && folded {
                            FoldedLine(count: middle.count) { withAnimation(.smooth) { showAll = true } }
                        }
                        if !(folded && middle.contains(i)) {
                            letter(m, many: many)
                        }
                    }
                    if let error {
                        Text(error).foregroundStyle(.secondary).padding(20)
                    }
                    // The end of the page clears the row over it.
                    Color.clear.frame(height: pane || replying != nil ? 12 : barHeight)
                }
            }
            .onScrollGeometryChange(for: Scrolled.self) { geometry in
                Scrolled(
                    offset: geometry.contentOffset.y,
                    atEnd: geometry.contentOffset.y + geometry.containerSize.height >= geometry.contentSize.height - 1)
            } action: { old, new in
                // At the end, where what to do with the message is decided, it is back.
                var shows = barShown
                if new.atEnd || new.offset <= 0 {
                    shows = true
                } else if new.offset > old.offset + 2 {
                    shows = false
                } else if new.offset < old.offset - 2 {
                    shows = true
                }
                if shows != barShown { withAnimation(.smooth(duration: 0.25)) { barShown = shows } }
            }
            .onChange(of: unread) { _, new in
                // The first message is under the title: it opens at the top.
                guard let new, let first = shown.first(where: { new.contains($0.id) }), first.id != shown.first?.id else { return }
                proxy.scrollTo(first.id, anchor: .top)
            }
        }
        .middle(Room.letter)
        .safeAreaInset(edge: .top, spacing: 0) {
            if pane {
                // Back to the mailboxes beside the list.
                Button(action: model.closePane) {
                    Image(systemName: "chevron.left").font(.title3.weight(.semibold)).frame(width: 44, height: 44)
                }
                .accessibilityLabel(Text("action_back"))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 8)
            }
        }
        .overlay(alignment: .bottom) {
            if !pane && replying == nil && barShown {
                bar(answer: answer).transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .overlay(alignment: .bottom) {
            // The list's toast, over the message: Undo for a reply, or where a draft went.
            if !pane, let toast = model.toast {
                ToastView(toast: toast, onUndo: model.tapUndo, onGo: model.tapGo)
                    .padding(.bottom, replying == nil && barShown ? barHeight : 12)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.smooth, value: model.toast)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let replying {
                QuickReply(
                    mail: mail,
                    replying: Binding { self.replying ?? replying } set: { self.replying = $0 },
                    onClose: { kept in
                        self.replying = nil
                        if kept { model.kept() }
                    },
                    onExpand: { draft in
                        self.replying = nil
                        model.composing = Composing(draft: draft, original: replying.original)
                    },
                    onSent: { sent($0, self.replying ?? replying) })
                .id(replying.draft.id)
                .transition(.move(edge: .bottom).combined(with: .opacity))
            } else if pane {
                row(answer: answer, newest: shown.last ?? message)
            }
        }
        .animation(.smooth, value: replying == nil)
        .confirmationDialog("action_more", isPresented: $more) {
            if !pane {
                Button("message_reply_all") { reply(answer, all: true) }
                Button("message_forward") { forward(shown.last ?? message) }
            }
            // As the conversation has it now, flagged here or elsewhere.
            let flagged = (thread.first { $0.id == message.id } ?? message).flagged
            Button(flagged ? "action_unflag" : "action_flag") {
                Task { await model.flag(message, flagged: !flagged) }
            }
            Button("action_mark_unread") {
                leave { await model.setRead([message], read: false) }
            }
            if message.canMoveToInbox {
                Button("action_move_to_inbox") {
                    leave { await model.moveToInbox([message]) }
                }
            }
            if message.canMove {
                Button("action_move_to_folder") {
                    Task {
                        do {
                            targets = try await moveOrder(mail.moveTargets(of: message))
                        } catch {
                            model.error = error.textKey()
                        }
                    }
                }
            }
            if message.canSpam {
                Button("action_spam", role: .destructive) {
                    leave { await model.spam([message]) }
                }
            }
        }
        .confirmationDialog(
            "move_title", isPresented: Binding { targets != nil } set: { if !$0 { targets = nil } },
            titleVisibility: .visible, presenting: targets
        ) { targets in
            ForEach(targets, id: \.folder.id) { target in
                Button(target.title) {
                    leave { await model.moveToFolder([message], to: target.folder, title: target.title) }
                }
            }
        } message: { targets in
            if targets.isEmpty { Text("move_none") }
        }
        .confirmationDialog(
            person?.name ?? "", isPresented: Binding { person != nil } set: { if !$0 { person = nil } },
            titleVisibility: .visible, presenting: person
        ) { person in
            Button("message_write_to") { model.write(to: person.email) }
            Button("message_copy_address") { UIPasteboard.general.string = person.email }
        } message: { person in
            if person.name != person.email { Text(verbatim: person.email) }
        }
        .attachmentActions(files)
        .onDisappear { filling?.cancel() }
        .task(id: message.id) {
            do {
                // Read before it is marked read, so what was new opens.
                var first = try await mail.thread(of: message)
                if first.isEmpty { first = [message] }
                let new = Set(first.filter { !$0.seen }.map(\.id))
                // What opens comes with who it went to and its body, so the page shows whole at
                // once, not line by line. A body still to download holds it up only a moment.
                let opens = first.filter { new.contains($0.id) || (new.isEmpty && $0.id == message.id) }
                for m in opens {
                    if let found = try? await mail.people(of: m) { people[m.id] = found }
                }
                let bodies = bodies
                await Task {
                    await withTaskGroup(of: Void.self) { group in
                        for m in opens { group.addTask { _ = try? await bodies.of(m, mail: mail) } }
                    }
                }.wait(upTo: Self.bodyWait)
                thread = first
                known = Set(first.map(\.id))
                unread = new
                // The rest downloads quietly, so a closed message shows how it starts and opens at once.
                filling = Task {
                    guard await bodies.fill(first, mail: mail) else { return }
                    if let found = try? await mail.thread(of: message), !found.isEmpty { thread = found }
                }
            } catch {
                self.error = error.text()
            }
            do {
                if message.threadUnread > 0 { try await mail.markRead([message]) }
            } catch {
                self.error = error.text()
            }
            // A reply that comes in meanwhile joins the conversation.
            for await _ in mail.changes() {
                do {
                    // Gone from the phone, say moved: what is on screen stays.
                    let found = try await mail.thread(of: message)
                    if !found.isEmpty { thread = found }
                } catch {
                    self.error = error.text()
                }
            }
        }
    }

    private func letter(_ m: MessageItem, many: Bool) -> some View {
        Letter(
            mail: mail, files: files, bodies: bodies, message: m, people: people[m.id], open: isOpen(m),
            unread: unread?.contains(m.id) == true, inThread: many, details: details.contains(m.id),
            onToggle: many ? { withAnimation(.smooth) { opened[m.id] = !isOpen(m) } } : nil,
            onDetails: {
                withAnimation(.smooth) {
                    if details.contains(m.id) { details.remove(m.id) } else { details.insert(m.id) }
                }
            },
            onPerson: { person = $0 },
            onReply: { reply(m, all: false) },
            onForward: { forward(m) },
            onDelete: { deleteAlone(m) })
        .id(m.id)
        .task(id: isOpen(m)) {
            guard isOpen(m), people[m.id] == nil else { return }
            // The line under the name stays empty otherwise; the message reads the same.
            if let found = try? await mail.people(of: m) { people[m.id] = found }
        }
    }

    /// Reply, Delete, Snooze, More and Done in one row; a long press on Reply offers Reply all
    /// and Forward.
    private func bar(answer: MessageItem) -> some View {
        HStack(spacing: 8) {
            BarAction(icon: "arrowshape.turn.up.left", label: "message_reply") { reply(answer, all: false) }
                .contextMenu {
                    Button("message_reply", systemImage: "arrowshape.turn.up.left") { reply(answer, all: false) }
                    Button("message_reply_all", systemImage: "arrowshape.turn.up.left.2") { reply(answer, all: true) }
                    Button("message_forward", systemImage: "arrowshape.turn.up.right") { forward(thread.last ?? message) }
                }
            BarAction(icon: "trash", label: message.canTrash ? "action_delete" : "action_delete_forever", action: delete)
            if message.canSnooze {
                BarAction(icon: "clock", label: "action_snooze", action: snooze)
            }
            BarAction(icon: "ellipsis", label: "action_more") { more = true }
            if message.canArchive {
                BarAction(icon: "checkmark", label: "action_done", tint: .white, background: .ozeroFill, action: done)
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 10)
        .padding(.bottom, 4)
        .background(.bar)
    }

    /// Beside the list, where the height is short: the answers and the actions in one row.
    private func row(answer: MessageItem, newest: MessageItem) -> some View {
        HStack(spacing: 8) {
            Button {
                reply(answer, all: false)
            } label: {
                Text("message_reply_prompt")
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, minHeight: paneButton, alignment: .leading)
                    .padding(.horizontal, 16)
                    .background(Color.ozeroCard, in: .capsule)
            }
            .buttonStyle(.plain)
            RoundButton(icon: "arrowshape.turn.up.left.2", label: "message_reply_all", size: paneButton) { reply(answer, all: true) }
            RoundButton(icon: "arrowshape.turn.up.right", label: "message_forward", size: paneButton) { forward(newest) }
            RoundButton(icon: "trash", label: message.canTrash ? "action_delete" : "action_delete_forever", size: paneButton, action: delete)
            if message.canSnooze {
                RoundButton(icon: "clock", label: "action_snooze", size: paneButton, action: snooze)
            }
            RoundButton(icon: "ellipsis", label: "action_more", size: paneButton) { more = true }
            if message.canArchive {
                RoundButton(icon: "checkmark", label: "action_done", size: paneButton, tint: .white, background: .ozeroFill, action: done)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(.bar)
    }

    /// A reply low on the page, to the sender; with `all` the others it went to are copied.
    private func reply(_ to: MessageItem, all: Bool) {
        guard replying == nil, !answering else { return }
        answering = true
        Task {
            defer { answering = false }
            do {
                async let draft = mail.reply(to: to, all: all)
                let others: [PersonItem]
                if let known = people[to.id] {
                    others = known.others
                } else {
                    let found = try await mail.people(of: to)
                    people[to.id] = found
                    others = found.others
                }
                replying = Replying(draft: try await draft, original: to, others: others, copied: all ? others.map(\.email) : [])
            } catch {
                self.error = error.text()
            }
        }
    }

    private func forward(_ newest: MessageItem) {
        model.write(newest) {
            try await mail.forward(newest, labels: forwardLabels, date: longDate(newest.date))
        }
    }

    /// Sent from the page: the toast can bring the reply back to go on writing.
    private func sent(_ id: Int64, _ reply: Replying) {
        replying = nil
        Task {
            guard await model.offerUndo(tr("inbox_sending")) else { return }
            do {
                if let draft = try await mail.undoSend(id) {
                    var again = reply
                    again.draft = draft
                    replying = again
                }
            } catch {
                self.error = error.text()
            }
        }
    }

    private func delete() {
        leave { await model.delete([message]) }
    }

    /// One message of the conversation; the last one left goes as the conversation does.
    private func deleteAlone(_ m: MessageItem) {
        let rest = thread.filter { $0.id != m.id }
        guard let other = rest.first else { return delete() }
        Task {
            do {
                let actions = try await mail.deleteAlone(m)
                let found = try await mail.thread(of: other)
                thread = found.isEmpty ? rest : found
                guard await model.offerUndo(tr(m.canTrash ? "inbox_trashed" : "inbox_deleted")) else { return }
                if (try? await mail.undo(actions)) != true { self.error = "inbox_undo_late" }
                let back = try await mail.thread(of: other)
                if !back.isEmpty { thread = back }
            } catch {
                self.error = error.text()
            }
        }
    }

    /// Back to the list, where the toast can undo it.
    private func done() {
        leave { await model.archive([message]) }
    }

    /// The message stays open until a time is picked; then back to the list, as `leave`.
    private func snooze() {
        model.snoozing = Snoozing(messages: [message]) { [model, message] in
            Self.close(message, in: model)
        }
    }

    /// Back to the list, then `act`, so the list shows the toast.
    private func leave(_ act: @escaping () async -> Void) {
        Self.close(message, in: model)
        Task { await act() }
    }

    private static func close(_ message: MessageItem, in model: InboxModel) {
        // Search over the list goes too: the toast shows in the inbox.
        model.path.removeAll { $0 == .message(message) || $0 == .search }
        if model.pane?.id == message.id { model.pane = nil }
    }

    private var forwardLabels: ForwardLabels {
        ForwardLabels(
            title: tr("compose_forward_title"), from: tr("compose_forward_from"),
            date: tr("compose_forward_date"), subject: tr("compose_forward_subject"))
    }
}

/// The round buttons beside the list, where the height is short.
private let paneButton: CGFloat = 40

/// How tall the bottom row is, for the page's end and the toast to clear it.
private let barHeight: CGFloat = 84

/// Where the page is scrolled to, and whether that is its end.
private struct Scrolled: Equatable {
    let offset: CGFloat
    let atEnd: Bool
}

/// A reply being written low on the page: its draft, the message it answers, and who else may
/// get a copy.
struct Replying: Equatable {
    var draft: DraftItem
    let original: MessageItem
    let others: [PersonItem]
    var copied: [String]
}

extension Task where Failure == Never {
    /// Waits until the task is done or `limit` has passed, whichever comes first; it runs on.
    func wait(upTo limit: Duration) async {
        let (done, finish) = AsyncStream<Void>.makeStream()
        let work = Task<Void, Never> {
            _ = await value
            finish.yield()
        }
        let timer = Task<Void, Never> {
            try? await Task<Never, Never>.sleep(for: limit)
            finish.yield()
        }
        for await _ in done { break }
        work.cancel()
        timer.cancel()
    }
}

/// A conversation's bodies, shared by its letters: each downloaded once however many ask for it.
@MainActor @Observable
final class Bodies {
    var loaded: [Int64: MessageBody] = [:]
    @ObservationIgnored private var loading: [Int64: Task<MessageBody, Error>] = [:]

    func of(_ message: MessageItem, mail: Mail) async throws -> MessageBody {
        if let saved = loaded[message.id] { return saved }
        // Not cancelled with the one that asked first: the others still wait for it.
        let task = loading[message.id] ?? Task { try await mail.body(of: message) }
        loading[message.id] = task
        defer { loading[message.id] = nil }
        let body = try await task.value
        loaded[message.id] = body
        return body
    }

    /// Downloads the missing ones one by one, stopping at a failure, as offline; true if it got any.
    func fill(_ messages: [MessageItem], mail: Mail) async -> Bool {
        var got = false
        for m in messages where loaded[m.id] == nil {
            guard !Task.isCancelled, (try? await of(m, mail: mail)) != nil else { break }
            got = true
        }
        return got
    }

    /// How many files a message has: as the list knew, or as its body says once loaded.
    func files(of message: MessageItem) -> Int {
        max(message.attachments.count, loaded[message.id]?.attachments.count ?? 0)
    }
}

/// One message: its line, and under it the message when open. In a conversation `onToggle`
/// opens or closes it, and Reply and Forward under it answer this one.
private struct Letter: View {
    let mail: Mail
    let files: Files
    let bodies: Bodies
    let message: MessageItem
    let people: PeopleItem?
    let open: Bool
    let unread: Bool
    let inThread: Bool
    let details: Bool
    let onToggle: (() -> Void)?
    let onDetails: () -> Void
    let onPerson: (PersonItem) -> Void
    let onReply: () -> Void
    let onForward: () -> Void
    let onDelete: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Divider().padding(.horizontal, 20)
            if open {
                From(
                    message: message, people: people, unread: unread, files: bodies.files(of: message), details: details,
                    onToggle: onToggle, onDetails: onDetails, onPerson: onPerson)
                LetterBody(mail: mail, files: files, bodies: bodies, message: message)
                    .transition(.opacity.combined(with: .move(edge: .top)))
                if inThread {
                    HStack(spacing: 8) {
                        if !message.sent { Chip(icon: "arrowshape.turn.up.left", label: "message_reply", action: onReply) }
                        Chip(icon: "arrowshape.turn.up.right", label: "message_forward", action: onForward)
                        Chip(icon: "trash", label: "action_delete", tint: .red, action: onDelete)
                    }
                    .padding(.horizontal, 20)
                    .padding(.bottom, 14)
                }
            } else {
                Closed(message: message, unread: unread, onToggle: onToggle)
            }
        }
    }
}

/// A small rounded button under a message.
private struct Chip: View {
    let icon: String
    let label: LocalizedStringKey
    var tint = Color.ozeroAccent
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(label, systemImage: icon)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(tint)
                .padding(.horizontal, 12)
                .frame(minHeight: 36)
                .background(Color.ozeroCard, in: .capsule)
        }
        .buttonStyle(.plain)
    }
}

/// The line of closed messages folded away; a tap shows them.
private struct FoldedLine: View {
    let count: Int
    let action: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Divider().padding(.horizontal, 20)
            Button(action: action) {
                Text(verbatim: tr("message_more", count))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Color.ozeroAccent)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 20)
                    .padding(.vertical, 13)
            }
        }
    }
}

/// A message's page, or its text with the quote folded; it loads when shown, once into
/// `bodies`. It is as tall as it is, and its files follow the text.
private struct LetterBody: View {
    let mail: Mail
    let files: Files
    let bodies: Bodies
    let message: MessageItem

    /// How long a page with remote images waits for them before it shows.
    private static let imagesWait = Duration.seconds(2)

    @State private var body_: MessageBody?
    @State private var error: LocalizedStringKey?
    /// The page again with its remote images.
    @State private var withImages: ImagesPage?
    @State private var imagesError: LocalizedStringKey?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let note = imagesNote {
                HiddenImages(note: note, onShow: showImages)
            }
            content
            if let attachments = body_?.attachments, !attachments.isEmpty {
                AttachmentList(mail: mail, files: files, message: message, items: attachments)
            }
        }
        .task(id: message.id) { await load() }
    }

    @ViewBuilder
    private var content: some View {
        if let full = withImages {
            page(full.html, images: full.images, styled: body_?.styled ?? false)
        } else if let loaded = body_, let html = loaded.html {
            page(html, images: [], styled: loaded.styled)
        } else if let loaded = body_ {
            PlainText(text: loaded.text, quote: loaded.quote)
        } else if let error {
            Text(error).foregroundStyle(.secondary).padding(20)
        } else {
            ProgressView().frame(maxWidth: .infinity, minHeight: 96)
        }
    }

    private func page(_ html: String, images: [RemoteImage?], styled: Bool) -> some View {
        // A new web view per page: it loads its page only once.
        // A newsletter keeps the light page it was made for, as a card on the screen.
        HTMLView(html: html, images: images, message: message.id)
            .id("\(message.id)-\(images.isEmpty)")
            .clipShape(.rect(cornerRadius: styled ? 16 : 0))
            .padding(.horizontal, styled ? 10 : 0)
            .padding(.top, 4)
            .padding(.bottom, styled ? 10 : 0)
    }

    private var imagesNote: LocalizedStringKey? {
        guard let loaded = body_, withImages == nil else { return nil }
        if let imagesError { return imagesError }
        return loaded.remoteImages > 0 && !loaded.showsImages ? "message_images_hidden" : nil
    }

    private func load() async {
        do {
            let loaded = try await bodies.of(message, mail: mail)
            if loaded.remoteImages > 0 && loaded.showsImages {
                // Wait a moment for the images, so the page does not load twice and jump.
                await Task { await loadImages() }.wait(upTo: Self.imagesWait)
            }
            body_ = loaded
        } catch {
            self.error = error.text()
        }
    }

    private func loadImages() async {
        do {
            if let page = try await mail.images(of: message) { withImages = page }
        } catch {
            imagesError = error.text()
        }
    }

    private func showImages() {
        // The note is closing: the images are here already.
        guard withImages == nil else { return }
        imagesError = nil
        Task {
            do {
                try await mail.allowImages(of: message)
                // The sender's other messages load theirs on their own now too.
                let allowed = bodies.loaded[message.id].map { body -> MessageBody in
                    var body = body
                    body.showsImages = true
                    return body
                }
                bodies.loaded = [:]
                if let allowed { bodies.loaded[message.id] = allowed }
                await loadImages()
            } catch {
                imagesError = error.text()
            }
        }
    }
}

/// The text, and the quote under it behind a mark that opens it.
private struct PlainText: View {
    let text: String
    let quote: String
    @State private var showQuote = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if !text.isEmpty {
                Text(verbatim: text).font(.reading).lineSpacing(6).textSelection(.enabled)
            }
            if !quote.isEmpty {
                Button {
                    withAnimation(.smooth) { showQuote.toggle() }
                } label: {
                    Text(verbatim: "•••")
                        .font(.caption.weight(.bold))
                        .kerning(1)
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 1)
                        .background(Color.ozeroCard, in: .rect(cornerRadius: 9))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("message_quote"))
                if showQuote {
                    Text(verbatim: quote).font(.readingQuote).foregroundStyle(.secondary).textSelection(.enabled)
                }
            }
        }
        .frame(maxWidth: 620, alignment: .leading)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 20)
        .padding(.top, 6)
        .padding(.bottom, 18)
    }
}

/// A note that the remote images are held back, with a way to see them from now on.
private struct HiddenImages: View {
    let note: LocalizedStringKey
    let onShow: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Text(note).font(.footnote).foregroundStyle(.secondary)
            Spacer()
            Button("action_show", action: onShow)
                .font(.footnote.weight(.semibold))
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(Color(uiColor: .systemBackground), in: .capsule)
        }
        .padding(.leading, 14)
        .padding([.trailing, .vertical], 8)
        .background(Color.ozeroCard, in: .rect(cornerRadius: 14))
        .padding(.horizontal, 16)
        .padding(.top, 4)
        .padding(.bottom, 8)
    }
}

extension MessageItem {
    /// The name the user is shown by in a message: "You" when they wrote it.
    fileprivate var from: String { sent ? tr("message_you") : sender }
}

/// Who wrote an open message, to whom, and when, in two lines: "Anna Lee" and "To you, Oleg and
/// Maria". A tap on the name offers what to do with the person; a tap on the second line shows
/// every address and the full date. With `onToggle` a tap elsewhere closes the message.
private struct From: View {
    let message: MessageItem
    let people: PeopleItem?
    let unread: Bool
    let files: Int
    let details: Bool
    let onToggle: (() -> Void)?
    let onDetails: () -> Void
    let onPerson: (PersonItem) -> Void

    var body: some View {
        let author = PersonItem(name: message.sender, first: message.sender, email: message.fromEmail, me: message.sent)
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Button { onPerson(author) } label: {
                        Text(verbatim: message.from).font(.body.weight(unread ? .bold : .semibold)).lineLimit(1)
                    }
                    .buttonStyle(.plain)
                    .disabled(message.sent)
                    if let line = people.map(toLine), !line.isEmpty {
                        Button(action: onDetails) {
                            HStack(spacing: 4) {
                                Text(verbatim: line).lineLimit(1)
                                Image(systemName: "chevron.down").font(.caption.weight(.semibold)).rotationEffect(.degrees(details ? 180 : 0))
                            }
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                    }
                }
                Spacer()
                VStack(alignment: .trailing, spacing: 4) {
                    Text(verbatim: message.sending ? tr("inbox_sending") : shortDate(message.date))
                        .font(.footnote.monospacedDigit())
                        .foregroundStyle(.secondary)
                    if files > 0 {
                        Label(String(files), systemImage: "paperclip").font(.footnote).foregroundStyle(.secondary)
                            .accessibilityLabel(Text(verbatim: tr("attachment_count", files)))
                    }
                }
                .padding(.top, 2)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
            .contentShape(.rect)
            .onTapGesture { onToggle?() }
            if details, let people {
                Details(message: message, author: author, people: people, onPerson: onPerson)
                    .transition(.opacity)
            }
        }
    }
}

/// "To you, Oleg and Maria": who a message went to, copies too, by first name.
private func toLine(_ people: PeopleItem) -> String {
    let names = people.shown.map { $0.me ? tr("message_you_to") : $0.first }
    return names.isEmpty ? "" : tr("message_to_line", joinNames(names))
}

/// Every address of a message and its full date, each person a tap away.
private struct Details: View {
    let message: MessageItem
    let author: PersonItem
    let people: PeopleItem
    let onPerson: (PersonItem) -> Void

    var body: some View {
        Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 10, verticalSpacing: 8) {
            field("message_from", [author])
            field("compose_to", people.to)
            field("compose_cc", people.cc)
            GridRow {
                Text("message_date").foregroundStyle(.secondary)
                Text(verbatim: longDate(message.date))
            }
        }
        .font(.footnote)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(Color.ozeroCard, in: .rect(cornerRadius: 14))
        .padding(.horizontal, 16)
        .padding(.bottom, 10)
    }

    @ViewBuilder
    private func field(_ label: LocalizedStringKey, _ persons: [PersonItem]) -> some View {
        if !persons.isEmpty {
            GridRow {
                Text(label).foregroundStyle(.secondary)
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(persons, id: \.email) { person in
                        Button { onPerson(person) } label: {
                            Text(verbatim: person.name == person.email ? person.email : "\(person.name) \(person.email)")
                                .multilineTextAlignment(.leading)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }
}

/// A closed message in a conversation: who, the start of its text, and when; a tap opens it.
private struct Closed: View {
    let message: MessageItem
    let unread: Bool
    let onToggle: (() -> Void)?

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: message.from).font(.body.weight(unread ? .bold : .semibold)).lineLimit(1)
                Text(verbatim: message.snippet).font(.footnote).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer()
            Text(verbatim: shortDate(message.date))
                .font(.footnote.monospacedDigit())
                .foregroundStyle(.secondary)
                .padding(.top, 2)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .contentShape(.rect)
        .onTapGesture { onToggle?() }
    }
}

/// The subject and the account it came to.
private struct Title: View {
    let message: MessageItem

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            message.subjectText.font(.title2.weight(.bold)).textSelection(.enabled)
            if let account = message.account {
                HStack(spacing: 7) {
                    AccountDot(color: account.tint, size: 8)
                    Text(verbatim: account.name).font(.footnote.weight(.medium)).foregroundStyle(.secondary)
                }
                .padding(.horizontal, 11)
                .padding(.vertical, 6)
                .background(Color.ozeroCard, in: .capsule)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 6)
        .padding(.bottom, 8)
    }
}

/// A reply written low on the page, over the keyboard: to the sender, with the others it went
/// to a tap away as copies. It is kept as a draft as it is written; `onExpand` takes it to the
/// whole screen, and `onClose` says whether it was kept.
private struct QuickReply: View {
    let mail: Mail
    @Binding var replying: Replying
    let onClose: (_ kept: Bool) -> Void
    let onExpand: (DraftItem) -> Void
    let onSent: (Int64) -> Void

    @State private var text = ""
    @State private var keeper: DraftKeeper?
    @State private var busy = false
    @State private var error: LocalizedStringKey?
    @FocusState private var focused: Bool
    @Environment(\.scenePhase) private var scenePhase

    private var start: DraftItem { replying.draft }

    private var current: DraftItem {
        var draft = start
        draft.text = text
        draft.cc = replying.copied.joined(separator: ", ")
        return draft
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Divider()
            Flow(spacing: 6) {
                let original = replying.original
                Person(text: start.to.caseInsensitiveCompare(original.fromEmail) == .orderedSame ? original.sender : start.to)
                ForEach(replying.others.filter { replying.copied.contains($0.email) }, id: \.email) { person in
                    Person(text: tr("message_copy_person", person.name)) {
                        replying.copied.removeAll { $0 == person.email }
                    }
                }
                let left = replying.others.filter { !replying.copied.contains($0.email) }
                if !left.isEmpty {
                    Person(text: tr("message_add_people", joinNames(left.map(\.first))), primary: true) {
                        replying.copied += left.map(\.email)
                    }
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 10)
            TextField("message_reply_prompt", text: $text, axis: .vertical)
                .font(.body)
                .lineLimit(2...6)
                .focused($focused)
                .padding(.horizontal, 20)
                .padding(.vertical, 8)
            if let error {
                Text(error).font(.footnote).foregroundStyle(.red).padding(.horizontal, 20)
            }
            HStack(spacing: 8) {
                RoundButton(icon: "xmark", label: "action_close", size: 44, action: close)
                RoundButton(icon: "arrow.up.left.and.arrow.down.right", label: "message_full_screen", size: 44, action: expand)
                    .disabled(busy)
                Spacer()
                RoundButton(icon: "arrow.up", label: "compose_send", size: 44, tint: .white, background: .ozeroFill, action: send)
                    .disabled(busy)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
        }
        .background(.bar)
        .onAppear {
            text = start.text
            keeper = DraftKeeper(mail: mail, id: start.id)
            focused = true
        }
        // Leaving the page keeps what is written; after a send this does nothing.
        .onDisappear {
            if !current.isBlank { keeper?.save(current) }
        }
        .onChange(of: scenePhase) { _, phase in
            if phase != .active, !current.isBlank { keeper?.save(current) }
        }
        // A pause in typing saves the draft, so each key does not.
        .task(id: current) {
            guard current.text != start.text || current.cc != start.cc else { return }
            try? await Task.sleep(for: DraftKeeper.pause)
            keeper?.save(current)
        }
    }

    private func close() {
        let draft = current
        if draft.isBlank {
            Task { _ = await keeper?.discard() }
            onClose(false)
        } else {
            keeper?.save(draft)
            onClose(true)
        }
    }

    private func expand() {
        guard let keeper else { return }
        let draft = current
        Task {
            do {
                var handed = draft
                handed.id = try await keeper.handOff(draft)
                onExpand(handed)
            } catch {
                self.error = error.text()
            }
        }
    }

    private func send() {
        guard !busy, let keeper else { return }
        busy = true
        error = nil
        let draft = current
        Task {
            defer { busy = false }
            do {
                onSent(try await keeper.send(draft))
            } catch {
                self.error = error.text()
            }
        }
    }
}

/// Someone a reply goes to, as a chip; `action` takes them off or adds them.
private struct Person: View {
    let text: String
    var primary = false
    var action: (() -> Void)?

    var body: some View {
        Button { action?() } label: {
            Text(verbatim: text)
                .font(.footnote.weight(.medium))
                .foregroundStyle(primary ? Color.ozeroAccent : .primary)
                .lineLimit(1)
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(Color.ozeroCard, in: .capsule)
        }
        .buttonStyle(.plain)
        .disabled(action == nil)
    }
}

/// A round button of a bar.
struct RoundButton: View {
    let icon: String
    let label: LocalizedStringKey
    var size: CGFloat = 46
    var tint: Color = .primary
    var background: Color = .ozeroCard
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.body.weight(.medium))
                .foregroundStyle(tint)
                .frame(width: size, height: size)
                .background(background, in: .circle)
        }
        .accessibilityLabel(Text(label))
    }
}

/// One of the big buttons along the bottom of a screen: an icon over its name.
struct BarAction: View {
    let icon: String
    let label: LocalizedStringKey
    var tint: Color = .primary
    var background: Color = .clear
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 3) {
                Image(systemName: icon).font(.system(size: 19, weight: .medium))
                Text(label).font(.caption2.weight(.medium)).lineLimit(1)
            }
            .foregroundStyle(tint)
            .frame(maxWidth: .infinity, minHeight: 52)
            .background(background, in: .rect(cornerRadius: 14))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

extension Font {
    /// A message's text: a serif made for reading, at a size it is read at.
    static let reading = Font.system(size: 17.5, design: .serif)

    /// Quoted text under a message, a step quieter.
    static let readingQuote = Font.system(size: 15.5, design: .serif)
}

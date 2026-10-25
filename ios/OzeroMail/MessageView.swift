import SwiftUI

/// A conversation, oldest first, on one page. The message opened from the list is open; the
/// others sit closed as a line each, unread ones in bold, and a tap opens or closes any of them.
/// Reply, Reply all, Forward and Done sit at the bottom, in reach of the thumb.
struct MessageView: View {
    let model: InboxModel
    let message: MessageItem

    @State private var thread: [MessageItem] = []
    @State private var error: LocalizedStringKey?
    /// What the user opened or closed; at first only the message from the list is open.
    @State private var opened: [Int64: Bool] = [:]
    @State private var showAll = false
    @State private var more = false
    @State private var files = Files()
    /// Loaded bodies, so a message opened again shows at once.
    @State private var bodies = Bodies()

    private var mail: Mail { model.mail }

    private func isOpen(_ m: MessageItem) -> Bool { opened[m.id] ?? (m.id == message.id) }

    var body: some View {
        let shown = thread.isEmpty ? [message] : thread
        // The newest message not from the user.
        let answer = shown.last { !$0.sent } ?? message
        Group {
            if shown.count > 1 || message.threadCount > 1 {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        Title(message: message)
                        // A long run of closed messages in the middle folds into one line.
                        let middle = 1..<max(1, shown.count - 2)
                        let folded = !showAll && middle.count >= 2 && !middle.contains { isOpen(shown[$0]) }
                        ForEach(Array(shown.enumerated()), id: \.element.id) { i, m in
                            if i == 1 && folded {
                                FoldedLine(count: middle.count) { withAnimation(.smooth) { showAll = true } }
                            }
                            if !(folded && middle.contains(i)) {
                                Letter(mail: mail, files: files, bodies: bodies, message: m, open: isOpen(m)) {
                                    withAnimation(.smooth) { opened[m.id] = !isOpen(m) }
                                }
                            }
                        }
                        if let error {
                            Text(error).foregroundStyle(.secondary).padding(20)
                        }
                    }
                }
            } else {
                VStack(alignment: .leading, spacing: 0) {
                    Title(message: message)
                    From(message: message, open: true, files: fileCount(message))
                    Divider().padding(.top, 4)
                    LetterBody(mail: mail, files: files, bodies: bodies, message: message, fill: true)
                }
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            VStack(spacing: 8) {
                HStack(spacing: 8) {
                    Button {
                        model.write(answer) { try await mail.reply(to: answer, all: false) }
                    } label: {
                        Text("message_reply_prompt")
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, minHeight: 46, alignment: .leading)
                            .padding(.horizontal, 16)
                            .background(Color.ozeroCard, in: .capsule)
                    }
                    .buttonStyle(.plain)
                    RoundButton(icon: "arrowshape.turn.up.left.2", label: "message_reply_all") {
                        model.write(answer) { try await mail.reply(to: answer, all: true) }
                    }
                    // The newest message, the user's own too.
                    RoundButton(icon: "arrowshape.turn.up.right", label: "message_forward") {
                        let newest = shown.last ?? message
                        model.write(newest) {
                            try await mail.forward(newest, labels: forwardLabels, date: longDate(newest.date))
                        }
                    }
                }
                HStack(spacing: 8) {
                    BarAction(icon: "trash", label: message.canTrash ? "action_delete" : "action_delete_forever") {
                        leave { await model.delete([message]) }
                    }
                    BarAction(icon: "ellipsis", label: "action_more") { more = true }
                    if message.canArchive {
                        BarAction(icon: "checkmark", label: "action_done", tint: .white, background: .ozeroFill) {
                            // Back to the list, where the toast can undo it.
                            leave { await model.archive([message]) }
                        }
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .padding(.bottom, 4)
            .background(.bar)
        }
        .confirmationDialog("action_more", isPresented: $more) {
            Button("action_mark_unread") {
                leave { await model.setRead([message], read: false) }
            }
            if message.canMoveToInbox {
                Button("action_move_to_inbox") {
                    leave { await model.moveToInbox([message]) }
                }
            }
        }
        .attachmentActions(files)
        .task(id: message.id) {
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

    /// Back to the list, then `act`, so the list shows the toast.
    private func leave(_ act: @escaping () async -> Void) {
        // Search over the list goes too: the toast shows in the inbox.
        model.path.removeAll { $0 == .message(message) || $0 == .search }
        Task { await act() }
    }

    /// How many files a message has: as the list knew, or as its body says once loaded.
    private func fileCount(_ m: MessageItem) -> Int {
        bodies.files(of: m)
    }

    private var forwardLabels: ForwardLabels {
        ForwardLabels(
            title: tr("compose_forward_title"), from: tr("compose_forward_from"),
            date: tr("compose_forward_date"), subject: tr("compose_forward_subject"))
    }
}

/// Loaded bodies of one conversation, shared by its letters.
@Observable
final class Bodies {
    var loaded: [Int64: MessageBody] = [:]

    /// How many files a message has: as the list knew, or as its body says once loaded.
    func files(of message: MessageItem) -> Int {
        max(message.attachments.count, loaded[message.id]?.attachments.count ?? 0)
    }
}

/// One message of a conversation: its line, and under it the message when open.
private struct Letter: View {
    let mail: Mail
    let files: Files
    let bodies: Bodies
    let message: MessageItem
    let open: Bool
    let onToggle: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Divider().padding(.horizontal, 20)
            From(message: message, open: open, files: bodies.files(of: message), onToggle: onToggle)
            if open {
                LetterBody(mail: mail, files: files, bodies: bodies, message: message, fill: false)
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
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
/// `bodies`. With `fill` it takes the space it is given and scrolls itself; otherwise it is as
/// tall as it is. Its files follow the text.
private struct LetterBody: View {
    let mail: Mail
    let files: Files
    let bodies: Bodies
    let message: MessageItem
    let fill: Bool

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
            if !fill || body_?.html != nil, let attachments = body_?.attachments, !attachments.isEmpty {
                AttachmentList(mail: mail, files: files, message: message, items: attachments)
            }
        }
        .frame(maxHeight: fill ? .infinity : nil, alignment: .top)
        .task(id: message.id) { await load() }
    }

    @ViewBuilder
    private var content: some View {
        if let full = withImages {
            page(full.html, images: full.images, styled: body_?.styled ?? false)
        } else if let loaded = body_, let html = loaded.html {
            page(html, images: [], styled: loaded.styled)
        } else if let loaded = body_ {
            if fill {
                ScrollView {
                    PlainText(text: loaded.text, quote: loaded.quote)
                    if !loaded.attachments.isEmpty {
                        AttachmentList(mail: mail, files: files, message: message, items: loaded.attachments)
                    }
                }
            } else {
                PlainText(text: loaded.text, quote: loaded.quote)
            }
        } else if let error {
            Text(error).foregroundStyle(.secondary).padding(20)
        } else {
            ProgressView().frame(maxWidth: .infinity, minHeight: fill ? nil : 96).frame(maxHeight: fill ? .infinity : nil)
        }
    }

    private func page(_ html: String, images: [RemoteImage?], styled: Bool) -> some View {
        // A new web view per page: it loads its page only once.
        // A newsletter keeps the light page it was made for, as a card on the screen.
        HTMLView(html: html, images: images, message: message.id, fits: !fill)
            .id("\(message.id)-\(images.isEmpty)")
            .clipShape(.rect(cornerRadius: styled ? 16 : 0))
            .padding(.horizontal, styled ? 10 : 0)
            .padding(.top, 4)
            .padding(.bottom, styled ? 10 : 0)
            .frame(maxHeight: fill ? .infinity : nil)
    }

    private var imagesNote: LocalizedStringKey? {
        guard let loaded = body_, withImages == nil else { return nil }
        if let imagesError { return imagesError }
        return loaded.remoteImages > 0 && !loaded.showsImages ? "message_images_hidden" : nil
    }

    private func load() async {
        do {
            let loaded: MessageBody
            if let saved = bodies.loaded[message.id] {
                loaded = saved
            } else {
                loaded = try await mail.body(of: message)
                bodies.loaded[message.id] = loaded
            }
            if loaded.remoteImages > 0 && loaded.showsImages {
                // Wait a moment for the images, so the page does not load twice and jump.
                let images = Task { await loadImages() }
                _ = await withTaskGroup(of: Void.self) { group in
                    group.addTask { await images.value }
                    group.addTask { try? await Task.sleep(for: .seconds(2)) }
                    await group.next()
                    group.cancelAll()
                }
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

/// Who sent a message and when. Open, it shows the address and the full date; closed, the start
/// of the text. With `onToggle` a tap opens or closes the message.
private struct From: View {
    let message: MessageItem
    let open: Bool
    let files: Int
    var onToggle: (() -> Void)?

    var body: some View {
        let unread = message.threadUnread > 0
        let detail = !open ? message.snippet : message.sender != message.fromEmail ? message.fromEmail : ""
        HStack(alignment: .center, spacing: 12) {
            Avatar(name: message.sender, key: message.fromEmail, size: open ? 40 : 32)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: message.sender).font(.body.weight(unread ? .bold : .semibold)).lineLimit(1)
                if !detail.isBlank {
                    Text(verbatim: detail).font(.footnote).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 4) {
                Text(verbatim: message.sending ? tr("inbox_sending") : open ? longDate(message.date) : shortDate(message.date))
                    .font(.footnote.monospacedDigit())
                    .foregroundStyle(.secondary)
                if open && files > 0 {
                    Label(String(files), systemImage: "paperclip").font(.footnote).foregroundStyle(.secondary)
                        .accessibilityLabel(Text(verbatim: tr("attachment_count", files)))
                }
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .contentShape(.rect)
        .onTapGesture { onToggle?() }
        .textSelection(.enabled)
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

/// A round button of a bar.
struct RoundButton: View {
    let icon: String
    let label: LocalizedStringKey
    var size: CGFloat = 46
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.body.weight(.medium))
                .foregroundStyle(.primary)
                .frame(width: size, height: size)
                .background(Color.ozeroCard, in: .circle)
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

/// A sender's initials on a colour that is the same every time for them.
struct Avatar: View {
    let name: String
    let key: String
    var size: CGFloat = 44

    var body: some View {
        Text(verbatim: initials(name))
            .font(.system(size: size * 0.36, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(avatarColor(key), in: .circle)
            .accessibilityHidden(true)
    }
}

/// The first letters of the first two words, or of the address.
func initials(_ name: String) -> String {
    let words = name.split { " .@_-".contains($0) }.prefix(2)
    let letters = words.compactMap(\.first).map { String($0) }.joined().uppercased()
    return letters.isEmpty ? "?" : letters
}

private let avatarColors: [UInt32] = [0x5A6B8E, 0x4D8789, 0x7D6CAE, 0xA87458, 0x5B8C62, 0x93688A, 0x6A7B96]

/// The same colour for one sender as on Android, which picks by Java's string hash.
private func avatarColor(_ key: String) -> Color {
    var hash: Int32 = 0
    for unit in key.lowercased().utf16 {
        hash = hash &* 31 &+ Int32(unit)
    }
    let count = Int32(avatarColors.count)
    return Color(rgb: avatarColors[Int(((hash % count) + count) % count)])
}

extension Font {
    /// A message's text: a serif made for reading, at a size it is read at.
    static let reading = Font.system(size: 17.5, design: .serif)

    /// Quoted text under a message, a step quieter.
    static let readingQuote = Font.system(size: 15.5, design: .serif)
}

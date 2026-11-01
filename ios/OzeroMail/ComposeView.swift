import PhotosUI
import SwiftUI

/// A message being written, and the one it answers or forwards.
struct Composing: Identifiable, Equatable {
    let id = UUID()
    var draft: DraftItem
    var original: MessageItem?
}

/// A new message, a reply or a forward, in a sheet. The fields sit low, near the
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
    @State private var cc = ""
    /// Kept while it is emptied, so it does not jump away.
    @State private var hasCc = false
    @State private var subject = ""
    @State private var text = ""
    @State private var accountId: Int64 = 0
    @State private var quote = ""
    @State private var signature = ""
    @State private var quotable = true
    @State private var accounts: [AccountItem] = []
    @State private var busy = false
    @State private var error: LocalizedStringKey?
    @State private var keeper: DraftKeeper?
    /// Whether anything was written here; until then a saved draft stays as it was.
    @State private var changed = false
    /// Sent or thrown away: nothing more to keep.
    @State private var finished = false
    /// Closed by its button: swiped down, the sheet closes the same way.
    @State private var closed = false
    /// A forward sends the message's files too, from the account it came to.
    @State private var forwarded: MessageItem?
    /// The draft as it opened, to tell a change from the fields being filled in.
    @State private var firstSeen: DraftItem?
    @State private var files: [DraftFileItem] = []
    /// The names of picked files still being read, in the order they were picked.
    @State private var adding: [String] = []
    @State private var choosingPhotos = false
    @State private var choosingFiles = false
    @State private var photos: [PhotosPickerItem] = []
    /// Files of saved mail to send on, offered in the paperclip's menu.
    @State private var recent: [RecentFileItem] = []
    /// The photos in the text made lighter, with their originals, by the file shown now.
    @State private var pictures: [Int64: Picture] = [:]
    /// The images of the text, read from the draft's files.
    @State private var images: [Int64: UIImage] = [:]
    /// Where in the text the caret is, for photos to go after its line.
    @State private var caret = 0
    @State private var textFocused = false
    @State private var textRequest = 0
    @State private var linking: Linking?
    @State private var linkURL = ""
    @FocusState private var focus: Field?
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.sideways) private var sideways
    /// To and Subject shown again while the text is typed on its side.
    @State private var unfolded = false
    /// Upright: the height of the page, and whether To and Subject are on it in sight.
    @State private var room: CGFloat = 0
    /// How tall the page and the bar under it are, for the sheet to fit them.
    @State private var pageHeight: CGFloat = 0
    @State private var barHeight: CGFloat = 0
    @State private var headingInSight = true
    /// The files the text shows as images, which go without a chip.
    @State private var shownFiles: [Int64] = []

    private enum Field { case to, cc, subject, signature }

    /// A photo in the text, as it was picked and made lighter; `full` when the original is sent.
    private struct Picture {
        let original: PickedFile
        let light: PickedFile
        let full: Bool
    }

    /// A link being given its address: the one it has, and what to do with the new one.
    private struct Linking {
        let url: String?
        let apply: (String?) -> Void
    }

    /// Anything being typed in, the text too.
    private var editing: Bool { focus != nil || textFocused }

    private var start: DraftItem { composing.draft }

    /// A new message starts from the first account.
    private var from: AccountItem? { accounts.first { $0.id == accountId } ?? accounts.first }

    /// Account 0 is kept as none, and opens with the first account again.
    private var current: DraftItem {
        DraftItem(
            id: 0, accountId: accountId, to: to, subject: subject, text: text, replyTo: start.replyTo, quote: quote,
            saved: 0, forward: start.forward, files: files, signature: signature, cc: cc)
    }

    /// Something written here or before, which Delete throws away.
    private var hasDraft: Bool { changed || start.id != 0 }

    var body: some View {
        Group {
            if sideways {
                // Close and the sender at the top left, Send at the right edge by the keyboard.
                HStack(alignment: .bottom, spacing: 0) {
                    VStack(spacing: 0) {
                        HStack(spacing: 8) {
                            RoundButton(icon: "xmark", label: "action_close", size: 44, action: close)
                            if folded {
                                Button {
                                    unfolded = true
                                } label: {
                                    Text(verbatim: summary)
                                        .font(.text)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                        .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                                        .contentShape(.rect)
                                }
                                .buttonStyle(.plain)
                            } else {
                                if hasDraft { deleteButton }
                                sender
                                Spacer(minLength: 0)
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        // With the keyboard down there is room for the message being answered;
                        // with it up the card only folds away, so it keeps what it loaded.
                        if let original = composing.original {
                            Original(mail: mail, message: original)
                                .frame(maxHeight: editing ? 0 : .infinity, alignment: .top)
                                .clipped()
                        }
                        ScrollView {
                            fields.padding(.vertical, 8)
                        }
                        .scrollDismissesKeyboard(.never)
                        errorLine
                    }
                    .middle(Room.compose)
                    VStack(spacing: 8) {
                        attachButton
                        sendButton
                    }
                    .padding(12)
                }
            } else {
                VStack(spacing: 0) {
                    // One page, low by the thumb: the message being answered, To and Subject, the text.
                    // A longer text grows up and scrolls with the rest, with no frame of its own.
                    ScrollView {
                        VStack(alignment: .leading, spacing: 12) {
                            if let original = composing.original {
                                Original(mail: mail, message: original)
                                    // A tap puts the keyboard away, so the message gets the room to read.
                                    .onTapGesture {
                                        focus = nil
                                        hideKeyboard()
                                    }
                            }
                            if !docked {
                                heading.onGeometryChange(for: Bool.self) { proxy in
                                    // Most of it within the page's visible part.
                                    let frame = proxy.frame(in: .scrollView)
                                    return frame.maxY - max(frame.minY, 0) >= frame.height * 0.6
                                } action: { headingInSight = $0 }
                                .zIndex(1)
                            }
                            // A new message starts with room to write in, not one line under an empty page.
                            writing(minHeight: growing ? Self.writingRoom : composing.original == nil ? room / 3 : 0)
                        }
                        .padding(.bottom, 20)
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { pageHeight = $0 }
                        .frame(maxWidth: .infinity, minHeight: room, alignment: .bottom)
                    }
                    .defaultScrollAnchor(.bottom)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { room = $0 }
                    // When a long text takes To and Subject out of sight, a line names them, and a tap
                    // brings them down here.
                    if docked {
                        Divider()
                        heading.padding(.vertical, 8)
                    } else if !headingInSight {
                        Button {
                            unfolded = true
                        } label: {
                            HStack {
                                Text(verbatim: summary).lineLimit(1)
                                Spacer(minLength: 0)
                                Image(systemName: "chevron.up").foregroundStyle(.secondary)
                            }
                            .font(.text)
                            .padding(.horizontal, 14)
                            .padding(.vertical, 10)
                            .background(Color.ozeroCard, in: .capsule)
                            .contentShape(.capsule)
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal, 12)
                        .padding(.bottom, 6)
                    }
                    errorLine
                    // Glass buttons need no line over them.
                    if !hasGlass { Divider() }
                    HStack(spacing: 8) {
                        RoundButton(icon: "xmark", label: "action_close", size: 44, action: close)
                        if hasDraft { deleteButton }
                        sender
                        Spacer(minLength: 0)
                        attachButton
                        sendButton
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { barHeight = $0 }
                }
            }
        }
        .presentationDetents(growing ? [fitted] : [.large])
        .animation(.smooth, value: pageHeight)
        .onChange(of: editing) { _, now in
            if !now { unfolded = false }
        }
        // Not swiped away while it sends.
        .interactiveDismissDisabled(busy)
        // Swiped down, what was written goes to the drafts, as with the close button.
        .onDisappear {
            if !closed && !finished { close() }
        }
        // Back to the text, To and Subject go back up the page.
        .onChange(of: textFocused) { _, now in
            if now && !sideways { unfolded = false }
        }
        .onChange(of: docked) { _, now in
            if now { focus = .to }
        }
        .onChange(of: focus) { _, now in
            // Taken out, a tap brings it back.
            if now == .signature, signature.isEmpty, let from { signature = from.signsWith }
        }
        // Photos come as JPEG and H.264, which every mail app opens.
        .photosPicker(isPresented: $choosingPhotos, selection: $photos, matching: .any(of: [.images, .videos]),
                      preferredItemEncoding: .compatible)
        .onChange(of: photos) { _, picked in
            guard !picked.isEmpty else { return }
            photos = []
            addPhotos(picked)
        }
        .fileImporter(isPresented: $choosingFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case .success(let urls) = result { addFiles(urls) }
        }
        .animation(.smooth, value: editing)
        .onChange(of: text, initial: true) { shownFiles = Self.shown(in: text) }
        .task(id: shownFiles) {
            // The images of the text, as the draft keeps them.
            for file in shownFiles where images[file] == nil {
                guard let data = try? await mail.draftFileData(file) else { continue }
                if let image = await Task.detached(operation: { shownImage(data) }).value { images[file] = image }
            }
        }
        .alert("compose_format_link", isPresented: Binding(get: { linking != nil }, set: { if !$0 { linking = nil } })) {
            TextField(text: $linkURL, prompt: Text(verbatim: "https://")) { Text("compose_format_link") }
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("action_done") {
                let url = linkURL.trimmingCharacters(in: .whitespaces)
                if !url.isEmpty && url != "https://" { linking?.apply(url) }
            }
            if linking?.url != nil {
                Button("compose_link_remove", role: .destructive) { linking?.apply(nil) }
            }
            Button("action_cancel", role: .cancel) {}
        }
        .onAppear(perform: begin)
        .task {
            // Only an offer, the menu goes without it; a forwarded message's files go with it already.
            recent = ((try? await mail.recentFiles()) ?? []).filter { $0.message != start.forward }
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
            try? await Task.sleep(for: DraftKeeper.pause)
            if !finished { keeper?.save(current) }
        }
        // The app may be closed in the background at any time.
        .onChange(of: scenePhase) { _, phase in
            if phase != .active { keep() }
        }
    }

    /// On its side, while the text is typed, To and Subject fold into one line, so the text has
    /// the little height the keyboard leaves; a tap on the line brings them back.
    private var folded: Bool { sideways && textFocused && !unfolded }

    /// Upright, To and Subject brought down over the bar.
    private var docked: Bool { !sideways && unfolded }

    /// Upright, a new message is a sheet only as tall as what is written, over the keyboard,
    /// growing up as the text does; the inbox shows above it, so no empty page waits there.
    private var growing: Bool { !sideways && composing.original == nil }

    private var fitted: PresentationDetent {
        .height(max(Self.leastHeight, (pageHeight + barHeight + Self.grabber).rounded()))
    }

    private static let leastHeight: CGFloat = 220
    /// The text's room in a new message before anything is written.
    private static let writingRoom: CGFloat = 96
    /// Room over the page for the sheet's grabber.
    private static let grabber: CGFloat = 20

    /// To and Subject in one line.
    private var summary: String {
        [to.isBlank ? tr("compose_to") : to, subject.isBlank ? tr("compose_subject") : subject].joined(separator: " · ")
    }

    /// The files `text` shows as images, which go without a chip.
    private static func shown(in text: String) -> [Int64] {
        richBlocks(text: text).compactMap { block in
            if case .image(_, _, let file, _) = block { file } else { nil }
        }
    }

    private var fields: some View {
        VStack(alignment: .leading, spacing: 12) {
            if !folded { heading.zIndex(1) }
            writing()
        }
    }

    private var heading: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline) {
                Recipients(mail: mail, text: $to, placeholder: "compose_to", focus: $focus, field: .to)
                    .submitLabel(.next)
                    .onSubmit { focus = hasCc ? .cc : .subject }
                if !hasCc {
                    Button("compose_cc") {
                        hasCc = true
                        focus = .cc
                    }
                    .font(.smallStrong)
                }
            }
            // The people found rise over what is around the field.
            .zIndex(2)
            if hasCc {
                Divider()
                Recipients(mail: mail, text: $cc, placeholder: "compose_cc", focus: $focus, field: .cc)
                    .submitLabel(.next)
                    .onSubmit { focus = .subject }
                    .zIndex(1)
            }
            Divider()
            TextField("compose_subject", text: $subject)
                .font(.subjectTitle)
                .focused($focus, equals: .subject)
                .submitLabel(.next)
                .onSubmit { textRequest += 1 }
        }
        .padding(.horizontal, 20)
    }

    private func writing(minHeight: CGFloat = 0) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            textField(minHeight: minHeight)
            let chips = files.filter { !shownFiles.contains($0.id) }
            if !chips.isEmpty || !adding.isEmpty {
                FileChips(files: chips, adding: adding, onRemove: detach)
            }
            // A forward is its quote, so it stays.
            let forwarding = start.forward != nil
            let forwardedFiles = forwarded.flatMap { $0.accountId == from?.id ? $0.attachments : nil } ?? []
            if !forwardedFiles.isEmpty { AttachmentChips(names: forwardedFiles) }
            // Its own for this message: a change here stays here. Taken out, a tap brings it back.
            if !signature.isEmpty || !(from?.signsWith.isBlank ?? true) {
                TextField("account_signature", text: $signature, axis: .vertical)
                    .font(.writing)
                    .foregroundStyle(.secondary)
                    .lineLimit(1...6)
                    .focused($focus, equals: .signature)
                    .accessibilityLabel(Text("compose_signature"))
            }
            if !quote.isEmpty {
                QuoteLine(text: composing.original == nil ? quote : nil, forwarding: forwarding,
                          onRemove: forwarding ? nil : { quote = "" })
            } else if let original = composing.original, quotable {
                Button("compose_add_quote") { addQuote(original) }
                    .font(.text)
                    .buttonStyle(.bordered)
                    .buttonBorderShape(.capsule)
            }
        }
        .padding(.horizontal, 20)
    }

    /// As tall as the text or `minHeight`, with no frame of its own: the page it is on scrolls.
    private func textField(minHeight: CGFloat) -> some View {
        RichBody(
            text: $text, files: files, images: images, focusRequest: textRequest,
            onCaret: { caret = $0 },
            onFocus: { textFocused = $0 },
            onLink: { url, apply in
                linkURL = url ?? "https://"
                linking = Linking(url: url, apply: apply)
            },
            minHeight: minHeight,
            imageMenu: imageMenu
        )
    }

    /// What can be done with an image in the text.
    @ViewBuilder
    private func imageMenu(_ file: Int64, _ size: ImageSize) -> some View {
        Picker(selection: Binding(get: { size }, set: { text = richChangeImage(text: text, file: file, to: file, size: $0) })) {
            Text("compose_image_small").tag(ImageSize.small)
            Text("compose_image_medium").tag(ImageSize.medium)
            Text("compose_image_full").tag(ImageSize.full)
        } label: {
            Text("compose_image")
        }
        .pickerStyle(.inline)
        Button { text = richMoveImage(text: text, file: file, up: true) } label: {
            Label("compose_image_up", systemImage: "arrow.up")
        }
        Button { text = richMoveImage(text: text, file: file, up: false) } label: {
            Label("compose_image_down", systemImage: "arrow.down")
        }
        if let picture = pictures[file] {
            Button { swap(file, picture, size) } label: {
                Label(picture.full ? "compose_image_lighter" : "compose_image_original", systemImage: "photo")
            }
        }
        Button { text = richRemoveImage(text: text, file: file) } label: {
            Label("compose_image_to_file", systemImage: "paperclip")
        }
        Button(role: .destructive) {
            text = richRemoveImage(text: text, file: file)
            if let shown = files.first(where: { $0.id == file }) { detach(shown) }
        } label: {
            Label("action_remove", systemImage: "trash")
        }
    }

    @ViewBuilder
    private var errorLine: some View {
        if let error {
            Text(error).foregroundStyle(Color.ozeroError).padding(.horizontal, 20).padding(.vertical, 8)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var deleteButton: some View {
        RoundButton(icon: "trash", label: "action_delete_draft", size: 44, action: discard)
    }

    /// The account the message goes from; a tap lists the others.
    @ViewBuilder
    private var sender: some View {
        if let from {
            Menu {
                ForEach(accounts, id: \.id) { account in
                    Button {
                        // Its signature comes too, unless this message's was changed.
                        if signature == from.signsWith { signature = account.signsWith }
                        accountId = account.id
                    } label: {
                        if account.id == from.id { Label(account.email, systemImage: "checkmark") } else { Text(account.email) }
                    }
                }
            } label: {
                HStack(spacing: 8) {
                    AccountInitial(name: from.name, color: from.tint, size: 22)
                    Text(verbatim: from.email).font(.text).lineLimit(1).foregroundStyle(.primary)
                }
                .padding(.leading, 8)
                .padding(.trailing, 12)
                .frame(height: 44)
                .contentShape(.capsule)
                .glassSurface(in: .capsule, fallback: Color.ozeroCard)
            }
            .disabled(accounts.count < 2)
        }
    }

    /// Where the files come from, in a menu like the sender's.
    private var attachButton: some View {
        Menu {
            Button { choosingPhotos = true } label: { Label("compose_attach_photos", systemImage: "photo.on.rectangle") }
            Button { choosingFiles = true } label: { Label("compose_attach_files", systemImage: "folder") }
            if !recent.isEmpty {
                Section("compose_attach_recent") {
                    ForEach(recent, id: \.self) { file in
                        Button { addFromMail(file) } label: {
                            Text(verbatim: attachmentTitle(file.name))
                            Text(verbatim: "\(file.sender) · \(ByteCountFormatter.string(fromByteCount: Int64(clamping: file.size), countStyle: .file))")
                        }
                    }
                }
            }
        } label: {
            Image(systemName: "paperclip")
                .font(.row)
                .foregroundStyle(.primary)
                .frame(width: 44, height: 44)
                .contentShape(.circle)
                .glassSurface(in: .circle, fallback: Color.ozeroCard)
        }
        .accessibilityLabel(Text("compose_attach"))
    }

    private var sendButton: some View {
        Button(action: send) {
            Image(systemName: "paperplane.fill")
                .foregroundStyle(.white)
                .frame(width: 56, height: 44)
                .contentShape(.capsule)
                .glassSurface(in: .capsule, tint: .ozeroFill, fallback: Color.ozeroFill.lifted)
        }
        .buttonStyle(.plain)
        // The files still being read go with it, so it waits for them.
        .disabled(busy || from == nil || !adding.isEmpty)
        .accessibilityLabel(Text("compose_send"))
    }

    private func begin() {
        guard firstSeen == nil else { return }
        to = start.to
        cc = start.cc
        hasCc = !start.cc.isBlank
        subject = start.subject
        text = start.text
        accountId = start.accountId
        quote = start.quote
        signature = start.signature
        files = start.files
        caret = (text as NSString).length
        keeper = DraftKeeper(mail: mail, id: start.id)
        firstSeen = current
        // A forward or a new message starts where it is missing: who it goes to.
        if to.isBlank { focus = .to }
    }

    private func keep() {
        if changed && !finished { keeper?.save(current) }
    }

    private func close() {
        closed = true
        hideKeyboard()
        keep()
        if !finished && hasDraft && !current.isBlank { onKept() } else { onClose() }
    }

    private func discard() {
        hideKeyboard()
        finished = true
        var draft = current
        draft.accountId = from?.id ?? accountId
        let thrown = draft
        Task {
            // Its id, so undo brings it back with its files.
            let id = await keeper?.discard() ?? 0
            var discarded = thrown
            discarded.id = id
            onDiscard(discarded)
        }
    }

    /// Photos go into the text after the caret's line, lighter; videos go with the files.
    private func addPhotos(_ items: [PhotosPickerItem]) {
        var lighter: [Int: Picture] = [:]
        add(items.map { _ in "" }, read: { index, _ in
            // Nil when the library has no photo or video for it to hand over.
            guard let media = try await items[index].loadTransferable(type: PickedMedia.self) else {
                throw CocoaError(.fileReadUnknown)
            }
            guard let file = media.file,
                  let light = await Task.detached(operation: { file.lighter() }).value else { return media.file }
            lighter[index] = Picture(original: file, light: light, full: false)
            return light
        }, then: { added in
            let shown = added.filter { $0.1.mime.hasPrefix("image/") }
            for (index, file) in shown { pictures[file.id] = lighter[index] }
            if !shown.isEmpty {
                text = richInsertImages(text: text, at: UInt32(clamping: caret), files: shown.map(\.1.id))
            }
        })
    }

    /// Sends the other of a photo's two versions: the original, or the lighter one again.
    private func swap(_ file: Int64, _ picture: Picture, _ size: ImageSize) {
        guard let shown = files.first(where: { $0.id == file }) else { return }
        let other = picture.full ? picture.light : picture.original
        add([other.name], read: { _, _ in other }, then: { added in
            guard let attached = added.first?.1 else { return }
            pictures[attached.id] = Picture(original: picture.original, light: picture.light, full: !picture.full)
            images[attached.id] = images[file]
            text = richChangeImage(text: text, file: file, to: attached.id, size: size)
            detach(shown)
        })
    }

    private func addFiles(_ urls: [URL]) {
        add(urls.map(\.lastPathComponent)) { index, room in
            let url = urls[index]
            return try await Task.detached {
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                return try PickedFile.read(url, room: room)
            }.value
        }
    }

    /// Adds picked files one at a time, so a message never holds more than fits: `read` gives
    /// each whole, or nil when it takes more than the room left; `names` show meanwhile.
    private func add(_ names: [String], read: @escaping (Int, UInt64) async throws -> PickedFile?,
                     then: @escaping ([(Int, DraftFileItem)]) -> Void = { _ in }) {
        guard let keeper, !names.isEmpty else { return }
        error = nil
        adding += names
        Task {
            var done = 0
            var added: [(Int, DraftFileItem)] = []
            // What did not come stops showing; another pick's files of the same name stay.
            defer {
                then(added)
                for name in names.dropFirst(done) {
                    if let shown = adding.firstIndex(of: name) { adding.remove(at: shown) }
                }
            }
            do {
                for index in names.indices {
                    let room = fileLimit() - min(fileLimit(), files.reduce(0) { $0 + $1.size })
                    // A photo is read against the whole limit, so it is measured here too.
                    guard let file = try await read(index, room), UInt64(file.data.count) <= room else {
                        throw CoreError.TooLarge
                    }
                    let attached = try await keeper.attach(current, file)
                    files.append(attached)
                    added.append((index, attached))
                    if let shown = adding.firstIndex(of: names[index]) { adding.remove(at: shown) }
                    done += 1
                }
            } catch let error as CoreError {
                self.error = error.text()
            } catch {
                self.error = "compose_file_unreadable"
            }
        }
    }

    private func addFromMail(_ file: RecentFileItem) {
        // A second tap while it downloads adds nothing.
        guard !adding.contains(file.name) else { return }
        add([file.name]) { [mail] _, room in file.size > room ? nil : try await mail.downloadFile(file) }
    }

    private func detach(_ file: DraftFileItem) {
        files.removeAll { $0.id == file.id }
        pictures[file.id] = nil
        Task {
            do {
                try await keeper?.detach(file)
            } catch {
                self.error = error.text()
            }
        }
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
                hideKeyboard()
                var sent = composing
                sent.draft = draft
                onSent(id, sent)
            } catch {
                self.error = error.text()
            }
        }
    }

    private func addQuote(_ original: MessageItem) {
        let header = tr("compose_quote_header", longDate(original.date), original.mailSender)
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
                Text(verbatim: message.sender).font(.textStrong).lineLimit(1)
                Spacer()
                Text(verbatim: shortDate(message.date)).font(.small).foregroundStyle(.secondary)
            }
            // As tall as its text, scrolling only when the room is short.
            ViewThatFits(in: .vertical) {
                quoted
                ScrollView { quoted }
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

    private var quoted: some View {
        Text(verbatim: text.isEmpty ? message.snippet : text)
            .font(.readingQuote)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
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
                .font(.text)
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
    /// How long typing pauses before the draft is saved, so each key does not save it.
    static let pause = Duration.seconds(1)

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

        func attach(_ draft: DraftItem, _ file: PickedFile, with mail: Mail) async throws -> DraftFileItem {
            var result: Result<DraftFileItem, Error> = .failure(CancellationError())
            await queue { state in
                do {
                    guard !state.done else { throw CoreError.Gone }
                    if state.id == 0 { state.id = try await mail.saveDraft(draft) }
                    result = .success(try await mail.attachFile(draft: state.id, file))
                } catch {
                    result = .failure(error)
                }
            }.value
            return try result.get()
        }

        func detach(_ file: DraftFileItem, with mail: Mail) async throws {
            var result: Result<Void, Error> = .success(())
            await queue { _ in
                do {
                    try await mail.detachFile(file.id)
                } catch {
                    result = .failure(error)
                }
            }.value
            try result.get()
        }

        func handOff(_ draft: DraftItem, with mail: Mail) async throws -> Int64 {
            latest = nil
            var result: Result<Int64, Error> = .failure(CancellationError())
            await queue { state in
                do {
                    if !state.done {
                        var draft = draft
                        draft.id = state.id
                        state.id = try await mail.saveDraft(draft)
                        state.done = true
                    }
                    result = .success(state.id)
                } catch {
                    result = .failure(error)
                }
            }.value
            return try result.get()
        }

        func discard(with mail: Mail) async -> Int64 {
            latest = nil
            done = true
            var thrown: Int64 = 0
            await queue { state in
                thrown = state.id
                if state.id != 0 { try? await mail.deleteDraft(state.id) }
            }.value
            return thrown
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

    /// Adds `file` to the draft, saving `draft` first while there is no saved one to add it to.
    func attach(_ draft: DraftItem, _ file: PickedFile) async throws -> DraftFileItem {
        try await state.attach(draft, file, with: mail)
    }

    /// Takes `file` off the draft, in turn with saves and the send, so a send after it goes
    /// without it.
    func detach(_ file: DraftFileItem) async throws {
        try await state.detach(file, with: mail)
    }

    /// Saves `draft` now and stops saving, for another screen to go on with it. Returns the id
    /// it is saved under.
    func handOff(_ draft: DraftItem) async throws -> Int64 {
        try await state.handOff(draft, with: mail)
    }

    /// Throws the saved draft away, and stops saving. Returns the id it had, 0 for none.
    func discard() async -> Int64 {
        await state.discard(with: mail)
    }
}

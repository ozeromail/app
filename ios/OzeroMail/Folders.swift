import SwiftUI

/// What the move sheet offers: where the mail can go, the folders to put first, and whether a
/// new folder can be made for it.
struct MoveChoice {
    let targets: [FolderItem]
    let suggestions: [MoveSuggestion]
    let canCreate: Bool
}

/// A folder in a list of them, `depth` steps in, named `title`; `inside` when folders of the
/// list are inside it.
struct TreeRow {
    let folder: FolderItem
    let title: String
    let depth: Int
    let inside: Bool
}

/// An account's own `folders` as a tree, `depth` steps in: each with the ones inside it under
/// it, unless `shows` says to keep those hidden. Among folders side by side the pinned come
/// first, in the person's order, then the rest by name.
func folderTree(_ folders: [FolderItem], shows: (Int64) -> Bool = { _ in true }, depth: Int = 0) -> [TreeRow] {
    let ids = Set(folders.map(\.id))
    let inside = Dictionary(grouping: folders) { folder in folder.parent.flatMap { ids.contains($0) ? $0 : nil } }
    func before(_ a: FolderItem, _ b: FolderItem) -> Bool {
        if a.pinned != b.pinned { return a.pinned }
        let (ra, rb) = (a.rank ?? .max, b.rank ?? .max)
        if ra != rb { return ra < rb }
        return a.shownName(ids).lowercased() < b.shownName(ids).lowercased()
    }
    func under(_ parent: Int64?, _ at: Int) -> [TreeRow] {
        (inside[parent] ?? []).sorted(by: before).flatMap { folder -> [TreeRow] in
            let more = !(inside[folder.id] ?? []).isEmpty
            let row = TreeRow(folder: folder, title: folder.shownName(ids), depth: at, inside: more)
            return [row] + (more && shows(folder.id) ? under(folder.id, at + 1) : [])
        }
    }
    return under(nil, depth)
}

extension FolderItem {
    /// Inside a folder that shows too, among `ids`, or at the top, its own name; else the whole path.
    func shownName(_ ids: Set<Int64>) -> String {
        (parent.map(ids.contains) ?? true) ? shortName : name
    }

    /// The folders this one is inside, outermost first, then itself, by their own names.
    func path(_ byId: [Int64: FolderItem]) -> String {
        var names = [shortName]
        var at = parent.flatMap { byId[$0] }
        // Deeper than any server nests: a guard against a loop in what a server said.
        while let folder = at, names.count < 32 {
            names.insert(folder.shortName, at: 0)
            at = folder.parent.flatMap { byId[$0] }
        }
        return names.joined(separator: " / ")
    }

    /// Where it is, for a list that names it alone; nil at the top.
    func parentPath(_ byId: [Int64: FolderItem]) -> String? {
        parent.flatMap { byId[$0] }?.path(byId)
    }
}

/// `folder` and the folders inside it, among `folders`.
private func withInside(_ folder: FolderItem, _ folders: [FolderItem]) -> Set<Int64> {
    let byId = Dictionary(folders.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    var out: Set<Int64> = [folder.id]
    for other in folders {
        var at: FolderItem? = other
        var steps = 0
        while let f = at, steps < 32 {
            if f.id == folder.id { out.insert(other.id); break }
            at = f.parent.flatMap { byId[$0] }
            steps += 1
        }
    }
    return out
}

/// Past this many folders, the move sheet has a search.
private let searchFrom = 8

/// Where `moving`'s messages can go: the mail on top, the folders suggested for it, then the
/// inbox, the archive and the account's own folders as a tree whose branches close as in
/// `groups`. With many folders, a search; where the account can have new folders, a way to
/// make one and move the mail into it at once.
struct MoveSheet: View {
    let mail: Mail
    let moving: Moving
    let folders: [FolderItem]
    @Binding var groups: Groups
    let onPick: (FolderItem, String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var making: String?

    private var own: [FolderItem] {
        let account = moving.messages.first?.accountId
        return folders.filter { $0.accountId == account && $0.role == .custom }
    }

    var body: some View {
        NavigationStack {
            list
                .navigationTitle("move_title")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("action_cancel") { dismiss() } }
                }
                .navigationDestination(isPresented: Binding { making != nil } set: { if !$0 { making = nil } }) {
                    if let making, let account = moving.messages.first?.accountId {
                        FolderForm(title: "folder_new", places: own, folder: nil, startName: making, startParent: nil,
                                   action: "folder_create_and_move") { name, parent in
                            let made = try await mail.createFolder(account: account, parent: parent, name: name)
                            onPick(made, made.shortName)
                        }
                    }
                }
        }
    }

    @ViewBuilder private var list: some View {
        let choice = moving.choice
        let byId = Dictionary(own.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let find = query.trimmingCharacters(in: .whitespaces)
        let content = List {
            Section { MailCard(messages: moving.messages) }
            if !find.isEmpty {
                Section {
                    ForEach(choice.targets.filter { $0.title.localizedCaseInsensitiveContains(find) }, id: \.id) { folder in
                        PickRow(icon: folder.role.icon, title: folder.title, under: folder.parentPath(byId)) { onPick(folder, folder.title) }
                    }
                    if choice.canCreate {
                        NewFolderButton(title: tr("folder_new_named", find)) { making = find }
                    }
                }
            } else {
                if !choice.suggestions.isEmpty {
                    Section("move_suggested") {
                        ForEach(choice.suggestions, id: \.folder.id) { suggestion in
                            let folder = suggestion.folder
                            let why = suggestion.count > 0 ? tr("move_from_sender", Int(suggestion.count), suggestion.sender) : tr("move_lately")
                            let under = folder.parentPath(byId).map { "\($0) · \(why)" } ?? why
                            PickRow(icon: folder.role.icon, title: folder.title, under: under) { onPick(folder, folder.title) }
                        }
                    }
                }
                Section {
                    if choice.targets.isEmpty { Text("move_none").foregroundStyle(.secondary) }
                    ForEach(choice.targets.filter { $0.role != .custom }.sorted { $0.role.rank < $1.role.rank }, id: \.id) { folder in
                        PickRow(icon: folder.role.icon, title: folder.title) { onPick(folder, folder.title) }
                    }
                    ForEach(folderTree(choice.targets.filter { $0.role == .custom }, shows: groups.showsInside), id: \.folder.id) { row in
                        HStack(spacing: 4) {
                            PickRow(icon: "folder", title: row.title, depth: row.depth) { onPick(row.folder, row.title) }
                            if row.inside {
                                Twisty(folder: row.title, open: groups.showsInside(row.folder.id)) {
                                    withAnimation(.smooth) { groups = groups.toggledInside(row.folder.id) }
                                }
                            }
                        }
                    }
                    if choice.canCreate {
                        NewFolderButton(title: tr("folder_new")) { making = "" }
                    }
                } header: {
                    if !choice.suggestions.isEmpty { Text("move_all") }
                }
            }
        }
        if choice.targets.count > searchFrom {
            content.searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: Text("move_find"))
        } else {
            content
        }
    }
}

/// What is being moved, on top of the move sheet: the message's sender and subject, or how
/// many messages.
private struct MailCard: View {
    let messages: [MessageItem]

    var body: some View {
        let one = messages.count == 1 ? messages.first : nil
        VStack(alignment: .leading, spacing: 2) {
            Text(verbatim: one?.sender ?? tr("move_messages", messages.count)).font(.textStrong).lineLimit(1)
            if let subject = messages.first?.subject, !subject.isEmpty {
                Text(verbatim: subject).font(.text).foregroundStyle(.secondary).lineLimit(1)
            }
        }
    }
}

/// A folder to pick, `depth` steps in, with where it is or why it is offered `under` its name.
private struct PickRow: View {
    let icon: String
    let title: String
    var under: String?
    var depth = 0
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: icon).foregroundStyle(.secondary).frame(width: 24)
                VStack(alignment: .leading, spacing: 1) {
                    Text(verbatim: title).lineLimit(1)
                    if let under { Text(verbatim: under).font(.small).foregroundStyle(.secondary).lineLimit(1) }
                }
                Spacer()
            }
            .padding(.leading, CGFloat(depth) * 38)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

/// A row that makes a folder, in the accent colour.
struct NewFolderButton: View {
    let title: String
    var depth = 0
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label { Text(verbatim: title) } icon: { Image(systemName: "plus") }
                .foregroundStyle(Color.ozeroAccent)
                .padding(.leading, CGFloat(depth) * 38)
        }
    }
}

/// At the end of a folder's row: shows or hides the folders inside it.
struct Twisty: View {
    let folder: String
    let open: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "chevron.right")
                .font(.smallStrong)
                .foregroundStyle(.secondary)
                .rotationEffect(.degrees(open ? 90 : 0))
                .frame(width: 44, height: 44)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(verbatim: tr("folder_inside_of", folder)))
        .accessibilityHint(Text(open ? "action_collapse" : "action_expand"))
    }
}

/// A sheet for one of the account's own folders: its card, with the question before deleting it
/// open with `deleting`, or the form for a new folder inside `parent`.
enum FolderSheet: Identifiable {
    case card(FolderItem, deleting: Bool = false)
    case new(account: Int64, parent: FolderItem?)

    var id: String {
        switch self {
        case .card(let folder, _): "card-\(folder.id)"
        case .new(let account, let parent): "new-\(account)-\(parent?.id ?? 0)"
        }
    }
}

extension View {
    /// Shows the inbox's folder sheet here while `when`; each change shows at once and reaches
    /// the server in the background.
    func folderSheets(_ model: InboxModel, when: Bool = true) -> some View {
        sheet(item: Binding { when ? model.folderSheet : nil } set: { model.folderSheet = $0 }) { sheet in
            NavigationStack {
                switch sheet {
                case .card(let shown, let deleting):
                    let folder = model.folders.first { $0.id == shown.id } ?? shown
                    FolderCard(model: model, folder: folder, deleting: deleting)
                case .new(let account, let parent):
                    FolderForm(title: "folder_new", places: model.folders.filter { $0.accountId == account && $0.role == .custom },
                               folder: nil, startName: "", startParent: parent?.id, action: "folder_create") { name, parent in
                        _ = try await model.mail.createFolder(account: account, parent: parent, name: name)
                        await model.loadFolders()
                        model.folderSheet = nil
                    }
                }
            }
        }
    }
}

/// A folder's card: its name and where it is, saved together; its pin; Delete, asked first with
/// what goes with it.
private struct FolderCard: View {
    let model: InboxModel
    let folder: FolderItem
    let deleting: Bool
    @State private var count: Int?

    var body: some View {
        let own = model.folders.filter { $0.accountId == folder.accountId && $0.role == .custom }
        FolderForm(title: "folder_title", places: own, folder: folder, startName: folder.shortName, startParent: folder.parent,
                   action: "action_save") { name, parent in
            try await model.mail.changeFolder(folder, parent: parent, name: name)
            await model.loadFolders()
            model.folderSheet = nil
        } more: {
            Section {
                Toggle("folder_pin_in_list", isOn: Binding(get: { folder.pinned }, set: { pinned in
                    model.changeAccount {
                        try await model.mail.setPinned(folder, pinned)
                        await model.loadFolders()
                    }
                }))
            }
            Section {
                if deleting {
                    let inside = own.contains { $0.parent == folder.id }
                    Text(verbatim: tr(inside ? "folder_delete_question_inside" : "folder_delete_question", folder.shortName, count ?? 0))
                    Button("action_delete", role: .destructive) {
                        Task { await model.deleteFolder(folder) }
                    }
                    Button("folder_keep") { model.folderSheet = .card(folder) }
                } else {
                    Button("folder_delete", role: .destructive) { model.folderSheet = .card(folder, deleting: true) }
                }
            }
        }
        .task(id: folder.id) { count = try? await model.mail.folderMail(folder) }
    }
}

/// A folder's name and the folder of the account's own it goes inside, from `places`, or the
/// top level; for `folder`, the folders inside it are not places for it. `action` sends them to
/// `onDone`, which throws for a name that cannot be there; `more` goes below.
struct FolderForm<More: View>: View {
    let title: LocalizedStringKey
    let places: [FolderItem]
    let folder: FolderItem?
    let startName: String
    let startParent: Int64?
    let action: LocalizedStringKey
    let onDone: (String, FolderItem?) async throws -> Void
    @ViewBuilder var more: () -> More

    @State private var name = ""
    @State private var parent: Int64?
    @State private var started = false
    @State private var busy = false
    @State private var error: String?
    @FocusState private var typing: Bool

    var body: some View {
        let allowed = folder.map { f in let out = withInside(f, places); return places.filter { !out.contains($0.id) } } ?? places
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        let ready = !busy && !trimmed.isEmpty && (folder.map { trimmed != $0.shortName || parent != $0.parent } ?? true)
        Form {
            Section {
                TextField("folder_name", text: $name)
                    .focused($typing)
                    .submitLabel(.done)
                    .onSubmit { if ready { send(trimmed) } }
                Picker("folder_where", selection: $parent) {
                    Text("folder_top").tag(Int64?.none)
                    ForEach(folderTree(allowed), id: \.folder.id) { row in
                        Text(verbatim: String(repeating: "    ", count: row.depth) + row.title).tag(Optional(row.folder.id))
                    }
                }
            } footer: {
                if let error { Text(LocalizedStringKey(error)).foregroundStyle(Color.ozeroError) }
            }
            Section {
                Button(action: { send(trimmed) }) { Text(action).frame(maxWidth: .infinity) }
                    .disabled(!ready)
            }
            more()
        }
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            guard !started else { return }
            started = true
            name = startName
            parent = startParent
            // A new folder starts with its name; a card opens on what is there.
            if folder == nil { typing = true }
        }
        .onChange(of: name) { error = nil }
    }

    private func send(_ name: String) {
        busy = true
        error = nil
        let inside = parent.flatMap { id in places.first { $0.id == id } }
        Task {
            do {
                try await onDone(name, inside)
            } catch {
                self.error = error.textKey()
            }
            busy = false
        }
    }
}

extension FolderForm where More == EmptyView {
    init(title: LocalizedStringKey, places: [FolderItem], folder: FolderItem?, startName: String, startParent: Int64?,
         action: LocalizedStringKey, onDone: @escaping (String, FolderItem?) async throws -> Void) {
        self.init(title: title, places: places, folder: folder, startName: startName, startParent: startParent,
                  action: action, onDone: onDone) { EmptyView() }
    }
}

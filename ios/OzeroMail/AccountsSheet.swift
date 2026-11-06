import SwiftUI

/// What the accounts sheet asks the inbox to do.
struct AccountsActions {
    let onMailbox: (Mailbox) -> Void
    let onDrafts: () -> Void
    let onInAll: (AccountItem, Bool) -> Void
    let onOrder: ([AccountItem]) -> Void
    let onPin: (FolderItem, Bool) -> Void
    /// A folder's card, or the form for a new folder.
    let onFolderSheet: (FolderSheet) -> Void
    /// The pinned folders of one account in their new order.
    let onFolderOrder: ([FolderItem]) -> Void
    let onSignIn: (AccountItem) -> Void
    /// Opens an account's settings.
    let onAccount: (AccountItem) -> Void
    let onSettings: () -> Void
}

/// The mailboxes, for going places. It grows out of the accounts button and lies on the sheet's
/// glass, without cards: all inboxes and each account's inbox as rows, the mailboxes of all
/// accounts as tiles. Folders, one row, open the folders of every account as a page. The settings sit
/// in the sheet's bar at the left, Edit at the right as in the iPhone's own Mail; Edit turns the
/// rows into switches to move, an account's settings open from its ⓘ there, or by holding it.
struct AccountsSheet: View {
    let accounts: [AccountItem]
    let folders: [FolderItem]
    let mailbox: Mailbox
    let drafts: Int
    /// Unread People mail of each account's inbox, by account id.
    let unread: [Int64: Int]
    @Binding var groups: Groups
    @Binding var allBoxes: AllBoxes
    let actions: AccountsActions
    @State private var editing = false
    @State private var showFolders = false
    /// How tall the mailboxes are, so the sheet opens showing all of them, tiles too, with no drag.
    @State private var fit: CGFloat = 0
    /// The height the sheet is at: kept when the rows change, so opening Other folders after
    /// dragging the sheet up leaves it up.
    @State private var detent: PresentationDetent = .medium
    @Environment(\.accountsZoom) private var zoom

    var body: some View {
        NavigationStack {
            List {
                if editing {
                    MailboxRows(
                        accounts: accounts, folders: folders, mailbox: mailbox, drafts: drafts, unread: unread,
                        allBoxes: $allBoxes, groups: $groups, actions: actions, editing: $editing, editInHeader: false,
                        settings: false)
                } else {
                    home
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            // Rows a thumb on the move hits, a little taller than the least the system allows.
            .environment(\.defaultMinListRowHeight, 52)
            .onScrollGeometryChange(for: CGFloat.self) { geometry in
                geometry.contentSize.height + geometry.contentInsets.top + geometry.contentInsets.bottom
            } action: { _, height in
                // A new set of heights makes the system pick among them again, so while the sheet is up
                // at its full height the set stays as it is.
                guard !editing && !showFolders, height != fit, detent != .large else { return }
                fit = height
                detent = .height(height)
            }
            .navigationTitle("folders_title")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button(action: actions.onSettings) { Image(systemName: "gear") }
                        .accessibilityLabel(Text("settings_title"))
                }
                // With one account and no folders to change there is nothing to edit.
                if editable(accounts: accounts, folders: folders) {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button(editing ? "action_done" : "action_edit") { withAnimation(.smooth) { editing.toggle() } }
                    }
                }
            }
            .navigationDestination(isPresented: $showFolders) {
                FoldersPage(accounts: accounts, folders: folders, mailbox: mailbox, groups: $groups, actions: actions)
            }
        }
        .presentationDetents(fit > 0 ? [.height(fit), .large] : [.medium, .large], selection: $detent)
        // Thick enough that the inbox's colours behind do not smudge the rows; still the sheet's glass.
        .presentationBackground(.ultraThickMaterial)
        .presentationDragIndicator(.visible)
        .modifier(ZoomFrom(zoom: zoom))
    }

    @ViewBuilder private var home: some View {
        Group {
            if drafts > 0 {
                FolderRow(icon: "square.and.pencil", title: tr("folder_drafts"), open: false, count: drafts, action: actions.onDrafts)
            }
            if let one = accounts.first, accounts.count == 1 {
                FolderRow(icon: "tray", title: tr("inbox_title"), open: mailbox == Mail.inbox, count: unread[one.id] ?? 0) {
                    actions.onMailbox(Mail.inbox)
                }
                SignInAgain(account: one, actions: actions)
                AccountFolders(account: one, folders: folders, mailbox: mailbox, groups: $groups, actions: actions)
            } else {
                let inAll = accounts.filter(\.inAll)
                FolderRow(title: tr("folder_all_inbox"), open: mailbox == Mail.inbox,
                          count: inAll.reduce(0) { $0 + (unread[$1.id] ?? 0) }, lead: { StackedDots(accounts: inAll) }) {
                    actions.onMailbox(Mail.inbox)
                }
                ForEach(accounts, id: \.id) { account in
                    let box = folders.first { $0.accountId == account.id && $0.role == .inbox }.map { Mailbox.folder(id: $0.id) }
                    FolderRow(title: account.name, open: box == mailbox, count: unread[account.id] ?? 0,
                              lead: { AccountDot(color: account.tint) }) {
                        if let box { actions.onMailbox(box) }
                    }
                    .contextMenu {
                        Button { actions.onAccount(account) } label: { Label("account_settings", systemImage: "gear") }
                    }
                    SignInAgain(account: account, actions: actions)
                }
                // The folders, one row that goes to a page of them: each row here does one thing.
                Button { showFolders = true } label: {
                    HStack(spacing: afterSlot) {
                        FolderIcon(name: "folder").frame(width: slot)
                        Text("folders_accounts")
                        Spacer()
                        Image(systemName: "chevron.right").font(.smallStrong).foregroundStyle(.tertiary)
                    }
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                let boxes = allBoxes.order.filter { allBoxes.shown.contains($0) && $0.listed(folders) }
                if !boxes.isEmpty {
                    Text("inbox_all_accounts")
                        .font(.smallStrong)
                        .foregroundStyle(.secondary)
                        .padding(.top, 14)
                    BoxTiles(boxes: boxes, mailbox: mailbox, onMailbox: actions.onMailbox)
                        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 16, trailing: 16))
                }
            }
        }
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
    }
}

/// The mailboxes of all accounts together, two to a row as Reminders shows its lists: a quiet
/// icon over the name, colour only on the open one.
private struct BoxTiles: View {
    let boxes: [AllBox]
    let mailbox: Mailbox
    let onMailbox: (Mailbox) -> Void
    @Environment(\.dynamicTypeSize) private var textSize

    var body: some View {
        // At the largest text sizes a name needs the whole width.
        let columns = Array(repeating: GridItem(.flexible(), spacing: 8), count: textSize.isAccessibilitySize ? 1 : 2)
        LazyVGrid(columns: columns, spacing: 8) {
            ForEach(boxes) { box in
                let open = box.mailbox == mailbox
                Button { onMailbox(box.mailbox) } label: {
                    VStack(alignment: .leading, spacing: 8) {
                        Image(systemName: box.icon)
                            .font(.row)
                            .foregroundStyle(open ? Color.ozeroAccent : .secondary)
                            .frame(width: 32, height: 32)
                            .background(Color(uiColor: .systemBackground), in: .circle)
                        Text(verbatim: box.shortTitle)
                            .font(.textStrong)
                            .foregroundStyle(open ? Color.ozeroAccent : .primary)
                            .lineLimit(2)
                            .multilineTextAlignment(.leading)
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.primary.opacity(0.06), in: .rect(cornerRadius: 20))
                    .contentShape(.rect(cornerRadius: 20))
                }
                .buttonStyle(.plain)
            }
        }
    }
}

/// Each account's folders under its name, a page of the sheet.
private struct FoldersPage: View {
    let accounts: [AccountItem]
    let folders: [FolderItem]
    let mailbox: Mailbox
    @Binding var groups: Groups
    let actions: AccountsActions

    var body: some View {
        List {
            ForEach(accounts, id: \.id) { account in
                Section {
                    AccountFolders(account: account, folders: folders, mailbox: mailbox, groups: $groups, actions: actions)
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                } header: {
                    Label { Text(verbatim: account.name) } icon: { AccountDot(color: account.tint) }
                        .font(.smallStrong)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .environment(\.defaultMinListRowHeight, 52)
        .navigationTitle("folders_accounts")
    }
}

/// The sheet grows out of the button that opened it, where there is one. Before iOS 26 a sheet
/// that zooms loses its heights, so it rises as before.
private struct ZoomFrom: ViewModifier {
    let zoom: Namespace.ID?

    func body(content: Content) -> some View {
        if #available(iOS 26, *), let zoom {
            content.navigationTransition(.zoom(sourceID: accountsZoomID, in: zoom))
        } else {
            content
        }
    }
}

/// The accounts button, which the accounts sheet grows out of.
struct ZoomSource: ViewModifier {
    let zoom: Namespace.ID?

    func body(content: Content) -> some View {
        if #available(iOS 26, *), let zoom {
            content.matchedTransitionSource(id: accountsZoomID, in: zoom)
        } else {
            content
        }
    }
}

/// Under an account the server signed out, a way to sign in again.
private struct SignInAgain: View {
    let account: AccountItem
    let actions: AccountsActions

    var body: some View {
        if account.needsSignIn {
            SheetAction(icon: "chevron.right", text: "signin_again_title", color: .ozeroError) { actions.onSignIn(account) }
        }
    }
}

/// What the accounts button and the sheet it opens share, so the sheet grows out of the button.
let accountsZoomID = "accounts"

extension EnvironmentValues {
    @Entry var accountsZoom: Namespace.ID?
}

/// The drafts, if any, then all inboxes, which always shows, the inboxes of the accounts, each
/// with its folders a tap on its arrow away, the mailboxes of all accounts together, and the
/// settings. With one account, its inbox and folders.
/// Rows of a list, in the accounts sheet and beside the inbox on its side. An account, when held,
/// opens its settings.
struct MailboxRows: View {
    let accounts: [AccountItem]
    let folders: [FolderItem]
    let mailbox: Mailbox
    let drafts: Int
    let unread: [Int64: Int]
    @Binding var allBoxes: AllBoxes
    @Binding var groups: Groups
    let actions: AccountsActions
    /// The inboxes and the mailboxes of all accounts show as switches to move.
    @Binding var editing: Bool
    /// Where there is no bar over the list for it, Edit sits in the inboxes' header.
    let editInHeader: Bool
    /// The settings as the last row, where they are not in a bar.
    var settings = true

    var body: some View {
        if drafts > 0 {
            FolderRow(icon: "square.and.pencil", title: tr("folder_drafts"), open: false, count: drafts, action: actions.onDrafts)
        }
        if let one = accounts.first, accounts.count == 1 {
            Section {
                FolderRow(icon: "tray", title: tr("inbox_title"), open: mailbox == Mail.inbox, count: unread[one.id] ?? 0) {
                    actions.onMailbox(Mail.inbox)
                }
                signInAgain(one)
                if editing {
                    EditFolders(account: one, folders: folders, actions: actions)
                } else {
                    AccountFolders(account: one, folders: folders, mailbox: mailbox, groups: $groups, actions: actions)
                }
            } header: {
                // Beside the list there is no bar for Edit to go in.
                if editInHeader && editable(accounts: accounts, folders: folders) {
                    HStack {
                        Spacer()
                        Button(editing ? "action_done" : "action_edit") { withAnimation(.smooth) { editing.toggle() } }
                            .font(.smallStrong)
                            .textCase(nil)
                    }
                }
            }
        } else {
            Section {
                let inAll = accounts.filter(\.inAll)
                FolderRow(title: tr("folder_all_inbox"), open: mailbox == Mail.inbox,
                          count: inAll.reduce(0) { $0 + (unread[$1.id] ?? 0) }, lead: { StackedDots(accounts: inAll) }) {
                    actions.onMailbox(Mail.inbox)
                }
            }
            inboxes
            if editing {
                // Each account's folders, to order, change and add to.
                ForEach(accounts.filter { account in account.editsFolders || folders.contains { $0.accountId == account.id && $0.pinned } }, id: \.id) { account in
                    Section {
                        EditFolders(account: account, folders: folders, actions: actions)
                    } header: {
                        Label { Text(verbatim: account.name) } icon: { AccountDot(color: account.tint) }
                    }
                }
            }
            allAccounts
        }
        if settings {
            Section {
                FolderRow(icon: "gear", title: tr("settings_title"), open: false, action: actions.onSettings)
            }
        }
    }

    private func toggle(_ change: (Groups) -> Groups) {
        withAnimation(.smooth) { groups = change(groups) }
    }

    /// Each account's inbox with its unread People mail, and its folders under it when its arrow
    /// is open; Edit turns the accounts into switches for All inboxes to move.
    private var inboxes: some View {
        let open = editing || groups.open(Groups.inboxes)
        return Section {
            if open {
                if editing {
                    ForEach(accounts, id: \.id) { account in
                        HStack {
                            Label { Text(verbatim: account.name) } icon: { AccountDot(color: account.tint) }
                            Spacer()
                            // Its settings, in sight: holding the row is not something to find on one's own.
                            Button { actions.onAccount(account) } label: {
                                Image(systemName: "info.circle").frame(width: 44, height: 44).contentShape(.rect)
                            }
                            .buttonStyle(.borderless)
                            .accessibilityLabel(Text("account_settings"))
                            // The unified inbox keeps one account at least; the row still moves.
                            Toggle(account.name, isOn: Binding(get: { account.inAll }, set: { actions.onInAll(account, $0) }))
                                .labelsHidden()
                                .disabled(account.inAllFixed)
                        }
                    }
                    .onMove { from, to in
                        var order = accounts
                        order.move(fromOffsets: from, toOffset: to)
                        actions.onOrder(order)
                    }
                    .environment(\.editMode, .constant(.active))
                } else {
                    ForEach(accounts, id: \.id) { account in
                        let box = folders.first { $0.accountId == account.id && $0.role == .inbox }.map { Mailbox.folder(id: $0.id) }
                        let shown = groups.open(account.id)
                        HStack(spacing: 4) {
                            FolderRow(title: account.name, open: box == mailbox, count: unread[account.id] ?? 0,
                                      lead: { AccountDot(color: account.tint) }) {
                                if let box { actions.onMailbox(box) }
                            }
                            FoldersArrow(account: account.name, open: shown) { toggle { $0.toggled(account.id) } }
                        }
                        .contextMenu {
                            Button { actions.onAccount(account) } label: { Label("account_settings", systemImage: "gear") }
                        }
                        signInAgain(account)
                        if shown {
                            AccountFolders(account: account, folders: folders, mailbox: mailbox, groups: $groups, actions: actions, depth: 1)
                        }
                    }
                }
            }
        } header: {
            PartHeader(title: "inboxes_title", open: open, onToggle: editing ? nil : { toggle { $0.toggled(Groups.inboxes) } }) {
                if editInHeader {
                    Button(editing ? "action_done" : "action_edit") { withAnimation(.smooth) { editing.toggle() } }
                        .font(.smallStrong)
                        .textCase(nil)
                }
            }
        } footer: {
            if editing { Text("inboxes_edit_hint") }
        }
    }

    /// The mailboxes of all accounts together the person chose, in their order; while editing,
    /// all of them as switches to move.
    private var allAccounts: some View {
        let open = editing || groups.open(Groups.allBoxes)
        return Section {
            if editing {
                ForEach(allBoxes.order) { box in
                    Toggle(isOn: Binding(
                        get: { allBoxes.shown.contains(box) },
                        set: { on in if on { allBoxes.shown.insert(box) } else { allBoxes.shown.remove(box) } }
                    )) {
                        Label { Text(verbatim: box.title) } icon: { FolderIcon(name: box.icon) }
                    }
                }
                .onMove { allBoxes.order.move(fromOffsets: $0, toOffset: $1) }
                .environment(\.editMode, .constant(.active))
            } else if open {
                ForEach(allBoxes.order.filter { allBoxes.shown.contains($0) && $0.listed(folders) }) { box in
                    FolderRow(icon: box.icon, title: box.title, open: box.mailbox == mailbox) { actions.onMailbox(box.mailbox) }
                }
            }
        } header: {
            PartHeader(title: "inbox_all_accounts", open: open, onToggle: editing ? nil : { toggle { $0.toggled(Groups.allBoxes) } })
        } footer: {
            // These switches hide a row; the inboxes' leave an account out of All inboxes.
            if editing { Text("all_boxes_edit_hint") }
        }
    }

    private func signInAgain(_ account: AccountItem) -> some View {
        SignInAgain(account: account, actions: actions)
    }
}

/// An account's folders, `depth` steps in: its system ones in the usual order, then its own. Of
/// those, the pinned ones show, or all while the account has few and none pinned; Other folders
/// shows the rest, each inside the one it is in, unless the person closed that one. Holding one
/// of its own offers what can be done with it, pinning first.
private struct AccountFolders: View {
    let account: AccountItem
    let folders: [FolderItem]
    let mailbox: Mailbox
    @Binding var groups: Groups
    let actions: AccountsActions
    var depth = 0

    @State private var all = false

    var body: some View {
        let mine = folders.filter { $0.accountId == account.id && $0.role != .drafts }
        ForEach(mine.filter { $0.role != .custom && $0.role != .inbox }.sorted { $0.role.rank < $1.role.rank }, id: \.id) { folder in
            let box = Mailbox.folder(id: folder.id)
            FolderRow(icon: folder.role.icon, title: folder.title, open: box == mailbox, depth: depth) { actions.onMailbox(box) }
        }
        let own = mine.filter { $0.role == .custom }
        let few = !own.contains(where: \.pinned) && own.count <= fewFolders
        let rows = folderTree(all || few ? own : own.filter(\.pinned), shows: groups.showsInside, depth: depth)
        ForEach(rows, id: \.folder.id) { row in
            let folder = row.folder
            let box = Mailbox.folder(id: folder.id)
            HStack(spacing: 4) {
                FolderRow(icon: folder.pinned && (all || few) ? "pin" : "folder", title: row.title,
                          open: box == mailbox, depth: row.depth) { actions.onMailbox(box) }
                if row.inside {
                    Twisty(folder: row.title, open: groups.showsInside(folder.id)) {
                        withAnimation(.smooth) { groups = groups.toggledInside(folder.id) }
                    }
                }
            }
            .contextMenu {
                Button { actions.onPin(folder, !folder.pinned) } label: {
                    Label(folder.pinned ? "action_unpin" : "action_pin", systemImage: folder.pinned ? "pin.slash" : "pin")
                }
                if account.editsFolders {
                    Divider()
                    Button { actions.onFolderSheet(.new(account: account.id, parent: folder)) } label: {
                        Label("folder_new_inside", systemImage: "folder.badge.plus")
                    }
                    Button { actions.onFolderSheet(.card(folder)) } label: { Label("folder_rename", systemImage: "pencil") }
                    Button { actions.onFolderSheet(.card(folder)) } label: { Label("folder_move", systemImage: "folder") }
                    Divider()
                    Button(role: .destructive) { actions.onFolderSheet(.card(folder, deleting: true)) } label: {
                        Label("folder_delete_ask", systemImage: "trash")
                    }
                }
            }
        }
        if !few {
            Button { withAnimation(.smooth) { all.toggle() } } label: {
                HStack(spacing: afterSlot) {
                    Image(systemName: all ? "chevron.up" : "ellipsis").frame(width: slot)
                    Text(all ? "folders_pinned_only" : "folders_other")
                    Spacer()
                    if !all { Text(verbatim: "\(own.filter { !$0.pinned }.count)").font(.small) }
                }
                .foregroundStyle(.secondary)
                .padding(.leading, indent(depth))
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            // Until the first folder is pinned.
            if all && !own.contains(where: \.pinned) {
                Text("folders_pin_hint").font(.small).foregroundStyle(.secondary).padding(.leading, indent(depth + 1))
            }
        }
    }
}

/// An account's own folders while editing: the pinned ones first, moved by their handles, then
/// the rest as a tree; a tap opens a folder's card, where the account's folders can change. Then
/// a new folder.
private struct EditFolders: View {
    let account: AccountItem
    let folders: [FolderItem]
    let actions: AccountsActions

    var body: some View {
        let own = folders.filter { $0.accountId == account.id && $0.role == .custom }
        let pinned = own.filter(\.pinned).sorted { ($0.rank ?? .max, $0.shortName.lowercased()) < ($1.rank ?? .max, $1.shortName.lowercased()) }
        ForEach(pinned, id: \.id) { folder in
            FolderRow(icon: "pin", title: folder.shortName, open: false) { open(folder) }
        }
        .onMove { from, to in
            var order = pinned
            order.move(fromOffsets: from, toOffset: to)
            actions.onFolderOrder(order)
        }
        .environment(\.editMode, .constant(.active))
        ForEach(folderTree(own.filter { !$0.pinned }), id: \.folder.id) { row in
            FolderRow(icon: "folder", title: row.title, open: false, depth: row.depth) { open(row.folder) }
        }
        if account.editsFolders {
            NewFolderButton(title: tr("folder_new")) { actions.onFolderSheet(.new(account: account.id, parent: nil)) }
        }
    }

    private func open(_ folder: FolderItem) {
        // Without folders to change, the card only pins.
        if account.editsFolders { actions.onFolderSheet(.card(folder)) } else { actions.onPin(folder, !folder.pinned) }
    }
}

/// Something to edit: accounts to switch and move, or folders to make, change or order.
private func editable(accounts: [AccountItem], folders: [FolderItem]) -> Bool {
    accounts.count > 1 || accounts.contains(where: \.editsFolders) || folders.filter(\.pinned).count > 1
}

/// An account with no more own folders than this and none pinned shows them all.
private let fewFolders = 5

/// A section's heading that hides or shows it, with a chevron that says which, and `trailing`
/// at its end. Without `onToggle` the chevron stays, pointing down.
private struct PartHeader<Trailing: View>: View {
    let title: LocalizedStringKey
    let open: Bool
    var onToggle: (() -> Void)?
    @ViewBuilder var trailing: () -> Trailing

    var body: some View {
        HStack {
            if let onToggle {
                Button(action: onToggle) { label.contentShape(.rect) }
                    .buttonStyle(.plain)
                    .accessibilityHint(Text(open ? "action_hide" : "action_show"))
            } else {
                label
            }
            Spacer()
            trailing()
        }
    }

    private var label: some View {
        HStack(spacing: 4) {
            Text(title)
            Image(systemName: "chevron.right")
                .font(.label)
                .rotationEffect(.degrees(open ? 90 : 0))
        }
    }
}

extension PartHeader where Trailing == EmptyView {
    init(title: LocalizedStringKey, open: Bool, onToggle: (() -> Void)?) {
        self.init(title: title, open: open, onToggle: onToggle) { EmptyView() }
    }
}

/// A mailbox's icon: as quiet as the headings, so colour in the list means an account or the
/// open one.
private struct FolderIcon: View {
    let name: String
    var open = false

    var body: some View {
        Image(systemName: name).foregroundStyle(open ? Color.ozeroAccent : .secondary)
    }
}

/// At the end of an account's row: shows or hides its folders under it.
private struct FoldersArrow: View {
    let account: String
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
        .accessibilityLabel(Text(verbatim: tr("account_folders_of", account)))
        .accessibilityHint(Text(open ? "action_collapse" : "action_expand"))
    }
}

/// A row's icon or dot sits in a slot this wide, the label this far after it.
private let slot: CGFloat = 24
private let afterSlot: CGFloat = 14

/// How far in a row `depth` steps down goes: one slot and its gap a step.
private func indent(_ depth: Int) -> CGFloat { CGFloat(depth) * (slot + afterSlot) }

private struct FolderRow<Lead: View>: View {
    let title: String
    let open: Bool
    var count = 0
    /// Steps in, under the account or folder it is in.
    var depth = 0
    @ViewBuilder let lead: () -> Lead
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: afterSlot) {
                lead().frame(width: slot)
                Text(verbatim: title)
                    .foregroundStyle(open ? Color.ozeroAccent : .primary)
                    .fontWeight(open ? .semibold : .regular)
                    .lineLimit(1)
                Spacer()
                if count > 0 {
                    Text(verbatim: "\(count)").font(.small).foregroundStyle(open ? Color.ozeroAccent : .secondary)
                }
            }
            .padding(.leading, indent(depth))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

extension FolderRow where Lead == FolderIcon {
    init(icon: String, title: String, open: Bool, count: Int = 0, depth: Int = 0, action: @escaping () -> Void) {
        self.init(title: title, open: open, count: count, depth: depth, lead: { FolderIcon(name: icon, open: open) }, action: action)
    }
}

/// A row with an icon and a coloured label, as at the end of a sheet.
struct SheetAction: View {
    let icon: String
    let text: LocalizedStringKey
    let color: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label { Text(text).foregroundStyle(color) } icon: { Image(systemName: icon).foregroundStyle(color) }
        }
    }
}

extension FolderRole {
    /// System folders first in the usual order, then the account's own by name.
    var rank: Int {
        switch self {
        case .inbox: 0
        case .snoozed: 1
        case .flagged: 2
        case .sent: 3
        case .drafts: 4
        case .archive: 5
        case .junk: 6
        case .trash: 7
        case .custom: 8
        }
    }

    var icon: String {
        switch self {
        case .inbox: "tray"
        case .snoozed: "clock"
        case .flagged: "flag"
        case .sent: "paperplane"
        case .drafts: "square.and.pencil"
        case .archive: "archivebox"
        case .junk: "xmark.bin"
        case .trash: "trash"
        case .custom: "folder"
        }
    }

    /// Ours for a system folder; `name`, the server's, for the account's own.
    func title(_ name: String) -> String {
        switch self {
        case .inbox: tr("inbox_title")
        case .snoozed: tr("folder_snoozed")
        case .flagged: tr("folder_flagged")
        case .sent: tr("folder_sent")
        case .drafts: tr("folder_drafts")
        case .archive: tr("folder_archive")
        case .junk: tr("folder_junk")
        case .trash: tr("folder_trash")
        case .custom: name
        }
    }

    /// A mailbox of all accounts together, told apart from one account's folder.
    var allTitle: String {
        switch self {
        case .snoozed: tr("folder_all_snoozed")
        case .flagged: tr("folder_all_flagged")
        case .sent: tr("folder_all_sent")
        case .archive: tr("folder_all_archive")
        case .junk: tr("folder_all_junk")
        case .trash: tr("folder_all_trash")
        default: tr("folder_all_inbox")
        }
    }
}

extension FolderItem {
    var title: String { role.title(name) }
}

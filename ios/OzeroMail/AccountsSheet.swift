import SwiftUI

/// What the accounts sheet asks the inbox to do.
struct AccountsActions {
    let onMailbox: (Mailbox) -> Void
    let onDrafts: () -> Void
    let onInAll: (AccountItem, Bool) -> Void
    let onOrder: ([AccountItem]) -> Void
    let onPin: (FolderItem, Bool) -> Void
    let onSignIn: (AccountItem) -> Void
    /// Opens an account's settings.
    let onAccount: (AccountItem) -> Void
    let onSettings: () -> Void
}

/// The mailboxes, for going places, and at the bottom the settings. Holding an account opens its
/// settings; `holdTip` says so until it was done.
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
    let holdTip: Bool

    var body: some View {
        NavigationStack {
            List {
                MailboxRows(
                    accounts: accounts, folders: folders, mailbox: mailbox, drafts: drafts, unread: unread,
                    allBoxes: $allBoxes, groups: $groups, actions: actions)
                Section {
                    FolderRow(icon: "gear", title: tr("settings_title"), open: false, action: actions.onSettings)
                }
                if holdTip && accounts.count > 1 {
                    Section { } footer: { Text("account_settings_hint") }
                }
            }
            .listStyle(.insetGrouped)
            .navigationTitle("folders_title")
            .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }
}

/// The drafts, if any, then all inboxes, which always shows, the inboxes of the accounts, each
/// with its folders a tap on its arrow away, and the mailboxes of all accounts together. With one
/// account, its inbox and folders.
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
    @State private var editing = false

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
                AccountFolders(account: one, folders: folders, mailbox: mailbox, actions: actions)
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
            allAccounts
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
                            AccountFolders(account: account, folders: folders, mailbox: mailbox, actions: actions, depth: 1)
                        }
                    }
                }
            }
        } header: {
            PartHeader(title: "inboxes_title", open: open, onToggle: editing ? nil : { toggle { $0.toggled(Groups.inboxes) } }) {
                Button(editing ? "action_done" : "action_edit") { withAnimation(.smooth) { editing.toggle() } }
                    .font(.footnote.weight(.semibold))
                    .textCase(nil)
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
        }
    }

    /// Under an account the server signed out, a way to sign in again.
    @ViewBuilder private func signInAgain(_ account: AccountItem) -> some View {
        if account.needsSignIn {
            SheetAction(icon: "chevron.right", text: "signin_again_title", color: .ozeroError) { actions.onSignIn(account) }
        }
    }
}

/// An account's folders, `depth` steps in: its system ones in the usual order, then its own. Of
/// those, the pinned ones show, or all while the account has few and none pinned; Other folders
/// shows the rest, each inside the one it is in. Holding one of its own pins or unpins it.
private struct AccountFolders: View {
    let account: AccountItem
    let folders: [FolderItem]
    let mailbox: Mailbox
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
        let shown = treeOrder(all || few ? own : own.filter(\.pinned))
        let ids = Set(shown.map(\.id))
        ForEach(shown, id: \.id) { folder in
            let box = Mailbox.folder(id: folder.id)
            // Inside a folder that shows too, it goes under it by its own name.
            let nested = folder.parent.map(ids.contains) ?? false
            FolderRow(icon: folder.pinned && (all || few) ? "pin" : "folder", title: folder.shownName(ids),
                      open: box == mailbox, depth: depth + (nested ? 1 : 0)) { actions.onMailbox(box) }
                .contextMenu {
                    Button { actions.onPin(folder, !folder.pinned) } label: {
                        Label(folder.pinned ? "action_unpin" : "action_pin", systemImage: folder.pinned ? "pin.slash" : "pin")
                    }
                }
        }
        if !few {
            Button { withAnimation(.smooth) { all.toggle() } } label: {
                HStack(spacing: afterSlot) {
                    Image(systemName: all ? "chevron.up" : "ellipsis").frame(width: slot)
                    Text(all ? "folders_pinned_only" : "folders_other")
                    Spacer()
                    if !all { Text(verbatim: "\(own.count - shown.count)").font(.footnote) }
                }
                .foregroundStyle(.secondary)
                .padding(.leading, indent(depth))
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            if all {
                Text("folders_pin_hint").font(.footnote).foregroundStyle(.secondary).padding(.leading, indent(depth + 1))
            }
        }
    }
}

/// Where a message can move, from `targets`, each with the name it shows with: the account's
/// inbox and archive, then its own folders in tree order.
func moveOrder(_ targets: [FolderItem]) -> [(folder: FolderItem, title: String)] {
    let ids = Set(targets.map(\.id))
    let usual = targets.filter { $0.role != .custom }.sorted { $0.role.rank < $1.role.rank }
    return usual.map { ($0, $0.title) } + treeOrder(targets.filter { $0.role == .custom }).map { ($0, $0.shownName(ids)) }
}

/// `folders` in tree order: each folder with the ones inside it right under it, by the names
/// they show with.
private func treeOrder(_ folders: [FolderItem]) -> [FolderItem] {
    let ids = Set(folders.map(\.id))
    let inside = Dictionary(grouping: folders) { folder in folder.parent.flatMap { ids.contains($0) ? $0 : nil } }
    func under(_ parent: Int64?) -> [FolderItem] {
        (inside[parent] ?? []).sorted { $0.shownName(ids).lowercased() < $1.shownName(ids).lowercased() }
            .flatMap { [$0] + under($0.id) }
    }
    return under(nil)
}

private extension FolderItem {
    /// Inside a folder that shows too, among `ids`, or at the top, its own name; else the whole path.
    func shownName(_ ids: Set<Int64>) -> String {
        (parent.map(ids.contains) ?? true) ? shortName : name
    }
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
                .font(.caption2.weight(.semibold))
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
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.secondary)
                .rotationEffect(.degrees(open ? 90 : 0))
                .frame(width: 36, height: 36)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(verbatim: account))
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
                    Text(verbatim: "\(count)").font(.footnote).foregroundStyle(open ? Color.ozeroAccent : .secondary)
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

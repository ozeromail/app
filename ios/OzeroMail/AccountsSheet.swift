import SwiftUI

/// What the accounts sheet asks the inbox to do.
struct AccountsActions {
    let onMailbox: (Mailbox) -> Void
    let onDrafts: () -> Void
    let onInAll: (AccountItem, Bool) -> Void
    let onOrder: ([AccountItem]) -> Void
    let onPin: (FolderItem, Bool) -> Void
    let onAdd: () -> Void
    let onColor: (AccountItem, Int) -> Void
    let onLabel: (AccountItem, String) -> Void
    let onSignature: (String) -> Void
    let onAccountSignature: (AccountItem, String) -> Void
    let onSignIn: (AccountItem) -> Void
    let onKeepDays: (AccountItem, Int) -> Void
    let onNewMail: (AccountItem, NewMail) -> Void
    let onRemove: (AccountItem) -> Void
    let onWrite: () -> Void
}

/// A page of the accounts sheet past the mailboxes.
enum SheetPage: Hashable {
    case settings
    case swipes
    case signature
    case account(Int64)
}

/// The mailboxes, for going places, and at the bottom the settings and a way to add an account.
/// Holding an account opens its settings in the same sheet.
struct AccountsSheet: View {
    let accounts: [AccountItem]
    let folders: [FolderItem]
    let mailbox: Mailbox
    let drafts: Int
    /// Unread People mail of each account's inbox, by account id.
    let unread: [Int64: Int]
    /// The default one, for accounts without their own.
    let signature: String
    @Binding var swipes: Swipes
    @Binding var groups: Groups
    @Binding var allBoxes: AllBoxes
    let actions: AccountsActions
    /// The page it opens on: an account's, as after adding it, or the settings.
    var startOn: SheetPage?

    @State private var path: [SheetPage] = []

    var body: some View {
        NavigationStack(path: $path) {
            List {
                MailboxRows(
                    accounts: accounts, folders: folders, mailbox: mailbox, drafts: drafts, unread: unread,
                    allBoxes: $allBoxes, groups: $groups, actions: actions, onAccount: { path.append(.account($0.id)) })
            }
            .listStyle(.insetGrouped)
            .navigationTitle("folders_title")
            .navigationBarTitleDisplayMode(.inline)
            .safeAreaInset(edge: .bottom) {
                HStack(spacing: 10) {
                    NavigationLink(value: SheetPage.settings) { FootLabel(icon: "gear", text: "settings_title") }
                    Button(action: actions.onAdd) { FootLabel(icon: "plus", text: "account_add_short") }
                }
                .buttonStyle(.bordered)
                .tint(.primary)
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
                .background(.bar)
            }
            .navigationDestination(for: SheetPage.self) { page in
                switch page {
                case .settings:
                    AppSettings(accounts: accounts, swipes: swipes, signature: signature, actions: actions)
                case .swipes:
                    SwipeSettings(swipes: $swipes)
                case .signature:
                    Form {
                        SignatureSection(saved: signature, label: "signature_title", placeholder: "", hint: "signature_hint",
                                         onSave: actions.onSignature)
                    }
                    .navigationTitle("signature_title")
                case .account(let id):
                    // Gone, say removed meanwhile: nothing to show.
                    if let account = accounts.first(where: { $0.id == id }) {
                        // Not for the last account in the unified inbox, which keeps one at least.
                        let last = account.inAll && accounts.filter(\.inAll).count == 1
                        AccountSettings(account: account, inAll: accounts.count > 1 && !last ? account.inAll : nil,
                                        signature: signature, actions: actions)
                    }
                }
            }
        }
        .onAppear { if let startOn { path = [startOn] } }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }
}

/// A wide label for a button at the bottom of the sheet.
private struct FootLabel: View {
    let icon: String
    let text: LocalizedStringKey

    var body: some View {
        Label(text, systemImage: icon)
            .fontWeight(.medium)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 6)
    }
}

/// The drafts, if any, then the inboxes, all accounts' and each one's, the mailboxes of all
/// accounts together and each account's folders. With one account, its inbox and folders.
/// Rows of a list, in the accounts sheet and beside the inbox on its side. `onAccount`, when an
/// account is held, opens its settings; nil leaves that out.
struct MailboxRows: View {
    let accounts: [AccountItem]
    let folders: [FolderItem]
    let mailbox: Mailbox
    let drafts: Int
    let unread: [Int64: Int]
    @Binding var allBoxes: AllBoxes
    @Binding var groups: Groups
    let actions: AccountsActions
    var onAccount: ((AccountItem) -> Void)?

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
            inboxes
            allAccounts
            if groups.open(Groups.folders) {
                ForEach(Array(accounts.enumerated()), id: \.element.id) { index, account in
                    FolderGroup(
                        title: account.name,
                        open: groups.open(account.id),
                        caption: { if index == 0 { foldersHeader } },
                        lead: { AccountDot(color: account.tint, size: 12) },
                        onToggle: { toggle { $0.toggled(account.id) } },
                        onHold: onAccount.map { open in { open(account) } }
                    ) {
                        AccountFolders(account: account, folders: folders, mailbox: mailbox, actions: actions)
                    }
                }
            } else {
                Section { } header: { foldersHeader }
            }
        }
    }

    private var foldersHeader: some View {
        PartHeader(title: "account_folders", open: groups.open(Groups.folders), onToggle: { toggle { $0.toggled(Groups.folders) } })
    }

    private func toggle(_ change: (Groups) -> Groups) {
        withAnimation(.smooth) { groups = change(groups) }
    }

    /// All inboxes together, then each account's, with their unread People mail; Edit turns the
    /// accounts into switches for All inboxes to move.
    private var inboxes: some View {
        let open = editing || groups.open(Groups.inboxes)
        return Section {
            if open {
                let inAll = accounts.filter(\.inAll)
                FolderRow(title: tr("folder_all_inbox"), open: mailbox == Mail.inbox,
                          count: inAll.reduce(0) { $0 + (unread[$1.id] ?? 0) }, lead: { StackedDots(accounts: inAll) }) {
                    actions.onMailbox(Mail.inbox)
                }
                if editing {
                    ForEach(accounts, id: \.id) { account in
                        Toggle(isOn: Binding(get: { account.inAll }, set: { actions.onInAll(account, $0) })) {
                            Label { Text(verbatim: account.name) } icon: { AccountDot(color: account.tint, size: 12) }
                        }
                        // The unified inbox keeps one account at least.
                        .disabled(account.inAll && inAll.count == 1)
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
                        FolderRow(title: account.name, open: box == mailbox, count: unread[account.id] ?? 0,
                                  lead: { AccountDot(color: account.tint, size: 12) }) {
                            if let box { actions.onMailbox(box) }
                        }
                        .contextMenu {
                            if let onAccount {
                                Button { onAccount(account) } label: { Label("account_settings", systemImage: "gear") }
                            }
                        }
                        signInAgain(account)
                    }
                }
            }
        } header: {
            PartHeader(title: "inboxes_title", open: open, onToggle: editing ? nil : { toggle { $0.toggled(Groups.inboxes) } }) {
                Button(editing ? "action_done" : "action_edit") { withAnimation(.smooth) { editing.toggle() } }
                    .font(.subheadline.weight(.semibold))
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

/// An account's folders: its system ones in the usual order, then its own. Of those, the pinned
/// ones show, or all while the account has few and none pinned; Other folders shows the rest,
/// each inside the one it is in. Holding one of its own pins or unpins it.
private struct AccountFolders: View {
    let account: AccountItem
    let folders: [FolderItem]
    let mailbox: Mailbox
    let actions: AccountsActions

    @State private var all = false

    var body: some View {
        let mine = folders.filter { $0.accountId == account.id && $0.role != .drafts }
        ForEach(mine.filter { $0.role != .custom && $0.role != .inbox }.sorted { $0.role.rank < $1.role.rank }, id: \.id) { folder in
            let box = Mailbox.folder(id: folder.id)
            FolderRow(icon: folder.role.icon, title: folder.title, open: box == mailbox) { actions.onMailbox(box) }
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
                      open: box == mailbox, indent: nested) { actions.onMailbox(box) }
                .contextMenu {
                    Button { actions.onPin(folder, !folder.pinned) } label: {
                        Label(folder.pinned ? "action_unpin" : "action_pin", systemImage: folder.pinned ? "pin.slash" : "pin")
                    }
                }
        }
        if !few {
            Button { withAnimation(.smooth) { all.toggle() } } label: {
                HStack(spacing: 14) {
                    Image(systemName: all ? "chevron.up" : "ellipsis").frame(width: 24)
                    Text(all ? "folders_pinned_only" : "folders_other")
                    Spacer()
                    if !all { Text(verbatim: "\(own.count - shown.count)").font(.footnote) }
                }
                .foregroundStyle(.secondary)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            if all {
                Text("folders_pin_hint").font(.footnote).foregroundStyle(.secondary)
            }
        }
    }
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
            Button { onToggle?() } label: {
                HStack(spacing: 4) {
                    Text(title)
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .rotationEffect(.degrees(open ? 90 : 0))
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .disabled(onToggle == nil)
            .accessibilityHint(Text(open ? "action_collapse" : "action_expand"))
            Spacer()
            trailing()
        }
    }
}

extension PartHeader where Trailing == EmptyView {
    init(title: LocalizedStringKey, open: Bool, onToggle: (() -> Void)?) {
        self.init(title: title, open: open, onToggle: onToggle) { EmptyView() }
    }
}

/// A group's name, which hides or shows its folders, under `caption` when it starts a part of
/// the list; holding it runs `onHold`.
private struct FolderGroup<Caption: View, Lead: View, Folders: View>: View {
    let title: String
    let open: Bool
    @ViewBuilder let caption: () -> Caption
    @ViewBuilder let lead: () -> Lead
    let onToggle: () -> Void
    var onHold: (() -> Void)?
    @ViewBuilder let folders: () -> Folders

    var body: some View {
        Section {
            if open { folders() }
        } header: {
            VStack(alignment: .leading, spacing: 12) {
                caption()
                HStack(spacing: 14) {
                    lead().frame(width: 24)
                    Button(action: onToggle) {
                        HStack(spacing: 4) {
                            Text(verbatim: title).font(.body.weight(.semibold)).foregroundStyle(.primary).lineLimit(1)
                            // Points down while the folders show, to the side while they are hidden.
                            Image(systemName: "chevron.right")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .rotationEffect(.degrees(open ? 90 : 0))
                        }
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint(Text(open ? "action_collapse" : "action_expand"))
                    .contextMenu {
                        if let onHold {
                            Button(action: onHold) { Label("account_settings", systemImage: "gear") }
                        }
                    }
                    Spacer()
                }
                .textCase(nil)
            }
        }
    }
}

private struct FolderIcon: View {
    let name: String

    var body: some View {
        Image(systemName: name).foregroundStyle(Color.ozeroAccent)
    }
}

private struct FolderRow<Lead: View>: View {
    let title: String
    let open: Bool
    var count = 0
    /// Inside the folder above it.
    var indent = false
    @ViewBuilder let lead: () -> Lead
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                lead().frame(width: 24)
                Text(verbatim: title)
                    .foregroundStyle(open ? Color.ozeroAccent : .primary)
                    .fontWeight(open ? .semibold : .regular)
                    .lineLimit(1)
                Spacer()
                if count > 0 {
                    Text(verbatim: "\(count)").font(.footnote).foregroundStyle(.secondary)
                }
            }
            .padding(.leading, indent ? 24 : 0)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

extension FolderRow where Lead == FolderIcon {
    init(icon: String, title: String, open: Bool, count: Int = 0, indent: Bool = false, action: @escaping () -> Void) {
        self.init(title: title, open: open, count: count, indent: indent, lead: { FolderIcon(name: icon) }, action: action)
    }
}

/// The settings: the accounts, in an order of the person's, each opening its own page; the
/// swipes, the signature and a letter to the author.
private struct AppSettings: View {
    let accounts: [AccountItem]
    let swipes: Swipes
    let signature: String
    let actions: AccountsActions

    var body: some View {
        List {
            Section {
                ForEach(accounts, id: \.id) { account in
                    NavigationLink(value: SheetPage.account(account.id)) {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(verbatim: account.name).lineLimit(1)
                                if account.name != account.email {
                                    Text(verbatim: account.email).font(.footnote).foregroundStyle(.secondary).lineLimit(1)
                                }
                            }
                        } icon: {
                            AccountDot(color: account.tint, size: 12)
                        }
                    }
                }
                .onMove { from, to in
                    var order = accounts
                    order.move(fromOffsets: from, toOffset: to)
                    actions.onOrder(order)
                }
                SheetAction(icon: "plus", text: "inbox_add_account", color: .ozeroAccent, action: actions.onAdd)
            } header: {
                Text("accounts_title")
            } footer: {
                if accounts.count > 1 { Text("account_settings_hint") }
            }
            Section("mail_settings") {
                NavigationLink(value: SheetPage.swipes) {
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("swipes_title")
                            Text(swipes.summary).font(.footnote).foregroundStyle(.secondary)
                        }
                    } icon: {
                        Image(systemName: "hand.draw").foregroundStyle(Color.ozeroAccent)
                    }
                }
                NavigationLink(value: SheetPage.signature) {
                    Label {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("signature_title")
                            Text(verbatim: signature.components(separatedBy: .newlines).first { !$0.isBlank } ?? tr("signature_none"))
                                .font(.footnote).foregroundStyle(.secondary).lineLimit(1)
                        }
                    } icon: {
                        Image(systemName: "signature").foregroundStyle(Color.ozeroAccent)
                    }
                }
                SheetAction(icon: "square.and.pencil", text: "feedback_write", color: .primary, action: actions.onWrite)
            }
        }
        .listStyle(.insetGrouped)
        .navigationTitle("settings_title")
        .toolbar {
            // Edit shows the handles that put the accounts in another order.
            if accounts.count > 1 { EditButton() }
        }
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

/// Which action each swipe does.
private struct SwipeSettings: View {
    @Binding var swipes: Swipes

    var body: some View {
        Form {
            side("swipes_left", $swipes.left)
            side("swipes_right", $swipes.right)
        }
        .navigationTitle("swipes_title")
    }

    private func side(_ title: LocalizedStringKey, _ selection: Binding<SwipeAction>) -> some View {
        Picker(title, selection: selection) {
            ForEach(SwipeAction.allCases) { Label($0.label, systemImage: $0.icon).tag($0) }
        }
        .pickerStyle(.inline)
    }
}

/// One account: its name and colour, when new mail comes, how much of its mail the phone keeps,
/// and a way to remove it. One the server signed out offers to sign in again first.
private struct AccountSettings: View {
    let account: AccountItem
    /// Whether the unified inbox takes it in; nil with one account.
    let inAll: Bool?
    /// The default one, shown while the account has none.
    let signature: String
    let actions: AccountsActions

    @State private var label: String
    /// The name last sent, so leaving after Done does not send it again.
    @State private var saved: String
    @State private var confirming = false
    @FocusState private var editing: Bool

    init(account: AccountItem, inAll: Bool?, signature: String, actions: AccountsActions) {
        self.account = account
        self.inAll = inAll
        self.signature = signature
        self.actions = actions
        _label = State(initialValue: account.label)
        _saved = State(initialValue: account.label)
    }

    var body: some View {
        Form {
            if account.needsSignIn {
                Section {
                    SheetAction(icon: "chevron.right", text: "signin_again_title", color: .ozeroAccent) {
                        actions.onSignIn(account)
                    }
                } footer: {
                    Text("account_signed_out")
                }
            }
            Section {
                HStack {
                    TextField(text: $label, prompt: Text(verbatim: account.email)) { Text("account_name") }
                        .textInputAutocapitalization(.sentences)
                        .submitLabel(.done)
                        .focused($editing)
                        .onSubmit(saveLabel)
                    if label.trimmingCharacters(in: .whitespaces) != saved {
                        Button("action_save") {
                            editing = false
                            saveLabel()
                        }
                        .fontWeight(.medium)
                    }
                }
            } header: {
                Text("account_name")
            } footer: {
                // The sender name is not what the list shows, so it is said here.
                let sender = account.displayName.trimmingCharacters(in: .whitespaces)
                let hint = tr("account_name_hint")
                Text(verbatim: sender.isEmpty || sender == account.email ? hint : tr("account_sender", hint, sender))
            }
            if let inAll {
                Section {
                    Toggle("all_in", isOn: Binding(get: { inAll }, set: { actions.onInAll(account, $0) }))
                }
            }
            SignatureSection(saved: account.signature, label: "account_signature", placeholder: signature,
                             hint: "account_signature_hint") { actions.onAccountSignature(account, $0) }
            Section("account_color") {
                Swatches(current: Int(account.colorIndex)) { actions.onColor(account, $0) }
            }
            Section {
                Picker("account_new_mail", selection: Binding(get: { account.newMail }, set: { actions.onNewMail(account, $0) })) {
                    ForEach(allNewMail, id: \.check) { Text($0.label).tag($0.check) }
                }
                .pickerStyle(.segmented)
            } header: {
                Text("account_new_mail")
            } footer: {
                switch account.newMail {
                case .manually: Text("account_new_mail_manually_hint")
                case .every: Text("account_new_mail_every_hint")
                }
            }
            Section {
                Picker("account_keep", selection: Binding(get: { Int(account.keepDays) }, set: { actions.onKeepDays(account, $0) })) {
                    ForEach(keepDays, id: \.days) { Text($0.label).tag($0.days) }
                }
                // Six spans are too many for segments on a phone.
                .pickerStyle(.menu)
            } footer: {
                Text("account_keep_hint")
            }
            Section {
                Button("account_remove", role: .destructive) { confirming = true }
            }
        }
        .navigationTitle(Text(verbatim: account.name))
        .onDisappear(perform: saveLabel)
        .onChange(of: editing) { _, now in if !now { saveLabel() } }
        .alert("account_remove_title", isPresented: $confirming) {
            Button("action_remove", role: .destructive) { actions.onRemove(account) }
            Button("action_cancel", role: .cancel) {}
        } message: {
            Text("account_remove_text")
        }
    }

    /// Saved on Done, when the field loses focus and when the page closes.
    private func saveLabel() {
        let name = label.trimmingCharacters(in: .whitespaces)
        guard name != saved else { return }
        saved = name
        actions.onLabel(account, label)
    }
}

/// When the phone looks for an account's new mail with the app closed, and what to call it.
private let allNewMail: [(check: NewMail, label: LocalizedStringKey)] = [
    (.every(minutes: 15), "account_new_mail_quarter"), (.every(minutes: 60), "account_new_mail_hourly"),
    (.manually, "account_new_mail_manually"),
]

/// A signature of a few lines under `label`, `placeholder` while empty; saved when it loses focus
/// and when the page closes.
private struct SignatureSection: View {
    let label: LocalizedStringKey
    let placeholder: String
    let hint: LocalizedStringKey
    let onSave: (String) -> Void

    @State private var text: String
    /// What was last sent, so leaving after a save does not send it again.
    @State private var sent: String
    @FocusState private var editing: Bool

    init(saved: String, label: LocalizedStringKey, placeholder: String, hint: LocalizedStringKey,
         onSave: @escaping (String) -> Void) {
        self.label = label
        self.placeholder = placeholder
        self.hint = hint
        self.onSave = onSave
        _text = State(initialValue: saved)
        _sent = State(initialValue: saved)
    }

    var body: some View {
        Section {
            TextField(text: $text, prompt: Text(verbatim: placeholder), axis: .vertical) { Text(label) }
                .lineLimit(3...8)
                .focused($editing)
        } header: {
            Text(label)
        } footer: {
            Text(hint)
        }
        .onDisappear(perform: save)
        .onChange(of: editing) { _, now in if !now { save() } }
    }

    private func save() {
        // Trimmed at the end only, as it is kept.
        let trimmed = text.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression)
        guard trimmed != sent else { return }
        sent = trimmed
        onSave(text)
    }
}

/// How many days back an account's mail stays on the phone, 0 for all of it, and what to call it.
private let keepDays: [(days: Int, label: LocalizedStringKey)] = [
    (3, "account_keep_three_days"), (7, "account_keep_week"), (30, "account_keep_month"), (90, "account_keep_quarter"), (365, "account_keep_year"), (0, "account_keep_all"),
]

/// The account colours in rows; the current one has a ring.
private struct Swatches: View {
    let current: Int
    let onPick: (Int) -> Void

    // In rainbow order, grey last, so each row reads as a run of hues; the saved place stays the same.
    private let colors = accountColors().enumerated().sorted { hueOrder($0.element) < hueOrder($1.element) }

    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible()), count: 5), spacing: 16) {
            ForEach(colors, id: \.offset) { index, color in
                let on = index == current
                Button { onPick(index) } label: {
                    Circle()
                        .fill(Color(rgb: color))
                        .frame(width: 40, height: 40)
                        .overlay {
                            if on { Image(systemName: "checkmark").font(.footnote.bold()).foregroundStyle(.white) }
                        }
                        .padding(4)
                        .overlay(Circle().stroke(on ? Color.primary : .clear, lineWidth: 2))
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .padding(.vertical, 8)
    }
}

/// Where a colour falls from red round to pink; greys go after all of them.
private func hueOrder(_ rgb: UInt32) -> Double {
    var hue: CGFloat = 0
    var saturation: CGFloat = 0
    UIColor(Color(rgb: rgb)).getHue(&hue, saturation: &saturation, brightness: nil, alpha: nil)
    // Reds just under 360° belong at the start.
    return saturation < 0.35 ? 1000 : (hue * 360 + 20).truncatingRemainder(dividingBy: 360)
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

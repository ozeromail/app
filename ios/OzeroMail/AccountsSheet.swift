import SwiftUI

/// What the accounts sheet asks the inbox to do.
struct AccountsActions {
    let onMailbox: (Mailbox) -> Void
    let onDrafts: () -> Void
    let onAdd: () -> Void
    let onColor: (AccountItem, Int) -> Void
    let onLabel: (AccountItem, String) -> Void
    let onSignIn: (AccountItem) -> Void
    let onKeepDays: (AccountItem, Int) -> Void
    let onNewMail: (AccountItem, NewMail) -> Void
    let onRemove: (AccountItem) -> Void
    let onWrite: () -> Void
}

/// The mailboxes, and below them the swipes and a way to add an account. A group's More button,
/// or Swipes, opens the settings in the same sheet.
struct AccountsSheet: View {
    let accounts: [AccountItem]
    let folders: [FolderItem]
    let mailbox: Mailbox
    let drafts: Int
    @Binding var swipes: Swipes
    @Binding var groups: Groups
    let actions: AccountsActions

    @Environment(\.dismiss) private var dismiss
    @State private var path: [Page]

    /// `startOn` is the account whose page it opens on, as after adding it.
    init(
        accounts: [AccountItem], folders: [FolderItem], mailbox: Mailbox, drafts: Int, swipes: Binding<Swipes>,
        groups: Binding<Groups>, actions: AccountsActions, startOn: Int64? = nil
    ) {
        self.accounts = accounts
        self.folders = folders
        self.mailbox = mailbox
        self.drafts = drafts
        _swipes = swipes
        _groups = groups
        self.actions = actions
        _path = State(initialValue: startOn.map { [.account($0)] } ?? [])
    }

    var body: some View {
        NavigationStack(path: $path) {
            List {
                MailboxRows(
                    accounts: accounts, folders: folders, mailbox: mailbox, drafts: drafts, groups: $groups,
                    settings: true, onMailbox: actions.onMailbox, onDrafts: actions.onDrafts)
                Section {
                    NavigationLink(value: Page.swipes) {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("swipes_title")
                                Text(swipes.summary).font(.footnote).foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: "hand.draw").foregroundStyle(Color.ozeroAccent)
                        }
                    }
                    SheetAction(icon: "plus", text: "inbox_add_account", color: .ozeroAccent, action: actions.onAdd)
                    SheetAction(icon: "square.and.pencil", text: "feedback_write", color: .primary, action: actions.onWrite)
                }
            }
            .listStyle(.insetGrouped)
            .navigationTitle("folders_title")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("action_close") { dismiss() }
                }
            }
            .navigationDestination(for: Page.self) { page in
                switch page {
                case .swipes:
                    SwipeSettings(swipes: $swipes)
                case .account(let id):
                    // Gone, say removed meanwhile: nothing to show.
                    if let account = accounts.first(where: { $0.id == id }) {
                        AccountSettings(account: account, actions: actions)
                    }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }

}

/// The drafts on the phone, if there are any, the mailboxes of all accounts together, then each
/// account's folders: rows of a list, in the accounts sheet and beside the inbox on its side.
/// A tap on a group's name hides or shows its folders; with `settings` its More button opens the
/// account's settings in the sheet.
struct MailboxRows: View {
    let accounts: [AccountItem]
    let folders: [FolderItem]
    let mailbox: Mailbox
    let drafts: Int
    @Binding var groups: Groups
    let settings: Bool
    let onMailbox: (Mailbox) -> Void
    let onDrafts: () -> Void

    private var many: Bool { accounts.count > 1 }

    var body: some View {
        if drafts > 0 {
            FolderRow(icon: "square.and.pencil", title: tr("folder_drafts"), open: false, count: drafts) {
                onDrafts()
            }
        }
        if many {
            FolderGroup(title: tr("inbox_all_accounts"), open: groups.open(Settings.allAccounts, many: true), lead: {
                StackedDots(accounts: accounts)
            }, onToggle: { toggle(Settings.allAccounts) }) {
                ForEach(unifiedRoles.filter { role in role == .inbox || folders.contains { $0.role == role } }, id: \.self) { role in
                    let box = Mailbox.unified(role: role)
                    FolderRow(icon: role.icon, title: role.allTitle, open: box == mailbox) { onMailbox(box) }
                }
            }
        }
        ForEach(accounts, id: \.id) { account in
            FolderGroup(title: account.name, open: groups.open(account.id, many: many), lead: {
                AccountDot(color: account.tint, size: 12)
            }, onToggle: { toggle(account.id) }, settings: settings ? account : nil) {
                ForEach(accountFolders(account), id: \.id) { folder in
                    // With one account its inbox is the unified one, which keeps the tabs.
                    let box = folder.role == .inbox && !many ? Mail.inbox : Mailbox.folder(id: folder.id)
                    FolderRow(icon: folder.role.icon, title: folder.title, open: box == mailbox) { onMailbox(box) }
                }
            }
        }
    }

    private func toggle(_ group: Int64) {
        withAnimation(.smooth) { groups = groups.toggled(group, many: many) }
    }

    private func accountFolders(_ account: AccountItem) -> [FolderItem] {
        folders.filter { $0.accountId == account.id && $0.role != .drafts }
            .sorted { ($0.role.rank, $0.name.lowercased()) < ($1.role.rank, $1.name.lowercased()) }
    }
}

/// A page of the sheet past the list.
private enum Page: Hashable {
    case swipes
    case account(Int64)
}

/// The mailboxes shared by all accounts, in order.
private let unifiedRoles: [FolderRole] = [.inbox, .snoozed, .flagged, .sent, .archive, .trash]

/// A group's name, which hides or shows its folders, and with `settings` a way to that
/// account's settings.
private struct FolderGroup<Lead: View, Folders: View>: View {
    let title: String
    let open: Bool
    @ViewBuilder let lead: () -> Lead
    let onToggle: () -> Void
    var settings: AccountItem?
    @ViewBuilder let folders: () -> Folders

    var body: some View {
        Section {
            if open { folders() }
        } header: {
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
                Spacer()
                if let settings {
                    NavigationLink(value: Page.account(settings.id)) {
                        Image(systemName: "ellipsis.circle").font(.title3).foregroundStyle(.secondary)
                    }
                    .accessibilityLabel(Text("account_settings"))
                }
            }
            .textCase(nil)
        }
    }
}

private struct FolderRow: View {
    let icon: String
    let title: String
    let open: Bool
    var count = 0
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: icon).foregroundStyle(Color.ozeroAccent).frame(width: 24)
                Text(verbatim: title)
                    .foregroundStyle(open ? Color.ozeroAccent : .primary)
                    .fontWeight(open ? .semibold : .regular)
                    .lineLimit(1)
                Spacer()
                if count > 0 {
                    Text(verbatim: "\(count)").font(.footnote).foregroundStyle(.secondary)
                }
            }
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
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
    let actions: AccountsActions

    @State private var label: String
    /// The name last sent, so leaving after Done does not send it again.
    @State private var saved: String
    @State private var confirming = false
    @FocusState private var editing: Bool

    init(account: AccountItem, actions: AccountsActions) {
        self.account = account
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
            Section("account_color") {
                Swatches(current: Int(account.colorIndex)) { actions.onColor(account, $0) }
            }
            Section {
                Picker("account_new_mail", selection: Binding(get: { account.newMail }, set: { actions.onNewMail(account, $0) })) {
                    ForEach(newMail, id: \.check) { Text($0.label).tag($0.check) }
                }
                .pickerStyle(.segmented)
            } header: {
                Text("account_new_mail")
            } footer: {
                switch account.newMail {
                case .push: Text("account_new_mail_push_hint_ios")
                case .manually: Text("account_new_mail_manually_hint")
                case .every: Text("account_new_mail_every_hint")
                }
            }
            Section {
                Picker("account_keep", selection: Binding(get: { Int(account.keepDays) }, set: { actions.onKeepDays(account, $0) })) {
                    ForEach(keepDays, id: \.days) { Text($0.label).tag($0.days) }
                }
                .pickerStyle(.segmented)
            } header: {
                Text("account_keep")
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

    /// The choices of when new mail comes; Instantly only where the server can.
    private var newMail: [(check: NewMail, label: LocalizedStringKey)] {
        allNewMail.filter { account.canPush || $0.check != .push }
    }
}

/// When the phone looks for an account's new mail with the app closed, and what to call it.
private let allNewMail: [(check: NewMail, label: LocalizedStringKey)] = [
    (.push, "account_new_mail_push"), (.every(minutes: 15), "account_new_mail_quarter"),
    (.every(minutes: 60), "account_new_mail_hourly"), (.manually, "account_new_mail_manually"),
]

/// How many days back an account's mail stays on the phone, 0 for all of it, and what to call it.
private let keepDays: [(days: Int, label: LocalizedStringKey)] = [
    (30, "account_keep_month"), (90, "account_keep_quarter"), (365, "account_keep_year"), (0, "account_keep_all"),
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
        case .trash: tr("folder_all_trash")
        default: tr("folder_all_inbox")
        }
    }
}

extension FolderItem {
    var title: String { role.title(name) }
}

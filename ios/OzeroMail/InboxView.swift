import Network
import SwiftUI
import UserNotifications

/// The unified inbox, or another mailbox picked in the accounts sheet: shows what is saved at
/// once, then syncs; pull down to sync again. Everything the thumb needs sits along the bottom.
struct InboxView: View {
    @State private var model: InboxModel
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.sideways) private var sideways

    init(mail: Mail, onNoAccounts: @escaping () -> Void) {
        _model = State(initialValue: InboxModel(mail: mail, onNoAccounts: onNoAccounts))
    }

    var body: some View {
        @Bindable var model = model
        NavigationStack(path: $model.path) {
            // On its side the mailboxes are beside the list, and make room for a message opened
            // beside it.
            HStack(spacing: 0) {
                if model.sideways && model.pane == nil {
                    Mailboxes(model: model)
                        .transition(.move(edge: .leading).combined(with: .opacity))
                    Divider()
                }
                InboxList(model: model)
                    .frame(maxWidth: model.pane == nil ? .infinity : Room.listPane)
                if let pane = model.pane {
                    Divider()
                    MessageView(model: model, message: pane, pane: true)
                        .id(pane.id)
                }
            }
            .animation(.smooth, value: model.pane == nil)
            .navigationDestination(for: Route.self) { route in
                switch route {
                case .message(let message):
                    MessageView(model: model, message: message)
                        .navigationBarTitleDisplayMode(.inline)
                case .search:
                    SearchView(model: model)
                case .drafts:
                    DraftsView(model: model)
                }
            }
        }
        .sheet(item: $model.snoozing) { snoozing in
            SnoozeSheet { until in
                model.snoozing = nil
                snoozing.then()
                Task { await model.snooze(snoozing.messages, until: until) }
            }
        }
        .sheet(isPresented: $model.accountsOpen) {
            AccountsSheet(
                accounts: model.accounts, folders: model.folders, mailbox: model.mailbox, drafts: model.drafts.count,
                swipes: $model.swipes, groups: $model.groups, actions: accountsActions)
            .task { await model.loadFolders() }
        }
        .fullScreenCover(isPresented: $model.addingAccount) {
            SignInView(mail: model.mail, onCancel: { model.addingAccount = false }) {
                model.addingAccount = false
                model.signedIn(new: true)
            }
        }
        .fullScreenCover(item: $model.signingIn) { account in
            SignInView(mail: model.mail, again: account, onCancel: { model.signingIn = nil }) {
                model.signingIn = nil
                model.signedIn()
            }
        }
        .fullScreenCover(item: $model.composing) { composing in
            ComposeView(
                mail: model.mail, composing: composing,
                onClose: { model.composing = nil }, onSent: model.sent, onKept: model.kept,
                onDiscard: { model.discarded($0, original: composing.original) })
        }
        .task {
            await model.loadAccounts()
            await model.load()
            await model.sync()
        }
        .task {
            for await _ in model.mail.changes() {
                await model.changed()
            }
        }
        .task {
            // Asked once the inbox shows, so it is clear what the notifications are for.
            _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])
            // Only once there is an account: without one a check has nothing to do.
            BackgroundCheck.schedule()
        }
        .task(id: WatchKey(active: scenePhase == .active, accounts: model.accounts)) {
            // While the app is on screen, new mail shows as it arrives. An account signed in to
            // again starts waiting, with its new secret.
            guard scenePhase == .active else { return }
            let inbox = self.model
            await inbox.mail.watch(inbox.accounts) { await inbox.sync(quiet: true) }
        }
        .task(id: scenePhase == .active) {
            guard scenePhase == .active else { return }
            for await _ in backOnline() {
                await model.backOnline()
            }
        }
        .onChange(of: sideways, initial: true) { _, now in model.sideways = now }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { Task { await model.returned() } }
        }
        .onChange(of: model.mailbox) {
            Task {
                await model.switched()
                // Another mailbox brings in what changed in it; the inbox keeps itself up to date.
                if !model.isInbox { await model.sync(model.mailbox) }
            }
        }
        .onChange(of: model.tab) {
            Task { await model.switched() }
        }
    }

    private var accountsActions: AccountsActions {
        AccountsActions(
            onMailbox: { box in
                model.accountsOpen = false
                model.mailbox = box
            },
            onDrafts: {
                model.accountsOpen = false
                model.path.append(.drafts)
            },
            onAdd: {
                model.accountsOpen = false
                model.addingAccount = true
            },
            onColor: { account, color in model.changeAccount { try await model.mail.setAccountColor(account, color: color) } },
            onLabel: { account, label in model.changeAccount { try await model.mail.setAccountLabel(account, label: label) } },
            onSignIn: { account in
                model.accountsOpen = false
                model.signingIn = account
            },
            onKeepDays: { account, days in model.changeAccount { try await model.mail.setKeepDays(account, days: days) } },
            onRemove: { account in Task { await model.remove(account) } },
            onWrite: {
                model.accountsOpen = false
                model.composing = Composing(draft: authorDraft())
            })
    }
}

/// Where a list's content top is, from the top of its frame: `offset` above it while scrolled,
/// below zero while pulled; `inset` is the bars over it.
struct ListTop: Equatable {
    var offset: CGFloat = 0
    var inset: CGFloat = 0
}

/// What the watch for new mail depends on: the app on screen, and which accounts are signed in.
private struct WatchKey: Equatable {
    let active: Bool
    let accounts: [Int64: Bool]

    init(active: Bool, accounts: [AccountItem]) {
        self.active = active
        self.accounts = Dictionary(accounts.map { ($0.id, $0.needsSignIn) }, uniquingKeysWith: { a, _ in a })
    }
}

extension AccountItem: Identifiable {}

/// The list under its large title, with the dock, the toast and errors along the bottom.
private struct InboxList: View {
    @Bindable var model: InboxModel
    @State private var more = false
    /// How the list is pulled down, as the water reads it.
    @State private var pull = PullState()
    /// Where the list's content top is: above the top while scrolled, below it while pulled.
    @State private var top = ListTop()
    @State private var dragging = false
    /// Only a pull shows the water's sync; syncs on their own stay quiet over the saved mail.
    @State private var refreshing = false
    /// After a sync the gap stays open until its water has calmed, so the calm is seen; and after a
    /// tug, until its drop is in.
    @State private var waterCalm = true
    /// How much of the gap is held open, from 0 to 1.
    @State private var held: CGFloat = 0
    /// A check the user waits on: how long it has gone (0, slow, given up), and whether it just went through.
    @State private var slow = 0
    @State private var upToDate = false
    /// Counts the checks asked for, so one let go of does not end a newer one.
    @State private var checks = 0

    /// The first mail of an empty list is on its way: the water waits, and says what for.
    private var waiting: Bool {
        // Only the first time: after that an empty list is empty until a sync brings something.
        model.syncing && model.listLoaded && model.messages.isEmpty && !model.everSynced.contains(model.mailbox.key)
    }

    private var checking: LakeWords? {
        if waiting { return LakeWords(title: "lake_loading_title", text: slow > 0 ? "lake_loading_slow" : "lake_loading_text") }
        if refreshing { return LakeWords(title: "lake_checking_title", text: slow > 0 ? "status_slow" : "lake_checking_text") }
        return nil
    }

    /// What the line over the dock says, and whether a new check may help.
    private var status: (text: String, retry: Bool)? {
        if let error = model.error {
            // Like Apple Mail, says which account it is when the others got through.
            let accounts = model.failing.joined(separator: ", ")
            if error == "error_network" {
                return (accounts.isEmpty ? tr("status_offline") : tr("status_offline_account", accounts), true)
            }
            // A sync's own failures; another error is not one a new check mends.
            let text = accounts.isEmpty ? tr(error) : tr("status_account_failed", accounts, tr(error))
            return (text, error == "error_server" || error == "error_tls")
        }
        if model.syncing && slow == 2 && !waiting { return (tr("status_background"), false) }
        // An empty list's words say it themselves.
        if refreshing && !model.messages.isEmpty { return (tr(slow > 0 ? "status_slow" : "status_checking"), false) }
        if upToDate { return (tr("status_up_to_date"), false) }
        return nil
    }

    private var picked: [MessageItem] {
        // Only what the list still shows: a sync may take a chosen message away.
        model.messages.filter { model.chosen.contains($0.id) }
    }

    private var choosing: Bool { !picked.isEmpty }

    var body: some View {
        let lake = model.messages.isEmpty && (!model.syncing || model.listLoaded) && model.error == nil
        List(selection: $model.chosen) {
            // The gap a pull leaves open while its water calms; the water shows through it.
            Color.clear
                .frame(height: held * Lake.pullGap)
                .listRowInsets(EdgeInsets())
                .listRowSeparator(.hidden)
                .listRowBackground(Color.clear)
                .selectionDisabled()
            if let owner = model.owner {
                Text(verbatim: choosing ? tr("select_hint") : owner)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .listRowSeparator(.hidden)
                    .selectionDisabled()
            }
            ForEach(model.signedOut, id: \.id) { account in
                NoticeRow(title: tr("inbox_signed_out", account.name), text: tr("inbox_signed_out_text"), tint: .ozeroAccent,
                          background: Color.ozeroCard) { model.signingIn = account }
            }
            if model.isInbox {
                ForEach(model.unsent, id: \.id) { failed in
                    NoticeRow(title: tr("inbox_not_sent"), text: failed.subject.isEmpty ? tr("inbox_no_subject") : failed.subject,
                              tint: .ozeroError, background: Color.ozeroError.opacity(0.1)) { model.fix(failed) }
                }
            }
            if model.swipeTip && !model.messages.isEmpty {
                SwipeTip(swipes: model.swipes, onDismiss: model.closeSwipeTip)
            }
            if lake {
                let words = model.emptyWords
                LakeNote(title: words.title, text: words.text, drawIn: model.hadMail, grey: !model.isInbox,
                         checking: checking, checkedAt: model.isInbox ? model.checkedAt : nil,
                         refreshing: refreshing, pull: pull)
                    .id(model.mailbox)
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)
                    .selectionDisabled()
            }
            ForEach(Array(model.messages.enumerated()), id: \.element.id) { index, message in
                OpenRow(model: model, message: message)
                .tag(message.id)
                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                    swipe(model.swipes.left, message)
                }
                .swipeActions(edge: .leading, allowsFullSwipe: true) {
                    swipe(model.swipes.right, message)
                }
                .contextMenu {
                    // A long press starts choosing, as on Android; the rest of the menu acts at once.
                    Button("action_select", systemImage: "checkmark.circle") { model.chosen.insert(message.id) }
                    ForEach(model.swipes.actions) { action in
                        swipe(action, message)
                    }
                }
                .onAppear {
                    // Near the end of the list, the next rows come before the finger gets there.
                    if index >= model.messages.count - InboxModel.ahead { Task { await model.loadMore() } }
                }
            }
            if model.loadingOlder {
                ProgressView().frame(maxWidth: .infinity).listRowSeparator(.hidden)
            }
        }
        .listStyle(.plain)
        .environment(\.defaultMinListRowHeight, 0)
        .onScrollGeometryChange(for: ListTop.self) { geometry in
            ListTop(offset: geometry.contentOffset.y + geometry.contentInsets.top, inset: geometry.contentInsets.top)
        } action: { _, now in
            top = now
            // How far the finger pulls the list, in pulls; a held gap counts from where it is held.
            let fraction = dragging ? max(0, -now.offset) / Lake.pullGap : 0
            pull.fraction = fraction
            pull.gap = max(0, held * Lake.pullGap - now.offset)
        }
        .onScrollPhaseChange { old, new in
            if new == .interacting { dragging = true }
            if old == .interacting && new != .interacting { letGo(lake: lake) }
        }
        .sensoryFeedback(.impact(weight: .light), trigger: pull.fraction >= 1)
        // There is one water, under the title, and it stays put: a pull over an empty list's lake
        // gathers a drop over it, and a pull on a list moves the rows down to uncover the same water.
        .overlay(alignment: .top) {
            if !lake {
                PullWater(grey: !model.isInbox, refreshing: refreshing, pull: pull) { waterCalm = $0 }
                    .frame(height: pull.gap)
                    .clipped()
                    .padding(.top, top.inset)
                    .allowsHitTesting(false)
            }
        }
        // A check the user waits on says when it is slow; a pull lets go after a while, and the sync
        // goes on behind. Its words then say so, until it ends.
        .task(id: refreshing || waiting) {
            guard refreshing || waiting else { return }
            slow = 0
            do {
                try await Task.sleep(for: Self.slowAfter)
                slow = 1
                try await Task.sleep(for: Self.giveUpAfter - Self.slowAfter)
            } catch {
                return
            }
            slow = 2
            refreshing = false
        }
        .onChange(of: model.syncing) { _, now in if !now { slow = 0 } }
        .task(id: upToDate) {
            guard upToDate, (try? await Task.sleep(for: Self.upToDateFor)) != nil else { return }
            upToDate = false
        }
        .onChange(of: !lake && (refreshing || !waterCalm)) { _, hold in
            withAnimation(.spring(duration: 0.35, bounce: 0)) { held = hold ? 1 : 0 }
        }
        .environment(\.editMode, .constant(choosing ? .active : .inactive))
        .animation(.smooth, value: model.messages.map(\.id))
        .middle(Room.list)
        .navigationTitle(choosing ? tr("select_count", picked.count) : model.title)
        // On its side a large title would take a third of the height.
        .navigationBarTitleDisplayMode(model.sideways ? .inline : .automatic)
        .toolbar {
            if !model.isInbox && !choosing {
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        model.mailbox = Mail.inbox
                    } label: {
                        Image(systemName: "chevron.left")
                    }
                    .accessibilityLabel(Text("action_back"))
                }
            }
            // On its side, always beside the title: the short height is for the mail.
            if model.sideways && !choosing {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button { model.path.append(.search) } label: { Image(systemName: "magnifyingglass") }
                        .accessibilityLabel(Text("search_prompt"))
                    Button { model.composing = Composing(draft: newDraft) } label: { Image(systemName: "square.and.pencil") }
                        .accessibilityLabel(Text("compose_new"))
                }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            VStack(spacing: 8) {
                // Low on the screen, in reach of the thumb that swiped.
                if let toast = model.toast {
                    ToastView(toast: toast, onUndo: model.tapUndo, onGo: model.tapGo)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                if let status {
                    HStack {
                        Text(verbatim: status.text)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        if status.retry {
                            Button("status_try_again", action: check).font(.footnote.weight(.semibold))
                        }
                    }
                    .padding(.horizontal, 20)
                    .transition(.opacity)
                }
                // On its side search and a new message are beside the title: without tabs the dock has nothing.
                if !model.sideways || model.isInbox || choosing || (model.emptyLabel != nil && !model.messages.isEmpty) {
                    Dock(model: model, choosing: choosing, picked: picked, onMore: { more = true })
                }
            }
            .animation(.smooth, value: model.toast)
            .animation(.smooth, value: status?.text)
        }
        .confirmationDialog("action_more", isPresented: $more) {
            Button("action_select_all") { model.chosen.formUnion(model.messages.map(\.id)) }
            // The sender of the message chosen last.
            if let last = picked.last {
                Button(tr("action_all_from", last.sender)) {
                    model.chosen.formUnion(model.messages.filter { $0.fromEmail == last.fromEmail }.map(\.id))
                }
            }
        }
    }

    /// The finger lets the list go: past a third of a pull a drop falls, sized by how far it went,
    /// and a whole pull syncs, unless a sync runs.
    private func letGo(lake: Bool) {
        let fraction = pull.fraction
        dragging = false
        pull.fraction = 0
        guard fraction >= Lake.tug else { return }
        pull.lastDrop = fraction
        pull.drops += 1
        if !lake { waterCalm = false }
        guard fraction >= 1 else { return }
        check()
    }

    /// A check the user asked for, by a pull or by Try again; not while one runs.
    private func check() {
        guard !refreshing else { return }
        refreshing = true
        checks += 1
        let mine = checks
        Task {
            let started = ContinuousClock.now
            let synced = await model.sync(model.mailbox)
            // The pulled drop lands before the gap closes, however quick the sync.
            try? await Task.sleep(until: started + .milliseconds(1100))
            // Not one let go of: nobody waits on it any more, and a newer one may run.
            guard mine == checks, refreshing else { return }
            upToDate = synced
            refreshing = false
        }
    }

    /// How long a check the user waits on goes before it says it is slow, and before it lets them
    /// go; and how long the line over the dock says it went through.
    private static let slowAfter: Duration = .seconds(10)
    private static let giveUpAfter: Duration = .seconds(30)
    private static let upToDateFor: Duration = .seconds(2)

    @ViewBuilder
    private func swipe(_ action: SwipeAction, _ message: MessageItem) -> some View {
        if action.applies(to: message) {
            let unread = message.threadUnread > 0
            Button(role: action == .delete ? .destructive : nil) {
                model.swiped(message, action)
            } label: {
                if action == .read {
                    Label(unread ? tr("action_read") : tr("action_unread"), systemImage: unread ? "envelope.open" : "envelope.badge")
                } else {
                    Label(action == .delete && !message.canTrash ? tr("action_delete_forever") : action.label, systemImage: action.icon)
                }
            }
            .tint(action.tint)
        }
    }
}

/// Everything the thumb needs, along the bottom: the inbox's tabs (none elsewhere), then the
/// accounts button, search and a new message; while choosing, what to do with the chosen. On its
/// side the mailboxes are beside the list, so the rest fits one row, which leaves the short height
/// to the mail; beside a message that row is only the tabs, where there are tabs.
private struct Dock: View {
    @Bindable var model: InboxModel
    let choosing: Bool
    let picked: [MessageItem]
    let onMore: () -> Void

    var body: some View {
        VStack(spacing: 8) {
            if !model.sideways {
                if model.isInbox { tabs }
                if !choosing { EmptyFolderButton(model: model) }
            }
            Group {
                if choosing {
                    ChoiceBar(model: model, picked: picked, onMore: onMore)
                } else if model.sideways {
                    // Search and a new message are beside the title.
                    if model.isInbox { tabs } else { EmptyFolderButton(model: model) }
                } else {
                    HStack(spacing: 8) {
                        AccountsButton(accounts: model.accounts) { model.accountsOpen = true }
                        Button {
                            model.path.append(.search)
                        } label: {
                            Label("search_prompt", systemImage: "magnifyingglass")
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
                                .padding(.horizontal, 16)
                                .background(Color.ozeroCard, in: .capsule)
                        }
                        .buttonStyle(.plain)
                        ComposeButton(model: model)
                    }
                }
            }
            .frame(height: model.sideways && !choosing ? 40 : 52)
            .animation(.smooth, value: choosing)
        }
        .padding(.horizontal, 12)
        // On its side the height is for the mail.
        .padding(.top, model.sideways ? 6 : 10)
        .padding(.bottom, 4)
        .background(.bar)
    }

    private var tabs: some View {
        Picker("inbox_title", selection: $model.tab) {
            ForEach(Category.all, id: \.self) { category in
                let unread = model.tabs.first { $0.category == category }?.unread ?? 0
                (Text(category.label) + Text(verbatim: unread > 0 ? " \(unread)" : "")).tag(category)
            }
        }
        .pickerStyle(.segmented)
    }
}

/// A row of the list that opens its message: beside the list on its side, on its own screen
/// upright.
struct OpenRow: View {
    let model: InboxModel
    let message: MessageItem

    var body: some View {
        if model.sideways {
            Button {
                // While choosing, the list's own selection takes the tap.
                if model.chosen.isEmpty { model.open(message) }
            } label: {
                MessageRow(message: message).contentShape(.rect)
            }
            .buttonStyle(.plain)
            .listRowBackground(model.pane?.id == message.id ? Color.ozeroCard : nil)
        } else {
            NavigationLink(value: Route.message(message)) { MessageRow(message: message) }
        }
    }
}

/// The mailboxes down the left on its side, always there; the rest of the accounts sheet opens
/// from the last row.
private struct Mailboxes: View {
    @Bindable var model: InboxModel

    var body: some View {
        List {
            MailboxRows(
                accounts: model.accounts, folders: model.folders, mailbox: model.mailbox, drafts: model.drafts.count,
                groups: $model.groups, settings: false,
                onMailbox: { model.mailbox = $0 }, onDrafts: { model.path.append(.drafts) })
            Section {
                SheetAction(icon: "ellipsis.circle", text: "mailboxes_more", color: .primary) { model.accountsOpen = true }
            }
        }
        .listStyle(.sidebar)
        .environment(\.defaultMinListRowHeight, 36)
        .frame(width: Room.mailboxes)
        // Read once here; a sync reads them again.
        .task { if model.folders.isEmpty { await model.loadFolders() } }
    }
}

/// Empties the open trash or spam for good; only there, and only with something in it.
private struct EmptyFolderButton: View {
    let model: InboxModel

    var body: some View {
        if let label = model.emptyLabel, !model.messages.isEmpty {
            Button(role: .destructive) {
                Task { await model.empty() }
            } label: {
                Text(label).frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
        }
    }
}

/// A new message, in the accent.
private struct ComposeButton: View {
    let model: InboxModel

    var body: some View {
        Button {
            model.composing = Composing(draft: newDraft)
        } label: {
            Image(systemName: "square.and.pencil")
                .font(.title3.weight(.semibold))
                .foregroundStyle(.white)
                .frame(width: 48, height: 48)
                .background(Color.ozeroFill, in: .rect(cornerRadius: 16))
        }
        .accessibilityLabel(Text("compose_new"))
    }
}

/// What to do with the chosen messages: they leave the list with one toast for all.
private struct ChoiceBar: View {
    let model: InboxModel
    let picked: [MessageItem]
    let onMore: () -> Void

    var body: some View {
        let done = picked.filter(\.canArchive).count
        let unread = picked.contains { $0.threadUnread > 0 }
        let forever = !picked.contains(where: \.canTrash)
        HStack(spacing: 8) {
            Button("action_cancel") { model.chosen = [] }
            Spacer()
            Button {
                model.choice { await model.setRead($0, read: unread) }
            } label: {
                Image(systemName: unread ? "envelope.open" : "envelope.badge")
            }
            .accessibilityLabel(Text(unread ? "action_read" : "action_unread"))
            Button(role: .destructive) {
                model.choice { await model.delete($0) }
            } label: {
                Image(systemName: "trash")
            }
            .accessibilityLabel(Text(forever ? "action_delete_forever" : "action_delete"))
            if picked.contains(where: \.canSnooze) {
                Button {
                    model.choice { model.snoozing = Snoozing(messages: $0.filter(\.canSnooze)) }
                } label: {
                    Image(systemName: "clock")
                }
                .accessibilityLabel(Text("action_snooze"))
            }
            Button(action: onMore) {
                Image(systemName: "ellipsis.circle")
            }
            .accessibilityLabel(Text("action_more"))
            if done > 0 {
                Button(tr("action_done_count", done)) {
                    model.choice { rows in await model.archive(rows.filter(\.canArchive)) }
                }
                .buttonStyle(.borderedProminent)
                .tint(Color.ozeroFill)
            }
        }
        .font(.title3)
        .padding(.horizontal, 4)
    }
}

/// The accounts' colours; opens the accounts.
private struct AccountsButton: View {
    let accounts: [AccountItem]
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if let one = accounts.first, accounts.count == 1 {
                    AccountInitial(name: one.name, color: one.tint)
                } else {
                    Grid(horizontalSpacing: 4, verticalSpacing: 4) {
                        ForEach(Array(stride(from: 0, to: min(accounts.count, 4), by: 2)), id: \.self) { start in
                            GridRow {
                                ForEach(accounts[start..<min(start + 2, accounts.count)], id: \.id) { AccountDot(color: $0.tint) }
                            }
                        }
                    }
                }
            }
            .frame(width: 48, height: 48)
            .background(Color.ozeroCard, in: .rect(cornerRadius: 16))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text("accounts_title"))
    }
}

/// The first letter of an account's name on its colour.
struct AccountInitial: View {
    let name: String
    let color: Color
    var size: CGFloat = 24

    var body: some View {
        Text(verbatim: String(name.first.map(Character.init) ?? "?").uppercased())
            .font(.system(size: size * 0.5, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(color, in: .circle)
    }
}

/// A card over the list that asks for a tap: a title in `tint` over a line or two of text.
private struct NoticeRow: View {
    let title: String
    let text: String
    let tint: Color
    let background: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: title).font(.subheadline.weight(.semibold)).foregroundStyle(tint)
                Text(verbatim: text).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .background(background, in: .rect(cornerRadius: 16))
        }
        .buttonStyle(.plain)
        .listRowSeparator(.hidden)
        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
        .selectionDisabled()
    }
}

/// Says what each swipe does, until the person has read it.
private struct SwipeTip: View {
    let swipes: Swipes
    let onDismiss: () -> Void

    var body: some View {
        HStack {
            Text(verbatim: tr("swipes_tip", swipes.left.label, swipes.right.label)).font(.subheadline)
            Spacer()
            Button("action_got_it", action: onDismiss).font(.subheadline.weight(.semibold))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Color.ozeroAccent.opacity(0.1), in: .rect(cornerRadius: 16))
        .listRowSeparator(.hidden)
        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
        .selectionDisabled()
    }
}

/// The undo toast: dark, rounded, low on the screen. A move also offers to show the folder.
private struct ToastView: View {
    let toast: Toast
    let onUndo: () -> Void
    let onGo: (Mailbox) -> Void

    var body: some View {
        HStack(spacing: 16) {
            Text(verbatim: toast.text).foregroundStyle(.white)
            Spacer()
            if let go = toast.go {
                Button("action_show") { onGo(go) }.foregroundStyle(Color(rgb: 0x8CC2FF))
            }
            if toast.undo {
                Button("action_undo", action: onUndo).fontWeight(.semibold).foregroundStyle(Color(rgb: 0x8CC2FF))
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .background(Color(rgb: 0x1B2639), in: .rect(cornerRadius: 14))
        .padding(.horizontal, 12)
    }
}

/// A row of a list. Unread shows as a blue dot and bold text; with several accounts, the
/// account's name by the time, after a small dot of its colour, marks it.
struct MessageRow: View {
    let message: MessageItem

    var body: some View {
        let unread = message.threadUnread > 0
        HStack(alignment: .top, spacing: 8) {
            Circle()
                .fill(unread ? Color.ozeroFill : .clear)
                .frame(width: 7, height: 7)
                .padding(.top, 7)
                .animation(.easeOut(duration: 0.15), value: unread)
            VStack(alignment: .leading, spacing: 4) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(verbatim: message.participants)
                        .font(.body.weight(unread ? .semibold : .medium))
                        .lineLimit(1)
                    if message.threadCount > 1 {
                        Text(verbatim: "\(message.threadCount)")
                            .font(.caption2.weight(.semibold).monospacedDigit())
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 1)
                            .background(.quaternary, in: .capsule)
                    }
                    Spacer(minLength: 4)
                    if let account = message.account {
                        AccountDot(color: account.tint, size: 6)
                        Text(verbatim: account.name)
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .frame(maxWidth: 70, alignment: .leading)
                    }
                    // Not out yet: once the server has it, its date shows.
                    Text(verbatim: message.sending ? tr("inbox_sending") : shortDate(message.date))
                        .font(.footnote.monospacedDigit())
                        .foregroundStyle(unread ? .primary : .secondary)
                }
                message.subjectText
                    .font(.subheadline.weight(unread ? .medium : .regular))
                    .lineLimit(1)
                if !message.snippet.isBlank {
                    Text(verbatim: message.snippet)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                if !message.attachments.isEmpty {
                    AttachmentChips(names: message.attachments)
                }
            }
        }
        .padding(.vertical, 6)
    }
}

/// Up to two attachments by name, and how many more there are.
struct AttachmentChips: View {
    let names: [String]

    var body: some View {
        HStack(spacing: 6) {
            ForEach(Array(names.prefix(2).enumerated()), id: \.offset) { _, name in
                Label {
                    Text(verbatim: attachmentTitle(name)).lineLimit(1).truncationMode(.middle)
                } icon: {
                    Image(systemName: "paperclip")
                }
                .font(.footnote)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(.quaternary))
                .frame(maxWidth: 168, alignment: .leading)
            }
            if names.count > 2 {
                Text(verbatim: "+\(names.count - 2)")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(.quaternary))
            }
        }
    }
}

extension MessageItem {
    /// The name, or the address when there is none.
    var sender: String {
        let name = fromName?.trimmingCharacters(in: .whitespaces) ?? ""
        return name.isEmpty ? fromEmail : name
    }

    /// Who wrote in a conversation, "Anna, me, Boris"; for one person, their whole name.
    var participants: String {
        guard senders.count > 1 else { return sender }
        let me = tr("message_me")
        return senders.map { $0.me ? me : firstName($0.name) }.joined(separator: ", ")
    }

    var subjectText: Text {
        subject.isBlank ? Text("inbox_no_subject") : Text(verbatim: subject)
    }
}

/// "Anna" of "Anna Lee", "Lee, Anna" or "anna@x.com".
private func firstName(_ name: String) -> String {
    var given = String(name.split(separator: "@", maxSplits: 1).first ?? "")
    if let comma = given.range(of: ", ") { given = String(given[comma.upperBound...]) }
    given = given.trimmingCharacters(in: .whitespaces)
    let first = given.split(separator: " ").first.map(String.init) ?? ""
    return first.isEmpty ? name : first
}

/// Time for today, otherwise the day and month; the year only when it is not this year.
func shortDate(_ seconds: Int64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(seconds))
    let calendar = Calendar.current
    if calendar.isDateInToday(date) {
        return date.formatted(date: .omitted, time: .shortened)
    }
    if calendar.isDate(date, equalTo: .now, toGranularity: .year) {
        return date.formatted(.dateTime.day().month(.abbreviated))
    }
    return date.formatted(.dateTime.day().month(.abbreviated).year())
}

/// The date and time, as a message header shows them.
func longDate(_ seconds: Int64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(seconds))
    let sameYear = Calendar.current.isDate(date, equalTo: .now, toGranularity: .year)
    let day = sameYear ? Date.FormatStyle.dateTime.day().month(.abbreviated) : .dateTime.day().month(.abbreviated).year()
    return date.formatted(day.hour().minute())
}

/// Each time the phone's network reaches the internet again; not for how it is now.
private func backOnline() -> AsyncStream<Void> {
    AsyncStream { continuation in
        let monitor = NWPathMonitor()
        var was: NWPath.Status?
        monitor.pathUpdateHandler = { path in
            if path.status == .satisfied, was != nil, was != .satisfied { continuation.yield() }
            was = path.status
        }
        monitor.start(queue: DispatchQueue(label: "com.ozeromail.app.network"))
        continuation.onTermination = { _ in monitor.cancel() }
    }
}

/// A blank message; account 0 is none, so compose starts from the first account.
let newDraft = DraftItem(id: 0, accountId: 0, to: "", subject: "", text: "", replyTo: nil, quote: "", saved: 0, forward: nil)

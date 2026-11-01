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
                    Mailboxes(model: model, actions: accountsActions)
                        .transition(.move(edge: .leading).combined(with: .opacity))
                    Divider()
                }
                ScrollViewReader { proxy in
                    InboxList(model: model, proxy: proxy)
                }
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
                case .person(let person):
                    PersonMailView(model: model, person: person)
                case .settings(let page):
                    SettingsView(page: page, accounts: model.accounts, signature: model.signature, swipes: $model.swipes,
                                 mail: model.mail, actions: settingsActions)
                }
            }
        }
        .modifier(ShakeToUndo(model: model))
        .sheet(item: $model.snoozing) { snoozing in
            SnoozeSheet { until in
                model.snoozing = nil
                snoozing.then()
                Task { await model.snooze(snoozing.messages, until: until) }
            }
        }
        .moveDialog($model.moving, targets: \.targets) { moving, folder, title in
            Task { await model.moveToFolder(moving.messages, to: folder, title: title) }
        }
        .sheet(isPresented: $model.accountsOpen) {
            AccountsSheet(
                accounts: model.accounts, folders: model.folders, mailbox: model.mailbox, drafts: model.drafts.count,
                unread: model.peopleUnread, groups: $model.groups, allBoxes: $model.allBoxes, actions: accountsActions,
                holdTip: model.holdTip)
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
        // A tall sheet, as in the iPhone's own Mail: swiped down, the message goes to the drafts.
        .sheet(item: $model.composing) { composing in
            ComposeView(
                mail: model.mail, composing: composing,
                onClose: { model.composing = nil }, onSent: model.sent, onKept: model.kept,
                onDiscard: { model.discarded($0, original: composing.original) })
        }
        .task {
            Task { await model.mail.giveContacts() }
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
            model.tabsHidden = false
            Task {
                await model.switched()
                // An account's inbox has tabs of its own.
                await model.counts()
                // Another mailbox brings in what changed in it; the inbox, and each account's,
                // keeps itself up to date.
                if !model.tabbed { await model.sync(model.mailbox) }
            }
        }
        .onChange(of: model.tabbed) {
            // The folders showed that the mailbox is an account's inbox, with tabs.
            Task {
                await model.switched()
                await model.counts()
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
            onInAll: { account, on in
                model.changeAccount {
                    try await model.mail.setInAll(account, on)
                    await model.load()
                }
            },
            onOrder: { order in
                // In the new order at once, so the moved row does not jump back meanwhile.
                model.accounts = order
                model.changeAccount { try await model.mail.setAccountOrder(order) }
            },
            onPin: { folder, pinned in
                model.changeAccount {
                    try await model.mail.setPinned(folder, pinned)
                    await model.loadFolders()
                }
            },
            onSignIn: { account in
                model.accountsOpen = false
                model.signingIn = account
            },
            onAccount: { model.openSettings(.account($0.id)) },
            onSettings: { model.openSettings(.home) })
    }

    private var settingsActions: SettingsActions {
        SettingsActions(
            onAdd: { model.addingAccount = true },
            onInAll: accountsActions.onInAll,
            onNotify: { account, on in model.changeAccount { try await model.mail.setNotify(account, on) } },
            onColor: { account, color in model.changeAccount { try await model.mail.setAccountColor(account, color: color) } },
            onLabel: { account, label in model.changeAccount(account) { try await model.mail.setAccountLabel(account, label: label) } },
            onSender: { account, name in model.changeAccount(account) { try await model.mail.setSender(account, name: name) } },
            onSignature: { text in model.changeAccount { try await model.mail.setSignature(text) } },
            onAccountSignature: { account, text in
                model.changeAccount(account) { try await model.mail.setAccountSignature(account, signature: text) }
            },
            onSignIn: { model.signingIn = $0 },
            onKeepDays: { account, days in model.changeAccount { try await model.mail.setKeepDays(account, days: days) } },
            onNewMail: { account, check in model.changeAccount { try await model.mail.setNewMail(account, check) } },
            onRemove: { account in Task { await model.remove(account) } },
            onWrite: { model.composing = Composing(draft: authorDraft()) })
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
    /// Scrolls the list: to the top, and back to where a tab was left.
    let proxy: ScrollViewProxy
    /// How the list is pulled down, as the water reads it.
    @State private var pull = PullState()
    /// Where the list's content top is: above the top while scrolled, below it while pulled.
    @State private var top = ListTop()
    @State private var dragging = false
    /// Only a pull shows the water's sync; syncs on their own stay quiet over the saved mail.
    @State private var refreshing = false
    /// After a sync the list stays open until its water has calmed, so the calm is seen.
    @State private var waterCalm = true
    /// Pulls the system holds open, waiting for the check to end and its water to calm.
    @State private var waiters: [CheckedContinuation<Void, Never>] = []
    /// The system holds the list open over its refresh, and while it does the top inset grows by
    /// what it holds; the water counts from the inset without it.
    @State private var holding = false
    @State private var base: CGFloat = 0
    /// A check the user waits on: how long it has gone (0, slow, given up), and whether it just went through.
    @State private var slow = 0
    @State private var upToDate = false
    /// Counts the checks asked for, so one let go of does not end a newer one.
    @State private var checks = 0
    /// The rows on screen, and the row at the top of each tab when it was left.
    @State private var onScreen = RowsOnScreen()
    @State private var places: [Category: Int64] = [:]
    /// The row to bring back to the top once the tab's mail is in.
    @State private var returning: Int64?

    /// The first mail of an empty list is on its way: the water waits, and says what for.
    private var waiting: Bool {
        // Only the first time: after that an empty list is empty until a sync brings something.
        model.syncing && model.listLoaded && model.messages.isEmpty && !model.everSynced.contains(model.mailbox.key)
    }

    private var checking: LakeWords? {
        if waiting { return LakeWords(title: "lake_loading_title", text: slow > 0 ? "lake_loading_slow" : nil) }
        if refreshing { return LakeWords(title: "lake_checking_title", text: slow > 0 ? "status_slow" : nil) }
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
        if upToDate { return (tr("lake_checked_now"), false) }
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
            if let owner = model.owner {
                Text(verbatim: owner)
                    .font(.small)
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
                SwipeTip(swipes: model.swipes, lift: !model.sideways, onDismiss: model.closeSwipeTip)
            }
            if lake {
                LakeNote(title: model.emptyTitle, drawIn: model.hadMail, grey: !model.tabbed,
                         checking: checking, checkedAt: model.isInbox ? model.checkedAt : nil,
                         refreshing: refreshing, pull: pull)
                    .id(model.mailbox)
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)
                    .selectionDisabled()
            }
            ForEach(Array(model.messages.enumerated()), id: \.element.id) { index, message in
                let swipes = model.swipes.on(message)
                OpenRow(model: model, message: message)
                .tag(message.id)
                .onScrollVisibilityChange(threshold: 0.5) { shown in
                    if shown { onScreen.ids.insert(message.id) } else { onScreen.ids.remove(message.id) }
                }
                // As in the iPhone's own Mail: a full swipe does the long action, a shorter one
                // shows both to tap. The first is the one at the edge.
                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                    if let long = swipes.leftLong, long != swipes.left { swipe(long, message) }
                    swipe(swipes.left, message)
                }
                .swipeActions(edge: .leading, allowsFullSwipe: true) {
                    if let long = swipes.rightLong, long != swipes.right { swipe(long, message) }
                    swipe(swipes.right, message)
                }
                .contextMenu {
                    // A long press starts choosing, as on Android; the rest of the menu acts at once.
                    Button("action_select", systemImage: "checkmark.circle") { model.chosen.insert(message.id) }
                    ForEach(swipes.actions) { action in
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
            if !holding { base = now.inset }
            // How far the list is pulled down, past where it rests, and in pulls while the finger is on it.
            let gap = max(0, now.inset - base - now.offset)
            pull.fraction = dragging ? gap / Lake.pullGap : 0
            pull.gap = gap
        }
        .onScrollPhaseChange { old, new in
            if new == .interacting { dragging = true }
            if old == .interacting && new != .interacting { letGo(lake: lake) }
        }
        // The system's own pull: its scroll holds the list open and lets it go, so the rows never
        // fight a second spring. Its spinner is hidden (OzeroMailApp); the water shows instead.
        .refreshable { await pulled() }
        .sensoryFeedback(.impact(weight: .light), trigger: pull.fraction >= 1)
        // There is one water, under the title, and it stays put: a pull over an empty list's lake
        // gathers a drop over it, and a pull on a list moves the rows down to uncover the same water.
        .overlay(alignment: .top) {
            if !lake {
                PullWater(grey: !model.tabbed, refreshing: refreshing, pull: pull) { waterCalm = $0 }
                    .frame(height: pull.gap)
                    .clipped()
                    .padding(.top, base)
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
        .onChange(of: refreshing || !waterCalm) { _, busy in
            guard !busy else { return }
            waiters.forEach { $0.resume() }
            waiters = []
        }
        .task(id: upToDate) {
            guard upToDate, (try? await Task.sleep(for: Self.upToDateFor)) != nil else { return }
            upToDate = false
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
                    Button { model.write() } label: { Image(systemName: "square.and.pencil") }
                        .accessibilityLabel(Text("compose_new"))
                }
            }
        }
        .bottomBar {
            VStack(spacing: 8) {
                // Low on the screen, in reach of the thumb that swiped; upright, the dock shows it
                // in place of search.
                if let toast = model.toast, model.sideways || choosing {
                    ToastView(toast: toast, onUndo: model.tapUndo, onGo: model.tapGo, onExtra: model.tapExtra)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                if let status {
                    HStack {
                        Text(verbatim: status.text)
                            .font(.small)
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        if status.retry {
                            Button("status_try_again", action: check).font(.smallStrong)
                        }
                    }
                    .padding(.horizontal, 20)
                    .transition(.opacity)
                }
                // On its side search and a new message are beside the title: without tabs the dock has nothing.
                if !model.sideways || model.tabbed || choosing || (model.emptyLabel != nil && !model.messages.isEmpty) {
                    Dock(model: model, choosing: choosing, picked: picked)
                }
            }
            .animation(.smooth, value: model.toast)
            .animation(.smooth, value: status?.text)
        }
        .onScrollGeometryChange(for: TabsScroll.self) { geometry in
            let offset = geometry.contentOffset.y + geometry.contentInsets.top
            let end = geometry.contentSize.height + geometry.contentInsets.bottom - geometry.containerSize.height
            return TabsScroll(offset: offset, nearEnd: geometry.contentOffset.y >= end - Self.tabsStay)
        } action: { old, now in
            hideTabs(old: old.offset, now: now)
        }
        // A tap on the open tab goes to the top of its list.
        .onChange(of: model.toTop) {
            guard let first = model.messages.first?.id else { return }
            withAnimation(.smooth) { proxy.scrollTo(first, anchor: .top) }
        }
        // Each tab keeps its place: the row at its top when it was left comes back to the top.
        .onChange(of: model.tab) { old, _ in
            if model.tabsHidden { model.tabsHidden = false }
            // Tabs passed quickly never showed their mail: the list is still an earlier tab's.
            if returning == nil {
                let first = model.messages.first { onScreen.ids.contains($0.id) }?.id
                places[old] = first == model.messages.first?.id ? nil : first
            }
            returning = places[model.tab]
        }
        // Another tab's mail starts with another message.
        .onChange(of: model.messages.first?.id) {
            guard let row = returning, !model.messages.isEmpty else { return }
            returning = nil
            if model.messages.contains(where: { $0.id == row }) { proxy.scrollTo(row, anchor: .top) }
        }
    }

    /// The tabs step aside while the list scrolls down, and are back as it scrolls up or reaches the
    /// top. Near the end they stay as they are: the room they leave or take moves the list there.
    private func hideTabs(old: CGFloat, now: TabsScroll) {
        guard model.tabbed, !model.sideways, !now.nearEnd else { return }
        var hidden = model.tabsHidden
        if now.offset <= Self.tabsStay {
            hidden = false
        } else if now.offset > old + 4 {
            hidden = true
        } else if now.offset < old - 4 {
            hidden = false
        }
        if hidden != model.tabsHidden { model.tabsHidden = hidden }
    }

    /// How far from the top and the end of the list the tabs stay put.
    private static let tabsStay: CGFloat = 80

    /// The finger lets the list go: past a third of a pull a drop falls, sized by how far it went,
    /// and a whole pull syncs, unless a sync runs.
    private func letGo(lake: Bool) {
        let fraction = pull.fraction
        dragging = false
        pull.fraction = 0
        guard fraction >= Lake.tug else { return }
        // A pull the system took for a refresh drops a whole drop, however far it went.
        pull.lastDrop = holding ? max(fraction, 1) : fraction
        pull.drops += 1
        if !lake { waterCalm = false }
    }

    /// The system's pull: the list stays open until the check is through and its water has calmed.
    private func pulled() async {
        holding = true
        // A check that has gone too long lets the list go while it runs on.
        check()
        if refreshing || !waterCalm { await withCheckedContinuation { waiters.append($0) } }
        // The list closes on the system's own animation; until it has, the inset still holds it.
        try? await Task.sleep(for: .milliseconds(400))
        holding = false
    }

    /// A check the user asked for, by a pull or by Try again; not while one runs.
    private func check() {
        guard !refreshing else { return }
        refreshing = true
        checks += 1
        let mine = checks
        Task {
            let started = ContinuousClock.now
            // An account's inbox, as the unified one, with the inbox sync.
            let synced = await model.sync(model.tabbed ? Mail.inbox : model.mailbox)
            // The pulled drop lands before the list closes, however quick the sync.
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
            Button(role: action == .delete ? .destructive : nil) {
                model.swiped(message, action)
            } label: {
                Label(action.label(for: message), systemImage: action.icon(for: message))
            }
            .tint(action.tint)
        }
    }
}

/// Everything the thumb needs, along the bottom: the inbox's tabs (none elsewhere), then the
/// accounts button, search and a new message; while choosing, what to do with the chosen. On its
/// side the mailboxes are beside the list, so the rest fits one row, which leaves the short height
/// to the mail; beside a message that row is only the tabs, where there are tabs. On iOS 26 the
/// dock is glass floating over the list.
private struct Dock: View {
    @Bindable var model: InboxModel
    let choosing: Bool
    let picked: [MessageItem]

    var body: some View {
        GlassGroup {
            VStack(spacing: 10) {
                if !model.sideways {
                    // Scrolling down, the tabs step aside for the mail, as on the iPhone's own Photos.
                    if model.tabbed && !choosing && !model.tabsHidden {
                        InboxTabs(model: model)
                            .transition(.move(edge: .bottom).combined(with: .opacity).combined(with: .scale(scale: 0.9, anchor: .bottom)))
                    }
                    if !choosing { EmptyFolderButton(model: model) }
                }
                Group {
                    if choosing {
                        ChoiceBar(model: model, picked: picked)
                    } else if model.sideways {
                        // Search and a new message are beside the title.
                        if model.tabbed { InboxTabs(model: model) } else { EmptyFolderButton(model: model) }
                    } else {
                        HStack(spacing: 10) {
                            AccountsButton(accounts: model.scope) { model.accountsOpen = true }
                            // For a moment after a move, Undo takes the place of search, as Apple's
                            // Mail shows Undo Send in its bar.
                            if let toast = model.toast {
                                ToastView(toast: toast, onUndo: model.tapUndo, onGo: model.tapGo, onExtra: model.tapExtra, inBar: true)
                                    .transition(.blurReplace)
                            } else {
                                Button {
                                    model.path.append(.search)
                                } label: {
                                    Label("search_prompt", systemImage: "magnifyingglass")
                                        .foregroundStyle(.secondary)
                                        .frame(maxWidth: .infinity, minHeight: glassButton, alignment: .leading)
                                        .padding(.horizontal, 16)
                                        .contentShape(.capsule)
                                }
                                .buttonStyle(.plain)
                                .glassSurface(in: .capsule, fallback: Color.ozeroCard.sunk)
                                .transition(.blurReplace)
                            }
                            ComposeButton(model: model)
                        }
                    }
                }
                .frame(height: model.sideways && !choosing ? 40 : 52)
                .animation(.smooth, value: choosing)
                .animation(.smooth, value: model.toast)
            }
        }
        .padding(.horizontal, hasGlass ? 16 : 12)
        // On its side the height is for the mail.
        .padding(.top, model.sideways ? 6 : 10)
        .padding(.bottom, 4)
        .dockBackground(floating: true)
        .animation(.smooth(duration: 0.3), value: model.tabsHidden)
        // Dragged up, the dock brings up the mailboxes, as the button does. The system's sheet
        // cannot follow a finger, so it rises once the drag is clearly upward.
        .simultaneousGesture(
            DragGesture(minimumDistance: 12).onChanged { drag in
                let up = -drag.translation.height
                guard !model.sideways, !choosing, !model.accountsOpen, up > 32, up > abs(drag.translation.width) else { return }
                model.accountsOpen = true
            }
        )
    }

}

/// The inbox's tabs: the system's segmented control, whose glass lens follows a finger along it on
/// iOS 26, on a glass capsule over the list. Each tab keeps its place in the list; a tap on the open
/// one goes to the top.
private struct InboxTabs: View {
    @Bindable var model: InboxModel
    @State private var width: CGFloat = 0
    /// When the tab last changed: the tap that changed it is not a tap on the open tab.
    @State private var changed = Date.distantPast

    var body: some View {
        Picker("inbox_title", selection: $model.tab) {
            ForEach(Category.all, id: \.self) { category in
                let unread = model.tabs.first { $0.category == category }?.unread ?? 0
                (Text(category.label) + Text(verbatim: unread > 0 ? " \(unread)" : "")).tag(category)
            }
        }
        .pickerStyle(.segmented)
        .controlSize(.large)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .simultaneousGesture(
            SpatialTapGesture().onEnded { tap in
                guard width > 0, Date.now.timeIntervalSince(changed) > 0.4 else { return }
                let index = Int(tap.location.x / (width / CGFloat(Category.all.count)))
                if Category.all[min(max(index, 0), Category.all.count - 1)] == model.tab { model.toTop += 1 }
            }
        )
        .padding(4)
        .glassSurface(in: .capsule, fallback: Color.clear)
        .onChange(of: model.tab) { changed = .now }
        .sensoryFeedback(.selection, trigger: model.tab)
    }
}

/// How far the list is scrolled, for the tabs, and whether it is near its end.
private struct TabsScroll: Equatable {
    var offset: CGFloat = 0
    var nearEnd = false
}

/// The rows on screen, kept out of sight of the view: scrolling changes them all the time, and
/// only a change of tab reads them.
private final class RowsOnScreen {
    var ids: Set<Int64> = []
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

/// The mailboxes down the left on its side, always there: the sheet's list of them; the
/// settings open in the sheet from the last row.
private struct Mailboxes: View {
    @Bindable var model: InboxModel
    let actions: AccountsActions

    var body: some View {
        List {
            MailboxRows(
                accounts: model.accounts, folders: model.folders, mailbox: model.mailbox, drafts: model.drafts.count,
                unread: model.peopleUnread, allBoxes: $model.allBoxes, groups: $model.groups, actions: actions)
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

/// A new message, in the accent: lifted off the page, or tinted glass on iOS 26.
private struct ComposeButton: View {
    let model: InboxModel

    var body: some View {
        RoundButton(icon: "square.and.pencil", label: "compose_new", size: glassButton, tint: .white, background: .ozeroFill) {
            model.write()
        }
    }
}

/// What to do with the chosen messages: they leave the list with one toast for all. The rest of
/// what applies to them is in the menu of ⋯.
private struct ChoiceBar: View {
    let model: InboxModel
    let picked: [MessageItem]

    /// The filled button: Done where mail is not filed yet, else back to the inbox, Not spam out of spam.
    private func main(done: Int, back: [MessageItem]) -> (label: String, act: ([MessageItem]) async -> Void)? {
        if done > 0 {
            return (tr("action_done_count", done), { rows in await model.archive(rows.filter(\.canArchive)) })
        }
        guard !back.isEmpty else { return nil }
        let label = back.allSatisfy(\.spam) ? "action_not_spam_count" : "action_inbox_count"
        return (tr(label, back.count), { rows in await model.moveToInbox(rows.filter(\.canMoveToInbox)) })
    }

    var body: some View {
        let done = picked.filter(\.canArchive).count
        let back = picked.filter(\.canMoveToInbox)
        let unread = picked.contains { $0.threadUnread > 0 }
        let forever = !picked.contains(where: \.canTrash)
        HStack(spacing: 10) {
            Button { model.chosen = [] } label: {
                Text("action_cancel")
                    .font(.row)
                    .padding(.horizontal, 16)
                    .frame(height: glassButton)
                    .contentShape(.capsule)
            }
            .buttonStyle(.plain)
            .glassSurface(in: .capsule, fallback: Color.clear)
            Spacer(minLength: 0)
            HStack(spacing: 0) {
                GlassIcon(icon: unread ? "envelope.open" : "envelope.badge", label: unread ? "action_read" : "action_unread") {
                    model.choice { await model.setRead($0, read: unread) }
                }
                GlassIcon(icon: "trash", label: forever ? "action_delete_forever" : "action_delete", tint: .ozeroError) {
                    model.choice { await model.delete($0) }
                }
                if picked.contains(where: \.canSnooze) {
                    GlassIcon(icon: "clock", label: "action_snooze") {
                        model.choice { model.snoozing = Snoozing(messages: $0.filter(\.canSnooze)) }
                    }
                }
                Menu { more } label: { GlassIconLabel(icon: "ellipsis") }
                    .accessibilityLabel(Text("action_more"))
            }
            .glassSurface(in: .capsule, fallback: Color.clear)
            if let main = main(done: done, back: back) {
                Button { model.choice(main.act) } label: {
                    Text(verbatim: main.label)
                        .font(.rowStrong)
                        .foregroundStyle(.white)
                        .padding(.horizontal, 18)
                        .frame(height: glassButton)
                        .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .glassSurface(in: .capsule, tint: .ozeroFill, fallback: Color.ozeroFill.lifted)
            }
        }
    }

    /// What the bar has no room for, where it applies to the chosen.
    @ViewBuilder
    private var more: some View {
        let back = picked.filter(\.canMoveToInbox)
        Button("action_select_all", systemImage: "checkmark.circle") { model.chosen.formUnion(model.messages.map(\.id)) }
        // When they are all from one sender, the rest of that sender's mail.
        if let first = picked.first, picked.allSatisfy({ $0.fromEmail == first.fromEmail }) {
            Button(tr("action_all_from", first.sender), systemImage: "person") {
                model.chosen.formUnion(model.messages.filter { $0.fromEmail == first.fromEmail }.map(\.id))
            }
        }
        Divider()
        if picked.contains(where: \.canArchive), !back.isEmpty {
            Button(LocalizedStringKey(back.allSatisfy(\.spam) ? "action_not_spam" : "action_move_to_inbox"), systemImage: "tray") {
                model.choice { await model.moveToInbox($0.filter(\.canMoveToInbox)) }
            }
        }
        // Folders of one account only.
        if picked.allSatisfy(\.canMove), Set(picked.map(\.accountId)).count == 1 {
            Button("action_move_to_folder", systemImage: "folder") { model.choice { await model.move($0) } }
        }
        let flag = picked.contains { !$0.flagged }
        Button(LocalizedStringKey(flag ? "action_flag" : "action_unflag"), systemImage: flag ? "flag" : "flag.slash") {
            model.choice { await model.flag($0, flagged: flag) }
        }
        if picked.contains(where: \.canSpam) {
            Button("action_spam", systemImage: "xmark.bin", role: .destructive) { model.choice { await model.spam($0.filter(\.canSpam)) } }
        }
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
                    // Up to four colours; the sheet lists the rest.
                    let shown = Array(accounts.prefix(4))
                    Grid(horizontalSpacing: 4, verticalSpacing: 4) {
                        GridRow {
                            ForEach(shown.prefix(2), id: \.id) { AccountDot(color: $0.tint) }
                        }
                        if shown.count > 2 {
                            GridRow {
                                ForEach(shown.dropFirst(2), id: \.id) { AccountDot(color: $0.tint) }
                            }
                        }
                    }
                }
            }
            .frame(width: glassButton, height: glassButton)
            .contentShape(.circle)
        }
        .buttonStyle(.plain)
        .glassSurface(in: .circle, fallback: Color.ozeroCard.sunk)
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
                Text(verbatim: title).font(.textStrong).foregroundStyle(tint)
                Text(verbatim: text).font(.text).foregroundStyle(.secondary).lineLimit(2)
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
    /// Whether the dock is there to drag up.
    let lift: Bool
    let onDismiss: () -> Void

    var body: some View {
        HStack {
            Text(verbatim: lift ? swipes.tip + " " + tr("swipes_tip_lift") : swipes.tip).font(.text)
            Spacer()
            Button("action_got_it", action: onDismiss).font(.textStrong)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Color.ozeroAccent.opacity(0.1), in: .rect(cornerRadius: 16))
        .listRowSeparator(.hidden)
        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
        .selectionDisabled()
    }
}

/// What was just done, with Undo for a while: a glass capsule low on the screen, or in the dock in
/// place of search (`inBar`). A move also offers to show the folder.
struct ToastView: View {
    let toast: Toast
    let onUndo: () -> Void
    let onGo: (Mailbox) -> Void
    var onExtra: () -> Void = {}
    var inBar = false

    var body: some View {
        HStack(spacing: 4) {
            Text(verbatim: toast.text).lineLimit(1)
            Spacer(minLength: 8)
            if let go = toast.go {
                button(Text("action_show")) { onGo(go) }
            }
            if let extra = toast.extra {
                button(Text(verbatim: extra), action: onExtra)
            }
            if toast.undo {
                button(Text("action_undo"), action: onUndo).fontWeight(.semibold)
            }
        }
        .font(.text)
        .padding(.leading, 18)
        .padding(.trailing, 6)
        .frame(minHeight: glassButton)
        .glassSurface(in: .capsule, fallback: Color.ozeroCard)
        .padding(.horizontal, inBar ? 0 : hasGlass ? 16 : 12)
    }

    private func button(_ label: Text, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            label.padding(.horizontal, 10).frame(minHeight: 40).contentShape(.capsule)
        }
        .buttonStyle(.plain)
        .foregroundStyle(Color.ozeroAccent)
    }
}

/// Shaking the phone, or three fingers swiped left, offers to undo what the toast offers to,
/// as everywhere on the iPhone.
private struct ShakeToUndo: ViewModifier {
    let model: InboxModel
    @Environment(\.undoManager) private var undoManager

    func body(content: Content) -> some View {
        content.onChange(of: model.toast) { _, toast in
            guard let undoManager else { return }
            undoManager.removeAllActions(withTarget: model)
            guard let toast, toast.undo else { return }
            undoManager.registerUndo(withTarget: model) { model in
                MainActor.assumeIsolated { model.tapUndo() }
            }
            undoManager.setActionName(toast.text)
        }
    }
}

/// A row of a list. Unread shows as a blue dot and bold text; with several accounts, the
/// account's name by the time, after a small dot of its colour, marks it.
struct MessageRow: View {
    let message: MessageItem

    var body: some View {
        let unread = message.threadUnread > 0
        // The text lines up with the title; the unread dot hangs in the margin before it.
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(verbatim: message.participants)
                    .font(unread ? .rowStrong : .row)
                    .lineLimit(1)
                if message.threadCount > 1 {
                    Text(verbatim: "\(message.threadCount)")
                        .font(.badge)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 4)
                if let account = message.account { AccountTag(account: account) }
                // Not out yet: once the server has it, its date shows.
                Text(verbatim: message.sending ? tr("inbox_sending") : shortDate(message.date))
                    .font(.time)
                    .foregroundStyle(unread ? .primary : .secondary)
            }
            message.subjectText
                .font(unread ? .textStrong : .text)
                .lineLimit(1)
            let snippet = message.invite.map { inviteLine($0) } ?? message.snippet
            if !snippet.isBlank {
                Text(verbatim: snippet)
                    .font(.text)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            if !message.attachments.isEmpty {
                AttachmentChips(names: message.attachments)
            }
        }
        .overlay(alignment: .topLeading) {
            Circle()
                .fill(Color.ozeroFill.window)
                .frame(width: 7, height: 7)
                .opacity(unread ? 1 : 0)
                .offset(x: -13, y: 7)
                .animation(.easeOut(duration: 0.15), value: unread)
        }
        .padding(.vertical, 6)
    }
}

/// A row's account, when there are several: a dot of its colour and its short name, before the time.
struct AccountTag: View {
    let account: AccountMark

    var body: some View {
        AccountDot(color: account.tint, size: 6)
        Text(verbatim: account.name)
            .font(.label)
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .frame(maxWidth: 70, alignment: .leading)
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
                .font(.small)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .overlay(RoundedRectangle(cornerRadius: 8).stroke(.quaternary))
                .frame(maxWidth: 168, alignment: .leading)
            }
            if names.count > 2 {
                Text(verbatim: "+\(names.count - 2)")
                    .font(.small)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(.quaternary))
            }
        }
    }
}

extension MessageItem {
    /// The name from the contacts, else the mail's, else the address.
    var sender: String { contactName ?? mailSender }

    /// The sender as the mail names them, for quotes: a contact's name is the user's own.
    var mailSender: String {
        let name = fromName?.trimmingCharacters(in: .whitespaces) ?? ""
        return name.isEmpty ? fromEmail : name
    }

    /// Who wrote in a conversation, "Anna, me, Boris"; for one person, their whole name.
    var participants: String {
        guard senders.count > 1 else { return sender }
        let me = tr("message_me")
        return senders.map { $0.me ? me : $0.first }.joined(separator: ", ")
    }

    var subjectText: Text {
        subject.isBlank ? Text("inbox_no_subject") : Text(verbatim: subject)
    }
}

/// "Anna", "Anna and Oleg", "Anna, Oleg and Maria".
func joinNames(_ names: [String]) -> String {
    guard let last = names.last, names.count > 1 else { return names.first ?? "" }
    return tr("message_and", names.dropLast().joined(separator: ", "), last)
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

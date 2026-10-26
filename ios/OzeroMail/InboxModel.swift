import SwiftUI
import UserNotifications

/// A screen pushed over the list.
enum Route: Hashable {
    case message(MessageItem)
    case search
    case drafts
}

/// A toast low on the screen: words, and for a while Undo; a move can also show the folder the
/// mail is in now.
struct Toast: Identifiable, Equatable {
    let id = UUID()
    let text: String
    var undo = false
    var go: Mailbox?
}

/// The inbox and everything around it: which mailbox and tab show, the saved list and its
/// syncs, the screens over it, and the toast. The UI reads only the local database; every action
/// changes it first and goes through the action queue.
@MainActor @Observable
final class InboxModel {
    /// How many more rows the list reads at a time, and how far from its end it starts to.
    static let page = 100
    static let ahead = 20
    /// How soon after a sync a return to the app syncs again.
    private static let resync: Duration = .seconds(30)
    /// How long syncs on their own may fail on the network before the error shows.
    private static let blip: Duration = .seconds(60)

    let mail: Mail
    let onNoAccounts: () -> Void

    var messages: [MessageItem] = []
    var mailbox = Mail.inbox
    var tab = Category.people
    var tabs: [InboxTab] = []
    var inboxEmpty = false
    var unsent: [UnsentItem] = []
    var accounts: [AccountItem] = []
    var folders: [FolderItem] = []
    var drafts: [DraftItem] = []
    var syncing = false
    /// Until the saved list is read, an empty one says nothing.
    var listLoaded = false
    /// Mailboxes synced since the app opened: until then, their empty list waits for its mail.
    var syncedBoxes: Set<Mailbox> = []
    /// Mailboxes ever synced on this phone: one opened again shows its saved mail and syncs quietly.
    var everSynced = Settings.syncedBoxes {
        didSet { Settings.syncedBoxes = everSynced }
    }
    var loadingOlder = false
    /// Whether this tab or mailbox had mail since it was picked: then the lake draws itself in as
    /// the last message leaves, and not on opening the app or on picking an empty tab.
    var hadMail = false
    /// The key of the error to show, if any.
    var error: String?
    /// When the inbox last synced, to say how fresh it is.
    var checkedAt: Date?
    var toast: Toast?

    var path: [Route] = []
    /// On its side the list stays on the left and the message opens beside it, here.
    var pane: MessageItem?
    /// On its side: the mailboxes beside the list, and the message beside the list.
    var sideways = false {
        didSet { if sideways != oldValue { turned() } }
    }
    var composing: Composing?
    var addingAccount = false
    /// The account being signed in to again.
    var signingIn: AccountItem?
    var accountsOpen = false
    /// Messages chosen to act on together.
    var chosen: Set<Int64> = []
    /// Messages waiting on the snooze sheet for a time.
    var snoozing: Snoozing?
    var swipes = Settings.swipes {
        didSet { Settings.swipes = swipes }
    }
    var groups = Settings.groups {
        didSet { Settings.groups = groups }
    }
    var swipeTip = !Settings.swipeTipSeen

    // A sync asked for while one runs, say for a new account, runs right after it; quiet if
    // every ask for it was.
    private var syncAgain: [Mailbox: Bool] = [:]
    /// Those who asked for it, waiting for it to end.
    private var inLine: [CheckedContinuation<Bool, Never>] = []
    /// When the last inbox sync that went through began.
    private var lastSync: ContinuousClock.Instant?
    /// Since when syncs on their own have met no network, if they do.
    private var offlineSince: ContinuousClock.Instant?
    /// How many rows the list reads; scrolling near its end reads more.
    private var listSize = page
    /// The saved mail ended before `listSize`: more has to come from the servers.
    private var savedEnds = false
    /// More of the saved mail is being read.
    private var readingMore = false
    /// The toast waiting for Undo, answered with whether it was tapped.
    private var waiting: CheckedContinuation<Bool, Never>?
    /// One answer or forward at a time: a second tap would replace the one being written.
    private var writing = false
    /// Brings snoozed mail back on time while the app is open.
    private var waking: Task<Void, Never>?

    init(mail: Mail, onNoAccounts: @escaping () -> Void) {
        self.mail = mail
        self.onNoAccounts = onNoAccounts
    }

    var isInbox: Bool { mailbox == Mail.inbox }

    var openFolder: FolderItem? {
        if case .folder(let id) = mailbox { folders.first { $0.id == id } } else { nil }
    }

    /// The mailbox's name: ours for a role, the server's for an account's own folder.
    var title: String {
        switch mailbox {
        case .unified(let role): role.title("")
        case .folder: openFolder?.title ?? ""
        }
    }

    /// What the mailbox holds: its role, or its folder's.
    var role: FolderRole? {
        switch mailbox {
        case .unified(let role): role
        case .folder: openFolder?.role
        }
    }

    /// Whose mail it is, when there are several accounts: all of theirs, or the folder's account's.
    var owner: String? {
        guard accounts.count > 1 else { return nil }
        switch mailbox {
        case .unified: return tr("inbox_all_accounts")
        case .folder: return accounts.first { $0.id == openFolder?.accountId }?.name
        }
    }

    /// Accounts whose server refused the login, each with a card to sign in again.
    var signedOut: [AccountItem] { isInbox ? accounts.filter(\.needsSignIn) : [] }

    /// What an empty list says: an empty folder, the whole inbox or its first tab done, or another
    /// tab done.
    var emptyWords: (title: LocalizedStringKey, text: LocalizedStringKey) {
        if !isInbox { return ("tab_empty_title", "folder_empty") }
        if inboxEmpty || tab == .people { return ("inbox_zero_title", "inbox_zero_text") }
        return ("tab_empty_title", "tab_empty_text")
    }

    /// What the button that empties a folder says; only the trash and spam can be emptied.
    var emptyLabel: LocalizedStringKey? {
        switch role {
        case .trash?: "action_empty_trash"
        case .junk?: "action_empty_junk"
        default: nil
        }
    }

    // MARK: Reading

    func loadList() async {
        let shown = (tab, mailbox)
        do {
            let size = listSize
            let found = isInbox ? try await mail.inbox(tab, limit: size) : try await mail.mailbox(mailbox, limit: size)
            // A tab or mailbox picked meanwhile loads its own list.
            if (tab, mailbox) == shown {
                // Beside the list, the message goes when its row does, as when it moved on another device.
                if let pane, messages.contains(where: { $0.id == pane.id }), !found.contains(where: { $0.id == pane.id }) {
                    self.pane = nil
                }
                // Only a real change: the list diffs every row otherwise.
                if found != messages { messages = found }
                if !found.isEmpty { hadMail = true }
                savedEnds = found.count < size
            }
        } catch {
            // Never leave the last list's messages under the new one.
            if (tab, mailbox) == shown { messages = [] }
            self.error = error.textKey()
        }
        listLoaded = true
    }

    func counts() async {
        do {
            let tabs = try await mail.tabs()
            if tabs != self.tabs { self.tabs = tabs }
            let unsent = try await mail.unsent()
            if unsent != self.unsent { self.unsent = unsent }
            inboxEmpty = try await mail.inboxEmpty()
            let drafts = try await mail.drafts()
            if drafts != self.drafts { self.drafts = drafts }
        } catch {
            self.error = error.textKey()
        }
    }

    func loadAccounts() async {
        do {
            let accounts = try await mail.accounts()
            if accounts != self.accounts { self.accounts = accounts }
        } catch {
            self.error = error.textKey()
        }
    }

    func loadFolders() async {
        do {
            let folders = try await mail.folders()
            if folders != self.folders { self.folders = folders }
        } catch {
            self.error = error.textKey()
        }
    }

    func load() async {
        await loadList()
        await counts()
    }

    /// A tab or mailbox was picked: its list from the top.
    func switched() async {
        chosen = []
        pane = nil
        hadMail = false
        listSize = Self.page
        await loadList()
    }

    /// The list is scrolled near its end: more of the saved mail, or once that ends, older mail
    /// from the servers of a mailbox synced since the app opened.
    func loadMore() async {
        if !savedEnds {
            // Each row near the end asks; one read at a time is enough.
            guard !readingMore else { return }
            readingMore = true
            defer { readingMore = false }
            listSize += Self.page
            await loadList()
            return
        }
        let box = mailbox
        guard !loadingOlder, syncedBoxes.contains(box) else { return }
        loadingOlder = true
        defer { loadingOlder = false }
        do {
            // Once the servers have nothing older, the core answers without asking them.
            _ = try await mail.older(box)
        } catch {
            self.error = error.textKey()
        }
        await loadList()
    }

    /// Syncs `box` in every account; false if one failed. One asked for while one runs follows
    /// it. A `quiet` one, not asked for by the user, keeps a short network blip off the screen,
    /// and the error already there until it goes through: the app tries again soon on its own.
    @discardableResult
    func sync(_ box: Mailbox = Mail.inbox, quiet: Bool = false) async -> Bool {
        if syncing {
            syncAgain[box] = quiet && syncAgain[box] != false
            // Over once the syncs in line are, with how the last one went.
            return await withCheckedContinuation { inLine.append($0) }
        }
        syncing = true
        if !quiet { error = nil }
        let started = ContinuousClock.now
        // A refused login shows as its account's card over the list, not as an error.
        var refused = false
        var synced: Bool
        do {
            if box == Mail.inbox {
                // Snoozed mail due by now is on top before the new mail comes.
                _ = try await mail.wake()
                wakeOnTime()
            }
            refused = try await mail.refresh(box)
            if box == Mail.inbox {
                lastSync = started
                checkedAt = .now
            }
            offlineSince = nil
            error = nil
            synced = true
        } catch is CancellationError {
            // The app went away mid-sync: nothing went wrong, and nothing new is on screen. The
            // next sync picks it up, with whatever waits in line.
            syncing = false
            finish(false)
            return false
        } catch {
            // Another error means the network is there.
            let since = error.isNetwork ? offlineSince ?? started : nil
            offlineSince = since
            if quiet, let since, started - since < Self.blip {
                _ = error.textKey()
            } else {
                self.error = error.textKey()
            }
            synced = false
        }
        if box == Mail.inbox {
            // Whatever synced is on screen now, also when one account failed: no notification later.
            do {
                try await mail.forgetNewMail()
            } catch {
                self.error = error.textKey()
            }
            UNUserNotificationCenter.current().removeAllDeliveredNotifications()
            mail.prefetch()
        }
        // The first read may have failed; and an account may need signing in again now, or no more.
        if accounts.isEmpty || refused { await loadAccounts() }
        // Folders show once a sync has listed them.
        await loadFolders()
        await load()
        syncing = false
        if synced {
            syncedBoxes.insert(box)
            if !everSynced.contains(box.key) { everSynced.insert(box.key) }
        }
        if let (next, quietly) = syncAgain.first {
            syncAgain[next] = nil
            return await sync(next, quiet: quietly)
        }
        finish(synced)
        return synced
    }

    /// Lets the syncs asked for while one ran know it is over.
    private func finish(_ synced: Bool) {
        let waiting = inLine
        inLine = []
        for sync in waiting { sync.resume(returning: synced) }
    }

    /// On opening and on every return to the app: mail that came meanwhile shows without a pull.
    /// Not again right after one.
    func returned() async {
        // A failure from before the app was away says nothing about the network now.
        offlineSince = nil
        if let lastSync, ContinuousClock.now - lastSync < Self.resync { return }
        await sync(quiet: true)
        if !isInbox { await sync(mailbox, quiet: true) }
    }

    /// The phone is back online: a failed sync goes again at once rather than at its next turn,
    /// and the error goes with it.
    func backOnline() async {
        guard offlineSince != nil || error == "error_network" else { return }
        await sync(quiet: true)
        if !isInbox { await sync(mailbox, quiet: true) }
    }

    /// What changed the mail, here or in the background, shows at once; so does an action the
    /// server refused and the phone undid.
    func changed() async {
        await load()
        await loadAccounts()
        do {
            if try await mail.takeReverted() { note(tr("inbox_reverted")) }
        } catch {
            self.error = error.textKey()
        }
    }

    // MARK: Toasts

    /// Shows a toast without undo, in place of any other.
    func note(_ text: String) {
        answer(false)
        toast = Toast(text: text)
        let shown = toast
        Task {
            try? await Task.sleep(for: .seconds(4))
            if toast == shown { toast = nil }
        }
    }

    /// Shows a toast for `undoSeconds()`; true if Undo was tapped. One toast at a time. With
    /// `go`, it can also show that folder.
    func offerUndo(_ text: String, go: Mailbox? = nil) async -> Bool {
        // The older toast is past undo anyway.
        answer(false)
        let shown = Toast(text: text, undo: true, go: go)
        toast = shown
        return await withCheckedContinuation { continuation in
            waiting = continuation
            Task {
                // Closes before the core sends, so Undo is never too late.
                try? await Task.sleep(for: .seconds(Int(undoSeconds())))
                if toast == shown { answer(false) }
            }
        }
    }

    func tapUndo() {
        answer(true)
    }

    func tapGo(_ box: Mailbox) {
        answer(false)
        mailbox = box
    }

    private func answer(_ undo: Bool) {
        toast = nil
        waiting?.resume(returning: undo)
        waiting = nil
    }

    /// Takes `actions` back, or says the server already has them.
    private func takeBack(_ actions: [Int64]) async {
        if (try? await mail.undo(actions)) != true { note(tr("inbox_undo_late")) }
    }

    // MARK: Opening

    /// Opens `message`: beside the list on its side, on a screen of its own upright.
    func open(_ message: MessageItem) {
        guard sideways else {
            path.append(.message(message))
            return
        }
        // The one left beside the list now has a snippet.
        if pane != nil { Task { await load() } }
        // Opened from search, it shows beside the list, so search goes.
        path.removeAll { $0 == .search }
        pane = message
    }

    /// The message read, on its own screen or beside the list: the same across a turn of the phone.
    private var readingID: Int64? {
        if case .message(let message)? = path.last { return message.id }
        return pane?.id
    }

    /// Closes the message beside the list.
    func closePane() {
        pane = nil
        Task { await load() }
    }

    /// Turning the phone keeps the message open: beside the list, or on its own screen.
    private func turned() {
        if sideways, case .message(let message)? = path.last, path.allSatisfy({ $0 == .search || $0 == path.last }) {
            // From search too: beside the list, as when it is opened there.
            path.removeAll()
            pane = message
        } else if !sideways, let pane {
            self.pane = nil
            path.append(.message(pane))
        }
    }

    // MARK: Acting on mail

    /// The messages leave the list at once, for their account's folder of `role` if they go to
    /// one; the toast can take them all back for a moment, or show that folder.
    /// True if they were taken back.
    @discardableResult
    private func remove(_ gone: [MessageItem], toast text: String, role: FolderRole?, move: ([MessageItem]) async throws -> [Int64]) async -> Bool {
        let accounts = Set(gone.map(\.accountId))
        let go = accounts.count == 1
            ? folders.first { $0.accountId == accounts.first && $0.role == role }.map { Mailbox.folder(id: $0.id) }
            : nil
        let actions: [Int64]
        do {
            actions = try await move(gone)
        } catch {
            self.error = error.textKey()
            return false
        }
        let moved = Set(gone.map(\.id))
        withAnimation(.smooth) { messages.removeAll { moved.contains($0.id) } }
        // Beside the list, the message goes with its row.
        if let pane, moved.contains(pane.id) { self.pane = nil }
        guard await offerUndo(text, go: go == mailbox ? nil : go) else { return false }
        await takeBack(actions)
        return true
    }

    /// Done: into the archive.
    func archive(_ gone: [MessageItem]) async {
        await remove(gone, toast: tr("inbox_archived"), role: .archive, move: mail.archive)
    }

    /// Delete: into the trash, or for good from there.
    func delete(_ gone: [MessageItem]) async {
        let forever = !gone.contains(where: \.canTrash)
        await remove(gone, toast: tr(forever ? "inbox_deleted" : "inbox_trashed"), role: forever ? nil : .trash, move: mail.delete)
    }

    /// Snooze: into the snoozed folder, back on top of the inbox at `until`, with a notification
    /// then, should the app be closed.
    func snooze(_ gone: [MessageItem], until: Date) async {
        let undone = await remove(gone, toast: tr("inbox_snoozed", snoozeTime(until)), role: .snoozed) {
            let actions = try await mail.snooze($0, until: until)
            wakeOnTime()
            await BackgroundCheck.remind(about: $0, at: until)
            return actions
        }
        if undone { BackgroundCheck.forgetReminders(about: gone) }
    }

    /// While the app is open, wakes the next snoozed mail when its time comes, then waits for
    /// the one after.
    func wakeOnTime() {
        waking?.cancel()
        // Weak: the wait may be days long.
        waking = Task { [weak self, mail] in
            guard let next = try? await mail.nextWake() else { return }
            try? await Task.sleep(for: .seconds(max(0, next.timeIntervalSinceNow)))
            guard !Task.isCancelled else { return }
            _ = try? await mail.wake()
            self?.wakeOnTime()
        }
    }

    /// Back into the inbox, out of the archive, trash or spam.
    func moveToInbox(_ gone: [MessageItem]) async {
        await remove(gone, toast: tr("inbox_moved_back"), role: .inbox, move: mail.moveToInbox)
    }

    /// Marks the rows `read` or unread; they stay in the list either way.
    func setRead(_ rows: [MessageItem], read: Bool) async {
        do {
            try await mail.markRead(rows, seen: read)
        } catch {
            self.error = error.textKey()
            return
        }
        let marked = Set(rows.map(\.id))
        messages = messages.map { message in
            guard marked.contains(message.id) else { return message }
            var changed = message
            changed.seen = read
            changed.threadUnread = read ? 0 : 1
            return changed
        }
    }

    func swiped(_ message: MessageItem, _ action: SwipeAction) {
        Task {
            switch action {
            case .done: await archive([message])
            case .delete: await delete([message])
            // Read if it is unread, unread if it is read.
            case .read: await setRead([message], read: message.threadUnread > 0)
            // The sheet asks until when.
            case .snooze: snoozing = Snoozing(messages: [message])
            }
        }
    }

    /// Empties the open trash or spam for good, on the server too; the toast can take it back
    /// for a moment.
    func empty() async {
        let from = mailbox
        let actions: [Int64]
        do {
            actions = try await mail.emptyMailbox(from)
        } catch {
            self.error = error.textKey()
            return
        }
        if mailbox == from {
            messages = []
            pane = nil
        }
        if await offerUndo(tr("inbox_emptied")) { await takeBack(actions) }
    }

    /// The chosen messages, as the list shows them; acting on them ends the choosing.
    func choice(_ act: @escaping ([MessageItem]) async -> Void) {
        let rows = messages.filter { chosen.contains($0.id) }
        chosen = []
        Task { await act(rows) }
    }

    // MARK: Accounts

    /// Saves a change to an account; the sheet and the rows show it once it is saved.
    func changeAccount(_ change: @escaping () async throws -> Void) {
        Task {
            do {
                try await change()
            } catch {
                self.error = error.textKey()
            }
            await loadAccounts()
        }
    }

    func remove(_ account: AccountItem) async {
        accountsOpen = false
        do {
            try await mail.removeAccount(account)
        } catch {
            self.error = error.textKey()
            return
        }
        UNUserNotificationCenter.current().removeAllDeliveredNotifications()
        // Gone also when the list cannot be read again.
        accounts.removeAll { $0.id == account.id }
        folders.removeAll { $0.accountId == account.id }
        // Its folders, and with them the mailboxes together, may be gone.
        mailbox = Mail.inbox
        await loadAccounts()
        if accounts.isEmpty {
            everSynced = []
            onNoAccounts()
        } else {
            await load()
        }
    }

    // MARK: Writing

    /// Opens compose with the draft `make` gives for `original`, once at a time; not if the user
    /// has left the message meanwhile, as a forward may wait for its text to download.
    func write(_ original: MessageItem, _ make: @escaping () async throws -> DraftItem) {
        guard !writing else { return }
        writing = true
        let reading = readingID
        Task {
            defer { writing = false }
            do {
                let draft = try await make()
                if readingID == reading { composing = Composing(draft: draft, original: original) }
            } catch {
                self.error = error.textKey()
            }
        }
    }

    /// Sent: back to the list, where the toast can bring the message back to edit it.
    func sent(_ id: Int64, _ message: Composing) {
        composing = nil
        if case .message? = path.last { path.removeLast() }
        pane = nil
        Task {
            guard await offerUndo(tr("inbox_sending")) else { return }
            if let draft = try? await mail.undoSend(id) {
                var again = message
                again.draft = draft
                composing = again
            }
        }
    }

    /// Closed with something written: the toast says where it went.
    func kept() {
        composing = nil
        note(tr("draft_saved"))
    }

    /// Thrown away: the toast can bring it back, with its files, saved again to go on writing.
    func discarded(_ draft: DraftItem, original: MessageItem?) {
        composing = nil
        Task {
            // Nothing written is not worth bringing back.
            if draft.isBlank { return note(tr("draft_deleted")) }
            guard await offerUndo(tr("draft_deleted")) else { return }
            do {
                let saved = try await mail.restoreDraft(draft)
                composing = Composing(draft: saved, original: original)
            } catch {
                self.error = error.textKey()
            }
        }
    }

    /// A draft from the list, with the message it answers if that is still saved.
    func openDraft(_ draft: DraftItem) {
        Task {
            let original = try? await draft.replyTo.asyncMap { try await mail.message($0) } ?? nil
            composing = Composing(draft: draft, original: original)
        }
    }

    /// A message the server refused, back in compose to fix.
    func fix(_ failed: UnsentItem) {
        Task {
            do {
                if let draft = try await mail.undoSend(failed.id) { composing = Composing(draft: draft) }
            } catch {
                self.error = error.textKey()
            }
            await counts()
        }
    }

    /// An account was added or signed in to again: its mail comes in.
    func signedIn(new: Bool = false) {
        // A new account's mail is all still to come in the mailboxes together.
        if new { everSynced = everSynced.filter { !$0.hasPrefix(Mailbox.unifiedKey) } }
        Task {
            await loadAccounts()
            await sync()
        }
    }

    func closeSwipeTip() {
        swipeTip = false
        Settings.swipeTipSeen = true
    }
}

extension Optional {
    fileprivate func asyncMap<T>(_ transform: (Wrapped) async throws -> T) async rethrows -> T? {
        guard let self else { return nil }
        return try await transform(self)
    }
}

extension DraftItem {
    /// Nothing worth keeping: no text or files, and for a new message no recipients or subject
    /// either.
    var isBlank: Bool {
        text.isBlank && files.isEmpty && (replyTo != nil || (to.isBlank && subject.isBlank))
    }
}

extension Mailbox {
    /// What names it when kept on the phone: a folder by its id, the unified mailboxes by their role.
    var key: String {
        switch self {
        case .folder(let id): "folder-\(id)"
        case .unified(let role): "\(Mailbox.unifiedKey)\(role)"
        }
    }

    static let unifiedKey = "all-"
}

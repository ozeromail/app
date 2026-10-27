import SwiftUI

/// What a swipe on a row does; the person picks one for each side.
enum SwipeAction: String, CaseIterable, Identifiable {
    case done = "DONE"
    case read = "READ"
    case delete = "DELETE"
    case snooze = "SNOOZE"

    var id: Self { self }

    var label: String {
        switch self {
        case .done: tr("action_done")
        case .read: tr("action_read")
        case .delete: tr("action_delete")
        case .snooze: tr("action_snooze")
        }
    }

    var icon: String {
        switch self {
        case .done: "checkmark"
        case .read: "envelope.badge"
        case .delete: "trash"
        case .snooze: "clock"
        }
    }

    var tint: Color {
        switch self {
        case .done: .ozeroFill
        case .delete: .ozeroError
        case .read: .gray
        case .snooze: .ozeroSnooze
        }
    }

    /// Where an action applies: Done and Snooze only where they file mail; Delete in the trash is
    /// for good.
    func applies(to message: MessageItem) -> Bool {
        switch self {
        case .done: message.canArchive
        case .snooze: message.canSnooze
        case .read, .delete: true
        }
    }
}

/// By default left is Done and right is Read, as on Android.
struct Swipes: Equatable {
    var left = SwipeAction.done
    var right = SwipeAction.read

    /// What a row's menu offers: each side's action, once.
    var actions: [SwipeAction] { left == right ? [left] : [left, right] }

    var summary: String {
        tr("swipes_summary", left.label, right.label)
    }
}

/// Which groups of the accounts sheet show their folders: account ids, or `Settings.allAccounts`.
/// A group the person closed or opened stays so. One never touched starts open, but with
/// `many` accounts each account's starts closed, so the sheet stays short.
struct Groups: Equatable {
    var collapsed: Set<Int64> = []
    var expanded: Set<Int64> = []

    func open(_ group: Int64, many: Bool) -> Bool {
        if collapsed.contains(group) { return false }
        if expanded.contains(group) { return true }
        return group == Settings.allAccounts || !many
    }

    func toggled(_ group: Int64, many: Bool) -> Groups {
        var next = self
        if open(group, many: many) {
            next.collapsed.insert(group)
            next.expanded.remove(group)
        } else {
            next.collapsed.remove(group)
            next.expanded.insert(group)
        }
        return next
    }
}

/// A mailbox of all accounts the sheet can show under the inboxes; `shown` are the ones it
/// shows until the person picks.
enum AllBox: String, CaseIterable, Identifiable {
    case unread = "UNREAD"
    case snoozed = "SNOOZED"
    case flagged = "FLAGGED"
    case sent = "SENT"
    case archive = "ARCHIVE"
    case files = "FILES"
    case junk = "JUNK"
    case trash = "TRASH"

    static let shown: Set<AllBox> = [.unread, .snoozed, .flagged, .sent, .archive]

    var id: Self { self }

    var mailbox: Mailbox {
        switch self {
        case .unread: .unread
        case .files: .files
        case .snoozed: .unified(role: .snoozed)
        case .flagged: .unified(role: .flagged)
        case .sent: .unified(role: .sent)
        case .archive: .unified(role: .archive)
        case .junk: .unified(role: .junk)
        case .trash: .unified(role: .trash)
        }
    }

    var icon: String {
        switch mailbox {
        case .unread: "envelope.badge"
        case .files: "paperclip"
        case .unified(let role): role.icon
        case .folder: "folder"
        }
    }

    var title: String {
        switch mailbox {
        case .unread: tr("folder_unread")
        case .files: tr("folder_files")
        case .unified(let role): role.allTitle
        case .folder: ""
        }
    }

    /// Shown in the list: a role's mailbox once some account has such a folder.
    func listed(_ folders: [FolderItem]) -> Bool {
        if case .unified(let role) = mailbox { return folders.contains { $0.role == role } }
        return true
    }
}

/// The mailboxes of all accounts in the person's order, and which of them show.
struct AllBoxes: Equatable {
    var order: [AllBox]
    var shown: Set<AllBox>
}

/// The person's choices, kept on the phone.
enum Settings {
    /// The group of the mailboxes of all accounts together; account ids are positive.
    static let allAccounts: Int64 = 0

    private static let defaults = UserDefaults.standard

    static var swipes: Swipes {
        get {
            let fallback = Swipes()
            return Swipes(
                left: defaults.string(forKey: "swipe_left").flatMap(SwipeAction.init) ?? fallback.left,
                right: defaults.string(forKey: "swipe_right").flatMap(SwipeAction.init) ?? fallback.right)
        }
        set {
            defaults.set(newValue.left.rawValue, forKey: "swipe_left")
            defaults.set(newValue.right.rawValue, forKey: "swipe_right")
        }
    }

    /// When each account was last checked in the background, by its id, in seconds.
    static var checked: [String: Double] {
        get { defaults.dictionary(forKey: "checked") as? [String: Double] ?? [:] }
        set { defaults.set(newValue, forKey: "checked") }
    }

    /// The tip about swipes was read, so it does not show again.
    static var swipeTipSeen: Bool {
        get { defaults.bool(forKey: "swipe_tip_seen") }
        set { defaults.set(newValue, forKey: "swipe_tip_seen") }
    }

    /// The mailboxes synced on this phone, by `Mailbox.key`: only one never synced waits for its first mail.
    static var syncedBoxes: Set<String> {
        get { Set(defaults.stringArray(forKey: "synced_boxes") ?? []) }
        set { defaults.set(Array(newValue), forKey: "synced_boxes") }
    }

    /// The groups of the accounts sheet the person closed or opened.
    static var groups: Groups {
        get { Groups(collapsed: ids("collapsed_groups"), expanded: ids("expanded_groups")) }
        set {
            defaults.set(newValue.collapsed.map(String.init), forKey: "collapsed_groups")
            defaults.set(newValue.expanded.map(String.init), forKey: "expanded_groups")
        }
    }

    /// The mailboxes of all accounts below the inboxes in the accounts sheet: their order and which show.
    static var allBoxes: AllBoxes {
        get {
            func boxes(_ key: String) -> [AllBox]? {
                defaults.string(forKey: key)?.split(separator: ",").compactMap { AllBox(rawValue: String($0)) }
            }
            // One added later goes at the end.
            let saved = boxes("all_boxes_order") ?? []
            return AllBoxes(order: saved + AllBox.allCases.filter { !saved.contains($0) },
                            shown: boxes("all_boxes_shown").map(Set.init) ?? AllBox.shown)
        }
        set {
            defaults.set(newValue.order.map(\.rawValue).joined(separator: ","), forKey: "all_boxes_order")
            defaults.set(newValue.shown.map(\.rawValue).joined(separator: ","), forKey: "all_boxes_shown")
        }
    }

    private static func ids(_ key: String) -> Set<Int64> {
        Set((defaults.stringArray(forKey: key) ?? []).compactMap { Int64($0) })
    }
}

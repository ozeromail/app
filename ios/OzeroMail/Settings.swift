import SwiftUI

/// What a swipe on a row does; the person picks one for each side.
enum SwipeAction: String, CaseIterable, Identifiable {
    case done = "DONE"
    case read = "READ"
    case delete = "DELETE"

    var id: Self { self }

    var label: String {
        switch self {
        case .done: tr("action_done")
        case .read: tr("action_read")
        case .delete: tr("action_delete")
        }
    }

    var icon: String {
        switch self {
        case .done: "checkmark"
        case .read: "envelope.badge"
        case .delete: "trash"
        }
    }

    /// Where an action applies: Done only where it files mail; Delete in the trash is for good.
    func applies(to message: MessageItem) -> Bool {
        self != .done || message.canArchive
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

    /// The tip about swipes was read, so it does not show again.
    static var swipeTipSeen: Bool {
        get { defaults.bool(forKey: "swipe_tip_seen") }
        set { defaults.set(newValue, forKey: "swipe_tip_seen") }
    }

    /// The groups of the accounts sheet the person closed or opened.
    static var groups: Groups {
        get { Groups(collapsed: ids("collapsed_groups"), expanded: ids("expanded_groups")) }
        set {
            defaults.set(newValue.collapsed.map(String.init), forKey: "collapsed_groups")
            defaults.set(newValue.expanded.map(String.init), forKey: "expanded_groups")
        }
    }

    private static func ids(_ key: String) -> Set<Int64> {
        Set((defaults.stringArray(forKey: key) ?? []).compactMap { Int64($0) })
    }
}

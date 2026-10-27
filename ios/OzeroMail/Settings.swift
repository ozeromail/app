import SwiftUI

/// What a swipe on a row does; the person picks a short and a long one for each side.
enum SwipeAction: String, CaseIterable, Identifiable {
    case done = "DONE"
    case read = "READ"
    case delete = "DELETE"
    case snooze = "SNOOZE"
    case flag = "FLAG"
    case move = "MOVE"
    case spam = "SPAM"

    var id: Self { self }

    var label: String {
        switch self {
        case .done: tr("action_done")
        case .read: tr("action_read")
        case .delete: tr("action_delete")
        case .snooze: tr("action_snooze")
        case .flag: tr("swipe_flag")
        case .move: tr("swipe_move")
        case .spam: tr("action_spam")
        }
    }

    /// The action's icon on `message`: an open envelope to mark read, a struck flag to unflag.
    func icon(for message: MessageItem) -> String {
        switch self {
        case .read: message.threadUnread > 0 ? "envelope.open" : "envelope.badge"
        case .flag where message.flagged: "flag.slash"
        default: icon
        }
    }

    /// What the action says on `message`: Read on a read one is Unread, Flag on a flagged one
    /// takes it off.
    func label(for message: MessageItem) -> String {
        switch self {
        case .read where message.threadUnread == 0: tr("action_unread")
        case .flag where message.flagged: tr("action_unflag")
        case .delete where !message.canTrash: tr("action_delete_forever")
        default: label
        }
    }

    var icon: String {
        switch self {
        case .done: "checkmark"
        case .read: "envelope.badge"
        case .delete: "trash"
        case .snooze: "clock"
        case .flag: "flag"
        case .move: "folder"
        case .spam: "xmark.bin"
        }
    }

    var tint: Color {
        switch self {
        case .done: .ozeroFill
        case .delete: .ozeroError
        case .read: .gray
        case .snooze: .ozeroSnooze
        case .flag: .orange
        case .move: .purple
        case .spam: .gray
        }
    }

    /// Where an action applies: Done and Snooze only where they file mail; Delete in the trash is
    /// for good.
    func applies(to message: MessageItem) -> Bool {
        switch self {
        case .done: message.canArchive
        case .snooze: message.canSnooze
        case .move: message.canMove
        case .spam: message.canSpam
        case .read, .delete, .flag: true
        }
    }
}

/// By default left is Done and, further, Delete; right is Read and, further, Snooze, as on
/// Android. A long swipe can be off, nil.
struct Swipes: Equatable {
    var left = SwipeAction.done
    var right = SwipeAction.read
    var leftLong: SwipeAction? = .delete
    var rightLong: SwipeAction? = .snooze

    /// What a row's menu offers: each swipe's action, once.
    var actions: [SwipeAction] {
        var seen = Set<SwipeAction>()
        return [left, leftLong, right, rightLong].compactMap { $0 }.filter { seen.insert($0).inserted }
    }

    /// What the tip says the swipes do, with the long ones when they are on.
    var tip: String {
        func side(_ short: SwipeAction, _ long: SwipeAction?) -> String {
            long.map { tr("swipes_further", short.label, $0.label) } ?? short.label
        }
        return tr("swipes_tip", side(left, leftLong), side(right, rightLong))
    }

    var summary: String {
        tr("swipes_summary", left.label, right.label)
    }
}

/// The accounts whose folders the accounts sheet shows; the rest stay closed, so the sheet stays short.
struct Groups: Equatable {
    var expanded: Set<Int64> = []
    /// The parts of the sheet the person closed; they start open.
    var closed: Set<String> = []

    static let inboxes = "inboxes"
    static let allBoxes = "all"
    static let folders = "folders"

    func open(_ account: Int64) -> Bool { expanded.contains(account) }

    func toggled(_ account: Int64) -> Groups {
        var next = self
        if open(account) { next.expanded.remove(account) } else { next.expanded.insert(account) }
        return next
    }

    func open(_ part: String) -> Bool { !closed.contains(part) }

    func toggled(_ part: String) -> Groups {
        var next = self
        if open(part) { next.closed.insert(part) } else { next.closed.remove(part) }
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

    /// The role of a mailbox of all accounts' folders of one role.
    private var role: FolderRole? {
        switch self {
        case .unread, .files: nil
        case .snoozed: .snoozed
        case .flagged: .flagged
        case .sent: .sent
        case .archive: .archive
        case .junk: .junk
        case .trash: .trash
        }
    }

    var mailbox: Mailbox {
        if let role { return .unified(role: role) }
        return self == .unread ? .unread : .files
    }

    var icon: String {
        if let role { return role.icon }
        return self == .unread ? "envelope.badge" : "paperclip"
    }

    var title: String {
        if let role { return role.allTitle }
        return tr(self == .unread ? "folder_unread" : "folder_files")
    }

    /// Shown in the list: a role's mailbox once some account has such a folder.
    func listed(_ folders: [FolderItem]) -> Bool {
        guard let role else { return true }
        return folders.contains { $0.role == role }
    }
}

/// The mailboxes of all accounts in the person's order, and which of them show.
struct AllBoxes: Equatable {
    var order: [AllBox]
    var shown: Set<AllBox>
}

/// The person's choices, kept on the phone.
enum Settings {
    private static let defaults = UserDefaults.standard

    static var swipes: Swipes {
        get {
            let fallback = Swipes()
            // A long swipe turned off is kept as OFF, so it does not come back as the default.
            // Who picked their swipes before long ones came keeps them off, so a full swipe does what it did.
            let picked = defaults.string(forKey: "swipe_left") != nil
            func long(_ key: String, _ usual: SwipeAction?) -> SwipeAction? {
                guard let name = defaults.string(forKey: key) else { return picked ? nil : usual }
                return name == off ? nil : SwipeAction(rawValue: name) ?? usual
            }
            return Swipes(
                left: defaults.string(forKey: "swipe_left").flatMap(SwipeAction.init) ?? fallback.left,
                right: defaults.string(forKey: "swipe_right").flatMap(SwipeAction.init) ?? fallback.right,
                leftLong: long("swipe_left_long", fallback.leftLong),
                rightLong: long("swipe_right_long", fallback.rightLong))
        }
        set {
            defaults.set(newValue.left.rawValue, forKey: "swipe_left")
            defaults.set(newValue.right.rawValue, forKey: "swipe_right")
            defaults.set(newValue.leftLong?.rawValue ?? off, forKey: "swipe_left_long")
            defaults.set(newValue.rightLong?.rawValue ?? off, forKey: "swipe_right_long")
        }
    }

    private static let off = "OFF"

    /// When each account was last checked in the background, by its id, in seconds.
    static var checked: [String: Double] {
        get { defaults.dictionary(forKey: "checked") as? [String: Double] ?? [:] }
        set { defaults.set(newValue, forKey: "checked") }
    }

    /// The size of mail's text, a place in `textScales`.
    static var textSize: Int {
        get { defaults.object(forKey: "text_size") == nil ? usualSize : min(max(defaults.integer(forKey: "text_size"), 0), textScales.count - 1) }
        set { defaults.set(newValue, forKey: "text_size") }
    }

    /// How much larger than usual mail's text is.
    static var textScale: Double { textScales[textSize] }

    /// The sizes of mail's text the person can pick from, against the usual one.
    static let textScales = [0.9, 1, 1.1, 1.2, 1.35]
    static let usualSize = 1

    /// An account was held in the accounts sheet, so the tip about it does not show again.
    static var holdTipSeen: Bool {
        get { defaults.bool(forKey: "hold_tip_seen") }
        set { defaults.set(newValue, forKey: "hold_tip_seen") }
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
        get { Groups(expanded: ids("expanded_groups"), closed: Set(defaults.stringArray(forKey: "closed_parts") ?? [])) }
        set {
            defaults.set(newValue.expanded.map(String.init), forKey: "expanded_groups")
            defaults.set(Array(newValue.closed), forKey: "closed_parts")
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

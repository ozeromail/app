import SwiftUI

/// How wide things get on a wide screen, so lines stay easy to read and scan.
enum Room {
    /// The list, search and sign-in.
    static let list: CGFloat = 640
    /// A message: a newsletter made for 600 points fits at its own size.
    static let letter: CGFloat = 720
    /// The fields of a message being written.
    static let compose: CGFloat = 680
    /// The mailboxes beside the list, and the list beside an open message.
    static let mailboxes: CGFloat = 200
    static let listPane: CGFloat = 320
}

extension View {
    /// Keeps the view to `max` wide, in the middle of the room it has.
    func middle(_ max: CGFloat) -> some View {
        frame(maxWidth: max).frame(maxWidth: .infinity)
    }
}

extension EnvironmentValues {
    /// On its side: the phone's height is short, the mailboxes and the message go beside the list.
    var sideways: Bool { verticalSizeClass == .compact }
}

/// Puts the keyboard away now, so it leaves with the screen that closes rather than after it.
@MainActor
func hideKeyboard() {
    UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
}

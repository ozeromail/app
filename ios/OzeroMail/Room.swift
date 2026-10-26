import SwiftUI

/// How wide things get on a wide screen, so lines stay easy to read and scan.
enum Room {
    /// The list, search and sign-in.
    static let list: CGFloat = 640
    /// A message: a newsletter made for 600 points fits at its own size.
    static let letter: CGFloat = 720
    /// The fields of a message being written.
    static let compose: CGFloat = 680
    /// The left rail with the inbox tabs, and a rail with buttons only.
    static let tabsRail: CGFloat = 172
    static let rail: CGFloat = 84
}

extension View {
    /// Keeps the view to `max` wide, in the middle of the room it has.
    func middle(_ max: CGFloat) -> some View {
        frame(maxWidth: max).frame(maxWidth: .infinity)
    }
}

/// A column of controls along an edge, held low where the thumb rests: on its side the phone's
/// height is short, so the bottom bars move here.
struct Rail<Content: View>: View {
    var width: CGFloat = Room.rail
    @ViewBuilder let content: Content

    var body: some View {
        VStack(spacing: 8) {
            Spacer(minLength: 0)
            content
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 12)
        .frame(width: width)
        .frame(maxHeight: .infinity)
        .background(.bar)
    }
}

extension EnvironmentValues {
    /// On its side: the phone's height is short, and the controls go to the rails.
    var sideways: Bool { verticalSizeClass == .compact }
}

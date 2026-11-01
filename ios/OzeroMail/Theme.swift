import SwiftUI

extension Color {
    /// The one accent colour; everything else stays neutral. It tints text too, so it takes Android's
    /// readable `primary`, not the brighter fill.
    static let ozeroAccent = dynamic(light: 0x1A6FE0, dark: 0x6AB2FF)

    /// The bright blue of buttons, Done, marks and water, the same in both themes.
    static let ozeroFill = Color(rgb: 0x2F8CFF)

    /// The late sun of Snooze.
    static let ozeroSnooze = dynamic(light: 0xE0922B, dark: 0xF0B25A)

    /// Errors and what cannot be undone, as text: dark enough to read on white.
    static let ozeroError = dynamic(light: 0xC94452, dark: 0xEE6672)

    /// The brighter red a Delete swipe fills with.
    static let ozeroErrorFill = dynamic(light: 0xDF5260, dark: 0xEE6672)

    /// Cards over the list, such as an account to sign in to again.
    static let ozeroCard = dynamic(light: 0xF2F5FA, dark: 0x1A202C)

    /// A colour from the core, as 0xRRGGBB.
    init(rgb: UInt32) {
        self.init(
            red: Double((rgb >> 16) & 0xFF) / 255,
            green: Double((rgb >> 8) & 0xFF) / 255,
            blue: Double(rgb & 0xFF) / 255)
    }

    private static func dynamic(light: UInt32, dark: UInt32, alpha: (light: Double, dark: Double) = (1, 1)) -> Color {
        Color(uiColor: UIColor { traits in
            let night = traits.userInterfaceStyle == .dark
            return UIColor(Color(rgb: night ? dark : light)).withAlphaComponent(night ? alpha.dark : alpha.light)
        })
    }
}

// A little depth in the icon's paper, kept faint. What lies on top of the page (the docks, the
// accent buttons) lifts with a soft shadow and light along its top edge; what holds things (fields,
// the accounts button) is cut in, with a shadow under its top edge; dots are tiny windows.
extension Color {
    /// Lifted off the page: light along its top edge and a soft shadow in its own colour.
    var lifted: some ShapeStyle {
        shadow(.inner(color: .white.opacity(0.3), radius: 0.5, y: 1))
            .shadow(.drop(color: opacity(0.4), radius: 5, y: 2))
    }

    /// Cut into the page: a shadow under its top edge.
    var sunk: some ShapeStyle {
        shadow(.inner(color: .ozeroShade(day: 0.1, night: 0.45), radius: 1.5, y: 1))
    }

    /// A round mark as a tiny window: darker under its top edge, lighter below.
    var window: some ShapeStyle {
        LinearGradient(colors: [mix(with: Color(rgb: 0x142036), by: 0.3), self, mix(with: .white, by: 0.2)], startPoint: .top, endPoint: .bottom)
    }

    /// A button's fill: the accent lifts off the page, the rest lies flat.
    var buttonFill: AnyShapeStyle {
        self == .ozeroFill ? AnyShapeStyle(lifted) : AnyShapeStyle(self)
    }

    /// The ink of shadows: the paper's navy by day, black at night, as see-through as asked.
    static func ozeroShade(day: Double, night: Double) -> Color {
        dynamic(light: 0x142036, dark: 0x000000, alpha: (day, night))
    }
}

extension View {
    /// A dock lying on the list: the system's bar with a soft shadow above its top edge.
    func dockBackground() -> some View {
        background(.bar)
            .overlay(alignment: .top) {
                LinearGradient(colors: [.clear, .ozeroShade(day: 0.06, night: 0.3)], startPoint: .top, endPoint: .bottom)
                    .frame(height: 12)
                    .offset(y: -12)
                    .allowsHitTesting(false)
            }
    }
}

extension AccountMark {
    /// The account's colour, from the core.
    var tint: Color { Color(rgb: color) }
}

extension AccountItem {
    /// The account's colour, from the core.
    var tint: Color { Color(rgb: color) }
}

/// A dot of an account's colour, a tiny window.
struct AccountDot: View {
    let color: Color
    var size: CGFloat = 10

    var body: some View {
        Circle().fill(color.window).frame(width: size, height: size)
    }
}

/// Up to three account colours overlapping.
struct StackedDots: View {
    let accounts: [AccountItem]

    var body: some View {
        ZStack(alignment: .leading) {
            ForEach(Array(accounts.prefix(3).enumerated()), id: \.offset) { i, account in
                AccountDot(color: account.tint)
                    .overlay(Circle().stroke(Color(uiColor: .systemBackground), lineWidth: 1.5))
                    .offset(x: 7 * CGFloat(i))
            }
        }
        .frame(width: 24, height: 10, alignment: .leading)
    }
}

/// The type scale, by role, on the system's text styles so Dynamic Type applies. Mail's own text
/// is `reading`, in MessageView; everything around it takes one of these.
extension Font {
    /// Screen titles: Inbox, a folder.
    static let screenTitle = Font.largeTitle.bold()

    /// An open message's subject, and the subject being written.
    static let subjectTitle = Font.title2.bold()

    /// Titles of sheets and notes.
    static let heading = Font.title3.weight(.semibold)

    /// A line under a heading, as on the first screen.
    static let lead = Font.title3

    /// A sender in a list, the name on an open message, rows of menus and settings.
    static let row = Font.body.weight(.medium)
    static let rowStrong = Font.body.weight(.semibold)
    static let rowBold = Font.body.weight(.bold)
    static let rowPlain = Font.body

    /// Words being written: a message's text (`Rich.font` in UIKit) and its signature.
    static let writing = Font.body

    /// A subject and a preview in a list, short text around the app.
    static let text = Font.subheadline
    static let textStrong = Font.subheadline.weight(.semibold)

    /// Who wrote to whom, hints, captions.
    static let small = Font.footnote
    static let smallStrong = Font.footnote.weight(.semibold)

    /// Times, with figures that line up.
    static let time = Font.footnote.monospacedDigit()

    /// Under the icons of a bar, a file's kind.
    static let label = Font.caption2.weight(.semibold)

    /// Counts on a thread or a tab.
    static let badge = Font.caption2.weight(.semibold).monospacedDigit()
}

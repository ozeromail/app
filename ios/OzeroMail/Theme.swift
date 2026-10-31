import SwiftUI

extension Color {
    /// The one accent colour; everything else stays neutral. It tints text too, so it takes Android's
    /// readable `primary`, not the brighter fill.
    static let ozeroAccent = dynamic(light: 0x1A6FE0, dark: 0x6AB2FF)

    /// The bright blue of buttons, Done, marks and water, the same in both themes.
    static let ozeroFill = Color(rgb: 0x2F8CFF)

    /// The late sun of Snooze.
    static let ozeroSnooze = dynamic(light: 0xE0922B, dark: 0xF0B25A)

    /// Errors and what cannot be undone.
    static let ozeroError = dynamic(light: 0xDF5260, dark: 0xEE6672)

    /// Cards over the list, such as an account to sign in to again.
    static let ozeroCard = dynamic(light: 0xF2F5FA, dark: 0x1A202C)

    /// A colour from the core, as 0xRRGGBB.
    init(rgb: UInt32) {
        self.init(
            red: Double((rgb >> 16) & 0xFF) / 255,
            green: Double((rgb >> 8) & 0xFF) / 255,
            blue: Double(rgb & 0xFF) / 255)
    }

    private static func dynamic(light: UInt32, dark: UInt32) -> Color {
        Color(uiColor: UIColor { traits in
            UIColor(Color(rgb: traits.userInterfaceStyle == .dark ? dark : light))
        })
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

/// A dot of an account's colour.
struct AccountDot: View {
    let color: Color
    var size: CGFloat = 10

    var body: some View {
        Circle().fill(color).frame(width: size, height: size)
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

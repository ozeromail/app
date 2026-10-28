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

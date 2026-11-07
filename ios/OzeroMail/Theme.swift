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

    /// The paper itself: white by day, the lake's deep blue at night, as on Android.
    static let ozeroPage = dynamic(light: 0xFFFFFF, dark: 0x141923)

    /// Text in the paper's navy ink, and what is said less loudly.
    static let ozeroInk = dynamic(light: 0x142036, dark: 0xE5E9F1)
    static let ozeroMuted = dynamic(light: 0x66728A, dark: 0x8792A7)

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
            .shadow(.drop(color: opacity(0.3), radius: 3, y: 1.5))
    }

    /// Cut into the page: a shadow under its top edge.
    var sunk: some ShapeStyle {
        shadow(.inner(color: .ozeroShade(day: 0.06, night: 0.35), radius: 1, y: 1))
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
    /// A dock lying on the list: the system's bar with a hairline along its top edge, no shadow. A
    /// `floating` dock has neither on iOS 26: its buttons are glass. A `solid` one is the paper
    /// itself, as Android's quick reply.
    @ViewBuilder
    func dockBackground(floating: Bool = false, solid: Bool = false) -> some View {
        if floating && hasGlass {
            self
        } else if solid {
            background(Color.ozeroPage).overlay(alignment: .top) { Divider() }
        } else {
            background(.bar).overlay(alignment: .top) { Divider() }
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
                    .overlay(Circle().stroke(Color.ozeroPage, lineWidth: 1.5))
                    .offset(x: 7 * CGFloat(i))
            }
        }
        .frame(width: 24, height: 10, alignment: .leading)
    }
}

/// The type scale, by role: Inter, on the system's text styles so Dynamic Type applies, as
/// Android sets its type in Inter. Mail's own text is `reading`, in MessageView, in Literata.
extension Font {
    /// Inter at a text style's size, growing with it.
    static func inter(_ size: CGFloat, _ style: Font.TextStyle, _ weight: Font.Weight = .regular) -> Font {
        .custom("Inter", size: size, relativeTo: style).weight(weight)
    }

    /// Screen titles: Inbox, a folder.
    static let screenTitle = inter(34, .largeTitle, .bold)

    /// An open message's subject, and the subject being written.
    static let subjectTitle = inter(22, .title2, .bold)

    /// Titles of sheets and notes.
    static let heading = inter(20, .title3, .semibold)

    /// A line under a heading, as on the first screen.
    static let lead = inter(20, .title3)

    /// A sender in a list, the name on an open message, rows of menus and settings.
    static let row = inter(17, .body, .medium)
    static let rowStrong = inter(17, .body, .semibold)
    static let rowBold = inter(17, .body, .bold)
    static let rowPlain = inter(17, .body)

    /// Words being written: a message's text (`Rich.font` in UIKit) and its signature.
    static let writing = inter(17, .body)

    /// A subject and a preview in a list, short text around the app.
    static let text = inter(15, .subheadline)
    static let textStrong = inter(15, .subheadline, .semibold)

    /// Who wrote to whom, hints, captions.
    static let small = inter(13, .footnote)
    static let smallStrong = inter(13, .footnote, .semibold)

    /// Times, with figures that line up.
    static let time = inter(13, .footnote).monospacedDigit()

    /// Under the icons of a bar, a file's kind.
    static let label = inter(11, .caption2, .semibold)

    /// Counts on a thread or a tab.
    static let badge = inter(11, .caption2, .semibold).monospacedDigit()
}

extension UIFont {
    /// Inter at `size` and `weight`, growing with the system's text size like `style`, for UIKit.
    static func inter(_ size: CGFloat, _ weight: UIFont.Weight, _ style: UIFont.TextStyle) -> UIFont {
        let descriptor = UIFontDescriptor(fontAttributes: [.family: "Inter", .traits: [UIFontDescriptor.TraitKey.weight: weight]])
        return UIFontMetrics(forTextStyle: style).scaledFont(for: UIFont(descriptor: descriptor, size: size))
    }
}

extension View {
    /// The paper's look for a screen and all it holds: Inter on the paper's page. The system's own
    /// controls (menus, segmented tabs, sheets) keep their system look.
    func paper() -> some View {
        font(.rowPlain).background(Color.ozeroPage)
    }

    /// Text in the paper's navy ink, and its quieter grey for `.secondary`. Only on what is read,
    /// never over buttons, which keep the accent.
    func ink() -> some View {
        foregroundStyle(Color.ozeroInk, Color.ozeroMuted, Color.ozeroMuted.opacity(0.7))
    }

    /// A sheet on the paper, as Android's, not the system's grey glass: what is behind does not
    /// show through, and the sheet's own shape and drag stay the system's.
    func paperSheet() -> some View {
        presentationBackground(Color.ozeroPage)
    }

    /// A list or form on the paper's page instead of the system's grey.
    func paperList() -> some View {
        scrollContentBackground(.hidden).background(Color.ozeroPage)
    }
}

/// A form on the paper: the page white or the lake's blue, its groups in the paper's card colour,
/// as Android's settings.
struct PaperForm<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        Form {
            Group { content }.listRowBackground(Color.ozeroCard)
        }
        .paperList()
    }
}

/// What a group of a form is, over it: small and quiet, as Android's captions.
struct FormCaption: View {
    let key: LocalizedStringKey

    init(_ key: LocalizedStringKey) { self.key = key }

    var body: some View {
        Text(key).font(.smallStrong).foregroundStyle(Color.ozeroMuted).textCase(nil)
    }
}

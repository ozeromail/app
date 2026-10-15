import SwiftUI

extension Color {
    /// The one accent colour; everything else stays neutral. Same values as on Android.
    static let ozeroAccent = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 0x5C / 255, green: 0x93 / 255, blue: 0xF0 / 255, alpha: 1)
            : UIColor(red: 0x2F / 255, green: 0x6F / 255, blue: 0xDE / 255, alpha: 1)
    })
}

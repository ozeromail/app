import SwiftUI

extension Color {
    /// The one accent colour; everything else stays neutral. Same values as on Android.
    static let ozeroAccent = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 0x6A / 255, green: 0xA3 / 255, blue: 0xF9 / 255, alpha: 1)
            : UIColor(red: 0x1F / 255, green: 0x5C / 255, blue: 0xCD / 255, alpha: 1)
    })
}

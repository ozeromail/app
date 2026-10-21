import SwiftUI

extension Color {
    /// The one accent colour; everything else stays neutral. It tints text too, so it takes Android's
    /// readable `primary`, not the brighter fill.
    static let ozeroAccent = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark
            ? UIColor(red: 0x6A / 255, green: 0xB2 / 255, blue: 0xFF / 255, alpha: 1)
            : UIColor(red: 0x1A / 255, green: 0x6F / 255, blue: 0xE0 / 255, alpha: 1)
    })
}

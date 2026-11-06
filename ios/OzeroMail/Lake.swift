import SwiftUI

/// A list with nothing in it, as the system shows one: still water over its title, and under it
/// when it was checked. While there are `checking` words, as the first mail is on its way, they
/// take the place of the title and say what it waits for. `grey` water is for plain folders.
struct LakeNote: View {
    let title: LocalizedStringKey
    var grey = false
    var checking: LakeWords?
    var checkedAt: Date?

    var body: some View {
        ContentUnavailableView {
            Label {
                Text(checking.map { LocalizedStringKey($0.title) } ?? title)
            } icon: {
                LakeMark(grey: grey)
            }
        } description: {
            if let checking {
                if let text = checking.text { Text(LocalizedStringKey(text)) }
            } else if let checkedAt {
                // "Just now" turns into a time.
                TimelineView(.explicit([checkedAt, checkedAt.addingTimeInterval(60)])) { now in
                    Text(now.date.timeIntervalSince(checkedAt) < 60
                         ? tr("lake_checked_now")
                         : tr("lake_checked_at", shortDate(Int64(checkedAt.timeIntervalSince1970))))
                }
            }
        }
        .animation(.smooth, value: checking)
    }
}

/// What an empty list waits for: the keys of its title and of the line under it, if any.
struct LakeWords: Equatable {
    let title: String
    let text: String?
}

/// Still water, drawn small: a horizon fading out at both ends, and its reflection under it. An
/// image, as the system's empty view takes one, tinted like the water.
private struct LakeMark: View {
    let grey: Bool

    var body: some View {
        Image(uiImage: Self.image)
            .foregroundStyle(grey ? Color(uiColor: .tertiaryLabel) : Color.ozeroFill)
            .accessibilityHidden(true)
    }

    private static let image = UIGraphicsImageRenderer(size: CGSize(width: 72, height: 18)).image { drawn in
        let lines: [(width: CGFloat, alpha: CGFloat)] = [(72, 0.9), (48, 0.35), (28, 0.18)]
        for (i, line) in lines.enumerated() {
            let left = (72 - line.width) / 2
            let y = 1 + CGFloat(i) * 7
            let colors = [UIColor.black.withAlphaComponent(0), UIColor.black.withAlphaComponent(line.alpha),
                          UIColor.black.withAlphaComponent(0)].map(\.cgColor) as CFArray
            guard let gradient = CGGradient(colorsSpace: nil, colors: colors, locations: [0, 0.5, 1]) else { continue }
            drawn.cgContext.saveGState()
            drawn.cgContext.clip(to: CGRect(x: left, y: y, width: line.width, height: 1.5))
            drawn.cgContext.drawLinearGradient(gradient, start: CGPoint(x: left, y: y), end: CGPoint(x: left + line.width, y: y), options: [])
            drawn.cgContext.restoreGState()
        }
    }.withRenderingMode(.alwaysTemplate)
}

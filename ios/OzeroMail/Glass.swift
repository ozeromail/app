import SwiftUI

// iOS 26's Liquid Glass for what floats over the page: the bars along the bottom and their
// buttons, round or capsule-shaped. Before iOS 26 they keep the page's own fills.

extension View {
    /// A floating control's surface in `shape`: glass that answers the finger, tinted for the one
    /// main action; before iOS 26, `fallback`.
    @ViewBuilder
    func glassSurface(in shape: some Shape, tint: Color? = nil, fallback: some ShapeStyle) -> some View {
        if #available(iOS 26, *) {
            glassEffect(tint.map { Glass.regular.tint($0).interactive() } ?? .regular.interactive(), in: shape)
        } else {
            background(fallback, in: shape)
        }
    }

    /// A bar along the bottom of a screen. On iOS 26 it floats over the content, which fades under
    /// it; before, it takes the room from the content.
    @ViewBuilder
    func bottomBar(@ViewBuilder _ content: () -> some View) -> some View {
        if #available(iOS 26, *) {
            safeAreaBar(edge: .bottom, spacing: 0, content: content)
        } else {
            safeAreaInset(edge: .bottom, spacing: 0, content: content)
        }
    }
}

/// Glass shapes near each other drawn as one, so they flow together as they move.
struct GlassGroup<Content: View>: View {
    let spacing: CGFloat
    let content: Content

    init(spacing: CGFloat = 10, @ViewBuilder content: () -> Content) {
        self.spacing = spacing
        self.content = content()
    }

    var body: some View {
        if #available(iOS 26, *) {
            GlassEffectContainer(spacing: spacing) { content }
        } else {
            content
        }
    }
}

/// An icon to tap in a glass bar.
struct GlassIcon: View {
    let icon: String
    let label: LocalizedStringKey
    var tint: Color = .primary
    let action: () -> Void

    var body: some View {
        Button(action: action) { GlassIconLabel(icon: icon, tint: tint) }
            .buttonStyle(.plain)
            .accessibilityLabel(Text(label))
    }
}

/// What a glass bar's icon shows, also for a menu's.
struct GlassIconLabel: View {
    let icon: String
    var tint: Color = .primary

    var body: some View {
        Image(systemName: icon)
            .font(.system(size: 19, weight: .medium))
            .foregroundStyle(tint)
            .frame(width: glassButton, height: glassButton)
            .contentShape(.rect)
    }
}

/// The height of a glass bar's buttons, round ones as wide.
let glassButton: CGFloat = 48

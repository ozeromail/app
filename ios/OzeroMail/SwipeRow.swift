import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// A row that does its swipes as on Android: let go past a quarter of the width, it does the
/// side's short action; past three fifths, the long one; a quick flick does the short one. Short,
/// a drop of the action's colour comes out from under the row; long, the colour floods the strip
/// and the drop rides ahead of the row.
struct SwipeRow<Content: View>: View {
    /// The swipes as they work on `message`.
    let swipes: Swipes
    let message: MessageItem
    let enabled: Bool
    /// The row's own fill, and the list's insets around it: the row covers them as it moves.
    let background: Color
    let insets: EdgeInsets
    let onSwipe: (SwipeAction) -> Void
    @ViewBuilder let content: Content

    @State private var offset: CGFloat = 0
    @State private var start: CGFloat = 0
    @State private var width: CGFloat = 0
    @State private var held = false
    /// The stage let go at once the row acts, so the strip keeps showing it as the row goes.
    @State private var acted: Int?
    @State private var group: CGFloat = 0

    /// The short and long action of a side; a long one that does not apply is no long one.
    private func side(left: Bool) -> (short: SwipeAction, long: SwipeAction?) {
        let (short, long) = left ? (swipes.left, swipes.leftLong) : (swipes.right, swipes.rightLong)
        return (short, long.flatMap { $0.applies(to: message) ? $0 : nil })
    }

    private func opens(left: Bool) -> Bool { enabled && side(left: left).short.applies(to: message) }

    /// 0 while letting go does nothing, 1 for the short action, 2 for the long one.
    private var stage: Int {
        if let acted { return acted }
        guard let at = SwipeSlot.at(offset, width: width) else { return 0 }
        return at.isLong && side(left: at.isLeft).long != nil ? 2 : 1
    }

    var body: some View {
        content
            .background {
                // The row's page, out over the list's insets, so the strip shows only where it has moved from.
                ZStack { Color.ozeroPage; background }
                    .padding(EdgeInsets(top: -insets.top, leading: -insets.leading, bottom: -insets.bottom, trailing: -insets.trailing))
            }
            .offset(x: offset)
            .background {
                if offset != 0 {
                    strip.padding(EdgeInsets(top: -insets.top, leading: -insets.leading, bottom: -insets.bottom, trailing: -insets.trailing))
                }
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.width + insets.leading + insets.trailing } action: { width = $0 }
            .gesture(SidewaysDrag(enabled: enabled, onBegan: began, onChanged: dragged, onEnded: release, onCancelled: springBack))
            .sensoryFeedback(trigger: stage) { old, new in
                guard held else { return nil }
                return new > old ? .impact(weight: .medium) : .selection
            }
            .accessibilityActions {
                ForEach(swipes.actions) { action in
                    if action.applies(to: message) {
                        Button(action.label(for: message)) { onSwipe(action) }
                    }
                }
            }
    }

    private var strip: some View {
        let left = offset < 0
        let (short, long) = side(left: left)
        let far = stage == 2
        let action = far ? long ?? short : short
        let way: CGFloat = left ? -1 : 1
        // Flooded, the drop and its word ride just ahead of the row's edge.
        let ahead = far ? max(0, abs(offset) - group - 30) : 0
        return ZStack(alignment: left ? .trailing : .leading) {
            Rectangle().fill(far ? action.tint : Color.ozeroPage)
            HStack(spacing: 12) {
                if left { word(action, far: far) }
                Image(systemName: action.icon(for: message))
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(stage > 0 ? .white : action.tint)
                    .frame(width: 36, height: 36)
                    .background(far ? Color.white.opacity(0.22) : action.tint.opacity(stage > 0 ? 1 : 0.16), in: .circle)
                    // It grows in as the row uncovers it.
                    .scaleEffect(min(1, max(0, (abs(offset) - 14) / 36)))
                if !left { word(action, far: far) }
            }
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { group = $0 }
            .offset(x: way * ahead)
            .padding(.horizontal, 18)
        }
        .animation(.smooth(duration: 0.2), value: stage)
    }

    private func word(_ action: SwipeAction, far: Bool) -> some View {
        Text(verbatim: action.label(for: message))
            .font(.text.weight(.semibold))
            .foregroundStyle(far ? .white : action.tint)
            .opacity(stage > 0 ? 1 : 0)
    }

    private func began() {
        start = offset
        held = true
    }

    private func dragged(_ by: CGFloat) {
        let to = start + by
        offset = (to > 0 && !opens(left: false)) || (to < 0 && !opens(left: true)) ? 0 : min(max(to, -width), width)
    }

    /// Taken from the finger by the system, as by a call: nothing is done.
    private func springBack() {
        held = false
        withAnimation(.smooth(duration: 0.3)) { offset = 0 }
    }

    private func release(_ velocity: CGFloat) {
        let now = stage
        held = false
        let left = offset < 0
        let (short, long) = side(left: left)
        // A quick flick the row's way does the short swipe, as a slow pull that far would not.
        let flicked = now == 0 && abs(offset) > width * 0.08 && abs(velocity) > 800 && (velocity < 0) == left
        let action: SwipeAction? = now == 2 ? long : now == 1 || flicked ? short : nil
        guard let action, offset != 0 else {
            withAnimation(.smooth(duration: 0.3)) { offset = 0 }
            return
        }
        if action.stays {
            onSwipe(action)
            withAnimation(.smooth(duration: 0.3)) { offset = 0 }
            return
        }
        acted = max(now, 1)
        withAnimation(.smooth(duration: 0.22)) {
            offset = left ? -width : width
        } completion: {
            onSwipe(action)
            // Taken away, the row is gone by then; kept, as a sheet that was closed, it is back in place.
            Task {
                try? await Task.sleep(for: .milliseconds(700))
                var still = Transaction()
                still.disablesAnimations = true
                withTransaction(still) {
                    offset = 0
                    acted = nil
                }
            }
        }
    }
}

/// A sideways drag on a row: it starts only when the finger goes more across than up or down,
/// so the list still scrolls, and leaves the very left edge to the system's swipe back.
private struct SidewaysDrag: UIGestureRecognizerRepresentable {
    let enabled: Bool
    let onBegan: () -> Void
    let onChanged: (CGFloat) -> Void
    let onEnded: (CGFloat) -> Void
    let onCancelled: () -> Void

    func makeUIGestureRecognizer(context: Context) -> SidewaysPan { SidewaysPan() }

    func updateUIGestureRecognizer(_ recognizer: SidewaysPan, context: Context) {
        recognizer.isEnabled = enabled
    }

    func handleUIGestureRecognizerAction(_ recognizer: SidewaysPan, context: Context) {
        switch recognizer.state {
        case .began:
            onBegan()
            onChanged(recognizer.translation(in: recognizer.view).x)
        case .changed: onChanged(recognizer.translation(in: recognizer.view).x)
        case .ended: onEnded(recognizer.velocity(in: recognizer.view).x)
        case .cancelled, .failed: onCancelled()
        default: break
        }
    }
}

final class SidewaysPan: UIPanGestureRecognizer, UIGestureRecognizerDelegate {
    init() {
        super.init(target: nil, action: nil)
        delegate = self
    }

    /// A finger that comes down at the very left edge is the system's swipe back, never the
    /// row's: the row lets go at once, so nothing waits on it.
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)
        if state == .possible, let window = view?.window, let touch = touches.first, touch.location(in: window).x < 24 {
            state = .failed
        }
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        let velocity = velocity(in: view)
        return abs(velocity.x) > abs(velocity.y) * 1.2
    }

    /// The stack the row is in, for its swipes back.
    private var navigation: UINavigationController? {
        var next: UIResponder? = view
        while let responder = next {
            if let controller = responder as? UIViewController { return controller.navigationController }
            next = responder.next
        }
        return nil
    }

    /// The list's scrolling, and the swipe back from anywhere on the page, wait to see whether
    /// the row is swiped.
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
        if other.view is UIScrollView && other is UIPanGestureRecognizer { return true }
        if #available(iOS 26, *) { return other === navigation?.interactiveContentPopGestureRecognizer }
        return false
    }
}

import SwiftUI
import UIKit.UIGestureRecognizerSubclass

/// The note of a list with nothing in it: still water over its title, and under it when it was
/// checked. While there are `checking` words, as the first mail is on its way, they take the
/// place of the title and say what it waits for, and the water swells; it swells too while
/// `refreshing`. `grey` water is for plain folders. A finger on the list, as `touch` tells, draws
/// the surface down under it, the more the further the list is pulled; let go, the water springs
/// back in waves.
struct LakeNote: View {
    let title: LocalizedStringKey
    var grey = false
    var checking: LakeWords?
    var checkedAt: Date?
    var refreshing = false
    var touch: ListTouch?

    @State private var still = Still()

    private var waiting: Bool { checking != nil }

    var body: some View {
        VStack(spacing: 6) {
            WaterCanvas(still: still, grey: grey, touch: touch)
                .frame(height: Lake.height)
            ZStack(alignment: .top) {
                VStack(spacing: 6) {
                    Text(title).font(.heading).multilineTextAlignment(.center)
                    if let checkedAt {
                        // "Just now" turns into a time.
                        TimelineView(.explicit([checkedAt, checkedAt.addingTimeInterval(60)])) { now in
                            Text(now.date.timeIntervalSince(checkedAt) < 60
                                 ? tr("lake_checked_now")
                                 : tr("lake_checked_at", shortDate(Int64(checkedAt.timeIntervalSince1970))))
                        }
                        .font(.small)
                        .foregroundStyle(.secondary)
                        .padding(.top, 4)
                    }
                }
                .opacity(waiting ? 0 : 1)
                // Until then the list is not empty yet, only on its way: these words take its place.
                if let shown = checking {
                    VStack(spacing: 6) {
                        Text(LocalizedStringKey(shown.title)).font(.heading).multilineTextAlignment(.center)
                        if let text = shown.text {
                            Text(LocalizedStringKey(text))
                                .foregroundStyle(.secondary)
                                .multilineTextAlignment(.center)
                                .frame(maxWidth: 280)
                                .padding(.horizontal, 32)
                        }
                    }
                    .transition(.opacity)
                }
            }
            .animation(.easeOut(duration: 0.4), value: checking)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, Lake.top)
        .padding(.bottom, 48)
        .onAppear { still.beat = waiting || refreshing }
        .onChange(of: waiting || refreshing) { _, now in still.beat = now }
        .accessibilityElement(children: .combine)
    }
}

/// What an empty list waits for: the keys of its title and of the line under it, if any.
struct LakeWords: Equatable {
    let title: String
    let text: String?
}

/// Where a finger is on the list, across the screen, and how far the list is pulled down, in
/// pulls. Not observed: the water reads it as it draws, and is woken when it changes.
@MainActor final class ListTouch {
    var x: CGFloat? {
        didSet { if x != oldValue { onChange?() } }
    }
    var pull: CGFloat = 0 {
        didSet { if pull != oldValue { onChange?() } }
    }
    fileprivate var onChange: (() -> Void)?
}

/// Tells a `ListTouch` where the finger on the view is, without taking its gestures from it.
struct TouchTracker: UIGestureRecognizerRepresentable {
    let touch: ListTouch
    /// Only while the lake shows: over rows it would be one more recognizer beside their swipes,
    /// whose open actions then may not close on a tap beside them.
    var enabled = true

    func makeUIGestureRecognizer(context: Context) -> FollowingRecognizer { FollowingRecognizer() }

    func updateUIGestureRecognizer(_ recognizer: FollowingRecognizer, context: Context) {
        recognizer.isEnabled = enabled
        if !enabled, touch.x != nil { touch.x = nil }
    }

    func handleUIGestureRecognizerAction(_ recognizer: FollowingRecognizer, context: Context) {
        switch recognizer.state {
        case .began, .changed: touch.x = context.converter.location(in: .global).x
        default: touch.x = nil
        }
    }
}

/// Follows a finger and never holds anything up: every other gesture goes on as if it were not there.
final class FollowingRecognizer: UIGestureRecognizer, UIGestureRecognizerDelegate {
    init() {
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
        delegate = self
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        if state == .possible { state = .began }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        if state == .began || state == .changed { state = .changed }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        // Only once the last finger is up.
        if event.allTouches?.allSatisfy({ $0.phase == .ended || $0.phase == .cancelled }) ?? true { state = .ended }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .cancelled
    }

    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool { false }

    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool { false }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,
                           shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }
}

/// Sizes of the water, in points.
enum Lake {
    static let height: CGFloat = 124
    static let horizon: CGFloat = 40
    static let top: CGFloat = 8
    /// How far a full pull moves the list: where the system's pull to refresh takes it.
    static let pullGap: CGFloat = 170
}

/// Runs the water's clock: every frame while something moves, and not at all for water at rest.
/// With Reduce Motion on, the water stands still.
private struct WaterCanvas: View {
    let still: Still
    let grey: Bool
    let touch: ListTouch?

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let water = grey ? Color(uiColor: .tertiaryLabel) : Color.ozeroFill
        TimelineView(.animation(paused: reduceMotion || !still.awake)) { timeline in
            Canvas { context, size in
                still.frame(at: timeline.date, frozen: reduceMotion)
                still.draw(in: &context, size: size, water: water, surface: Lake.horizon,
                           finger: touch?.x.map { $0 - still.left }, pull: touch?.pull ?? 0)
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).minX } action: { still.left = $0 }
        .onAppear { touch?.onChange = { [weak still] in still?.wake() } }
    }
}

/// The crests one impact sends out, `e` its energy, `t` seconds since.
private final class Wave {
    let x: Double
    let y: Double
    let e: Double
    var t: Double = 0

    init(x: Double, y: Double, e: Double) {
        self.x = x; self.y = y; self.e = e
    }
}

/// The water's state, stepped in small fixed steps of real physics: a finger draws the surface
/// down, and let go it springs back and sends out crests that slow as they spread. Nothing moves
/// without a cause. Only `awake` is observed; every frame redraws and steps from the clock.
@MainActor @Observable
final class Still {
    /// Whether the clock runs: something on the water moves.
    private(set) var awake = true

    /// A wait: the water swells until it is over.
    var beat = false {
        didSet { wake() }
    }

    /// Where the water starts across the screen, as the finger is told.
    @ObservationIgnored var left: CGFloat = 0

    @ObservationIgnored private var sleeps = 0
    @ObservationIgnored private var now: Double = 0
    @ObservationIgnored private var begun: Double = -1
    @ObservationIgnored private var waves: [Wave] = []
    @ObservationIgnored private var frozen = false
    @ObservationIgnored private var stepped: Double = -1
    @ObservationIgnored private var sincePulse: Double = 0
    @ObservationIgnored private var sea: Double = 0
    @ObservationIgnored private var calm: Double = 0
    @ObservationIgnored private var horizon: Double = 0
    @ObservationIgnored private var middle: Double = 0
    // Where a finger holds the water down, across, and how deep; nil with no finger.
    @ObservationIgnored private var holdX: Double?
    @ObservationIgnored private var holdDepth: Double = 0
    // The surface along the horizon: how far each point across is pressed down, and how fast it
    // moves. A finger pushes it; a wave runs along it, comes back off the banks and dies away.
    // Made as it first draws: a view may make one and never show it.
    @ObservationIgnored private var height: [Double] = []
    @ObservationIgnored private var speed: [Double] = []
    // Earlier shapes of it, so a nearer streak sees a wave a moment later, as if it came closer.
    @ObservationIgnored private var past: [[Double]] = []
    @ObservationIgnored private var pastAt = 0
    @ObservationIgnored private var pastTick = 0
    @ObservationIgnored private var rough: Double = 0
    @ObservationIgnored private var moving: Double = 0
    @ObservationIgnored private var crests: [(x: Double, y: Double, r: Double, amp: Double, depth: Double)] = []
    @ObservationIgnored private var ys: [Double] = []
    @ObservationIgnored private var glints: [Double] = []
    @ObservationIgnored private var soft: [Double] = []

    /// Whether the surface is level and no crest runs over it, so lines are drawn straight.
    private var flat: Bool { sea == 0 && settled && waves.isEmpty && holdX == nil }

    /// Whether the surface has stopped moving.
    private var settled: Bool { rough < 0.05 && moving < 0.01 }

    /// Whether something on the water is moving, so every frame counts.
    private var busy: Bool { beat || !flat }

    /// Wakes the clock of water at rest; the next frame starts afresh rather than a step late.
    func wake() {
        // A sleep on its way is called off.
        sleeps += 1
        if !awake {
            stepped = -1
            awake = true
        }
    }

    /// The clock for this frame. Water that never moves, for Reduce Motion, is level at once.
    func frame(at date: Date, frozen: Bool) {
        let ms = date.timeIntervalSinceReferenceDate * 1000
        if begun < 0 { begun = ms }
        now = ms
        self.frozen = frozen
        // Still water holds no dip.
        if frozen { holdX = nil }
    }

    /// A finger at `x`, the list pulled `pull` pulls down, or none: the surface under it follows.
    private func press(_ x: Double?, pull: Double) {
        guard !frozen, middle > 0 else { return }
        guard let x else {
            if let held = holdX {
                // Let go: the water springs back, and a ring goes out from where it was held.
                waves.append(Wave(x: held, y: horizon + 6, e: min(1, 0.25 + holdDepth / holdDeepest)))
                holdX = nil
                holdDepth = 0
            }
            return
        }
        if holdX == nil { waves.append(Wave(x: x, y: horizon + 6, e: 0.3)) }
        holdX = x
        // Deeper with the pull, against a growing tension.
        let p = max(0, pull)
        holdDepth = holdTouch + (holdDeepest - holdTouch) * (1 - 1 / (1 + p * 1.4))
    }

    private func step(_ dt: Double) {
        // A wait swells the water; a faint ring now and then comes from its middle. When the wait
        // is over, the swell lies down.
        let swelling = beat && !frozen
        sea += ((swelling ? 1 : 0) - sea) * min(1, dt / seaEase)
        if !swelling && sea < 0.002 { sea = 0 }
        sincePulse = swelling ? sincePulse + dt : 0
        if sincePulse > pulseEvery {
            sincePulse -= pulseEvery
            waves.append(Wave(x: middle, y: horizon + 40, e: 0.16))
        }
        flow(dt)
        var energy: Double = 0
        waves.removeAll { w in
            w.t += dt
            let a = w.e * exp(-w.t / 1.5)
            energy += a
            return a < 0.015
        }
        calm += (min(1, energy) - calm) * min(1, dt * 2)
    }

    /// One step of the surface: a damped wave between two banks, held under a finger, thicker once a wait is over.
    private func flow(_ dt: Double) {
        if settled && holdX == nil { return }
        let damping = beat ? 0.9 : 1.5
        for i in 0..<span {
            let bend = height[i > 0 ? i - 1 : 1] - 2 * height[i] + height[i < span - 1 ? i + 1 : span - 2]
            var force = stiff * bend - damping * speed[i]
            if let held = holdX {
                // The finger draws the water toward where it holds it, and lets the rest be.
                let d = middle - banks + Double(i) * spacing - held
                let hold = exp(-d * d / (2 * holdWidth * holdWidth))
                force += hold * (holdPull * (holdDepth - height[i]) - holdDamp * speed[i])
            }
            speed[i] += force * dt
        }
        var fast: Double = 0
        rough = 0
        for i in 0..<span {
            height[i] += speed[i] * dt
            fast = max(fast, abs(speed[i]))
            rough = max(rough, abs(height[i]))
        }
        moving += (min(1, fast / 25) - moving) * min(1, dt * 4)
        if settled && holdX == nil {
            // Level again, past shapes too, so the next wave starts from still water.
            for i in 0..<span {
                height[i] = 0
                speed[i] = 0
            }
            for k in 0..<pastCount { past[k] = [Double](repeating: 0, count: span) }
            moving = 0
            return
        }
        pastTick += 1
        if pastTick % 2 == 0 {
            pastAt = (pastAt + 1) % pastCount
            past[pastAt] = height
        }
    }

    /// How far into perspective the crests of `w` lie: 0 at the horizon, 1 near the viewer.
    private func depthOf(_ w: Wave) -> Double { min(1, max(0, (w.y - horizon) / 220)) }

    /// How far crest `k` of `w` has spread: they slow as they go.
    private func radius(_ w: Wave, _ k: Int) -> Double {
        64 * w.e.squareRoot() * (1 - exp(-w.t / 0.85)) + 7 * w.t - Double(k) * (6 + 5 * w.t)
    }

    /// How strong crest `k` of `w` is at `r`: it fades with time and distance, and comes in at once.
    private func strength(_ w: Wave, _ k: Int, _ r: Double) -> Double {
        w.e * exp(-w.t / 1.5) * (1 - Double(k) * 0.32) / (1 + r / 40).squareRoot() * min(1, w.t * 12)
    }

    /// Gathers this frame's crests: centre, radius, strength and depth.
    private func gatherCrests() {
        crests.removeAll(keepingCapacity: true)
        for w in waves {
            let depth = depthOf(w)
            for k in 0...2 {
                let r = radius(w, k)
                if r <= 0.5 { continue }
                let amp = strength(w, k, r)
                if amp < 0.01 { continue }
                crests.append((w.x, w.y, r, amp, depth))
            }
        }
    }

    /// How far the crests running through lift the water at `x`, `y`: a crest up, a trough after it.
    private func crest(_ x: Double, _ y: Double) -> Double {
        var lift: Double = 0
        for c in crests {
            let dx = x - c.x
            let dy = (y - c.y) / (0.13 + 0.3 * c.depth)
            let d = (dx * dx + dy * dy).squareRoot() / (1 + 0.8 * c.depth) - c.r
            // Far from its ring a crest lifts nothing worth the reckoning.
            if d > 8 || d < -12 { continue }
            lift -= 2.6 * c.amp * (exp(-d * d / 8) - 0.6 * exp(-(d + 4) * (d + 4) / 8))
        }
        return lift
    }

    /// How far the surface is pressed down at `x` on a line `near` the viewer, 0 at the horizon.
    private func rise(_ x: Double, _ near: Double, _ t: Double) -> Double {
        let shape = near > 0 ? past[(pastAt - Int(near * 12) + pastCount * 2) % pastCount] : height
        let at = min(Double(span - 1), max(0, (x - (middle - banks)) / spacing))
        let i = min(Int(at), span - 2)
        let r = at - Double(i)
        var y = (shape[i] * (1 - r) + shape[i + 1] * r) * (1 + 0.7 * near)
        if sea > 0 {
            for s in swells {
                y += s.amp * (1.6 + 1.3 * near) * sea * sin(s.dir * s.k * (x - middle) - s.w * t + s.phase + near * 1.4)
            }
        }
        return y
    }

    /// Steps the water up to now.
    private func advance() {
        var dt = stepped < 0 ? 0 : min(0.05, (now - stepped) / 1000)
        stepped = now
        while dt > 0 {
            let s = min(dt, 1.0 / 120)
            step(s)
            dt -= s
        }
        // Not while the frame draws: the observed value changes right after it.
        if !busy && awake {
            let asked = sleeps
            Task { @MainActor in
                if self.awake && !self.busy && self.sleeps == asked { self.awake = false }
            }
        }
    }

    /// The horizon lies `surface` down and the water fills the rest; a finger at `finger` across,
    /// the list pulled `pull` pulls, draws it down.
    func draw(in context: inout GraphicsContext, size: CGSize, water: Color, surface: CGFloat, finger: CGFloat?, pull: CGFloat) {
        if height.isEmpty {
            height = [Double](repeating: 0, count: span)
            speed = height
            past = [[Double]](repeating: height, count: pastCount)
            ys = [Double](repeating: 0, count: linePoints)
            glints = ys
            soft = ys
        }
        let hz = Double(surface)
        horizon = hz
        middle = size.width / 2
        let bottom = Double(size.height)
        press(finger.map { Double($0) }, pull: Double(pull))
        advance()
        gatherCrests()
        let t = (now - begun) / 1000
        let stroke: CGFloat = 1
        var below = context
        below.clip(to: Path(CGRect(x: 0, y: hz, width: size.width, height: max(0, bottom - hz))))
        // The reflection: a soft glow under the horizon, flattened by perspective, and a few
        // streaks that follow the surface. It breathes deeper while the water swells.
        let r = 150.0
        // Never deeper than the water, so the glow fades out before it ends.
        let flatten = min(r * 0.32 * (1 + 0.14 * sea * sin(t * 1.3)), max(1, bottom - hz))
        // Eased, so the glow has no rim where it ends.
        let strength = 0.14 + 0.04 * sea
        let glow = Gradient(stops: [
            .init(color: water.opacity(strength), location: 0),
            .init(color: water.opacity(strength * 0.6), location: 0.35),
            .init(color: water.opacity(strength * 0.2), location: 0.7),
            .init(color: water.opacity(0), location: 1),
        ])
        var lit = below
        lit.translateBy(x: middle, y: hz)
        lit.scaleBy(x: 1, y: flatten / r)
        lit.fill(
            Path(ellipseIn: CGRect(x: -r, y: -r, width: r * 2, height: r * 2)),
            with: .radialGradient(glow, center: .zero, startRadius: 0, endRadius: r))
        // A crest passing through bends them, as a finger does.
        for (i, s) in streaks.enumerated() {
            waterline(&below, alpha: s.alpha, x: middle, half: s.half, y: hz + s.dy, near: Double(i + 1) / 4, t: t, color: water, width: stroke)
        }
        // Waves: two or three crests an impact, slowing as they spread, fading with distance. Each
        // point of a crest rides the swell, and it shows only where it faces the eye.
        for c in crests where c.amp >= 0.02 {
            let near = 0.4 + 0.6 * c.depth
            let rx = c.r * (1 + 0.8 * c.depth)
            let ry = rx * (0.13 + 0.3 * c.depth)
            let n = max(24, Int(rx / 2))
            var px: Double = 0
            var py: Double = 0
            for i in 0...n {
                let a = Double(i) * 2 * .pi / Double(n)
                let x = c.x + rx * cos(a)
                let y = c.y + ry * sin(a) + rise(x, near, t)
                if i > 0 {
                    let face = sin(a - .pi / Double(n))
                    // The near side catches more light than the far one.
                    let alpha = 0.5 * c.amp * abs(face * face * face) * (face > 0 ? 1 : 0.55) * edge(x, y, below: bottom)
                    if alpha > 0.008 {
                        var line = Path()
                        line.move(to: CGPoint(x: px, y: py))
                        line.addLine(to: CGPoint(x: x, y: y))
                        below.stroke(line, with: .color(water.opacity(min(0.42, alpha))), lineWidth: stroke)
                    }
                }
                px = x
                py = y
            }
        }
        waterline(&context, alpha: 0.55, x: middle, half: 150, y: hz, near: 0, t: t, color: water, width: stroke)
    }

    /// How much of the water shows at `x`, `y`: all of it in the middle, fading out toward its sides
    /// and toward the eye before it ends `below`, so a wave never ends on an edge.
    private func edge(_ x: Double, _ y: Double, below: Double) -> Double {
        let side = min(x, middle * 2 - x) / edgeSide
        let near = (below - y) / edgeNear
        let v = min(max(min(side, near), 0), 1)
        return v * v * (3 - 2 * v)
    }

    /// A hairline on the water, `half` to each side of `x` at `y`, on a line `near` the viewer:
    /// straight while the water is level; otherwise it follows the surface and lights up where a
    /// wave's face turns to the eye.
    private func waterline(_ context: inout GraphicsContext, alpha: Double, x: Double, half: Double, y: Double,
                           near: Double, t: Double, color: Color, width: CGFloat) {
        let n = min(linePoints - 1, Int(half * 2 / spacing))
        if flat || n < 4 { return faded(&context, alpha: alpha, x: x, half: half, y: y, color: color, width: width) }
        let gap = half * 2 / Double(n)
        for i in 0...n { ys[i] = y + rise(x - half + Double(i) * gap, near, t) + crest(x - half + Double(i) * gap, y) }
        // The slope over a few points, so light catches whole faces of a wave, not single points.
        let facing = 0.06 + 0.05 * near
        let light = min(1, sea * 2 + calm + moving)
        for i in 0...n {
            let l = max(0, i - 2)
            let r = min(n, i + 2)
            let d = (abs(ys[r] - ys[l]) / (Double(r - l) * gap) - facing) / 0.05
            glints[i] = light < 0.01 ? 0 : exp(-d * d) * light
        }
        // Softened twice, through a second buffer so it stays even.
        blur(from: glints, into: &soft, n)
        blur(from: soft, into: &glints, n)
        // The line fades out at both ends, as a straight one does, with glints over it.
        let base = alpha * (1 - 0.45 * sea)
        let bright = min(0.42, alpha * 2.6)
        for i in 1...n {
            let x0 = x - half + Double(i - 1) * gap
            let fade = max(0, 1 - abs(x0 + gap / 2 - x) / half)
            let a = base * fade + bright * (glints[i] + glints[i - 1]) / 2 * fade.squareRoot()
            if a < 0.004 { continue }
            var line = Path()
            line.move(to: CGPoint(x: x0, y: ys[i - 1]))
            line.addLine(to: CGPoint(x: x0 + gap, y: ys[i]))
            context.stroke(line, with: .color(color.opacity(min(1, a))), lineWidth: width)
        }
    }

    /// Softens `from` into `into`, over points 0 to `n`; the ends stay.
    private func blur(from: [Double], into: inout [Double], _ n: Int) {
        into[0] = from[0]
        into[n] = from[n]
        for i in 1..<n { into[i] = (from[i - 1] + from[i] * 2 + from[i + 1]) / 4 }
    }

    /// A hairline `half` to each side of `x`, fading out at both ends.
    private func faded(_ context: inout GraphicsContext, alpha: Double, x: Double, half: Double, y: Double, color: Color, width: CGFloat) {
        guard half >= 0.5 else { return }
        var line = Path()
        line.move(to: CGPoint(x: x - half, y: y))
        line.addLine(to: CGPoint(x: x + half, y: y))
        context.stroke(line, with: .linearGradient(
            Gradient(colors: [color.opacity(0), color.opacity(alpha), color.opacity(0)]),
            startPoint: CGPoint(x: x - half, y: y), endPoint: CGPoint(x: x + half, y: y)), lineWidth: width)
    }
}

/// A reflection streak, under the horizon.
private let streaks: [(dy: Double, half: Double, alpha: Double)] = [
    (9, 104, 0.1), (20, 84, 0.075), (35, 62, 0.055), (55, 40, 0.035),
]

/// How far from its sides, and from where it ends below, the water fades out.
private let edgeSide = 48.0
private let edgeNear = 28.0

/// How slowly a wait's swell comes and goes, in seconds, and how often a faint ring goes out.
private let seaEase: Double = 0.72
private let pulseEvery: Double = 2.6

/// The surface: points across, the space between them, a wave's speed along it, its banks from the middle.
private let span = 101
private let spacing: Double = 3
private let waveSpeed: Double = 150
private let banks: Double = 150
private let stiff = waveSpeed * waveSpeed / (spacing * spacing)

/// A finger on the water: how deep a touch alone draws it down and the deepest a pull does; how
/// wide the dip is; and how firmly it holds the surface there, and calms it.
private let holdTouch: Double = 2
private let holdDeepest: Double = 18
private let holdWidth: Double = 22
private let holdPull: Double = 260
private let holdDamp: Double = 14

/// Earlier shapes kept for nearer streaks, one every two steps.
private let pastCount = 20
private let linePoints = 104

/// A swell's wave, by the deep-water law ω = √(g·k): longer waves run faster.
private struct Swell {
    let amp: Double
    let dir: Double
    let phase: Double
    let k: Double
    let w: Double

    init(length: Double, amp: Double, dir: Double, phase: Double) {
        self.amp = amp
        self.dir = dir
        self.phase = phase
        k = 2 * .pi / length
        w = (118 * k).squareRoot()
    }
}

private let swells = [
    Swell(length: 150, amp: 1.25, dir: 1, phase: 0),
    Swell(length: 96, amp: 0.85, dir: -1, phase: 1.7),
    Swell(length: 60, amp: 0.5, dir: 1, phase: 4.1),
    Swell(length: 40, amp: 0.22, dir: -1, phase: 2.6),
]

import SwiftUI

/// The words of a list with nothing in it. While there are `checking` words, as the first mail is
/// on its way, they take the place of the title and say what it waits for. `checkedAt` shows under
/// the words. It is the list's content: a pull's water is over the title (`PullWater`), as on a
/// list with mail.
struct LakeNote: View {
    let title: LocalizedStringKey
    var checking: LakeWords?
    var checkedAt: Date?

    /// The checking words, kept while they fade out.
    @State private var shownChecking: LakeWords?

    private var waiting: Bool { checking != nil }

    var body: some View {
        ZStack(alignment: .top) {
            VStack(spacing: 6) {
                words(title, nil)
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
            .offset(y: waiting ? 8 : 0)
            if let shown = checking ?? shownChecking {
                VStack(spacing: 6) { words(LocalizedStringKey(shown.title), shown.text.map { LocalizedStringKey($0) }) }
                    .opacity(waiting ? 1 : 0)
                    .offset(y: waiting ? 0 : 8)
            }
        }
        .animation(.easeOut(duration: 0.4), value: waiting)
        .onAppear { shownChecking = checking }
        .onChange(of: checking) { _, now in if let now { shownChecking = now } }
        .frame(maxWidth: .infinity)
        // Where they stood under the lake that was here, so an empty screen stays as it was.
        .padding(.top, 138)
        .padding(.bottom, 48)
        .accessibilityElement(children: .combine)
    }

    /// A title, and a second line only when it tells something: a slow wait.
    private func words(_ title: LocalizedStringKey, _ text: LocalizedStringKey?) -> some View {
        Group {
            Text(title).font(.heading).multilineTextAlignment(.center)
            if let text {
                Text(text)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 280)
                    .padding(.horizontal, 32)
            }
        }
    }
}

/// What an empty list waits for: the keys of its title and of the line under it, if any.
struct LakeWords: Equatable {
    let title: String
    let text: String?
}

/// Still water in the gap a pull opens over the title: it stays where it is and the list uncovers
/// it. The finger presses its surface down, and as it lets go the surface springs back in waves;
/// a pull the system took sends a ring across it too, on each of `rings`.
struct PullWater: View {
    var grey = false
    let pull: PullState

    @State private var still = Still()

    var body: some View {
        WaterCanvas(still: still, grey: grey, pressed: pull.fraction, bottom: max(pull.gap, Lake.pullGap))
            .onChange(of: pull.rings) { _, _ in still.ring() }
    }
}

/// How the list is pulled: how far, in pulls (1 is where the system takes it), how tall the gap
/// over the title is, and the pulls the system took, as they are let go.
struct PullState: Equatable {
    var fraction: CGFloat = 0
    var gap: CGFloat = 0
    var rings = 0
}

/// Sizes of the water, in points.
enum Lake {
    /// How far down the gap the horizon lies.
    static let horizon: CGFloat = 28
    /// How far a full pull opens the list: where the system's pull to refresh takes it.
    static let pullGap: CGFloat = 170
    /// How tall the system's large title is, over the list, where it cannot be found on screen.
    static let largeTitle: CGFloat = 52
}

/// Runs the water's clock: every frame while something moves, and not at all for water at rest.
/// With Reduce Motion on, the water stands still.
private struct WaterCanvas: View {
    let still: Still
    let grey: Bool
    let pressed: CGFloat
    let bottom: CGFloat

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let water = grey ? Color(uiColor: .tertiaryLabel) : Color.ozeroFill
        TimelineView(.animation(paused: reduceMotion || !still.awake)) { timeline in
            Canvas { context, size in
                still.frame(at: timeline.date)
                still.draw(in: &context, size: size, water: water, surface: Lake.horizon, bottom: bottom,
                           pressed: reduceMotion ? 0 : pressed)
            }
        }
        .onChange(of: pressed) { _, _ in still.wake() }
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

/// The water's state, stepped in small fixed steps of real physics: a pressed surface springs back
/// in waves that run to the banks and die away, and a ring's crests slow as they spread. Nothing
/// moves without a cause. Only `awake` is observed; every frame redraws and steps from the clock.
@MainActor @Observable
final class Still {
    /// Whether the clock runs: something on the water moves.
    private(set) var awake = true

    @ObservationIgnored private var now: Double = 0
    @ObservationIgnored private var begun: Double = -1
    @ObservationIgnored private var waves: [Wave] = []
    @ObservationIgnored private var stepped: Double = -1
    @ObservationIgnored private var pressed: Double = 0
    @ObservationIgnored private var calm: Double = 0
    @ObservationIgnored private var horizon: Double = 0
    @ObservationIgnored private var middle: Double = 0
    // The surface along the horizon: how far each point across is lifted, and how fast it moves.
    // The finger holds it pressed; let go, a wave runs along it, comes back off the banks and dies away.
    @ObservationIgnored private var height = [Double](repeating: 0, count: span)
    @ObservationIgnored private var speed = [Double](repeating: 0, count: span)
    // Earlier shapes of it, so a nearer streak sees a wave a moment later, as if it came closer.
    @ObservationIgnored private var past = [[Double]](repeating: [Double](repeating: 0, count: span), count: pastCount)
    @ObservationIgnored private var pastAt = 0
    @ObservationIgnored private var pastTick = 0
    @ObservationIgnored private var rough: Double = 0
    @ObservationIgnored private var moving: Double = 0
    @ObservationIgnored private var crests: [(x: Double, y: Double, r: Double, amp: Double, depth: Double)] = []
    @ObservationIgnored private var ys = [Double](repeating: 0, count: linePoints)
    @ObservationIgnored private var glints = [Double](repeating: 0, count: linePoints)
    @ObservationIgnored private var soft = [Double](repeating: 0, count: linePoints)

    /// Whether the surface is level and no crest runs over it, so lines are drawn straight.
    private var flat: Bool { pressed == 0 && settled && waves.isEmpty }

    /// Whether the surface has stopped moving.
    private var settled: Bool { rough < 0.05 && moving < 0.01 }

    /// Whether something on the water is moving, so every frame counts.
    private var busy: Bool { pressed > 0 || !waves.isEmpty || !flat }

    /// Wakes the clock of water at rest; the next frame starts afresh rather than a step late.
    func wake() {
        if !awake {
            stepped = -1
            awake = true
        }
    }

    /// One ring from the middle, across the water.
    func ring() {
        guard middle > 0 else { return }
        waves.append(Wave(x: middle, y: horizon + 10, e: 0.5))
        wake()
    }

    /// The clock for this frame.
    func frame(at date: Date) {
        let ms = date.timeIntervalSinceReferenceDate * 1000
        if begun < 0 { begun = ms }
        now = ms
    }

    private func step(_ dt: Double) {
        if pressed > 0 { press() } else { flow(dt) }
        var energy: Double = 0
        waves.removeAll { w in
            w.t += dt
            let a = w.e * exp(-w.t / 1.5)
            energy += a
            return a < 0.015
        }
        calm += (min(1, energy) - calm) * min(1, dt * 2)
    }

    /// The finger holds the surface pressed in the middle, deeper as it pulls, less and less past
    /// where the system takes the pull. Water is not lost: what is pressed down rises around it.
    private func press() {
        let q = pressed <= 1 ? pressed : 1 + 0.3 * (1 - 1 / pressed)
        var mean: Double = 0
        for i in 0..<span {
            let d = Double(i - span / 2) * spacing
            height[i] = pressDepth * q * exp(-d * d / (2 * pressWidth * pressWidth))
            speed[i] = 0
            mean += height[i]
        }
        mean /= Double(span)
        rough = 0
        for i in 0..<span {
            height[i] -= mean
            rough = max(rough, abs(height[i]))
        }
        moving = 0.02
        remember()
    }

    /// One step of the surface: a damped wave between two banks.
    private func flow(_ dt: Double) {
        if settled { return }
        for i in 0..<span {
            let bend = height[i > 0 ? i - 1 : 1] - 2 * height[i] + height[i < span - 1 ? i + 1 : span - 2]
            speed[i] += (stiff * bend - damping * speed[i]) * dt
        }
        var fast: Double = 0
        rough = 0
        for i in 0..<span {
            height[i] += speed[i] * dt
            fast = max(fast, abs(speed[i]))
            rough = max(rough, abs(height[i]))
        }
        moving += (min(1, fast / 25) - moving) * min(1, dt * 4)
        if settled {
            // Level again, past shapes too, so the next wave starts from still water.
            for i in 0..<span {
                height[i] = 0
                speed[i] = 0
            }
            for k in 0..<pastCount { past[k] = [Double](repeating: 0, count: span) }
            moving = 0
            return
        }
        remember()
    }

    /// Keeps the surface's shape now and then, for the nearer streaks.
    private func remember() {
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

    /// How far the surface is lifted at `x` on a line `near` the viewer, 0 at the horizon.
    private func rise(_ x: Double, _ near: Double) -> Double {
        let shape = near > 0 ? past[(pastAt - Int(near * 12) + pastCount * 2) % pastCount] : height
        let at = min(Double(span - 1), max(0, (x - (middle - banks)) / spacing))
        let i = min(Int(at), span - 2)
        let r = at - Double(i)
        return (shape[i] * (1 - r) + shape[i + 1] * r) * (1 + 0.7 * near)
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
            Task { @MainActor in
                if self.awake && !self.busy { self.awake = false }
            }
        }
    }

    /// The horizon lies `surface` down, pressed as far as `pressed` pulls; the water ends at `bottom`.
    func draw(in context: inout GraphicsContext, size: CGSize, water: Color, surface: CGFloat,
              bottom: CGFloat, pressed: CGFloat) {
        let hz = Double(surface)
        horizon = hz
        middle = size.width / 2
        self.pressed = Double(pressed)
        advance()
        gatherCrests()
        let stroke: CGFloat = 1
        var below = context
        below.clip(to: Path(CGRect(x: 0, y: hz, width: size.width, height: max(0, bottom - hz))))
        // The reflection: a soft glow under the horizon, flattened by perspective, and a few
        // streaks that follow the surface.
        let r: Double = 151
        // Never deeper than the water, so the glow fades out before it ends.
        let flatten = min(r * 0.32, max(1, Double(bottom) - hz))
        // Eased, so the glow has no rim where it ends.
        let glow = Gradient(stops: [
            .init(color: water.opacity(0.14), location: 0),
            .init(color: water.opacity(0.084), location: 0.35),
            .init(color: water.opacity(0.028), location: 0.7),
            .init(color: water.opacity(0), location: 1),
        ])
        var lit = below
        lit.translateBy(x: middle, y: hz)
        lit.scaleBy(x: 1, y: flatten / r)
        lit.fill(
            Path(ellipseIn: CGRect(x: -r, y: -r, width: r * 2, height: r * 2)),
            with: .radialGradient(glow, center: .zero, startRadius: 0, endRadius: r))
        // A crest passing through bends them; nothing else moves them.
        for (i, s) in streaks.enumerated() {
            waterline(&below, alpha: s.alpha, x: middle, half: s.half, y: hz + s.dy, near: Double(i + 1) / 4, color: water, width: stroke)
        }
        // Waves: two or three crests an impact, slowing as they spread, fading with distance. Each
        // point of a crest rides the surface, and it shows only where it faces the eye.
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
                let y = c.y + ry * sin(a) + rise(x, near)
                if i > 0 {
                    let face = sin(a - .pi / Double(n))
                    // The near side catches more light than the far one.
                    let alpha = 0.5 * c.amp * abs(face * face * face) * (face > 0 ? 1 : 0.55) * edge(x, y, below: Double(bottom))
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
        waterline(&context, alpha: 0.55, x: middle, half: 150, y: hz, near: 0, color: water, width: stroke)
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
                           near: Double, color: Color, width: CGFloat) {
        let n = min(linePoints - 1, Int(half * 2 / spacing))
        if flat || n < 4 { return faded(&context, alpha: alpha, x: x, half: half, y: y, color: color, width: width) }
        let gap = half * 2 / Double(n)
        for i in 0...n { ys[i] = y + rise(x - half + Double(i) * gap, near) + crest(x - half + Double(i) * gap, y) }
        // The slope over a few points, so light catches whole faces of a wave, not single points.
        let facing = 0.06 + 0.05 * near
        let light = min(1, calm + moving)
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
        let bright = min(0.42, alpha * 2.6)
        for i in 1...n {
            let x0 = x - half + Double(i - 1) * gap
            let fade = max(0, 1 - abs(x0 + gap / 2 - x) / half)
            let a = alpha * fade + bright * (glints[i] + glints[i - 1]) / 2 * fade.squareRoot()
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

/// How deep, in points, a full pull presses the surface, and how wide the press is.
private let pressDepth: Double = 7
private let pressWidth: Double = 34

/// The surface: points across, the space between them, a wave's speed along it, its banks from the
/// middle, and how fast a wave dies away.
private let span = 101
private let spacing: Double = 3
private let waveSpeed: Double = 150
private let banks: Double = 150
private let stiff = waveSpeed * waveSpeed / (spacing * spacing)
private let damping: Double = 1.5

/// Earlier shapes kept for nearer streaks, one every two steps.
private let pastCount = 20
private let linePoints = 104

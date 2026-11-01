import SwiftUI

/// The words of a list with nothing in it, right under the list's surface. While there are
/// `checking` words, as the first mail is on its way, they take the place of the title and say
/// what it waits for. `checkedAt` shows under the words. With `drawIn`, as the last message leaves,
/// they come in once the surface's wave has gone out; otherwise they are just there.
struct LakeNote: View {
    let title: LocalizedStringKey
    var drawIn = false
    var checking: LakeWords?
    var checkedAt: Date?

    /// The checking words, kept while they fade out.
    @State private var shownChecking: LakeWords?
    @State private var shown: Bool

    private var waiting: Bool { checking != nil }

    init(title: LocalizedStringKey, drawIn: Bool = false, checking: LakeWords? = nil, checkedAt: Date? = nil) {
        self.title = title
        self.drawIn = drawIn
        self.checking = checking
        self.checkedAt = checkedAt
        // Only as it first shows: the caller's flag turns off right after.
        _shown = State(initialValue: !drawIn)
    }

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
            .opacity(waiting || !shown ? 0 : 1)
            .offset(y: waiting || !shown ? 8 : 0)
            if let shown = checking ?? shownChecking {
                VStack(spacing: 6) { words(LocalizedStringKey(shown.title), shown.text.map { LocalizedStringKey($0) }) }
                    .opacity(waiting ? 1 : 0)
                    .offset(y: waiting ? 0 : 8)
            }
        }
        .animation(.easeOut(duration: 0.4), value: waiting)
        .onAppear {
            shownChecking = checking
            if !shown { withAnimation(.easeOut(duration: 0.4).delay(0.45)) { shown = true } }
        }
        .onChange(of: checking) { _, now in if let now { shownChecking = now } }
        .frame(maxWidth: .infinity)
        .padding(.top, 20)
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

/// The first row of every list, with mail or none: still water under the title. At rest only its
/// horizon, a quiet line. It is content, so it rides down with the title on a pull and scrolls away
/// with it. As the list is pulled its reflection comes in under the horizon and the finger bends
/// its surface; let go, the bend goes out as a wave. It swells while `refreshing`, and each of
/// `waves` sends one out, as the last message leaves. Its height never changes: a row that grew
/// under the finger would fight the system's pull, and a row's drawing ends at its edges.
struct Surface: View {
    var grey = false
    let pull: PullState
    let refreshing: Bool
    let waves: Int

    @State private var still = Still()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        WaterCanvas(still: still, grey: grey, pressed: reduceMotion ? 0 : pull.fraction,
                    shown: reduceMotion ? 0 : min(1, pull.gap / Lake.reveal))
            .frame(height: Lake.row)
            .onAppear { still.beat = refreshing && !reduceMotion }
            .onChange(of: refreshing && !reduceMotion) { _, now in still.beat = now }
            .onChange(of: waves) { _, _ in if !reduceMotion { still.wave(0.8) } }
    }
}

/// How the list is pulled: how far, in pulls (1 is where the system takes it), and how far down.
struct PullState: Equatable {
    var fraction: CGFloat = 0
    var gap: CGFloat = 0
}

/// Sizes of the water, in points.
enum Lake {
    /// The surface's row, and its horizon from the row's top.
    static let row: CGFloat = 28
    static let horizon: CGFloat = 8
    /// How far a full pull opens the list: where the system's pull to refresh takes it.
    static let pullGap: CGFloat = 170
    /// How far a pull goes before the reflection is all in.
    static let reveal: CGFloat = 60
}

/// Runs the water's clock: every frame while something moves, and not at all for water at rest.
private struct WaterCanvas: View {
    let still: Still
    let grey: Bool
    let pressed: CGFloat
    let shown: CGFloat

    var body: some View {
        let water = grey ? Color(uiColor: .tertiaryLabel) : Color.ozeroFill
        TimelineView(.animation(paused: !still.awake)) { timeline in
            Canvas { context, size in
                still.frame(at: timeline.date)
                still.draw(in: &context, size: size, water: water, shown: shown, pressed: pressed)
            }
        }
        .onChange(of: pressed) { _, _ in still.wake() }
        .onChange(of: shown) { _, _ in still.wake() }
    }
}

/// The water's state, stepped in small fixed steps of real physics: a bent surface springs back in
/// waves that run to the banks and die away, and it swells through a wait. Nothing moves without a
/// cause. Only `awake` is observed; every frame redraws and steps from the clock.
@MainActor @Observable
final class Still {
    /// Whether the clock runs: something on the water moves.
    private(set) var awake = true

    /// A wait: the water swells until it is over.
    var beat = false {
        didSet { wake() }
    }

    @ObservationIgnored private var now: Double = 0
    @ObservationIgnored private var begun: Double = -1
    @ObservationIgnored private var stepped: Double = -1
    @ObservationIgnored private var pressed: Double = 0
    @ObservationIgnored private var sea: Double = 0
    @ObservationIgnored private var middle: Double = 0
    // The surface along the horizon: how far each point across is lifted, and how fast it moves.
    // The finger holds it bent; let go, a wave runs along it, comes back off the banks and dies away.
    @ObservationIgnored private var height = [Double](repeating: 0, count: span)
    @ObservationIgnored private var speed = [Double](repeating: 0, count: span)
    // Earlier shapes of it, so a nearer streak sees a wave a moment later, as if it came closer.
    @ObservationIgnored private var past = [[Double]](repeating: [Double](repeating: 0, count: span), count: pastCount)
    @ObservationIgnored private var pastAt = 0
    @ObservationIgnored private var pastTick = 0
    @ObservationIgnored private var rough: Double = 0
    @ObservationIgnored private var moving: Double = 0
    @ObservationIgnored private var ys = [Double](repeating: 0, count: linePoints)
    @ObservationIgnored private var glints = [Double](repeating: 0, count: linePoints)
    @ObservationIgnored private var soft = [Double](repeating: 0, count: linePoints)

    /// Whether the surface is level, so lines are drawn straight.
    private var flat: Bool { pressed == 0 && sea == 0 && settled }

    /// Whether the surface has stopped moving.
    private var settled: Bool { rough < 0.05 && moving < 0.01 }

    /// Whether something on the water is moving, so every frame counts.
    private var busy: Bool { beat || !flat }

    /// Wakes the clock of water at rest; the next frame starts afresh rather than a step late.
    func wake() {
        if !awake {
            stepped = -1
            awake = true
        }
    }

    /// One wave, as if a pull of `fraction` had bent the surface and let it go.
    func wave(_ fraction: Double) {
        bend(fraction)
        wake()
    }

    /// The clock for this frame.
    func frame(at date: Date) {
        let ms = date.timeIntervalSinceReferenceDate * 1000
        if begun < 0 { begun = ms }
        now = ms
    }

    private func step(_ dt: Double) {
        if pressed > 0 { bend(pressed) } else { flow(dt) }
        sea += ((beat ? 1 : 0) - sea) * min(1, dt / seaEase)
        if !beat && sea < 0.002 { sea = 0 }
    }

    /// Holds the surface bent in the middle, deeper as a pull of `fraction` goes, less and less past
    /// where the system takes the pull. Water is not lost: what is pressed down rises around it.
    private func bend(_ fraction: Double) {
        let q = fraction <= 1 ? fraction : 1 + 0.25 * (1 - 1 / fraction)
        let depth = bendDepth * pow(q, 1.2)
        var mean: Double = 0
        for i in 0..<span {
            let d = Double(i - span / 2) * spacing
            height[i] = depth * exp(-d * d / (2 * bendWidth * bendWidth))
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
        let damping = beat ? 0.9 : 1.5
        for i in 0..<span {
            let curve = height[i > 0 ? i - 1 : 1] - 2 * height[i] + height[i < span - 1 ? i + 1 : span - 2]
            speed[i] += (stiff * curve - damping * speed[i]) * dt
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

    /// How far the surface is lifted at `x` on a line `near` the viewer, 0 at the horizon: the same
    /// on every line, a moment later nearer, so the lines stay in the row.
    private func rise(_ x: Double, _ near: Double, _ t: Double) -> Double {
        let shape = near > 0 ? past[(pastAt - Int(near * 12) + pastCount * 2) % pastCount] : height
        let at = min(Double(span - 1), max(0, (x - (middle - banks)) / spacing))
        let i = min(Int(at), span - 2)
        let r = at - Double(i)
        var y = shape[i] * (1 - r) + shape[i + 1] * r
        if sea > 0 {
            for s in swells {
                y += s.amp * 1.6 * sea * sin(s.dir * s.k * (x - middle) - s.w * t + s.phase + near * 1.4)
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
            Task { @MainActor in
                if self.awake && !self.busy { self.awake = false }
            }
        }
    }

    /// The horizon lies `Lake.horizon` down, its middle bent as far as `pressed` pulls. Under it the
    /// reflection, as far as it is `shown` or the water swells, fades out before the row ends.
    func draw(in context: inout GraphicsContext, size: CGSize, water: Color, shown: CGFloat, pressed: CGFloat) {
        let hz = Double(Lake.horizon)
        middle = size.width / 2
        self.pressed = Double(pressed)
        advance()
        let t = (now - begun) / 1000
        let stroke: CGFloat = 1
        let room = Double(size.height) - hz - 1
        let lit = max(Double(shown), 0.5 * sea)
        if lit > 0.01 {
            // The reflection: a soft glow under the horizon, flattened into the row, breathing while
            // the water swells.
            let r: Double = 151
            let glowing = lit * (1 + 0.14 * sea * sin(t * 1.3))
            let glow = Gradient(stops: [
                .init(color: water.opacity(0.14 * glowing), location: 0),
                .init(color: water.opacity(0.084 * glowing), location: 0.35),
                .init(color: water.opacity(0.028 * glowing), location: 0.7),
                .init(color: water.opacity(0), location: 1),
            ])
            var under = context
            under.clip(to: Path(CGRect(x: 0, y: hz, width: size.width, height: room)))
            under.translateBy(x: middle, y: hz)
            under.scaleBy(x: 1, y: room / r)
            under.fill(
                Path(ellipseIn: CGRect(x: -r, y: -r, width: r * 2, height: r * 2)),
                with: .radialGradient(glow, center: .zero, startRadius: 0, endRadius: r))
            // Only the streaks the row has room for, fading in before its edge.
            for (i, s) in streaks.enumerated() {
                let fit = min(1, max(0, (room + 1 - s.dy - 6) / 10)) * lit
                if fit < 0.01 { continue }
                waterline(&context, alpha: s.alpha * fit, x: middle, half: s.half, y: hz + s.dy, near: Double(i + 1) / 4, t: t,
                          color: water, width: stroke)
            }
        }
        waterline(&context, alpha: 0.55, x: middle, half: 150, y: hz, near: 0, t: t, color: water, width: stroke)
    }

    /// A hairline on the water, `half` to each side of `x` at `y`, on a line `near` the viewer:
    /// straight while the water is level; otherwise it follows the surface and lights up where a
    /// wave's face turns to the eye.
    private func waterline(_ context: inout GraphicsContext, alpha: Double, x: Double, half: Double, y: Double,
                           near: Double, t: Double, color: Color, width: CGFloat) {
        let n = min(linePoints - 1, Int(half * 2 / spacing))
        if flat || n < 4 { return faded(&context, alpha: alpha, x: x, half: half, y: y, color: color, width: width) }
        let gap = half * 2 / Double(n)
        for i in 0...n { ys[i] = y + rise(x - half + Double(i) * gap, near, t) }
        // The slope over a few points, so light catches whole faces of a wave, not single points.
        let facing = 0.06 + 0.05 * near
        let light = min(1, sea * 2 + moving)
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
            let fade = eased(1 - abs(x0 + gap / 2 - x) / half)
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
        // Eased, so no end of it stops short.
        let stops = (0...8).map { i in
            let at = Double(i) / 8
            return Gradient.Stop(color: color.opacity(alpha * eased(1 - abs(at * 2 - 1))), location: at)
        }
        context.stroke(line, with: .linearGradient(
            Gradient(stops: stops), startPoint: CGPoint(x: x - half, y: y), endPoint: CGPoint(x: x + half, y: y)), lineWidth: width)
    }
}

/// Eases `v` from 0 to 1, flat at both ends.
private func eased(_ v: Double) -> Double {
    let c = min(max(v, 0), 1)
    return c * c * (3 - 2 * c)
}

/// A reflection streak, under the horizon.
private let streaks: [(dy: Double, half: Double, alpha: Double)] = [
    (9, 104, 0.1), (20, 84, 0.075), (35, 62, 0.055), (55, 40, 0.035),
]

/// How deep, in points, a full pull bends the surface, and how wide the bend is.
private let bendDepth: Double = 9
private let bendWidth: Double = 52

/// How slowly a wait's swell comes and goes, in seconds.
private let seaEase: Double = 0.72

/// The surface: points across, the space between them, a wave's speed along it, its banks from the middle.
private let span = 101
private let spacing: Double = 3
private let waveSpeed: Double = 150
private let banks: Double = 150
private let stiff = waveSpeed * waveSpeed / (spacing * spacing)

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

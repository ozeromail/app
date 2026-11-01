import SwiftUI

/// The note of a list with nothing left in it: still water. A horizon and a soft reflection under
/// it, at rest until something moves it. With `drawIn`, as the last message leaves, a drop lands on
/// the water and the horizon comes out of its waves. A touch sends a wave out too. `grey` water is
/// for plain empty folders: it never draws in. While there are `checking` words, the water swells
/// and they say what it waits for, as through a pull's sync while `refreshing`: water never moves
/// under words that say nothing is going on. `checkedAt` shows under the words. As the list is
/// pulled, a drop gathers over the water, growing with the pull, and falls on each of `drops`.
struct LakeNote: View {
    let title: LocalizedStringKey
    var drawIn = false
    var grey = false
    var checking: LakeWords?
    var checkedAt: Date?
    var refreshing = false
    var pull: PullState

    @State private var still: Still
    /// The checking words, kept while they fade out.
    @State private var shownChecking: LakeWords?

    private var waiting: Bool { checking != nil }

    init(title: LocalizedStringKey, drawIn: Bool = false, grey: Bool = false,
         checking: LakeWords? = nil, checkedAt: Date? = nil, refreshing: Bool = false, pull: PullState) {
        self.title = title
        self.drawIn = drawIn
        self.grey = grey
        self.checking = checking
        self.checkedAt = checkedAt
        self.refreshing = refreshing
        self.pull = pull
        // Only as it first shows: the caller's flag turns off right after.
        _still = State(initialValue: Still(falling: drawIn))
    }

    var body: some View {
        VStack(spacing: 6) {
            WaterCanvas(still: still, grey: grey, surface: Lake.horizon, fall: Lake.fall, pulled: pull.fraction)
                .frame(height: Lake.height)
                .contentShape(.rect)
                .gesture(SpatialTapGesture().onEnded { tap in
                    if still.ready { still.touch(x: tap.location.x, y: tap.location.y) }
                })
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
                .opacity(waiting ? 0 : still.wordsShown)
                .offset(y: waiting ? 8 : (1 - still.wordsShown) * 8)
                // Until then the water is not empty yet, only checking: these words take its place.
                if let shown = checking ?? shownChecking {
                    VStack(spacing: 6) { words(LocalizedStringKey(shown.title), shown.text.map { LocalizedStringKey($0) }) }
                        .opacity(waiting ? 1 : 0)
                        .offset(y: waiting ? 0 : 8)
                }
            }
            .animation(.easeOut(duration: 0.4), value: waiting)
            .onAppear { shownChecking = checking }
            .onChange(of: checking) { _, now in if let now { shownChecking = now } }
        }
        .frame(maxWidth: .infinity)
        .padding(.top, Lake.top)
        .padding(.bottom, 48)
        .onAppear { still.beat = waiting || refreshing }
        .onChange(of: waiting || refreshing) { _, now in still.beat = now }
        .onChange(of: pull.drops) { _, _ in still.release(pull.lastDrop) }
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

/// The same water as an empty list's, in the gap a pull opens over the rows: the water stays where
/// it is and the list uncovers it. A drop gathers with the pull and falls on each release; the water
/// swells through the sync. `onCalm` hears whether the water has calmed since, so the list can wait
/// for it before it covers the water.
struct PullWater: View {
    var grey = false
    let refreshing: Bool
    let pull: PullState
    let onCalm: (Bool) -> Void

    @State private var still = Still(falling: false)

    var body: some View {
        WaterCanvas(still: still, grey: grey, surface: Lake.top + Lake.horizon, fall: Lake.pullFall,
                    pulled: pull.fraction, bottom: max(pull.gap, Lake.pullGap))
            .onAppear { still.beat = refreshing }
            .onChange(of: refreshing) { _, now in still.beat = now }
            .onChange(of: pull.drops) { _, _ in still.release(pull.lastDrop) }
            .onChange(of: still.calmed) { _, now in onCalm(now) }
    }
}

/// How the list is pulled: how far, in pulls (1 is enough to sync), how tall the gap over the rows
/// is, and the drops let go, each with how far it was pulled.
struct PullState: Equatable {
    var fraction: CGFloat = 0
    var gap: CGFloat = 0
    var drops = 0
    var lastDrop: CGFloat = 0
}

/// Sizes of the water, in points.
enum Lake {
    static let height: CGFloat = 124
    static let horizon: CGFloat = 40
    static let top: CGFloat = 8
    static let fall: CGFloat = 40
    /// How far a full pull opens the list: where the system's pull to refresh takes it.
    static let pullGap: CGFloat = 170
    static let pullFall: CGFloat = 24
    /// How tall the system's large title is, over the list.
    static let largeTitle: CGFloat = 52
    /// How far a pull must go, in pulls, to drop a drop when let go.
    static let tug: CGFloat = 1.0 / 3

    /// How far a pull of `fraction` moves the list: with the finger up to the threshold, then less and less.
    static func gap(_ fraction: CGFloat) -> CGFloat {
        fraction <= 1 ? pullGap * fraction : pullGap * (1 + 0.6 * (1 - 1 / fraction))
    }
}

/// Runs the water's clock: every frame while something moves, and not at all for water at rest.
/// With Reduce Motion on, the water stands still.
private struct WaterCanvas: View {
    let still: Still
    let grey: Bool
    let surface: CGFloat
    let fall: CGFloat
    let pulled: CGFloat
    var bottom: CGFloat?

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let water = grey ? Color(uiColor: .tertiaryLabel) : Color.ozeroFill
        TimelineView(.animation(paused: reduceMotion || !still.awake)) { timeline in
            Canvas { context, size in
                still.frame(at: timeline.date, frozen: reduceMotion)
                still.draw(in: &context, size: size, water: water, drop: .ozeroFill, surface: surface,
                           bottom: bottom ?? size.height, pull: pulled, fall: fall)
            }
        }
        .onChange(of: pulled) { _, _ in still.wake() }
    }
}

/// A drop in the air; positions in points, speed in points a second.
private final class Drop {
    let x: Double
    var y: Double
    var v: Double
    let land: Double
    let r: Double
    let point: Double
    let opens: Bool

    init(x: Double, y: Double, v: Double, land: Double, r: Double, point: Double, opens: Bool = false) {
        self.x = x; self.y = y; self.v = v; self.land = land; self.r = r; self.point = point; self.opens = opens
    }
}

/// The tiny drop a landing throws back up.
private final class Jet {
    let x: Double
    var y: Double
    var v: Double
    let base: Double

    init(x: Double, y: Double, v: Double, base: Double) {
        self.x = x; self.y = y; self.v = v; self.base = base
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

/// How a drop hangs while it gathers.
private struct Hanging {
    let r: Double
    let point: Double
    let y: Double
}

/// The water's state, stepped in small fixed steps of real physics: drops fall with gravity, throw
/// a tiny drop back up as they land, and send out crests that slow as they spread. Nothing moves
/// without a cause. Only `awake`, `calmed` and `wordsShown` are observed; every frame redraws and
/// steps from the clock.
@MainActor @Observable
final class Still {
    /// Whether the clock runs: something on the water moves.
    private(set) var awake = true
    /// Whether a wait's swell has gone, or never came.
    private(set) var calmed = true
    /// How far the words have come in.
    private(set) var wordsShown: Double = 1

    /// A wait: the water swells until it is over.
    var beat = false {
        didSet { wake() }
    }

    @ObservationIgnored private var falling: Bool
    @ObservationIgnored private var now: Double = 0
    @ObservationIgnored private var begun: Double = -1
    @ObservationIgnored private var drops: [Drop] = []
    @ObservationIgnored private var jets: [Jet] = []
    @ObservationIgnored private var waves: [Wave] = []
    @ObservationIgnored private var frozen = false
    // When the drop that opens the water landed.
    @ObservationIgnored private var opened: Double = -1
    @ObservationIgnored private var stepped: Double = -1
    @ObservationIgnored private var level: Double
    @ObservationIgnored private var rising = false
    @ObservationIgnored private var pulled: Double = 0
    @ObservationIgnored private var released = false
    @ObservationIgnored private var letGoAt: Double = 0
    @ObservationIgnored private var spent = false
    // How far off the middle the next drop gathers: one tug after another, no two land in one place.
    @ObservationIgnored private var aim: Double = 0
    @ObservationIgnored private var sincePulse: Double = 0
    @ObservationIgnored private var sea: Double = 0
    @ObservationIgnored private var wobble: Double = 0
    @ObservationIgnored private var wobbleV: Double = 0
    @ObservationIgnored private var wasFull = false
    @ObservationIgnored private var calm: Double = 0
    @ObservationIgnored private var horizon: Double = 0
    @ObservationIgnored private var land: Double = 0
    @ObservationIgnored private var middle: Double = 0
    // The surface along the horizon: how far each point across is lifted, and how fast it moves.
    // Drops and touches push it; a wave runs along it, comes back off the banks and dies away.
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

    init(falling: Bool) {
        self.falling = falling
        level = falling ? 0 : 1
        wordsShown = falling ? 0 : 1
    }

    /// Whether the surface is level and no crest runs over it, so lines are drawn straight.
    private var flat: Bool { sea == 0 && settled && waves.isEmpty }

    /// Whether the surface has stopped moving.
    private var settled: Bool { rough < 0.05 && moving < 0.01 }

    var ready: Bool { !frozen && level > 0.5 && middle > 0 }

    /// Whether something on the water is moving, so every frame counts.
    private var busy: Bool {
        beat || released || !drops.isEmpty || !jets.isEmpty || !waves.isEmpty || rising
            || abs(wobble) > 0.002 || abs(wobbleV) > 0.02 || (falling && now - wordsFrom < wordsMs) || !flat
    }

    /// Wakes the clock of water at rest; the next frame starts afresh rather than a step late.
    func wake() {
        if !awake {
            stepped = -1
            awake = true
        }
    }

    /// The gathered drop lets go, on the next step, as it hung at a pull of `fraction`; one a pull.
    func release(_ fraction: CGFloat) {
        guard ready, !spent else { return }
        released = true
        letGoAt = fraction
        wake()
    }

    /// A wave from under a finger at `x`, `y`.
    func touch(x: CGFloat, y: CGFloat) {
        waves.append(Wave(x: x, y: max(y, horizon + 6), e: 0.55))
        kick(x, -22)
        wake()
    }

    /// The clock for this frame. Water that never moves, for Reduce Motion, is level at once.
    func frame(at date: Date, frozen: Bool) {
        let ms = date.timeIntervalSinceReferenceDate * 1000
        if begun < 0 { begun = ms }
        now = ms
        if frozen && !self.frozen {
            self.frozen = true
            falling = false
            drops.removeAll()
            level = 1
        }
    }

    /// When the words start to come in: as the drop lands, or once it is long overdue.
    private var wordsFrom: Double {
        let overdue = begun + openWaitMs
        return opened >= 0 ? min(opened, overdue) : overdue
    }

    private func words() -> Double {
        if !falling { return 1 }
        if begun < 0 { return 0 }
        return calmCurve(min(1, max(0, (now - wordsFrom) / wordsMs)))
    }

    /// A drop for a pull of `p`: it keeps growing past the threshold, a little slower, and draws to a point.
    private func hanging(_ p: Double) -> Hanging {
        let q = p <= 1 ? p : 1 + 0.3 * (1 - 1 / p)
        let r = 1.2 + 5.2 * pow(q, 0.75)
        return Hanging(r: r, point: min(1, q) * 1.1, y: 12 + 10 * min(1, q) + r)
    }

    private func letGo(_ h: Hanging, land: Double) {
        drops.append(Drop(x: middle + aim, y: h.y, v: 0, land: land, r: h.r, point: h.point))
        wobble = 0
        wobbleV = 0
    }

    private func step(_ dt: Double, land: Double) {
        if released {
            released = false
            spent = true
            letGo(hanging(letGoAt), land: land)
        }
        // A wait swells the water once nothing is falling; a faint ring now and then comes from
        // where the drop fell. When the wait is over, the swell lies down.
        let swelling = beat && ready && drops.isEmpty
        sea += ((swelling ? 1 : 0) - sea) * min(1, dt / seaEase)
        if !swelling && sea < 0.002 { sea = 0 }
        // A drop on its way is not calm yet either.
        let lying = !swelling && sea < 0.06 && !released && drops.isEmpty
        if lying != calmed { calmed = lying }
        sincePulse = swelling ? sincePulse + dt : 0
        if sincePulse > pulseEvery {
            sincePulse -= pulseEvery
            waves.append(Wave(x: middle, y: land, e: 0.16))
        }
        flow(dt)
        // The hanging drop gives a little jolt as it fills, and trembles as it settles.
        let full = !spent && pulled >= 1
        if full && !wasFull { wobbleV += 2.4 }
        wasFull = full
        wobbleV += (-220 * wobble - 7 * wobbleV) * dt
        wobble += wobbleV * dt

        drops.removeAll { d in
            d.v += gravity * dt
            d.y += d.v * dt
            if d.y < d.land { return false }
            let e = max(0.18, min(1, d.r / 6.6))
            waves.append(Wave(x: d.x, y: d.land, e: e))
            kick(d.x, -40 * e)
            // A real drop throws a tiny one back up, which falls again as a smaller ring.
            if d.r > 2 { jets.append(Jet(x: d.x, y: d.land, v: -(2 * gravity * 4 * e + 40).squareRoot(), base: d.land)) }
            if d.opens {
                rising = true
                opened = now
            }
            return true
        }
        jets.removeAll { j in
            j.v += gravity * dt
            j.y += j.v * dt
            let down = j.v > 0 && j.y >= j.base
            if down { waves.append(Wave(x: j.x, y: j.base, e: 0.32)) }
            return down
        }
        var energy: Double = 0
        waves.removeAll { w in
            w.t += dt
            let a = w.e * exp(-w.t / 1.5)
            energy += a
            return a < 0.015
        }
        calm += (min(1, energy) - calm) * min(1, dt * 2)
        if rising {
            level = min(1, level + dt * 1000 / levelMs)
            if level >= 1 { rising = false }
        }
    }

    /// One step of the surface: a damped wave between two banks, thicker once the wait is over.
    private func flow(_ dt: Double) {
        if settled { return }
        let damping = beat ? 0.9 : 1.5
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

    /// Pushes the surface at `x`. Water is not lost: what is pressed down rises around it.
    private func kick(_ x: Double, _ amount: Double) {
        var mean: Double = 0
        for i in 0..<span {
            let d = middle - banks + Double(i) * spacing - x
            speed[i] += amount * exp(-d * d / (2 * kickWidth * kickWidth))
            mean += speed[i]
        }
        mean /= Double(span)
        for i in 0..<span { speed[i] -= mean }
        moving = max(moving, 0.02)
        wake()
    }

    /// How far the surface is lifted at `x` on a line `near` the viewer, 0 at the horizon.
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
            step(s, land: land)
            dt -= s
        }
        let shown = words()
        let sleeping = !busy
        // Not while the frame draws: the observed values change right after it.
        if shown != wordsShown || sleeping == awake {
            Task { @MainActor in
                self.wordsShown = shown
                if sleeping && self.awake && !self.busy { self.awake = false }
            }
        }
    }

    /// The horizon lies `surface` down, drops gather near the top and land `fall` under the horizon;
    /// the water ends at `bottom`.
    func draw(in context: inout GraphicsContext, size: CGSize, water: Color, drop: Color, surface: CGFloat,
              bottom: CGFloat, pull: CGFloat, fall: CGFloat) {
        let hz = Double(surface)
        horizon = hz
        middle = size.width / 2
        // Once a drop has gone, the next gathers as the finger pulls again, somewhere else.
        if spent && !released && pull > pulled {
            spent = false
            aim = nextAim(aim)
        }
        pulled = pull
        land = hz + fall
        if falling && level == 0 && !rising && drops.isEmpty && begun >= 0 {
            // The last message has left: a drop falls and the water comes out of its waves.
            drops.append(Drop(x: middle, y: hz - 70, v: 120, land: land, r: 2.8, point: 0.9, opens: true))
        }
        advance()
        gatherCrests()
        let t = (now - begun) / 1000
        let lv = calmCurve(level)
        let stroke: CGFloat = 1
        var below = context
        below.clip(to: Path(CGRect(x: 0, y: hz, width: size.width, height: max(0, bottom - hz))))
        if lv > 0 {
            // The reflection: a soft glow under the horizon, flattened by perspective, and a few
            // streaks that follow the surface. It breathes deeper while the water swells.
            let r = 150 * lv + 1
            // Never deeper than the water, so the glow fades out before it ends.
            let flatten = min(r * 0.32 * (1 + 0.14 * sea * sin(t * 1.3)), max(1, Double(bottom) - hz))
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
            // A crest passing through bends them; nothing else moves them.
            for (i, s) in streaks.enumerated() {
                waterline(&below, alpha: s.alpha, x: middle, half: s.half * lv, y: hz + s.dy, near: Double(i + 1) / 4, t: t, color: water, width: stroke)
            }
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
        // Each drop shows in the water under it, clearer as it comes close.
        for d in drops {
            let near = 1 - (d.land - d.y) / 90
            if near <= 0 || d.y >= d.land { continue }
            let y = d.land + (d.land - d.y) * 0.3
            below.fill(Path(ellipseIn: CGRect(x: d.x - d.r, y: y - d.r * 0.45, width: d.r * 2, height: d.r * 0.9)),
                       with: .color(drop.opacity(0.22 * near * edge(d.x, y, below: Double(bottom)))))
        }
        if lv > 0 { waterline(&context, alpha: 0.55, x: middle, half: 150 * lv, y: hz, near: 0, t: t, color: water, width: stroke) }
        // The gathering drop grows with the pull, a round bead that fills into a drop.
        let gathering = !spent && !released && ready ? Double(pull) : 0
        if gathering > 0.02 {
            let h = hanging(gathering)
            bead(&context, color: drop, x: middle + aim, y: h.y, r: h.r, point: h.point, stretch: 1 + wobble)
        }
        // Falling, it draws out with its speed.
        for d in drops { bead(&context, color: drop, x: d.x, y: d.y, r: d.r, point: d.point, stretch: 1 + min(0.35, d.v / 1100)) }
        for j in jets {
            context.fill(Path(ellipseIn: CGRect(x: j.x - 1.3, y: j.y - 1.3, width: 2.6, height: 2.6)), with: .color(drop.opacity(0.6)))
        }
    }

    /// How much of the water shows at `x`, `y`: all of it in the middle, fading out toward its sides
    /// and toward the eye before it ends `below`, so a wave never ends on an edge.
    private func edge(_ x: Double, _ y: Double, below: Double) -> Double {
        let side = min(x, middle * 2 - x) / edgeSide
        let near = (below - y) / edgeNear
        let v = min(max(min(side, near), 0), 1)
        return v * v * (3 - 2 * v)
    }

    /// A water drop: round below, rising to a soft point above as `point` goes from 0 to 1.
    private func bead(_ context: inout GraphicsContext, color: Color, x: Double, y: Double, r: Double, point: Double, stretch: Double) {
        var path = Path()
        for i in 0...dropSteps {
            let a = Double(i) * 2 * .pi / Double(dropSteps)
            let c = cos(a)
            let px = r * sin(a) * pow(abs(sin(a / 2)), point)
            let py = -r * c * (c > 0 ? 1 + 0.45 * point : 1) * stretch
            let p = CGPoint(x: x + px, y: y + py)
            if i == 0 { path.move(to: p) } else { path.addLine(to: p) }
        }
        path.closeSubpath()
        // Glass: a hairline like the water's, brighter below, over the faintest tint.
        context.fill(path, with: .color(color.opacity(0.13)))
        let top = y - r * (1 + 0.45 * point) * stretch
        context.stroke(path, with: .linearGradient(
            Gradient(colors: [color.opacity(0.3), color.opacity(0.95)]),
            startPoint: CGPoint(x: x, y: top), endPoint: CGPoint(x: x, y: y + r)), lineWidth: 1)
        // Light inside the lower curve, as in a lens.
        if r < 2.4 { return }
        let lens = r * 0.6
        var arc = Path()
        arc.addArc(center: CGPoint(x: x, y: y), radius: lens, startAngle: .degrees(112), endAngle: .degrees(171), clockwise: false)
        context.stroke(arc, with: .color(color.opacity(0.5)), lineWidth: 1)
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

/// Where a drop gathers after one at `last`, off the middle: anywhere near, but not on the same spot.
private func nextAim(_ last: Double) -> Double {
    let next = Double.random(in: -aimSpan...aimSpan)
    if abs(next - last) >= aimSpan / 3 { return next }
    return min(aimSpan, max(-aimSpan, next + (next > last ? aimSpan / 3 : -aimSpan / 3)))
}

/// Eases in a touch, then a long, soft ease-out, as Android's `Calm` curve.
private func calmCurve(_ x: Double) -> Double {
    // A cubic Bézier (0.2, 0, 0, 1), solved for x by bisection.
    var low = 0.0
    var high = 1.0
    for _ in 0..<20 {
        let mid = (low + high) / 2
        let bx = 3 * (1 - mid) * (1 - mid) * mid * 0.2 + mid * mid * mid
        if bx < x { low = mid } else { high = mid }
    }
    let s = (low + high) / 2
    return 3 * (1 - s) * s * s + s * s * s
}

/// How far off the middle a drop gathers.
private let aimSpan: Double = 64

/// A reflection streak, under the horizon.
private let streaks: [(dy: Double, half: Double, alpha: Double)] = [
    (9, 104, 0.1), (20, 84, 0.075), (35, 62, 0.055), (55, 40, 0.035),
]

/// Gravity, points a second squared: a drop falls its 40–50 points in about a third of a second.
private let gravity: Double = 760
/// How far from its sides, and from where it ends below, the water fades out.
private let edgeSide = 48.0
private let edgeNear = 28.0
private let dropSteps = 48

/// How slowly a wait's swell comes and goes, in seconds, and how often a faint ring goes out.
private let seaEase: Double = 0.72
private let pulseEvery: Double = 2.6

/// The surface: points across, the space between them, a wave's speed along it, its banks from the middle.
private let span = 101
private let spacing: Double = 3
private let waveSpeed: Double = 150
private let banks: Double = 150
private let stiff = waveSpeed * waveSpeed / (spacing * spacing)

/// How wide a push is.
private let kickWidth: Double = 13

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
private let levelMs: Double = 1900
/// How long the words take to come in, however they come.
private let wordsMs: Double = 400
/// How long the clock waits for the drop that opens the water, should it never be drawn; the words come then.
private let openWaitMs: Double = 3000

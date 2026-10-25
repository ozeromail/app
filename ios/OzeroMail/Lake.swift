import SwiftUI

/// The note of a list with nothing left in it: still water. A horizon and a soft reflection under
/// it, at rest until something moves it. A touch sends a wave out, and so does the drop that
/// lands as the note first shows. `grey` water is for plain empty folders. While `waiting` for its
/// mail, the water swells and says it is checking.
struct LakeNote: View {
    let title: LocalizedStringKey
    let text: LocalizedStringKey
    var grey = false
    var waiting = false

    @State private var water = Water()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: 18) {
            TimelineView(.animation(minimumInterval: 1 / 60, paused: reduceMotion || (!waiting && water.still))) { timeline in
                Canvas { context, size in
                    water.draw(in: &context, size: size, at: timeline.date, color: grey ? .secondary : Color.ozeroFill, swell: waiting)
                }
            }
            .frame(height: 120)
            .contentShape(.rect)
            .gesture(SpatialTapGesture().onEnded { tap in water.touch(atX: tap.location.x) })
            VStack(spacing: 6) {
                if waiting {
                    Text("inbox_checking").font(.subheadline).foregroundStyle(.secondary)
                } else {
                    Text(title).font(.title3.weight(.semibold))
                    Text(text).font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }
            }
            .padding(.horizontal, 32)
            .transition(.opacity)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 24)
        .animation(.easeInOut(duration: 0.4), value: waiting)
        .onAppear {
            // The drop that opens the water.
            if !reduceMotion { water.touch(atX: nil) }
        }
        .accessibilityElement(children: .combine)
    }
}

/// The water's state: the crests impacts send out, each slowing and fading as it spreads.
@MainActor @Observable
private final class Water {
    private struct Crest {
        /// Across the water, from 0 to 1; nil for its middle.
        let x: Double?
        let start: Date
    }

    @ObservationIgnored private var crests: [Crest] = []
    private static let life: TimeInterval = 3.5

    /// No crest is moving: the clock stops, so still water costs nothing.
    private(set) var still = true

    func touch(atX x: CGFloat?) {
        crests.removeAll { Date.now.timeIntervalSince($0.start) > Self.life }
        let crest = Crest(x: x.map(Double.init), start: .now)
        crests.append(crest)
        still = false
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(Self.life))
            // A later touch keeps the water moving for its own life.
            if crests.last?.start == crest.start { still = true }
        }
    }

    func draw(in context: inout GraphicsContext, size: CGSize, at date: Date, color: Color, swell: Bool) {
        let horizon = size.height * 0.4
        let middle = size.width / 2
        let points = 80
        var surface = Path()
        for i in 0...points {
            let x = size.width * CGFloat(i) / CGFloat(points)
            var y = horizon
            for crest in crests {
                let t = date.timeIntervalSince(crest.start)
                guard t >= 0, t < Self.life else { continue }
                let origin = crest.x ?? middle
                // A ring spreading out from where it hit, lower and wider as it goes.
                let distance = abs(x - origin)
                let front = 90 * t
                let falloff = exp(-pow((distance - front) / 28, 2))
                y += CGFloat(sin(distance / 9 - t * 9) * 6 * falloff * exp(-t * 1.2))
            }
            if swell {
                let t = date.timeIntervalSinceReferenceDate
                y += CGFloat(sin(Double(x) / 40 + t * 1.6) * 2.5)
            }
            // The banks hold the water still.
            let bank = min(x, size.width - x) / (size.width / 2)
            y = horizon + (y - horizon) * min(1, bank * 3)
            if i == 0 { surface.move(to: CGPoint(x: x, y: y)) } else { surface.addLine(to: CGPoint(x: x, y: y)) }
        }
        context.stroke(surface, with: .color(color.opacity(0.9)), style: StrokeStyle(lineWidth: 1.6, lineCap: .round))
        // The soft reflection: streaks under the horizon, shorter and fainter further down.
        for row in 1...4 {
            let y = horizon + CGFloat(row) * 12
            let width = size.width * (0.6 - CGFloat(row) * 0.11)
            var streak = Path()
            streak.move(to: CGPoint(x: middle - width / 2, y: y))
            streak.addLine(to: CGPoint(x: middle + width / 2, y: y))
            context.stroke(streak, with: .color(color.opacity(0.28 - Double(row) * 0.05)), style: StrokeStyle(lineWidth: 1.2, lineCap: .round))
        }
    }
}

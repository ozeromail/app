package com.ozeromail.app

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.MotionDurationScale
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay

/**
 * The note of a list with nothing in it: still water over its title, and under it when it was
 * [checkedAt]. While there are [checking] words, as the first mail is on its way, they take the
 * place of the title and say what it waits for, and the water swells; it swells too while
 * [refreshing]. [grey] water is for plain folders. A finger on the list, as [finger] tells, draws
 * the surface down under it, the more the further it [pulled]; let go, the water springs back in
 * waves. The tabs' [glide] sends a wave along it. [water] is how much of it shows; [wordsModifier]
 * moves the words alone, while the water stays.
 */
@Composable
fun StillNote(
    title: String,
    grey: Boolean = false,
    checking: Pair<String, String?>? = null,
    checkedAt: Long? = null,
    refreshing: Boolean = false,
    finger: Finger? = null,
    pulled: () -> Float = { 0f },
    glide: () -> Float = { 0f },
    water: () -> Float = { 1f },
    wordsModifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    val still = rememberStill()
    val known by animateFloatAsState(if (checking != null) 0f else 1f, Motion.calm(Motion.NOTE), label = "words")
    val waitWords = rememberLast(checking)
    val fading by remember { derivedStateOf { known < 1f } }
    SideEffect { still.beat = checking != null || refreshing }
    Column(
        Modifier.fillMaxWidth().padding(top = LAKE_TOP, bottom = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        val tint = if (grey) colors.outline else colors.accent
        // Where the water starts across the window, so a finger's place on it is known.
        val left = remember { FloatArray(1) }
        Canvas(
            Modifier.fillMaxWidth().height(LAKE_HEIGHT).graphicsLayer { alpha = water() }
                .onGloballyPositioned { left[0] = it.positionInRoot().x },
        ) {
            still.draw(this, tint, HORIZON, size.height, finger?.x?.minus(left[0]), pulled(), glide())
        }
        Box(wordsModifier, contentAlignment = Alignment.TopCenter) {
            Words(title, null, Modifier.graphicsLayer { alpha = known }) { checkedAt?.let { Checked(it) } }
            // Until then the list is not empty yet, only on its way: these words take its place.
            if (fading) {
                waitWords?.let { (title, text) -> Words(title, text, Modifier.graphicsLayer { alpha = 1f - known }) }
            }
        }
    }
}

/** Where a finger is on the list, across the window in px, or null while none is on it. */
@Stable
class Finger {
    private val at = mutableFloatStateOf(Float.NaN)

    /** Where the tracked view starts across the window. */
    internal var origin = 0f

    val x: Float? get() = at.floatValue.takeUnless { it.isNaN() }

    fun set(x: Float?) {
        at.floatValue = x?.let { it + origin } ?: Float.NaN
    }
}

/** Tells [finger] where the first finger on this is, without taking its gesture from anything under it. */
fun Modifier.tracks(finger: Finger) = onGloballyPositioned { finger.origin = it.positionInRoot().x }.pointerInput(finger) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        try {
            finger.set(down.position.x)
            do {
                val event = awaitPointerEvent(PointerEventPass.Initial)
                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                finger.set(change.position.x)
            } while (change.pressed)
        } finally {
            // Also when this goes away under the finger: the water is not held by one gone.
            finger.set(null)
        }
    }
}

/**
 * Runs the water's clock: every frame while something moves, and not at all for water at rest.
 * With the system's animations off, the water stands still.
 */
@Composable
private fun rememberStill(): Still {
    val still = remember { Still() }
    LaunchedEffect(still) {
        if (coroutineContext[MotionDurationScale]?.scaleFactor == 0f) {
            still.hold()
            return@LaunchedEffect
        }
        while (true) {
            withFrameMillis { still.now = it }
            if (!still.busy) {
                still.rest()
                still.wake.receive()
            }
        }
    }
    return still
}

/** The crests one impact sends out, [e] its energy, [t] seconds since. */
private class Wave(val x: Float, val y: Float, val e: Float, var t: Float = 0f)

/**
 * The water's state, stepped in small fixed steps of real physics: a finger draws the surface
 * down, and let go it springs back and sends out crests that slow as they spread. Nothing moves
 * without a cause. Not snapshot state: [now] is, and every frame redraws and steps from it.
 */
private class Still {
    private val clock = mutableLongStateOf(0L)
    var now: Long
        get() = clock.longValue
        set(value) {
            if (begun < 0) begun = value
            asleep = false
            clock.longValue = value
        }

    // While the clock sleeps, a draw does not step: its time is the last frame's, not now.
    private var asleep = false

    /** Wakes the clock of water at rest. */
    val wake = Channel<Unit>(Channel.CONFLATED)

    /** A wait: the water swells until it is over. */
    var beat = false
        set(value) {
            if (value == field) return
            field = value
            wake.trySend(Unit)
        }

    private val waves = ArrayList<Wave>()
    private var frozen = false
    private var begun = -1L
    private var stepped = -1L
    private var sincePulse = 0f
    private var sea = 0f
    private var calm = 0f
    private var horizon = 0f
    private var middle = 0f
    private var tint = Color.Unspecified
    private var line: Brush = SolidColor(Color.Transparent)
    private var glow: Brush = SolidColor(Color.Transparent)

    // Where a finger holds the water down, in dp across, and how deep, in dp; NaN with no finger.
    private var holdX = Float.NaN
    private var holdDepth = 0f

    // The surface along the horizon: how far each point across is pressed down, in dp, and how
    // fast it moves. A finger and the tabs push it; a wave runs along it, comes back off
    // the banks and dies away.
    private val height = FloatArray(SPAN)
    private val speed = FloatArray(SPAN)
    // Earlier shapes of it, so a nearer streak sees a wave a moment later, as if it came closer.
    private val past = Array(PAST) { FloatArray(SPAN) }
    private var pastAt = 0
    private var pastTick = 0
    private var rough = 0f
    private var moving = 0f
    private var glidAt = 0f
    private var glideV = 0f
    private var push = 0f
    private var jolt = 0f
    private val ys = FloatArray(LINE_POINTS)
    private var crests = FloatArray(0)
    private var crestCount = 0
    private val glints = FloatArray(LINE_POINTS)
    private val soft = FloatArray(LINE_POINTS)

    private val holding get() = !holdX.isNaN()

    /** Whether the surface is level and no crest runs over it, so lines are drawn straight. */
    private val flat get() = sea == 0f && settled && waves.isEmpty() && !holding

    /** Whether the surface has stopped moving. */
    private val settled get() = rough < 0.05f && moving < 0.01f

    /** Whether something on the water is moving, so every frame counts. */
    val busy get() = beat || !flat || abs(jolt) > 1f || abs(glideV) > 0.01f

    /** The clock goes to sleep: the next frame starts afresh rather than a step late. */
    fun rest() {
        asleep = true
        stepped = -1
        glideV = 0f
        jolt = 0f
    }

    /** Water that never moves, for when the system's animations are off. */
    fun hold() {
        frozen = true
        now = 1
    }

    /** A finger at [x] dp, pulled [pull] pulls down, or none: the surface under it follows. */
    private fun press(x: Float?, pull: Float) {
        if (frozen || middle <= 0f) return
        if (x == null) {
            if (holding) {
                // Let go: the water springs back, and a ring goes out from where it was held.
                waves += Wave(holdX, horizon + 6f, (0.25f + holdDepth / HOLD_DEEPEST).coerceAtMost(1f))
                holdX = Float.NaN
                holdDepth = 0f
                wake.trySend(Unit)
            }
            return
        }
        if (!holding) waves += Wave(x, horizon + 6f, 0.3f)
        holdX = x
        // Deeper with the pull, against a growing tension.
        val p = pull.coerceAtLeast(0f)
        holdDepth = HOLD_TOUCH + (HOLD_DEEPEST - HOLD_TOUCH) * (1f - 1f / (1f + p * 1.4f))
        wake.trySend(Unit)
    }

    private fun step(dt: Float) {
        // A wait swells the water; a faint ring now and then comes from its middle. When the wait
        // is over, the swell lies down.
        val swelling = beat && !frozen
        sea += ((if (swelling) 1f else 0f) - sea) * min(1f, dt / SEA_EASE)
        if (!swelling && sea < 0.002f) sea = 0f
        sincePulse = if (swelling) sincePulse + dt else 0f
        if (sincePulse > PULSE_EVERY) {
            sincePulse -= PULSE_EVERY
            waves += Wave(middle, horizon + 40f, 0.16f)
        }
        // The tabs' push, smoothed: a finger's speed comes in steps.
        jolt += (push - jolt) * min(1f, dt * 25f)
        flow(dt)
        var energy = 0f
        waves.removeAll { w ->
            w.t += dt
            val a = w.e * exp(-w.t / 1.5f)
            energy += a
            a < 0.015f
        }
        calm += (min(1f, energy) - calm) * min(1f, dt * 2f)
    }

    /** One step of the surface: a damped wave between two banks, held under a finger, thicker once a wait is over. */
    private fun flow(dt: Float) {
        if (settled && abs(jolt) < 1f && !holding) return
        val damping = if (beat) 0.9f else 1.5f
        val mid = (SPAN - 1) / 2f
        for (i in 0 until SPAN) {
            val bend = height[if (i > 0) i - 1 else 1] - 2 * height[i] + height[if (i < SPAN - 1) i + 1 else SPAN - 2]
            // A sideways jolt of the tabs presses the water toward one bank.
            var force = STIFF * bend - damping * speed[i] - jolt * JOLT * (i - mid) / mid
            if (holding) {
                // The finger draws the water toward where it holds it, and lets the rest be.
                val d = middle - BANKS + i * SPACING - holdX
                val hold = exp(-d * d / (2 * HOLD_WIDTH * HOLD_WIDTH))
                force += hold * (HOLD_PULL * (holdDepth - height[i]) - HOLD_DAMP * speed[i])
            }
            speed[i] += force * dt
        }
        var fast = 0f
        rough = 0f
        for (i in 0 until SPAN) {
            height[i] += speed[i] * dt
            fast = maxOf(fast, abs(speed[i]))
            rough = maxOf(rough, abs(height[i]))
        }
        moving += (min(1f, fast / 25f) - moving) * min(1f, dt * 4f)
        if (settled && abs(jolt) < 1f && !holding) {
            // Level again, past shapes too, so the next wave starts from still water.
            height.fill(0f)
            speed.fill(0f)
            past.forEach { it.fill(0f) }
            moving = 0f
            return
        }
        if (++pastTick % 2 == 0) {
            pastAt = (pastAt + 1) % PAST
            height.copyInto(past[pastAt])
        }
    }

    /** How far into perspective the crests of [w] lie: 0 at the horizon, 1 near the viewer. */
    private fun depthOf(w: Wave) = ((w.y - horizon) / 220f).coerceIn(0f, 1f)

    /** How far crest [k] of [w] has spread, in dp: they slow as they go. */
    private fun radius(w: Wave, k: Int) = 64f * sqrt(w.e) * (1f - exp(-w.t / 0.85f)) + 7f * w.t - k * (6f + 5f * w.t)

    /** How strong crest [k] of [w] is at [r] dp: it fades with time and distance, and comes in at once. */
    private fun strength(w: Wave, k: Int, r: Float) =
        w.e * exp(-w.t / 1.5f) * (1f - k * 0.32f) / sqrt(1f + r / 40f) * min(1f, w.t * 12f)

    /** Gathers this frame's crests, [CREST] numbers each: centre x and y, radius, strength and depth, all in dp. */
    private fun gatherCrests() {
        if (crests.size < waves.size * 3 * CREST) crests = FloatArray(waves.size * 6 * CREST)
        crestCount = 0
        for (w in waves) {
            val depth = depthOf(w)
            for (k in 0..2) {
                val r = radius(w, k)
                if (r <= 0.5f) continue
                val amp = strength(w, k, r)
                if (amp < 0.01f) continue
                val o = crestCount++ * CREST
                crests[o] = w.x
                crests[o + 1] = w.y
                crests[o + 2] = r
                crests[o + 3] = amp
                crests[o + 4] = depth
            }
        }
    }

    /** How far the crests running through lift the water at [x], [y] dp: a crest up, a trough after it. */
    private fun crest(x: Float, y: Float): Float {
        var lift = 0f
        for (c in 0 until crestCount) {
            val o = c * CREST
            val depth = crests[o + 4]
            val dx = x - crests[o]
            val dy = (y - crests[o + 1]) / (0.13f + 0.3f * depth)
            val d = sqrt(dx * dx + dy * dy) / (1f + 0.8f * depth) - crests[o + 2]
            // Far from its ring a crest lifts nothing worth the reckoning.
            if (d > 8f || d < -12f) continue
            lift -= 2.6f * crests[o + 3] * (exp(-d * d / 8f) - 0.6f * exp(-(d + 4f) * (d + 4f) / 8f))
        }
        return lift
    }

    /** How far the surface is pressed down at [x] dp on a line [near] the viewer, 0 at the horizon. */
    private fun rise(x: Float, near: Float, t: Float): Float {
        val shape = if (near > 0f) past[(pastAt - (near * 12f).toInt() + PAST * 2) % PAST] else height
        val at = ((x - (middle - BANKS)) / SPACING).coerceIn(0f, SPAN - 1f)
        val i = min(at.toInt(), SPAN - 2)
        val r = at - i
        var y = (shape[i] * (1f - r) + shape[i + 1] * r) * (1f + 0.7f * near)
        if (sea > 0f) {
            for (s in SWELLS) y += s.amp * (1.6f + 1.3f * near) * sea * sin(s.dir * s.k * (x - middle) - s.w * t + s.phase + near * 1.4f)
        }
        return y
    }

    /** Steps the water up to [now], for the tabs standing at [glide]. */
    private fun advance(glide: Float) {
        if (glide != glidAt && stepped < 0) wake.trySend(Unit)
        var dt = if (stepped < 0 || asleep) 0f else min(0.05f, (now - stepped) / 1000f)
        stepped = if (asleep) -1 else now
        // How sharply the tabs move. A snap to the next tab is a jump, not a move.
        if (dt > 0f) {
            val v = if (abs(glide - glidAt) > 0.5f) glideV else (glide - glidAt) / dt
            push = ((v - glideV) / dt).coerceIn(-JOLT_MAX, JOLT_MAX)
            glideV = v
        } else {
            push = 0f
        }
        glidAt = glide
        while (dt > 0f) {
            val s = min(dt, 1f / 120f)
            step(s)
            dt -= s
        }
        if (busy) wake.trySend(Unit)
    }

    /** Brushes one unit across, scaled into place, so a frame makes no new shaders. */
    private fun paint(color: Color) {
        if (color == tint) return
        tint = color
        line = Brush.horizontalGradient(listOf(Color.Transparent, color, Color.Transparent), -1f, 1f)
        // Two stops only: more, on a gradient this small and stretched this far, took the test emulator down.
        glow = Brush.radialGradient(listOf(color, Color.Transparent), Offset.Zero, 1f)
    }

    /**
     * The horizon lies [surface] down and the water ends at [bottom] px, in [water]'s color. A
     * finger at [finger] px across, pulled [pull] pulls, draws it down. The tabs stand at [glide].
     */
    fun draw(scope: DrawScope, water: Color, surface: Dp, bottom: Float, finger: Float?, pull: Float, glide: Float) = with(scope) {
        val u = density
        val hz = surface.value
        horizon = hz
        middle = size.width / u / 2
        press(finger?.let { it / u }, pull)
        advance(glide)
        gatherCrests()
        paint(water)
        val t = (now - begun) / 1000f
        val cx = middle * u
        val hzPx = hz * u
        val stroke = 1.dp.toPx()
        val below = bottom / u
        clipRect(top = hzPx, bottom = bottom) {
            // The reflection: a soft glow under the horizon, flattened by perspective, and a few
            // streaks that follow the surface. It breathes deeper while the water swells.
            val r = 150.dp.toPx()
            // Never deeper than the water, so the glow fades out before it ends.
            val flatten = min(r * 0.32f * (1f + 0.14f * sea * sin(t * 1.3f)), maxOf(1f, bottom - hzPx))
            withTransform({
                translate(cx, hzPx)
                scale(r, flatten, Offset.Zero)
            }) {
                drawRect(glow, Offset(-1f, 0f), Size(2f, (bottom - hzPx) / flatten), alpha = 0.14f + 0.04f * sea)
            }
            // A crest passing through bends them, as a finger does.
            STREAKS.forEachIndexed { i, s ->
                waterline(s.alpha, middle, s.half, hz + s.dy, (i + 1) / 4f, t, water, stroke)
            }
            // Waves: two or three crests an impact, slowing as they spread, fading with distance.
            // Each point of a crest rides the swell, and it shows only where it faces the eye,
            // near and far, as light on the same water rather than a ring laid over it.
            for (c in 0 until crestCount) {
                val o = c * CREST
                val amp = crests[o + 3]
                if (amp < 0.02f) continue
                val depth = crests[o + 4]
                val near = 0.4f + 0.6f * depth
                val wx = crests[o]
                val wy = crests[o + 1]
                val rx = crests[o + 2] * (1f + 0.8f * depth)
                val ry = rx * (0.13f + 0.3f * depth)
                val n = maxOf(24, (rx / 2f).toInt())
                var px = 0f
                var py = 0f
                for (i in 0..n) {
                    val a = i * 2f * PI.toFloat() / n
                    val x = wx + rx * cos(a)
                    val y = wy + ry * sin(a) + rise(x, near, t)
                    if (i > 0) {
                        val face = sin(a - PI.toFloat() / n)
                        // The near side catches more light than the far one.
                        val alpha = 0.5f * amp * abs(face * face * face) * (if (face > 0f) 1f else 0.55f) * edge(x, y, below)
                        if (alpha > 0.008f) drawLine(water.copy(alpha = min(0.42f, alpha)), Offset(px * u, py * u), Offset(x * u, y * u), stroke)
                    }
                    px = x
                    py = y
                }
            }
        }
        waterline(0.55f, middle, 150f, hz, 0f, t, water, stroke)
    }

    /**
     * How much of the water shows at [x], [y] dp: all of it in the middle, fading out toward its
     * sides and toward the eye before it ends [below] dp down, so a wave never ends on an edge.
     */
    private fun edge(x: Float, y: Float, below: Float): Float {
        val side = min(x, middle * 2 - x) / EDGE_SIDE
        val near = (below - y) / EDGE_NEAR
        return smooth(min(side, near))
    }

    private fun smooth(v: Float): Float = v.coerceIn(0f, 1f).let { it * it * (3f - 2f * it) }

    /**
     * A hairline on the water, [half] dp to each side of [x] at [y] dp, on a line [near] the viewer:
     * straight while the water is level; otherwise it follows the surface and lights up where a
     * wave's face turns to the eye.
     */
    private fun DrawScope.waterline(alpha: Float, x: Float, half: Float, y: Float, near: Float, t: Float, color: Color, width: Float) {
        val u = density
        val n = min(LINE_POINTS - 1, (half * 2 / SPACING).toInt())
        if (flat || n < 4) return faded(alpha, x * u, half * u, y * u, width)
        val gap = half * 2 / n
        for (i in 0..n) ys[i] = y + rise(x - half + i * gap, near, t) + crest(x - half + i * gap, y)
        // The slope over a few dp, so light catches whole faces of a wave, not single points.
        val facing = 0.06f + 0.05f * near
        val light = min(1f, sea * 2f + calm + moving)
        for (i in 0..n) {
            val l = maxOf(0, i - 2)
            val r = min(n, i + 2)
            val d = (abs(ys[r] - ys[l]) / ((r - l) * gap) - facing) / 0.05f
            glints[i] = if (light < 0.01f) 0f else exp(-d * d) * light
        }
        // Softened twice, through a second buffer so it stays even.
        blur(glints, soft, n)
        blur(soft, glints, n)
        // The line fades out at both ends, as a straight one does, with glints over it.
        val base = alpha * (1f - 0.45f * sea)
        val bright = min(0.42f, alpha * 2.6f)
        for (i in 1..n) {
            val x0 = x - half + (i - 1) * gap
            val fade = maxOf(0f, 1f - abs(x0 + gap / 2 - x) / half)
            val a = base * fade + bright * (glints[i] + glints[i - 1]) / 2 * sqrt(fade)
            if (a < 0.004f) continue
            drawLine(color.copy(alpha = min(1f, a)), Offset(x0 * u, ys[i - 1] * u), Offset((x0 + gap) * u, ys[i] * u), width)
        }
    }

    /** Softens [from] into [into], over points 0 to [n]; the ends stay. */
    private fun blur(from: FloatArray, into: FloatArray, n: Int) {
        into[0] = from[0]
        into[n] = from[n]
        for (i in 1 until n) into[i] = (from[i - 1] + from[i] * 2 + from[i + 1]) / 4
    }

    /** A hairline [half] to each side of [x], fading out at both ends. */
    private fun DrawScope.faded(alpha: Float, x: Float, half: Float, y: Float, width: Float) {
        if (half < 0.5f) return
        withTransform({
            translate(x, y)
            scale(half, 1f, Offset.Zero)
        }) {
            drawLine(line, Offset(-1f, 0f), Offset(1f, 0f), width, alpha = alpha)
        }
    }
}

/** When the list was checked [at], ms on the clock: just now, then the time. */
@Composable
private fun Checked(at: Long) {
    val context = LocalContext.current
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(at) {
        now = System.currentTimeMillis()
        delay(at + CHECKED_NOW_MS - now)
        now = System.currentTimeMillis()
    }
    Text(
        if (now - at < CHECKED_NOW_MS) stringResource(R.string.lake_checked_now)
        else stringResource(R.string.lake_checked_at, shortDate(context, at / 1000)),
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        style = Type.Caption,
        modifier = Modifier.padding(top = 4.dp),
    )
}

/** Within this long a check is "just now". */
private const val CHECKED_NOW_MS = 60_000L

/** An empty list's title and the line under it, and [more] under them. */
@Composable
private fun Words(title: String, text: String?, modifier: Modifier, more: @Composable () -> Unit = {}) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(title, style = Type.Heading, textAlign = TextAlign.Center)
        // A second line only when it tells something: a slow wait.
        text?.let {
            Text(
                it,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.widthIn(max = 280.dp).padding(horizontal = 32.dp),
            )
        }
        more()
    }
}

/** Numbers kept for each crest a frame. */
private const val CREST = 5

/** A reflection streak, in dp under the horizon. */
private class Streak(val dy: Float, val half: Float, val alpha: Float)

private val LAKE_HEIGHT = 124.dp
private val HORIZON = 40.dp
private val STREAKS = listOf(
    Streak(9f, 104f, 0.1f),
    Streak(20f, 84f, 0.075f),
    Streak(35f, 62f, 0.055f),
    Streak(55f, 40f, 0.035f),
)

/** The lake's water under a title. */
private val LAKE_TOP = 8.dp

/** How slowly a wait's swell comes and goes, in seconds, and how often a faint ring goes out. */
private const val SEA_EASE = 0.72f
private const val PULSE_EVERY = 2.6f

/** The surface: points across, dp between them, a wave's speed along it in dp a second, its banks dp from the middle. */
private const val SPAN = 101
private const val SPACING = 3f
private const val WAVE_SPEED = 150f
private const val BANKS = 150f
private const val STIFF = WAVE_SPEED * WAVE_SPEED / (SPACING * SPACING)

/**
 * A finger on the water: how deep a touch alone draws it down and the deepest a pull does, in
 * dp; how wide the dip is, in dp; and how firmly it holds the surface there, and calms it.
 */
private const val HOLD_TOUCH = 2f
private const val HOLD_DEEPEST = 18f
private const val HOLD_WIDTH = 22f
private const val HOLD_PULL = 260f
private const val HOLD_DAMP = 14f

/** Earlier shapes kept for nearer streaks, one every two steps. */
private const val PAST = 20
private const val LINE_POINTS = 104

/** How hard the tabs push the water, and the sharpest move it feels, in tabs a second squared. */
private const val JOLT = 6f
private const val JOLT_MAX = 800f

/** A swell's wave, by the deep-water law ω = √(g·k): longer waves run faster. */
private class Swell(length: Float, val amp: Float, val dir: Float, val phase: Float) {
    val k = 2f * PI.toFloat() / length
    val w = sqrt(SWELL_G * k)
}

private const val SWELL_G = 118f
private val SWELLS = listOf(
    Swell(150f, 1.25f, 1f, 0f),
    Swell(96f, 0.85f, -1f, 1.7f),
    Swell(60f, 0.5f, 1f, 4.1f),
    Swell(40f, 0.22f, -1f, 2.6f),
)

/** How far in from its sides, and up from where it ends, the water fades out, in dp. */
private const val EDGE_SIDE = 48f
private const val EDGE_NEAR = 28f

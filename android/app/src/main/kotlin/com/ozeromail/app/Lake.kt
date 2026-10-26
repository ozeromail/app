package com.ozeromail.app

import androidx.compose.animation.core.CubicBezierEasing
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
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameMillis
import androidx.compose.ui.Alignment
import androidx.compose.ui.MotionDurationScale
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt
import kotlin.random.Random
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

/**
 * The note of a list with nothing left in it: still water. A horizon and a soft reflection under
 * it, at rest until something moves it. With [drawIn], as the last message leaves, a drop lands
 * on the water and the horizon comes out of its waves. A touch sends a wave out too. [grey] water
 * is for plain empty folders: it never draws in. The words come in one way, once the list is known
 * to be empty, with [checked] under them when there is one. While there are [checking] words, the
 * water swells and they say what it waits for, as through a pull's sync while [refreshing]: water
 * never moves under words that say nothing is going on. As the list is [pulled], a drop gathers over the water,
 * growing with the pull, and falls on each of [drops], which carry how far it was pulled. The tabs'
 * [glide] sends a wave along it. [water] is how much of the water shows; [wordsModifier] moves
 * the words alone, while the water stays.
 */
@Composable
fun StillNote(
    title: String,
    text: String,
    drawIn: Boolean,
    grey: Boolean = false,
    checking: Pair<String, String>? = null,
    checkedAt: Long? = null,
    refreshing: Boolean = false,
    pulled: () -> Float = { 0f },
    drops: Flow<Float> = emptyFlow(),
    glide: () -> Float = { 0f },
    water: () -> Float = { 1f },
    wordsModifier: Modifier = Modifier,
) {
    val colors = MaterialTheme.colorScheme
    val still = rememberStill(fall = drawIn)
    val waiting = checking != null
    val known by animateFloatAsState(if (waiting) 0f else 1f, Motion.calm(WORDS_MS), label = "words")
    val waitWords = rememberLast(checking)
    val fading by remember { derivedStateOf { known < 1f } }
    LaunchedEffect(waiting, refreshing) { still.beat = waiting || refreshing }
    LaunchedEffect(drops) { drops.collect { still.release(it) } }
    Column(
        Modifier.fillMaxWidth().padding(top = LAKE_TOP, bottom = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // Grey water still takes a drop of the accent's color: a drop is a drop.
        val tint = colors.water(grey)
        Canvas(
            Modifier.fillMaxWidth().height(LAKE_HEIGHT).graphicsLayer { alpha = water() }.touches(still),
        ) {
            still.draw(this, tint, colors.accent, HORIZON, size.height, pulled(), FALL, glide())
        }
        Box(wordsModifier, contentAlignment = Alignment.TopCenter) {
            Words(
                title,
                text,
                Modifier.graphicsLayer {
                    val shown = still.words() * known
                    alpha = shown
                    translationY = (1f - shown) * 8.dp.toPx()
                },
            ) {
                checkedAt?.let { Checked(it) }
            }
            // Until then the water is not empty yet, only checking: these words take its place.
            if (fading) {
                waitWords?.let { (title, text) ->
                    Words(
                        title,
                        text,
                        Modifier.graphicsLayer {
                            alpha = 1f - known
                            translationY = known * 8.dp.toPx()
                        },
                    )
                }
            }
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
private fun Words(title: String, text: String, modifier: Modifier, more: @Composable () -> Unit = {}) {
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(title, style = Type.Heading, textAlign = TextAlign.Center)
        Text(
            text,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(max = 280.dp).padding(horizontal = 32.dp),
        )
        more()
    }
}

/**
 * The same water as an empty list's, under the title, in the gap of [depth] px the list opens:
 * the water stays where it is and the list uncovers it. [top] is where the gap begins, or null
 * while the title is off screen. As in the empty list, a drop gathers with [fraction] and falls on
 * each of [drops]; the water swells through the sync. [onCalm] hears whether the water has calmed
 * since, so the list can wait for it before it covers the water. [grey] as in [StillNote].
 */
@Composable
fun PullWater(
    fraction: () -> Float,
    depth: () -> Float,
    refreshing: Boolean,
    top: () -> Float?,
    drops: Flow<Float>,
    onCalm: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    grey: Boolean = false,
) {
    val colors = MaterialTheme.colorScheme
    val water = colors.water(grey)
    val still = rememberStill(fall = false)
    LaunchedEffect(refreshing) { still.beat = refreshing }
    LaunchedEffect(drops) { drops.collect { still.release(it) } }
    LaunchedEffect(still) { snapshotFlow { still.calmed }.collect(onCalm) }
    Canvas(modifier) {
        val depth = depth()
        val y = top() ?: run {
            // Out of sight, the water still calms, so the list need not wait for it.
            still.advance()
            return@Canvas
        }
        // Drawn even when closed, so the water hears that the pull is over.
        translate(top = y) {
            clipRect(top = 0f, bottom = depth) {
                still.draw(this, water, colors.accent, LAKE_TOP + HORIZON, maxOf(depth, PULL_GAP.toPx()), fraction(), PULL_FALL)
            }
        }
    }
}

/** The water's color: the accent, or the outline's grey for plain folders. */
private fun ColorScheme.water(grey: Boolean): Color = if (grey) outline else accent

/** How far a pull of [fraction] moves the list: with the finger up to the threshold, then less and less. */
fun Density.pullGap(fraction: Float): Float {
    val full = PULL_GAP.toPx()
    return if (fraction <= 1f) full * fraction else full * (1f + 0.6f * (1f - 1f / fraction))
}

/**
 * Runs the water's clock: every frame while something moves, and not at all for water at rest.
 * With the system's animations off, the water stands still.
 */
@Composable
private fun rememberStill(fall: Boolean): Still {
    // Only as it first shows: the caller's flag turns off right after.
    val still = remember { Still(fall) }
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

/** A touch on the water sends a wave out from under the finger. */
private fun Modifier.touches(still: Still) = pointerInput(still) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        if (still.ready) still.touch(down.position.x.toDp().value, down.position.y.toDp().value)
    }
}

/** A drop in the air; positions in dp, speed in dp a second. */
private class Drop(val x: Float, var y: Float, var v: Float, val land: Float, val r: Float, val point: Float, val opens: Boolean = false)

/** The tiny drop a landing throws back up. */
private class Jet(val x: Float, var y: Float, var v: Float, val base: Float)

/** The crests one impact sends out, [e] its energy, [t] seconds since. */
private class Wave(val x: Float, val y: Float, val e: Float, var t: Float = 0f)

/** How a drop hangs while it gathers. */
private class Hanging(val r: Float, val point: Float, val y: Float)

/**
 * The water's state, stepped in small fixed steps of real physics: drops fall with gravity,
 * throw a tiny drop back up as they land, and send out crests that slow as they spread. Nothing
 * moves without a cause. Not snapshot state: [now] is, and every frame redraws and steps from it.
 */
private class Still(private var falling: Boolean) {
    private val clock = mutableLongStateOf(0L)
    var now: Long
        get() = clock.longValue
        set(value) {
            if (begun < 0) begun = value
            clock.longValue = value
        }

    /** Wakes the clock of water at rest. */
    val wake = Channel<Unit>(Channel.CONFLATED)

    /** A wait: the water swells until it is over. */
    var beat = false
        set(value) {
            field = value
            wake.trySend(Unit)
        }

    private val drops = ArrayList<Drop>()
    private val jets = ArrayList<Jet>()
    private val waves = ArrayList<Wave>()
    private val path = Path()
    private var hairline = Stroke(1f)
    private var frozen = false
    private var begun = -1L
    // When the drop that opens the water landed.
    private var opened = -1L
    private var stepped = -1L
    private var level = if (falling) 0f else 1f
    private var rising = false
    private var pulled = 0f
    private var released = false
    private var letGoAt = 0f
    private var spent = false
    // How far off the middle the next drop gathers, in dp: one tug after another, no two land in one place.
    private var aim = 0f
    private var sincePulse = 0f
    private var sea = 0f
    private var wobble = 0f
    private var wobbleV = 0f
    private var wasFull = false
    private var calm = 0f
    private var horizon = 0f
    private var land = 0f
    private var middle = 0f
    private var tint = Color.Unspecified
    private var line: Brush = SolidColor(Color.Transparent)
    private var glow: Brush = SolidColor(Color.Transparent)

    // The surface along the horizon: how far each point across is lifted, in dp, and how fast it
    // moves. Drops, touches and the tabs push it; a wave runs along it, comes back off the banks
    // and dies away.
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

    /** Whether a wait's swell has gone, or never came. */
    var calmed by mutableStateOf(true)
        private set

    /** Whether the surface is level and no crest runs over it, so lines are drawn straight. */
    private val flat get() = sea == 0f && settled && waves.isEmpty()

    /** Whether the surface has stopped moving. */
    private val settled get() = rough < 0.05f && moving < 0.01f

    val ready get() = !frozen && level > 0.5f && middle > 0f

    /** Whether something on the water is moving, so every frame counts. */
    val busy
        get() = beat || released || drops.isNotEmpty() || jets.isNotEmpty() || waves.isNotEmpty() || rising ||
            abs(wobble) > 0.002f || abs(wobbleV) > 0.02f || (falling && now - wordsFrom < WORDS_MS) ||
            !flat || abs(jolt) > 1f || abs(glideV) > 0.01f

    /** The clock goes to sleep: the next frame starts afresh rather than a step late. */
    fun rest() {
        stepped = -1
        glideV = 0f
        jolt = 0f
    }

    /** Water that never moves, for when the system's animations are off. */
    fun hold() {
        frozen = true
        falling = false
        drops.clear()
        level = 1f
        now = 1
    }

    /** When the words start to come in: as the drop lands, or once it is long overdue. */
    private val wordsFrom get() = (begun + OPEN_WAIT_MS).let { overdue -> if (opened >= 0) minOf(opened, overdue) else overdue }

    /** How far the words have come in: once the drop is in the water when it falls, at once otherwise. */
    fun words(): Float {
        if (!falling) return 1f
        if (begun < 0) return 0f
        return Calm.transform(((now - wordsFrom) / WORDS_MS.toFloat()).coerceIn(0f, 1f))
    }

    /** The gathered drop lets go, on the next step, as it hung at a pull of [fraction]; one a pull. */
    fun release(fraction: Float) {
        if (!ready || spent) return
        released = true
        letGoAt = fraction
        wake.trySend(Unit)
    }

    /** A wave from under a finger at [x], [y] dp. */
    fun touch(x: Float, y: Float) {
        waves += Wave(x, maxOf(y, horizon + 6f), 0.55f)
        kick(x, -22f)
        wake.trySend(Unit)
    }

    /** A drop for a pull of [p]: it keeps growing past the threshold, a little slower, and draws to a point. */
    private fun hanging(p: Float): Hanging {
        val q = if (p <= 1f) p else 1f + 0.3f * (1f - 1f / p)
        val r = 1.2f + 5.2f * q.pow(0.75f)
        return Hanging(r, min(1f, q) * 1.1f, 12f + 10f * min(1f, q) + r)
    }

    private fun letGo(h: Hanging, land: Float) {
        drops += Drop(middle + aim, h.y, 0f, land, h.r, h.point)
        wobble = 0f
        wobbleV = 0f
    }

    private fun step(dt: Float, land: Float) {
        if (released) {
            released = false
            spent = true
            letGo(hanging(letGoAt), land)
        }
        // A wait swells the water once nothing is falling; a faint ring now and then comes from
        // where the drop fell. When the wait is over, the swell lies down.
        val swelling = beat && ready && drops.isEmpty()
        sea += ((if (swelling) 1f else 0f) - sea) * min(1f, dt / SEA_EASE)
        if (!swelling && sea < 0.002f) sea = 0f
        // A drop on its way is not calm yet either.
        val lying = !swelling && sea < 0.06f && !released && drops.isEmpty()
        if (lying != calmed) calmed = lying
        sincePulse = if (swelling) sincePulse + dt else 0f
        if (sincePulse > PULSE_EVERY) {
            sincePulse -= PULSE_EVERY
            waves += Wave(middle, land, 0.16f)
        }
        // The tabs' push, smoothed: a finger's speed comes in steps.
        jolt += (push - jolt) * min(1f, dt * 25f)
        flow(dt)
        // The hanging drop gives a little jolt as it fills, and trembles as it settles.
        val full = !spent && pulled >= 1f
        if (full && !wasFull) wobbleV += 2.4f
        wasFull = full
        wobbleV += (-220f * wobble - 7f * wobbleV) * dt
        wobble += wobbleV * dt

        drops.removeAll { d ->
            d.v += G * dt
            d.y += d.v * dt
            if (d.y < d.land) return@removeAll false
            val e = maxOf(0.18f, min(1f, d.r / 6.6f))
            waves += Wave(d.x, d.land, e)
            kick(d.x, -40f * e)
            // A real drop throws a tiny one back up, which falls again as a smaller ring.
            if (d.r > 2f) jets += Jet(d.x, d.land, -sqrt(2f * G * 4f * e + 40f), d.land)
            if (d.opens) {
                rising = true
                opened = now
            }
            true
        }
        jets.removeAll { j ->
            j.v += G * dt
            j.y += j.v * dt
            val down = j.v > 0f && j.y >= j.base
            if (down) waves += Wave(j.x, j.base, 0.32f)
            down
        }
        var energy = 0f
        waves.removeAll { w ->
            w.t += dt
            val a = w.e * exp(-w.t / 1.5f)
            energy += a
            a < 0.015f
        }
        calm += (min(1f, energy) - calm) * min(1f, dt * 2f)
        if (rising) {
            level = min(1f, level + dt * 1000f / LEVEL_MS)
            if (level >= 1f) rising = false
        }
    }

    /** One step of the surface: a damped wave between two banks, thicker once the wait is over. */
    private fun flow(dt: Float) {
        if (settled && abs(jolt) < 1f) return
        val damping = if (beat) 0.9f else 1.5f
        val mid = (SPAN - 1) / 2f
        for (i in 0 until SPAN) {
            val bend = height[if (i > 0) i - 1 else 1] - 2 * height[i] + height[if (i < SPAN - 1) i + 1 else SPAN - 2]
            // A sideways jolt of the tabs presses the water toward one bank.
            speed[i] += (STIFF * bend - damping * speed[i] - jolt * JOLT * (i - mid) / mid) * dt
        }
        var fast = 0f
        rough = 0f
        for (i in 0 until SPAN) {
            height[i] += speed[i] * dt
            fast = maxOf(fast, abs(speed[i]))
            rough = maxOf(rough, abs(height[i]))
        }
        moving += (min(1f, fast / 25f) - moving) * min(1f, dt * 4f)
        if (settled && abs(jolt) < 1f) {
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

    /** Pushes the surface at [x] dp. Water is not lost: what is pressed down rises around it. */
    private fun kick(x: Float, amount: Float) {
        var mean = 0f
        for (i in 0 until SPAN) {
            val d = middle - BANKS + i * SPACING - x
            speed[i] += amount * exp(-d * d / (2 * KICK * KICK))
            mean += speed[i]
        }
        mean /= SPAN
        for (i in 0 until SPAN) speed[i] -= mean
        moving = maxOf(moving, 0.02f)
        wake.trySend(Unit)
    }

    /** How far the surface is lifted at [x] dp on a line [near] the viewer, 0 at the horizon. */
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

    /**
     * Steps the water up to [now], for the tabs standing at [glide]. Drawing does it, and so does
     * water out of sight, which is not drawn but still calms.
     */
    fun advance(glide: Float = glidAt) {
        if (glide != glidAt && stepped < 0) wake.trySend(Unit)
        var dt = if (stepped < 0) 0f else min(0.05f, (now - stepped) / 1000f)
        stepped = now
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
            step(s, land)
            dt -= s
        }
        if (busy) wake.trySend(Unit)
    }

    /** Brushes one unit across, scaled into place, so a frame makes no new shaders. */
    private fun paint(color: Color) {
        if (color == tint) return
        tint = color
        line = Brush.horizontalGradient(listOf(Color.Transparent, color, Color.Transparent), -1f, 1f)
        // Eased, so the glow has no rim where it ends.
        glow = Brush.radialGradient(listOf(color, Color.Transparent), Offset.Zero, 1f)
    }

    /**
     * The horizon lies [surface] down, drops gather [pull] near the top and land [fall] under the
     * horizon; the water ends at [bottom] px. Water is [water], drops [drop]. The tabs stand at [glide].
     */
    fun draw(scope: DrawScope, water: Color, drop: Color, surface: Dp, bottom: Float, pull: Float, fall: Dp, glide: Float = 0f) = with(scope) {
        val u = density
        val hz = surface.value
        horizon = hz
        middle = size.width / u / 2
        // A pull moves nothing else, so it wakes the clock. Once a drop has gone, the next gathers
        // as the finger pulls again, somewhere else.
        if (pull != pulled) wake.trySend(Unit)
        if (spent && !released && pull > pulled) {
            spent = false
            aim = nextAim(aim)
        }
        pulled = pull
        land = hz + fall.value
        if (falling && level == 0f && !rising && drops.isEmpty() && begun >= 0) {
            // The last message has left: a drop falls and the water comes out of its waves.
            drops += Drop(middle, hz - 70f, 120f, land, 2.8f, 0.9f, opens = true)
        }
        advance(glide)
        gatherCrests()
        paint(water)
        val t = (now - begun) / 1000f
        val cx = middle * u
        val hzPx = hz * u
        val lv = Calm.transform(level)
        val stroke = 1.dp.toPx()
        val below = bottom / u
        clipRect(top = hzPx, bottom = bottom) {
            if (lv > 0f) {
                // The reflection: a soft glow under the horizon, flattened by perspective, and a
                // few streaks that follow the surface. It breathes deeper while the water swells.
                val r = 150.dp.toPx() * lv + 1f
                // Never deeper than the water, so the glow fades out before it ends.
                val flatten = r * 0.32f * (1f + 0.14f * sea * sin(t * 1.3f))
                withTransform({
                    translate(cx, hzPx)
                    scale(r, flatten, Offset.Zero)
                }) {
                    drawRect(glow, Offset(-1f, 0f), Size(2f, (bottom - hzPx) / flatten), alpha = 0.14f + 0.04f * sea)
                }
                // A crest passing through bends them; nothing else moves them.
                STREAKS.forEachIndexed { i, s ->
                    waterline(s.alpha, middle, s.half * lv, hz + s.dy, (i + 1) / 4f, t, water, stroke)
                }
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
            // Each drop shows in the water under it, clearer as it comes close.
            for (d in drops) {
                val near = 1f - (d.land - d.y) / 90f
                if (near <= 0f || d.y >= d.land) continue
                val y = d.land + (d.land - d.y) * 0.3f
                drawOval(drop.copy(alpha = 0.22f * near * edge(d.x, y, below)), Offset((d.x - d.r) * u, (y - d.r * 0.45f) * u), Size(d.r * 2 * u, d.r * 0.9f * u))
            }
        }
        if (lv > 0f) waterline(0.55f, middle, 150f * lv, hz, 0f, t, water, stroke)
        // The gathering drop grows with the pull, a round bead that fills into a drop.
        val gathering = if (!spent && !released && ready) pull else 0f
        if (gathering > 0.02f) {
            val h = hanging(gathering)
            bead(drop, middle + aim, h.y, h.r, h.point, 1f + wobble)
        }
        // Falling, it draws out with its speed.
        for (d in drops) bead(drop, d.x, d.y, d.r, d.point, 1f + min(0.35f, d.v / 1100f))
        for (j in jets) drawCircle(drop.copy(alpha = 0.6f), 1.3.dp.toPx(), Offset(j.x * u, j.y * u))
    }

    /**
     * How much of the water shows at [x], [y] dp: all of it in the middle, fading out toward its
     * sides and toward the eye before it ends [below] dp down, so a wave never ends on an edge.
     */
    private fun edge(x: Float, y: Float, below: Float): Float {
        val side = min(x, middle * 2 - x) / EDGE_SIDE
        val near = (below - y) / EDGE_NEAR
        return 1f
    }

    private fun smooth(v: Float): Float = v.coerceIn(0f, 1f).let { it * it * (3f - 2f * it) }

    /** A water drop, in dp: round below, rising to a soft point above as [point] goes from 0 to 1. */
    private fun DrawScope.bead(color: Color, x: Float, y: Float, r: Float, point: Float, stretch: Float) {
        val u = density
        path.reset()
        for (i in 0..DROP_STEPS) {
            val a = i * 2f * PI.toFloat() / DROP_STEPS
            val c = cos(a)
            val px = r * sin(a) * abs(sin(a / 2)).pow(point)
            val py = -r * c * (if (c > 0f) 1f + 0.45f * point else 1f) * stretch
            if (i == 0) path.moveTo((x + px) * u, (y + py) * u) else path.lineTo((x + px) * u, (y + py) * u)
        }
        path.close()
        // Glass: a hairline like the water's, brighter below, over the faintest tint.
        drawPath(path, color.copy(alpha = 0.13f))
        val top = (y - r * (1f + 0.45f * point) * stretch) * u
        val rim = Brush.verticalGradient(listOf(color.copy(alpha = 0.3f), color.copy(alpha = 0.95f)), top, (y + r) * u)
        if (hairline.width != 1.dp.toPx()) hairline = Stroke(1.dp.toPx())
        drawPath(path, rim, style = hairline)
        // Light inside the lower curve, as in a lens.
        if (r < 2.4f) return
        val lens = r * 0.6f * u
        drawArc(color.copy(alpha = 0.5f), 112f, 59f, false, Offset(x * u - lens, y * u - lens), Size(lens * 2, lens * 2), style = hairline)
    }

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

/** Where a drop gathers after one at [last], in dp off the middle: anywhere near, but not on the same spot. */
private fun nextAim(last: Float): Float {
    val next = (Random.nextFloat() * 2f - 1f) * AIM
    if (abs(next - last) >= AIM / 3f) return next
    return (next + if (next > last) AIM / 3f else -AIM / 3f).coerceIn(-AIM, AIM)
}

/** Numbers kept for each crest a frame. */
private const val CREST = 5

/** How far off the middle a drop gathers, in dp. */
private const val AIM = 64f

/** A reflection streak, in dp under the horizon. */
private class Streak(val dy: Float, val half: Float, val alpha: Float)

internal val LAKE_HEIGHT = 124.dp
internal val HORIZON = 40.dp
private val STREAKS = listOf(
    Streak(9f, 104f, 0.1f),
    Streak(20f, 84f, 0.075f),
    Streak(35f, 62f, 0.055f),
    Streak(55f, 40f, 0.035f),
)
private val PULL_GAP = 88.dp
internal val FALL = 40.dp

/** The lake's water under a title; the pull's lies at the same height, so the two are one. */
private val LAKE_TOP = 8.dp

/** The pull's water is shallower than a lake: the gap is all there is of it. */
private val PULL_FALL = 24.dp

/** Gravity, dp/s²: a drop falls its 40–50 dp in about a third of a second. */
private const val G = 760f
private const val DROP_STEPS = 48

/** How slowly a wait's swell comes and goes, in seconds, and how often a faint ring goes out. */
private const val SEA_EASE = 0.72f
private const val PULSE_EVERY = 2.6f

/** The surface: points across, dp between them, a wave's speed along it in dp a second, its banks dp from the middle. */
private const val SPAN = 101
private const val SPACING = 3f
private const val WAVE_SPEED = 150f
private const val BANKS = 150f
private const val STIFF = WAVE_SPEED * WAVE_SPEED / (SPACING * SPACING)

/** How wide a push is, in dp. */
private const val KICK = 13f

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
private const val LEVEL_MS = 1900f
/** How far in from its sides, and up from where it ends, the water fades out, in dp. */
private const val EDGE_SIDE = 48f
private const val EDGE_NEAR = 28f

/** How long the words take to come in, however they come. */
private const val WORDS_MS = 400

/** How long the clock waits for the drop that opens the water, should it never be drawn; the words come then. */
private const val OPEN_WAIT_MS = 3000L

private val Calm = CubicBezierEasing(0.2f, 0f, 0f, 1f)

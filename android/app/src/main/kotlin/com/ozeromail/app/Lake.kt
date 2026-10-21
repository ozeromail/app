package com.ozeromail.app

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
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
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
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
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow

/**
 * The note of a list with nothing left in it: still water. A horizon and a soft reflection under
 * it, at rest until something moves it. With [drawIn], as the last message leaves, a drop lands
 * on the water and the horizon comes out of its waves. A touch sends a wave out too. [grey] water
 * is for plain empty folders: it never draws in. While [waiting] for the list's mail, the words
 * hold back and a long wait drips. As the list is [pulled], a drop gathers over the water,
 * growing with the pull, and falls on each of [drops].
 */
@Composable
fun StillNote(
    title: String,
    text: String,
    drawIn: Boolean,
    grey: Boolean = false,
    waiting: Boolean = false,
    pulled: () -> Float = { 0f },
    drops: Flow<Unit> = emptyFlow(),
) {
    val colors = MaterialTheme.colorScheme
    val still = rememberStill(fall = drawIn)
    // The words wait for the first sync only; once said, a later one just stirs the water.
    var said by remember { mutableStateOf(!waiting) }
    val words by animateFloatAsState(if (said) 1f else 0f, Motion.calm(Motion.SMALL), label = "words")
    LaunchedEffect(waiting) {
        still.beat = waiting
        if (!waiting) said = true
    }
    LaunchedEffect(drops) { drops.collect { still.release() } }
    Column(
        Modifier.fillMaxWidth().padding(top = LAKE_TOP, bottom = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        // Grey water still takes a drop of the accent's colour: a drop is a drop.
        val water = if (grey) colors.outline else colors.primary
        Canvas(Modifier.fillMaxWidth().height(LAKE_HEIGHT).touches(still)) {
            still.draw(this, water, colors.primary, HORIZON, size.height, pulled(), FALL)
        }
        Column(
            Modifier.graphicsLayer {
                val shown = still.words() * words
                alpha = shown
                translationY = (1f - shown) * 8.dp.toPx()
            },
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(title, style = Type.Heading, textAlign = TextAlign.Center)
            Text(
                text,
                color = colors.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.widthIn(max = 280.dp).padding(horizontal = 32.dp),
            )
        }
    }
}

/**
 * The same water as an empty list's, under the title, in the gap the list opens as it is
 * pulled: the water stays where it is and the list uncovers it. [top] is where the gap begins,
 * or null while the title is off screen. As in the empty list, a drop gathers with [fraction]
 * and falls on each of [drops]; a long refresh drips.
 */
@Composable
fun PullWater(fraction: () -> Float, refreshing: Boolean, top: () -> Float?, drops: Flow<Unit>, modifier: Modifier = Modifier) {
    val water = MaterialTheme.colorScheme.primary
    val pulled by remember { derivedStateOf { fraction() > 0f } }
    val still = rememberStill(fall = false, running = pulled || refreshing)
    LaunchedEffect(refreshing) { still.beat = refreshing }
    LaunchedEffect(drops) { drops.collect { still.release() } }
    Canvas(modifier) {
        val depth = pullGap(fraction())
        val y = top() ?: return@Canvas
        // Drawn even when closed, so the water hears that the pull is over.
        translate(top = y) {
            clipRect(top = 0f, bottom = depth) {
                still.draw(this, water, water, LAKE_TOP + HORIZON, maxOf(depth, PULL_GAP.toPx()), if (refreshing) 0f else fraction(), PULL_FALL)
            }
        }
    }
}

/** How far a pull of [fraction] moves the list: with the finger up to the threshold, then less and less. */
fun Density.pullGap(fraction: Float): Float {
    val full = PULL_GAP.toPx()
    return if (fraction <= 1f) full * fraction else full * (1f + 0.6f * (1f - 1f / fraction))
}


/**
 * Runs the water's clock while [running]: every frame while something moves, and not at all for
 * water at rest. With the system's animations off, the water stands still.
 */
@Composable
private fun rememberStill(fall: Boolean, running: Boolean = true): Still {
    // Only as it first shows: the caller's flag turns off right after.
    val still = remember { Still(fall) }
    LaunchedEffect(still, running) {
        if (coroutineContext[MotionDurationScale]?.scaleFactor == 0f) {
            still.hold()
            return@LaunchedEffect
        }
        if (!running) return@LaunchedEffect
        while (true) {
            withFrameMillis { still.now = it }
            if (!still.busy) still.wake.receive()
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

    /** Rings of a wait: a long one drips, a drop every few seconds. */
    var beat = false
        set(value) {
            if (value && !field) waited = 0f
            field = value
            wake.trySend(Unit)
        }

    private val drops = ArrayList<Drop>()
    private val jets = ArrayList<Jet>()
    private val waves = ArrayList<Wave>()
    private val path = Path()
    private var frozen = false
    private var begun = -1L
    private var stepped = -1L
    private var level = if (falling) 0f else 1f
    private var rising = false
    private var pulled = 0f
    private var released = false
    private var spent = false
    private var waited = 0f
    private var sinceDrip = 0f
    private var drip = 0f
    private var wobble = 0f
    private var wobbleV = 0f
    private var wasFull = false
    private var calm = 0f
    private var horizon = 0f
    private var middle = 0f
    private var tint = Color.Unspecified
    private var line: Brush = SolidColor(Color.Transparent)
    private var glow: Brush = SolidColor(Color.Transparent)

    val ready get() = !frozen && level > 0.5f && middle > 0f

    /** Whether something on the water is moving, so every frame counts. */
    val busy
        get() = beat || released || drops.isNotEmpty() || jets.isNotEmpty() || waves.isNotEmpty() || rising ||
            abs(wobble) > 0.002f || abs(wobbleV) > 0.02f || (falling && now - begun < WORDS + WORDS_MS)

    /** Water that never moves, for when the system's animations are off. */
    fun hold() {
        frozen = true
        falling = false
        drops.clear()
        level = 1f
        now = 1
    }

    /** How far the words have come in: after the drop when it falls, at once otherwise. */
    fun words(): Float {
        if (!falling) return 1f
        val t = now - begun
        return if (begun < 0) 0f else Calm.transform(((t - WORDS) / WORDS_MS).coerceIn(0f, 1f))
    }

    /** The gathered drop lets go, on the next step; one a pull. */
    fun release() {
        if (!ready || spent) return
        released = true
        wake.trySend(Unit)
    }

    fun touch(x: Float, y: Float) {
        waves += Wave(x, maxOf(y, horizon + 6f), 0.55f)
        wake.trySend(Unit)
    }

    /** A drop for a pull of [p]: it keeps growing past the threshold, a little slower, and draws to a point. */
    private fun hanging(p: Float): Hanging {
        val q = if (p <= 1f) p else 1f + 0.3f * (1f - 1f / p)
        val r = 1.2f + 5.2f * q.pow(0.75f)
        return Hanging(r, min(1f, q) * 1.1f, 12f + 10f * min(1f, q) + r)
    }

    private fun letGo(h: Hanging, land: Float) {
        drops += Drop(middle, h.y, 0f, land, h.r, h.point)
        wobble = 0f
        wobbleV = 0f
    }

    private fun step(dt: Float, land: Float) {
        waited += dt
        sinceDrip += dt
        if (released) {
            released = false
            spent = true
            letGo(hanging(maxOf(pulled, 1f)), land)
            sinceDrip = 0f
            drip = 0f
        }
        // Nothing gathers again until the list is back at rest.
        if (pulled <= 0f && drops.isEmpty() && !beat) spent = false
        // A long wait drips: a drop gathers by itself, falls, and the next waits its turn.
        if (beat && waited > DRIP_AFTER && ready) {
            if (sinceDrip > DRIP_EVERY - DRIP_GATHER) drip = min(1f, drip + dt / DRIP_GATHER)
            if (drip >= 1f) {
                letGo(hanging(1f), land)
                drip = 0f
                sinceDrip = 0f
            }
        } else if (!beat) {
            drip = 0f
        }
        // The hanging drop gives a little jolt as it fills, and trembles as it settles.
        val full = !spent && !beat && pulled >= 1f
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
            // A real drop throws a tiny one back up, which falls again as a smaller ring.
            if (d.r > 2f) jets += Jet(d.x, d.land, -sqrt(2f * G * 4f * e + 40f), d.land)
            if (d.opens) rising = true
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

    /** Brushes one unit across, scaled into place, so a frame makes no new shaders. */
    private fun paint(color: Color) {
        if (color == tint) return
        tint = color
        line = Brush.horizontalGradient(listOf(Color.Transparent, color, Color.Transparent), -1f, 1f)
        glow = Brush.radialGradient(listOf(color, Color.Transparent), Offset.Zero, 1f)
    }

    /**
     * The horizon lies [line] down, drops gather [pull] near the top and land [fall] under the
     * horizon; the water ends at [bottom] px. Water is [water], drops [drop].
     */
    fun draw(scope: DrawScope, water: Color, drop: Color, line: Dp, bottom: Float, pull: Float, fall: Dp) = with(scope) {
        val u = density
        val hz = line.value
        val land = hz + fall.value
        horizon = hz
        middle = size.width / u / 2
        pulled = pull
        if (falling && level == 0f && !rising && drops.isEmpty() && begun >= 0) {
            // The last message has left: a drop falls and the water comes out of its waves.
            drops += Drop(middle, hz - 70f, 120f, land, 2.8f, 0.9f, opens = true)
        }
        var dt = if (stepped < 0) 0f else min(0.05f, (now - stepped) / 1000f)
        stepped = now
        while (dt > 0f) {
            val s = min(dt, 1f / 120f)
            step(s, land)
            dt -= s
        }
        if (busy) wake.trySend(Unit)
        paint(water)
        val t = now / 1000f
        val cx = middle * u
        val hzPx = hz * u
        val lv = Calm.transform(level)
        val stroke = 1.dp.toPx()
        clipRect(top = hzPx, bottom = bottom) {
            if (lv > 0f) {
                // The reflection: a soft glow under the horizon, flattened by perspective, and a
                // few streaks that only stir when the water does.
                val r = 150.dp.toPx() * lv + 1f
                withTransform({
                    translate(cx, hzPx)
                    scale(r, r * 0.32f, Offset.Zero)
                }) {
                    drawRect(glow, Offset(-1f, 0f), Size(2f, (bottom - hzPx) / (r * 0.32f)), alpha = 0.14f)
                }
                STREAKS.forEachIndexed { i, s ->
                    val stir = calm * 5f * sin(t * (2.1f + i * 0.7f) + i)
                    val y = hz + s.dy + calm * 0.8f * sin(t * 3f + i)
                    val half = s.half * lv * (1f + calm * 0.06f * sin(t * 2.3f + i))
                    faded(s.alpha, cx + stir * u, half * u, y * u, stroke)
                }
            }
            // Waves: two or three crests an impact, slowing as they spread, fading with distance.
            for (w in waves) {
                val spread = 64f * sqrt(w.e) * (1f - exp(-w.t / 0.85f)) + 7f * w.t
                val depth = ((w.y - hz) / 220f).coerceIn(0f, 1f)
                for (k in 0..2) {
                    val r = spread - k * (6f + 5f * w.t)
                    if (r <= 0.5f) continue
                    val amp = w.e * exp(-w.t / 1.5f) * (1f - k * 0.32f) / sqrt(1f + r / 40f)
                    val alpha = 0.7f * amp * min(1f, w.t * 12f)
                    if (alpha < 0.01f) continue
                    val rx = r * (1f + 0.8f * depth) * u
                    val ry = rx * (0.13f + 0.3f * depth)
                    val at = Offset(w.x * u - rx, w.y * u - ry)
                    val oval = Size(rx * 2, ry * 2)
                    // The near side of a crest catches more light than the far one.
                    drawArc(water.copy(alpha = min(1f, alpha * 1.15f)), 0f, 180f, false, at, oval, style = Stroke(stroke))
                    drawArc(water.copy(alpha = alpha * 0.6f), 180f, 180f, false, at, oval, style = Stroke(stroke))
                }
            }
            // Each drop shows in the water under it, clearer as it comes close.
            for (d in drops) {
                val near = 1f - (d.land - d.y) / 90f
                if (near <= 0f || d.y >= d.land) continue
                val y = d.land + (d.land - d.y) * 0.3f
                drawOval(drop.copy(alpha = 0.22f * near), Offset((d.x - d.r) * u, (y - d.r * 0.45f) * u), Size(d.r * 2 * u, d.r * 0.9f * u))
            }
        }
        if (lv > 0f) faded(0.55f, cx, 150.dp.toPx() * lv, hzPx, stroke)
        // The gathering drop grows with the pull, a round bead that fills into a drop.
        val gathering = maxOf(if (!spent && !beat && ready) pull else 0f, drip)
        if (gathering > 0.02f) {
            val h = hanging(gathering)
            bead(drop, middle, h.y, h.r, h.point, 1f + wobble)
        }
        // Falling, it draws out with its speed.
        for (d in drops) bead(drop, d.x, d.y, d.r, d.point, 1f + min(0.35f, d.v / 1100f))
        for (j in jets) drawCircle(drop, 1.3.dp.toPx(), Offset(j.x * u, j.y * u))
    }

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
        drawPath(path, color)
        // A small highlight, so the drop reads as water rather than a dot.
        if (r < 2f) return
        val gx = (x - r * 0.38f) * u
        val gy = (y - r * 0.1f) * u
        rotate(20f, Offset(gx, gy)) {
            drawOval(Color.White.copy(alpha = 0.4f), Offset(gx - r * 0.16f * u, gy - r * 0.3f * u), Size(r * 0.32f * u, r * 0.6f * u))
        }
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
private val PULL_GAP = 88.dp
private val FALL = 40.dp

/** The lake's water under a title; the pull's lies at the same height, so the two are one. */
private val LAKE_TOP = 8.dp

/** The pull's water is shallower than a lake: the gap is all there is of it. */
private val PULL_FALL = 24.dp

/** Gravity, dp/s²: a drop falls its 24–40 dp in about a quarter of a second. */
private const val G = 760f
private const val DROP_STEPS = 48
private const val DRIP_AFTER = 1.4f
private const val DRIP_EVERY = 3f
private const val DRIP_GATHER = 1.2f
private const val LEVEL_MS = 1900f
private const val WORDS = 1300f
private const val WORDS_MS = 1000f

private val Calm = CubicBezierEasing(0.2f, 0f, 0f, 1f)

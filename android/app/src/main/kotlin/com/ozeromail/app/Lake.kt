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
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.hypot
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt
import kotlin.random.Random
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The note of a list with nothing left in it: still water. A horizon, a soft reflection under
 * it, and rings that lie on the water in perspective. With [drawIn], as the last message leaves,
 * a drop lands on the water and the horizon comes out of its rings. A touch leaves rings too.
 * [grey] water is for plain empty folders: it never draws in, and only moves when touched. While
 * [waiting] for a list's first mail, rings spread one after another and the words hold back.
 * As the list is [pulled], a drop gathers over the water, full at the threshold, and falls on
 * each of [drops].
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
    val still = rememberStill(fall = drawIn, ambient = !grey)
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
        val water = if (grey) colors.outline else colors.primary
        Canvas(Modifier.fillMaxWidth().height(LAKE_HEIGHT).touches(still)) {
            still.draw(this, water, HORIZON, size.height, pulled())
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
 * and falls on each of [drops]; while [refreshing], quiet rings spread one after another.
 */
@Composable
fun PullWater(fraction: () -> Float, refreshing: Boolean, top: () -> Float?, drops: Flow<Unit>, modifier: Modifier = Modifier) {
    val water = MaterialTheme.colorScheme.primary
    val pulled by remember { derivedStateOf { fraction() > 0f } }
    val still = rememberStill(fall = false, ambient = false, running = pulled || refreshing)
    LaunchedEffect(refreshing) { still.beat = refreshing }
    LaunchedEffect(drops) { drops.collect { still.release() } }
    Canvas(modifier) {
        val depth = pullGap(fraction())
        val y = top() ?: return@Canvas
        if (depth <= 0f) return@Canvas
        translate(top = y) {
            clipRect(top = 0f, bottom = depth) {
                still.draw(this, water, LAKE_TOP + HORIZON, maxOf(depth, PULL_GAP.toPx()), if (refreshing) 0f else fraction(), PULL_HANG, PULL_FALL)
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
 * Runs the water's clock while [running]: every frame while something moves, a few times a
 * second for living water at rest, and not at all for water nobody touches. With the system's
 * animations off, the water stands still.
 */
@Composable
private fun rememberStill(fall: Boolean, ambient: Boolean, running: Boolean = true): Still {
    // Only as it first shows: the caller's flag turns off right after.
    val still = remember { Still(fall, ambient) }
    LaunchedEffect(still, running) {
        if (coroutineContext[MotionDurationScale]?.scaleFactor == 0f) {
            still.hold()
            return@LaunchedEffect
        }
        if (!running) return@LaunchedEffect
        while (true) {
            withFrameMillis { still.now = it }
            when {
                still.busy -> Unit
                // Living water breathes a few times a second, and a touch wakes it at once.
                still.ambient -> withTimeoutOrNull(IDLE_MS) { still.wake.receive() }
                else -> still.wake.receive()
            }
        }
    }
    return still
}

/** A touch on the water leaves a ring and a softer one behind it; a finger moving, a wake. */
private fun Modifier.touches(still: Still) = pointerInput(still) {
    val trail = 14.dp.toPx()
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        if (!still.ready) return@awaitEachGesture
        still.touch(down.position, 1f, echo = true)
        var last = down.position
        var at = still.now
        while (true) {
            val change = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: break
            if (!change.pressed) break
            val p = change.position
            if (hypot(p.x - last.x, p.y - last.y) > trail && still.now - at > 60) {
                still.touch(p, 0.4f, echo = false)
                last = p
                at = still.now
            }
        }
    }
}

/** A ring on the water; one made by a touch starts on the next frame, [after] it. */
private class Ring(val x: Float, val y: Float, val size: Float, var start: Long, val after: Long = 0)

/** The water's state. Not snapshot state: [now] is, and every frame redraws from it. */
private class Still(private var falling: Boolean, var ambient: Boolean) {
    private val clock = mutableLongStateOf(0L)
    var now: Long
        get() = clock.longValue
        set(value) {
            if (begun < 0) begin(value)
            // A released drop falls from the first fresh frame: a draw before it may see an old time.
            if (pullDrop == RELEASED) pullDrop = value
            clock.longValue = value
        }

    /** Wakes the clock of water at rest when it is touched. */
    val wake = Channel<Unit>(Channel.CONFLATED)

    private var level = if (falling) 0f else 1f
    private val rings = ArrayList<Ring>()
    private var frozen = false
    private var begun = -1L
    private var drop = -1L
    private var pullDrop = -1L
    private var land = -1L
    private var next = 0L
    private var lastBeat = 0L
    private var horizon = 0f
    private var width = 0f
    private var tint = Color.Unspecified
    private var line: Brush = SolidColor(Color.Transparent)
    private var glow: Brush = SolidColor(Color.Transparent)

    /** Rings while the list refreshes. */
    var beat = false
        set(value) {
            field = value
            wake.trySend(Unit)
        }

    val ready get() = !frozen && level > 0.5f && width > 0f

    /** Whether something on the water is moving, so every frame counts. */
    val busy get() = beat || drop >= 0 || pullDrop != -1L || land >= 0 || rings.isNotEmpty() || (falling && now - begun < WORDS + WORDS_MS)

    private fun begin(at: Long) {
        begun = at
        next = at + 9000
        if (falling) {
            drop = at
            level = 0f
        }
    }

    /** Water that never moves, for when the system's animations are off. */
    fun hold() {
        frozen = true
        falling = false
        ambient = false
        drop = -1
        level = 1f
        now = 1
    }

    /** How far the words have come in: after the drop when it falls, at once otherwise. */
    fun words(): Float {
        if (!falling) return 1f
        val t = now - begun
        return if (begun < 0) 0f else Calm.transform(((t - WORDS) / WORDS_MS).coerceIn(0f, 1f))
    }

    /** The gathered drop lets go and falls on the water, from the next frame; one at a time. */
    fun release() {
        if (!ready || pullDrop != -1L) return
        pullDrop = RELEASED
        wake.trySend(Unit)
    }

    /** The clock may be asleep, so its time is old: the ring starts when the next frame comes. */
    fun touch(at: Offset, size: Float, echo: Boolean) {
        ring(at.x, at.y, size, NEXT_FRAME, echo)
        wake.trySend(Unit)
    }

    private fun ring(x: Float, y: Float, size: Float, start: Long, echo: Boolean) {
        val onWater = maxOf(y, horizon + 4f)
        rings += Ring(x, onWater, size, start)
        if (echo) rings += Ring(x, onWater, size * 0.65f, start, after = 150)
        while (rings.size > MAX_RINGS) rings.removeAt(0)
    }

    /** Brushes one unit across, scaled into place, so a frame makes no new shaders. */
    private fun paint(color: Color) {
        if (color == tint) return
        tint = color
        line = Brush.horizontalGradient(listOf(Color.Transparent, color, Color.Transparent), -1f, 1f)
        glow = Brush.radialGradient(listOf(color, Color.Transparent), Offset.Zero, 1f)
    }

    /** The horizon lies [line] down; the pulled drop hangs [hang] over it and lands [fall] under it. */
    fun draw(scope: DrawScope, color: Color, line: Dp, bottom: Float, pulled: Float = 0f, hang: Dp = HANG, fall: Dp = FALL) = with(scope) {
        val hz = line.toPx()
        val t = now
        horizon = hz
        width = size.width
        paint(color)
        val cx = size.width / 2
        val landY = hz + fall.toPx()
        if (drop >= 0) {
            val p = ((t - drop) / DROP_MS).coerceIn(0f, 1f)
            if (p < 1f) {
                val x = cx + 60.dp.toPx() * (1f - Glide.transform(p))
                val y = landY - 120.dp.toPx() * (1f - Fall.transform(p))
                drawCircle(color.copy(alpha = min(1f, p * 5f)), 2.6.dp.toPx(), Offset(x, y))
            } else {
                drop = -1
                land = t
                next = t + 8000
                for (i in LANDING.indices) ring(cx, landY, LANDING[i], t + i * 180L, echo = false)
            }
        }
        val hangY = hz - hang.toPx()
        if (pullDrop >= 0) {
            val p = ((t - pullDrop) / DROP_MS).coerceIn(0f, 1f)
            if (p < 1f) {
                val r = SWOLLEN.toPx()
                val y = hangY + (landY - hangY) * Fall.transform(p)
                bead(color, cx, y, r, 1.35f + 0.1f * p)
            } else {
                pullDrop = -1
                // The rings of the landing first, then the quiet ones of the wait.
                lastBeat = t + 850
                for (i in PULL_LANDING.indices) ring(cx, landY, PULL_LANDING[i], t + i * 170L, echo = false)
            }
        } else if ((pulled > 0.01f || pullDrop == RELEASED) && ready) {
            // Only over water that can take it: a drop that cannot fall does not gather.
            // The drop gathers with the pull and stretches, ready to fall, at the threshold.
            val p = if (pullDrop == RELEASED) 1f else pulled.coerceAtMost(1f)
            val r = SWOLLEN.toPx() * sqrt(p)
            val stretch = if (p >= 1f) 1.35f else 1f + 0.2f * p
            bead(color.copy(alpha = 0.35f + 0.65f * p), cx, hangY, r, stretch)
        }
        if (land >= 0) {
            level = Calm.transform(((t - land) / LEVEL_MS).coerceIn(0f, 1f))
            if (level >= 1f) land = -1
        }
        val lv = level
        val breath = 0.85f + 0.15f * sin(t / 12000f * 2 * PI.toFloat())
        val stroke = 1.dp.toPx()
        clipRect(top = hz, bottom = bottom) {
            if (lv > 0f) {
                // The reflection: a soft glow under the horizon, flattened by perspective.
                val r = 150.dp.toPx() * lv + 1f
                withTransform({
                    translate(cx, hz)
                    scale(r, r * 0.32f, Offset.Zero)
                }) {
                    drawRect(glow, Offset(-1f, 0f), Size(2f, (bottom - hz) / (r * 0.32f)), alpha = 0.15f * breath)
                }
                STREAKS.forEachIndexed { i, s ->
                    val w = s.half.toPx() * lv * (0.88f + 0.12f * sin(t / (5000f + 1300 * i) + i))
                    val x0 = cx + 6.dp.toPx() * sin(t / (9000f + 1700 * i) + i * 2)
                    faded(s.alpha * breath, x0, w, hz + s.dy.toPx(), stroke)
                }
            }
            val far = 300.dp.toPx()
            rings.removeAll { r ->
                if (r.start == NEXT_FRAME) r.start = t
                val age = t - r.start - r.after
                if (age < 0) return@removeAll false
                val p = age / (2200f + 900f * r.size)
                if (p >= 1f) return@removeAll true
                // Nearer the viewer, a ring is rounder and spreads wider.
                val d = ((r.y - hz) / far).coerceIn(0f, 1f)
                val rx = (60f + 150f * d).dp.toPx() * r.size * Spread.transform(p) + 1f
                val ry = rx * (0.1f + 0.35f * d)
                val alpha = 0.5f * min(1f, sqrt(r.size)) * (1f - p) * min(1f, p * 10f)
                drawOval(color.copy(alpha = alpha), Offset(r.x - rx, r.y - ry), Size(rx * 2, ry * 2), style = Stroke(stroke))
                false
            }
        }
        if (lv > 0f) faded(0.55f, cx, 150.dp.toPx() * lv, hz, stroke)
        if (ambient && lv >= 1f && t > next) {
            ring(cx + (Random.nextFloat() - 0.5f) * 180.dp.toPx(), hz + (18 + Random.nextFloat() * 70).dp.toPx(), 0.5f + Random.nextFloat() * 0.25f, t, echo = true)
            next = t + 7000 + Random.nextLong(6000)
        }
        if (beat && pullDrop == -1L && t - lastBeat > 650) {
            lastBeat = t
            ring(cx + (Random.nextFloat() - 0.5f) * 60.dp.toPx(), hz + (bottom - hz) * 0.55f, 0.55f, t, echo = false)
        }
    }

    /** A drop of radius [r] around [x], [y], drawn out tall by [stretch]. */
    private fun DrawScope.bead(color: Color, x: Float, y: Float, r: Float, stretch: Float) =
        drawOval(color, Offset(x - r, y - r * stretch), Size(r * 2, r * 2 * stretch))

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

private class Streak(val dy: Dp, val half: Dp, val alpha: Float)

private val LAKE_HEIGHT = 124.dp
private val HORIZON = 40.dp
private val STREAKS = listOf(
    Streak(9.dp, 104.dp, 0.1f),
    Streak(20.dp, 84.dp, 0.075f),
    Streak(35.dp, 62.dp, 0.055f),
    Streak(55.dp, 40.dp, 0.035f),
)
private val PULL_GAP = 88.dp
private val LANDING = floatArrayOf(2.2f, 1.5f, 1f)
private val PULL_LANDING = floatArrayOf(1.6f, 1.1f, 0.75f)
private val HANG = 32.dp
private val FALL = 40.dp

/** The lake's water under a title; the pull's lies at the same height, so the two are one. */
private val LAKE_TOP = 8.dp

/** The pull's water is shallower than a lake: the gap is all there is of it. */
private val PULL_HANG = 22.dp
private val PULL_FALL = 24.dp
private val SWOLLEN = 3.2.dp
private const val MAX_RINGS = 48
private const val IDLE_MS = 250L
private const val NEXT_FRAME = -1L
private const val RELEASED = -2L
private const val DROP_MS = 750f
private const val LEVEL_MS = 1900f
private const val WORDS = 1300f
private const val WORDS_MS = 1000f

private val Glide = CubicBezierEasing(0.2f, 0.6f, 0.3f, 1f)
private val Fall = CubicBezierEasing(0.6f, 0f, 0.9f, 0.4f)
private val Spread = CubicBezierEasing(0.15f, 0.6f, 0.25f, 1f)
private val Calm = CubicBezierEasing(0.2f, 0f, 0f, 1f)

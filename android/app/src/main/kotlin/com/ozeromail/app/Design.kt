package com.ozeromail.app

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.lazy.LazyItemScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sqrt
import kotlinx.coroutines.flow.first

/**
 * Spacing shared by every screen. Text lines up on [screen], message rows on [row]; controls in the bottom bars
 * and the cards in a list keep their own edges, so each kind of thing sits the same way
 * everywhere.
 */
object Gap {
    /** Titles, headers, fields and body text from the screen edge. */
    val screen = 20.dp

    /** A message row's text from the screen edge, in line with the title; the unread dot hangs before it. */
    val row = screen

    /** Rows of a sheet from its edge. */
    val sheet = 22.dp

    /** Bottom bars, the tabs and the toast from the screen edge. */
    val bar = 12.dp

    /** Between the controls of a bar. */
    val inBar = 8.dp

    /** Above and below the controls of a bottom bar. */
    val barTop = 10.dp
    val barBottom = 12.dp

    /** Cards in a screen (older messages, the original, notes) from its edge, and inside them. */
    val card = 16.dp
    val inCard = 14.dp

    /** Captions and hints over and under the cards and fields of a page, in line with what starts their rows. */
    val inset = screen + card

    /** A sheet row's leading icon or dot sits in a slot this wide, so all the labels line up. */
    val slot = 24.dp
    val afterSlot = 14.dp
}

/** Corner radii, from a segment's thumb to a sheet. */
object Radius {
    val thumb = 11.dp
    val medium = 14.dp
    val tile = 16.dp
    val dialog = 20.dp
    val sheet = 26.dp
}

/**
 * Calm, slow motion that still keeps up with a finger: screens and sheets glide in with a
 * long ease-out, small things settle on a spring with no bounce, and every animation can be
 * interrupted. The system's animation scale, reduced motion included, applies to all of it.
 */
object Motion {
    /** Eases in over the first few frames, so nothing jumps, then a long, soft ease-out. */
    val Ease = CubicBezierEasing(0.25f, 0.1f, 0.1f, 1f)

    /** A screen pushed in or a sheet rising. */
    const val SCREEN = 420

    /** The dim behind a sheet. */
    const val SCRIM = 380

    /** Toasts and notes that come and go. */
    const val NOTE = 400

    /** The quickest changes, for what happens all day: a read dot going out. */
    const val QUICK = 150

    /** Small changes in place: a title fading, a colour, a body showing. */
    const val SMALL = 250

    /** Rows closing the gap a done message leaves, or making room for new mail. */
    val Row: FiniteAnimationSpec<IntOffset> = tween(NOTE, easing = Ease)

    fun <T> calm(ms: Int): FiniteAnimationSpec<T> = tween(ms, easing = Ease)

    /** For what follows a finger or moves in place: no bounce, interrupted without a jump. */
    fun <T> settle(): SpringSpec<T> = spring(dampingRatio = 1f, stiffness = 700f)

    /**
     * For something [depth] places behind what leads, held back as if by water: each place
     * keeps [per] of the stiffness of the one before, so a column of them follows in a wave.
     */
    fun <T> behind(depth: Int, per: Float, visibilityThreshold: T? = null): SpringSpec<T> =
        spring(dampingRatio = 1f, stiffness = 700f * per.pow(depth), visibilityThreshold = visibilityThreshold)

    /** How much of the stiffness each row down keeps as a tab's list pours in. */
    const val POUR = 0.68f

    /** How much each row further below keeps as rows close up after one that left. */
    const val CLOSE = 0.6f

    /** What is drawn along through water: slower than the finger, and without bounce. */
    fun <T> drift(): SpringSpec<T> = spring(dampingRatio = 1f, stiffness = 260f)

    /** A small pop when something changes state under a finger: a little overshoot, then still. */
    fun <T> pop(): SpringSpec<T> = spring(dampingRatio = 0.7f, stiffness = 700f)

    /** A light touch on something at rest, which rocks once and settles. */
    fun <T> nudge(): SpringSpec<T> = spring(dampingRatio = 0.55f, stiffness = 700f)

    /** After a fling: the same spring with a little give, since the finger threw it. */
    fun <T> fling(): SpringSpec<T> = spring(dampingRatio = 0.85f, stiffness = 500f)

    /**
     * How far into a [SCREEN] animation to seek so the screen sits where a back gesture at
     * [progress] has taken it: the curve is not a straight line, so the time is read back from it.
     */
    fun follow(progress: Float): Float {
        var low = 0f
        var high = 1f
        repeat(20) {
            val mid = (low + high) / 2
            if (Ease.transform(mid) < progress) low = mid else high = mid
        }
        return (low + high) / 2
    }
}

/**
 * Keeps this value following [target] for as long as it is called, on the [spring] for that
 * moment, until it is within [rest] of it. It moves on every frame, from where it is and as fast as it was going, even while a
 * finger moves the target on every frame too; a fresh animation each time would start still.
 */
suspend fun Animatable<Float, AnimationVector1D>.follow(spring: (to: Float) -> SpringSpec<Float>, rest: Float = 0.001f, target: () -> Float) {
    var speed = 0f
    var then = -1L
    while (true) {
        val to = target()
        if (value == to && speed == 0f) {
            snapshotFlow(target).first { it != value }
            then = -1L
            continue
        }
        val now = withFrameNanos { it }
        // A stalled frame counts as a short one, so nothing leaps; the first moves at once.
        var left = if (then < 0) FIRST_STEP_S else min(now - then, MAX_STEP_NS) / 1e9f
        then = now
        val spec = spring(to)
        val k = spec.stiffness
        val c = 2f * spec.dampingRatio * sqrt(k)
        var x = value - to
        // Small steps keep a stiff spring steady.
        while (left > 0f) {
            val dt = min(left, SUB_STEP_S)
            speed += (-k * x - c * speed) * dt
            x += speed * dt
            left -= dt
        }
        if (abs(x) < rest && abs(speed) < rest * 10f) {
            x = 0f
            speed = 0f
        }
        snapTo(to + x)
    }
}

private const val MAX_STEP_NS = 50_000_000L
private const val SUB_STEP_S = 0.004f
private const val FIRST_STEP_S = 1f / 60f

/** A list row that slides into its new place when rows above it come or go. */
fun LazyItemScope.rowMove(placement: FiniteAnimationSpec<IntOffset> = Motion.Row): Modifier =
    Modifier.animateItem(fadeInSpec = null, placementSpec = placement, fadeOutSpec = null)

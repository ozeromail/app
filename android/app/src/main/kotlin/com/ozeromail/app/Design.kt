package com.ozeromail.app

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.lazy.LazyItemScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp

/**
 * Spacing shared by every screen. Text lines up on [screen]; controls in the bottom bars
 * and the cards in a list keep their own edges, so each kind of thing sits the same way
 * everywhere.
 */
object Gap {
    /** Titles, headers, fields and body text from the screen edge. */
    val screen = 20.dp

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
    /** The prototype's curve: quick to start, long and soft at the end. */
    val Ease = CubicBezierEasing(0.3f, 0.7f, 0.1f, 1f)

    /** A screen pushed in or a sheet rising. */
    const val SCREEN = 500

    /** The dim behind a sheet. */
    const val SCRIM = 450

    /** Toasts and notes that come and go. */
    const val NOTE = 400

    /** Small changes in place: a title fading, a colour, a body showing. */
    const val SMALL = 250

    /** Rows closing the gap a done message leaves, or making room for new mail. */
    val Row: FiniteAnimationSpec<IntOffset> = tween(NOTE, easing = Ease)

    fun <T> calm(ms: Int): FiniteAnimationSpec<T> = tween(ms, easing = Ease)

    /** For what follows a finger or moves in place: no bounce, interrupted without a jump. */
    fun <T> settle(): SpringSpec<T> = spring(dampingRatio = 1f, stiffness = 500f)

    /**
     * How far into a [SCREEN] animation to seek so the screen sits where a back gesture at
     * [progress] has taken it: the curve runs ahead early on, so the time is read back from it.
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

/** A list row that slides into its new place when rows above it come or go. */
fun LazyItemScope.rowMove(): Modifier = Modifier.animateItem(fadeInSpec = null, placementSpec = Motion.Row, fadeOutSpec = null)

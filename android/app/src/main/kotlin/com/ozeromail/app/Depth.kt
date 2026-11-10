package com.ozeromail.app

import android.graphics.BlurMaskFilter
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.material3.ColorScheme
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.isSimple
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathOperation
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.asAndroidPath
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeStyle
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeEffect

// A little depth in the icon's paper, kept faint. What lies on top of the page (the accent
// buttons, the tabs' thumb, a screen pushed in) lifts with a short, straight shadow and
// light along its top edge; what holds things (fields, the tabs' track, the accounts button) is
// cut in, with a shadow under its top edge; dots are tiny windows. At night shadows hardly show,
// so the thumb is a shade lighter and a lit edge carries the lifted buttons; the docks stay the
// page's colour, so the fields cut into them still read lighter than the dock.

val ColorScheme.night: Boolean
    get() = background.luminance() < 0.5f

/** The ink of shadows, as see-through as asked: the paper's navy by day, black at night. */
private fun ColorScheme.shade(day: Float, night: Float): Color =
    if (this.night) Color.Black.copy(alpha = night) else onBackground.copy(alpha = day)

/** The light along a lifted thing's top edge. */
private val ColorScheme.lit: Color
    get() = Color.White.copy(alpha = if (night) 0.07f else 0.35f)


/** Beneath the screens, where one sinks as the next is laid over it. */
val ColorScheme.deep: Color
    get() = if (night) surfaceContainerLowest else surfaceDim

/** What a lifted thing is filled with, when it is not the accent: the tabs' thumb. */
val ColorScheme.raised: Color
    get() = if (night) surfaceContainerHighest else background

/**
 * A soft shadow under [shape], drawn by hand: straight down by [y], blurred by [blur], in [color].
 * The system's elevation shadow is lit from the top of the screen, so near the bottom it falls
 * long and sideways; this one looks the same anywhere, as on iOS. Android's blur radius is about
 * 1.4 times SwiftUI's for the same softness. [lit] draws light along the top edge, over the content.
 */
fun Modifier.cast(shape: Shape, color: Color, blur: Dp, y: Dp, lit: Color? = null): Modifier = drawWithCache {
    val outline = shape.createOutline(size, layoutDirection, this)
    val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
        this.color = color.toArgb()
        if (blur > 0.dp) maskFilter = BlurMaskFilter(blur.toPx(), BlurMaskFilter.Blur.NORMAL)
    }
    val down = y.toPx()
    // A round rect blurs on Skia's fast path; any other shape goes through a path.
    val rounded = (outline as? Outline.Rounded)?.roundRect?.takeIf { it.isSimple }
    val path = if (rounded == null) Path().apply { addOutline(outline); translate(Offset(0f, down)) }.asAndroidPath() else null
    val crescent = lit?.let {
        val whole = Path().apply { addOutline(outline) }
        val below = Path().apply { addPath(whole, Offset(0f, 1.dp.toPx())) }
        Path.combine(PathOperation.Difference, whole, below)
    }
    onDrawWithContent {
        drawIntoCanvas {
            val canvas = it.nativeCanvas
            if (path != null) {
                canvas.drawPath(path, paint)
            } else if (rounded != null) {
                val r = rounded.topLeftCornerRadius.x
                canvas.drawRoundRect(0f, down, size.width, size.height + down, r, r, paint)
            }
        }
        drawContent()
        if (crescent != null && lit != null) drawPath(crescent, lit)
    }
}

/** Lifts a thing off the page: a short shadow in [glow] and light along its top edge. Goes before its background. */
fun Modifier.lifted(colors: ColorScheme, shape: Shape, glow: Color = colors.shade(0.12f, 0.4f), blur: Dp = 2.dp, y: Dp = 1.dp): Modifier =
    cast(shape, glow, blur, y, colors.lit)

/** Only a button in the accent lifts, with a soft shadow of its own blue as on iOS; the rest lie flat on the page. */
fun Modifier.liftedIfAccent(colors: ColorScheme, background: Color, shape: Shape): Modifier =
    if (background == colors.accent) lifted(colors, shape, colors.accent.copy(alpha = 0.3f), blur = 4.dp, y = 1.5.dp) else this

/** What floats over the screen, such as suggestions over a field: a wide, soft shadow. Leave room around it. */
fun Modifier.floating(colors: ColorScheme, shape: Shape): Modifier =
    cast(shape, colors.shade(0.12f, 0.45f), blur = 10.dp, y = 4.dp)

/** Cuts a field into the page: a shadow under its top edge. Goes after the field's clip and background. */
fun Modifier.sunk(colors: ColorScheme): Modifier = drawWithCache {
    val deep = 2.dp.toPx()
    val brush = Brush.verticalGradient(listOf(colors.shade(0.06f, 0.35f), Color.Transparent), endY = deep)
    onDrawBehind { drawRect(brush, size = Size(size.width, deep)) }
}

/** A bottom dock over the list: its fill, see-through as the bars unless [solid], and a hairline along its top edge. */
fun Modifier.dock(colors: ColorScheme, solid: Boolean = false): Modifier =
    background(if (solid) colors.background else colors.bar)
        .hairline(colors.outlineVariant.copy(alpha = 0.6f), top = true)

/** What the floating dock's glass shows through, blurred: the list under it. Null where nothing floats. */
val LocalGlass = staticCompositionLocalOf<HazeState?> { null }

/**
 * Frosted glass, as the bars on iOS: what lies under it shows through, blurred and veiled in the
 * paper's colour, with a soft shadow and a faint rim. Without a [state] it is the grey fill cut into the page.
 */
fun Modifier.glass(state: HazeState?, colors: ColorScheme, shape: Shape): Modifier {
    if (state == null) return clip(shape).background(colors.surfaceVariant).sunk(colors)
    val veil = if (colors.night) colors.surfaceContainerHigh.copy(alpha = 0.66f) else colors.background.copy(alpha = 0.62f)
    return floating(colors, shape)
        .clip(shape)
        .hazeEffect(state, frost(colors, veil))
        .border(1.dp, if (colors.night) colors.lit else colors.outlineVariant.copy(alpha = 0.8f), shape)
}

/** [glass] where a list under it shows through; elsewhere a card of the paper floating on its own shadow. */
fun Modifier.floatingGlass(state: HazeState?, colors: ColorScheme, shape: Shape): Modifier {
    if (state != null) return glass(state, colors, shape)
    return floating(colors, shape)
        .clip(shape)
        .background(if (colors.night) colors.surfaceContainerHigh else colors.background)
        .border(1.dp, if (colors.night) colors.lit else colors.outlineVariant, shape)
}

/** A bar across the screen over the list, such as the compact title: frosted as [glass], edge to edge, faded by [alpha]. */
fun Modifier.frosted(state: HazeState, colors: ColorScheme, alpha: () -> Float = { 1f }): Modifier =
    hazeEffect(state, frost(colors, colors.background.copy(alpha = 0.72f))) { this.alpha = alpha() }

/** The blur of [glass] and [frosted], veiled in [veil]. */
private fun frost(colors: ColorScheme, veil: Color) =
    HazeStyle(backgroundColor = colors.background, tint = HazeTint(veil), blurRadius = 24.dp, noiseFactor = 0f)

/** A screen pushed in over another: its left edge casts a soft shadow on the one beneath. */
fun Modifier.sheetEdge(colors: ColorScheme): Modifier = drawWithCache {
    val fall = 12.dp.toPx()
    val brush = Brush.horizontalGradient(listOf(Color.Transparent, colors.shade(0.06f, 0.3f)), startX = -fall, endX = 0f)
    onDrawBehind { drawRect(brush, Offset(-fall, 0f), Size(fall, size.height)) }
}

/** The colours down a round mark drawn as a tiny window: darker under its top edge, lighter below. */
fun ColorScheme.window(color: Color): List<Color> {
    val solid = color.copy(alpha = 1f)
    return listOf(lerp(solid, shade(1f, 1f), 0.3f), solid, lerp(solid, Color.White, 0.2f))
}

/** A round mark in [window] colours, faded by [alpha]. */
fun DrawScope.windowDot(window: List<Color>, radius: Float, center: Offset = this.center, alpha: Float = 1f) {
    drawCircle(Brush.verticalGradient(window, startY = center.y - radius, endY = center.y + radius), radius, center, alpha)
}

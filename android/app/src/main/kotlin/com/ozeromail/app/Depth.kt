package com.ozeromail.app

import androidx.compose.foundation.background
import androidx.compose.material3.ColorScheme
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

// A little depth in the icon's paper, kept faint. What lies on top of the page (the docks, the
// accent buttons, the tabs' thumb, a screen pushed in) lifts with a soft shadow and light along
// its top edge; what holds things (fields, the tabs' track, the accounts button) is cut in, with
// a shadow under its top edge; dots are tiny windows. At night shadows hardly show, so lifted
// things are a shade lighter and a lit edge carries them.

val ColorScheme.night: Boolean
    get() = background.luminance() < 0.5f

/** The ink of shadows, as see-through as asked: the paper's navy by day, black at night. */
private fun ColorScheme.shade(day: Float, night: Float): Color =
    if (this.night) Color.Black.copy(alpha = night) else onBackground.copy(alpha = day)

/** The light along a lifted thing's top edge. */
private val ColorScheme.lit: Color
    get() = Color.White.copy(alpha = if (night) 0.07f else 0.35f)

/** The bottom docks: the page by day, a shade lighter at night, since they lie on top. */
private val ColorScheme.dockFill: Color
    get() = if (night) surfaceContainerHigh else background

/** Beneath the screens, where one sinks as the next is laid over it. */
val ColorScheme.deep: Color
    get() = if (night) surfaceContainerLowest else surfaceDim

/** What a lifted thing is filled with, when it is not the accent: the tabs' thumb. */
val ColorScheme.raised: Color
    get() = if (night) surfaceContainerHighest else background

/** Lifts a button off the page: a soft shadow tinted by [glow] and light along its top edge. */
fun Modifier.lifted(colors: ColorScheme, shape: Shape, glow: Color = colors.shade(1f, 1f), elevation: Dp = 4.dp): Modifier {
    val lit = colors.lit
    return shadow(elevation, shape, ambientColor = glow.copy(alpha = 0.5f), spotColor = glow)
        .drawWithContent {
            drawContent()
            drawLine(lit, Offset(0f, 0.5.dp.toPx()), Offset(size.width, 0.5.dp.toPx()), 1.dp.toPx())
        }
}

/** Only a button in the accent lifts; the rest lie flat on the page. */
fun Modifier.liftedIfAccent(colors: ColorScheme, background: Color, shape: Shape): Modifier =
    if (background == colors.accent) lifted(colors, shape, colors.accent) else this

/** Cuts a field into the page: a shadow under its top edge. Goes after the field's clip and background. */
fun Modifier.sunk(colors: ColorScheme): Modifier = drawWithCache {
    val deep = 3.dp.toPx()
    val brush = Brush.verticalGradient(listOf(colors.shade(0.08f, 0.32f), Color.Transparent), endY = deep)
    onDrawBehind { drawRect(brush, size = Size(size.width, deep)) }
}

/** A bottom dock lying on the list: its fill, see-through as the bars unless [solid], with a soft shadow above its top edge. */
fun Modifier.dock(colors: ColorScheme, solid: Boolean = false): Modifier =
    background(colors.dockFill.copy(alpha = if (solid) 1f else 0.94f))
        .drawWithCache {
            val fall = 12.dp.toPx()
            val brush = Brush.verticalGradient(listOf(Color.Transparent, colors.shade(0.06f, 0.3f)), startY = -fall, endY = 0f)
            onDrawBehind { drawRect(brush, Offset(0f, -fall), Size(size.width, fall)) }
        }
        .hairline(colors.outlineVariant.copy(alpha = 0.6f), top = true)

/** A screen pushed in over another: its left edge casts a soft shadow on the one beneath. */
fun Modifier.sheetEdge(colors: ColorScheme): Modifier = drawWithCache {
    val fall = 16.dp.toPx()
    val brush = Brush.horizontalGradient(listOf(Color.Transparent, colors.shade(0.1f, 0.4f)), startX = -fall, endX = 0f)
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

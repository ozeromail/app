package com.ozeromail.app

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Easing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathMeasure
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The note of a list with nothing left in it: three still lines, a lake. With [drawIn], as the
 * last message leaves, a drop falls in, rings spread past the screen's edges and settle into
 * the lines, then the words come; in one movement, with no pause.
 */
@Composable
fun LakeNote(title: String, text: String, drawIn: Boolean) {
    val colors = MaterialTheme.colorScheme
    // Only as it first shows: the caller's flag turns off right after.
    val animate = remember { drawIn }
    val time = remember { Animatable(if (animate) 0f else END) }
    if (animate) {
        LaunchedEffect(Unit) { time.animateTo(END, tween(END.toInt(), easing = LinearEasing)) }
    }
    // The lines drift a little, each at its own pace, so the water looks alive but quiet.
    val drift = rememberInfiniteTransition(label = "lake")
    val drifts = listOf(6000, 7000, 8000).mapIndexed { i, ms ->
        drift.animateFloat(
            if (i == 1) 1f else -1f,
            if (i == 1) -1f else 1f,
            infiniteRepeatable(tween(ms, easing = Calm), RepeatMode.Reverse),
            label = "drift$i",
        )
    }
    val water = colors.primary
    Column(
        Modifier.fillMaxWidth().padding(top = 24.dp, bottom = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Canvas(Modifier.fillMaxWidth().height(LAKE_HEIGHT)) {
            val t = time.value
            val center = Offset(size.width / 2, size.height / 2)
            drop(t, center, water)
            RINGS.forEachIndexed { i, start ->
                val p = phase(t, start, RING_MS, Spread)
                if (p > 0f && p < 1f) {
                    // In over the first tenth, then fading as it spreads.
                    val alpha = if (p < 0.1f) p / 0.1f * 0.75f else 0.75f * (1f - p) / 0.9f
                    val rx = RING_RX.toPx() * p
                    val ry = RING_RY.toPx() * p
                    drawOval(
                        water.copy(alpha = alpha),
                        Offset(center.x - rx, center.y - ry),
                        Size(rx * 2, ry * 2),
                        style = Stroke((1.4f - 0.2f * i).dp.toPx()),
                    )
                }
            }
            LINES.forEachIndexed { i, line ->
                val p = phase(t, line.start, LINE_MS, Spread)
                val shift = drifts[i].value * 5.dp.toPx() * phase(t, END - 400f, 400f, LinearEasing)
                line(line, Offset(center.x + shift, center.y), p, water)
            }
        }
        val words = phase(time.value, WORDS, WORDS_MS, Calm)
        Column(
            Modifier.graphicsLayer {
                alpha = words
                translationY = (1f - words) * 8.dp.toPx()
            },
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(title, fontWeight = FontWeight.SemiBold, fontSize = 18.sp, textAlign = TextAlign.Center)
            Text(
                text,
                color = colors.onSurfaceVariant,
                textAlign = TextAlign.Center,
                lineHeight = 21.sp,
                modifier = Modifier.widthIn(max = 280.dp).padding(horizontal = 32.dp),
            )
        }
    }
}

/** The drop: from where the message left, on a curve to the middle, then flat into the water. */
private fun DrawScope.drop(t: Float, center: Offset, color: Color) {
    val p = phase(t, DROP, DROP_MS, LinearEasing)
    if (p <= 0f || p >= 1f) return
    val x = center.x + 90.dp.toPx() * (1f - Glide.transform(p))
    val y = center.y - 80.dp.toPx() * (1f - Fall.transform(p))
    // Grows in, then spreads flat as it touches the surface.
    val alpha = if (p < 0.2f) p / 0.2f else if (p > 0.85f) (1f - p) / 0.15f else 1f
    val r = 3.dp.toPx()
    val flat = if (p > 0.85f) (p - 0.85f) / 0.15f else 0f
    drawOval(color.copy(alpha = alpha), Offset(x - r * (1 + 1.4f * flat), y - r * (1 - 0.7f * flat)), Size(2 * r * (1 + 1.4f * flat), 2 * r * (1 - 0.7f * flat)))
}

/** One line of the surface, drawn from its middle out to both ends as [p] goes to 1. */
private fun DrawScope.line(line: Line, center: Offset, p: Float, color: Color) {
    if (p <= 0f) return
    val half = line.half.toPx()
    val step = 20.dp.toPx()
    val amp = line.amp.toPx()
    val y = center.y + line.dy.toPx()
    val path = Path().apply {
        var x = center.x - half
        moveTo(x, y)
        var up = true
        while (x < center.x + half - 0.5f) {
            quadraticTo(x + step / 2, if (up) y - amp else y + amp, x + step, y)
            x += step
            up = !up
        }
    }
    val measure = PathMeasure().apply { setPath(path, false) }
    val length = measure.length
    val part = Path()
    measure.getSegment(length / 2 * (1f - p), length / 2 * (1f + p), part, true)
    drawPath(part, color.copy(alpha = line.alpha), style = Stroke(1.8.dp.toPx(), cap = StrokeCap.Round))
}

/** How far a part that starts at [start] and lasts [ms] has got at [t], eased. */
private fun phase(t: Float, start: Float, ms: Float, easing: Easing): Float =
    easing.transform(((t - start) / ms).coerceIn(0f, 1f))

private class Line(val half: Dp, val dy: Dp, val amp: Dp, val alpha: Float, val start: Float)

private val LAKE_HEIGHT = 72.dp
private val RING_RX = 240.dp
private val RING_RY = 54.dp

private const val DROP = 150f
private const val DROP_MS = 750f
private val RINGS = listOf(720f, 860f, 1000f, 1140f)
private const val RING_MS = 2400f
private val LINES = listOf(
    Line(120.dp, (-10).dp, 5.dp, 0.6f, 850f),
    Line(90.dp, 0.dp, 4.dp, 0.38f, 1000f),
    Line(60.dp, 10.dp, 3.dp, 0.22f, 1150f),
)
private const val LINE_MS = 1500f
private const val WORDS = 1300f
private const val WORDS_MS = 1000f
private const val END = 3600f

private val Glide = CubicBezierEasing(0.2f, 0.6f, 0.3f, 1f)
private val Fall = CubicBezierEasing(0.6f, 0f, 0.9f, 0.4f)
private val Spread = CubicBezierEasing(0.15f, 0.6f, 0.25f, 1f)
private val Calm = CubicBezierEasing(0.2f, 0f, 0f, 1f)

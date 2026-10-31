package com.ozeromail.app

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.lerp
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.unit.toSize
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.test.services.storage.TestStorage
import kotlin.math.abs
import kotlin.math.roundToInt
import org.json.JSONArray
import org.json.JSONObject

/**
 * A gesture filmed frame by frame with the animation clock stopped: where each tracked node is
 * on every frame, and a picture of every [every]th. [save] writes the frames as JSON and the
 * pictures as one contact sheet, which the CI job keeps.
 */
class Film(private val rule: ComposeTestRule, val name: String, private val every: Int = 2) {
    class Frame(val ms: Long, val finger: Offset?, val at: Map<String, Rect?>, val seen: Int?)

    private val tracks = linkedMapOf<String, SemanticsMatcher>()
    val frames = mutableListOf<Frame>()
    private val pictures = mutableListOf<Pair<Int, Bitmap>>()
    private var finger: Offset? = null

    /** Pictures are slow; a stretch that must pass quickly, as before Undo, goes without. */
    var shooting = true

    /** Something read off the whole screen on every frame, as [Frame.seen]: what no node tells. */
    var look: ((Bitmap) -> Int)? = null

    fun track(label: String, matcher: SemanticsMatcher) {
        tracks[label] = matcher
    }

    /** Where [label] is on the last frame, or null if it was not on screen. */
    fun last(label: String): Rect? = frames.lastOrNull()?.at?.get(label)

    /** The next frame. */
    fun frame() {
        rule.mainClock.advanceTimeByFrame()
        record()
    }

    fun frames(count: Int) = repeat(count) { frame() }

    /** Frames until [done], at most [max]; whether it came. */
    fun until(max: Int, done: () -> Boolean): Boolean {
        repeat(max) {
            if (done()) return true
            frame()
        }
        return done()
    }

    fun down(at: Offset) {
        rule.onRoot().performTouchInput { down(at) }
        finger = at
        frame()
    }

    fun moveTo(to: Offset) {
        rule.onRoot().performTouchInput { moveTo(to) }
        finger = to
        frame()
    }

    fun up() {
        rule.onRoot().performTouchInput { up() }
        finger = null
        frame()
    }

    /** A finger down at [from], drawn to [to] over [ms] at an even pace, and held there. */
    fun drag(from: Offset, to: Offset, ms: Int) {
        down(from)
        val steps = (ms / FRAME_MS).coerceAtLeast(1)
        for (step in 1..steps) moveTo(lerp(from, to, step.toFloat() / steps))
    }

    /** A drag let go at its end: a swipe or a fling. */
    fun swipe(from: Offset, to: Offset, ms: Int) {
        drag(from, to, ms)
        up()
    }

    private fun record() {
        // Not boundsInRoot: that is cut to the screen, so a row swiped off would seem to stay.
        val at = tracks.mapValues { (_, matcher) ->
            rule.onAllNodes(matcher).fetchSemanticsNodes().firstOrNull()?.let { Rect(it.positionInRoot, it.size.toSize()) }
        }
        val index = frames.size
        val picture = shooting && index % every == 0
        val looking = look
        val shot = if (picture || looking != null) rule.onRoot().captureToImage().asAndroidBitmap() else null
        frames += Frame(rule.mainClock.currentTime, finger, at, shot?.let { looking?.invoke(it) })
        if (shot != null) {
            if (picture) pictures += index to Bitmap.createScaledBitmap(shot, shot.width / SCALE, shot.height / SCALE, true)
            shot.recycle()
        }
    }

    /** Frames where [label] came back after being off screen for a few frames: what the eye sees as a blink. */
    fun blinks(label: String, gap: Int = 5): List<String> {
        val found = mutableListOf<String>()
        var seen = false
        var missing = 0
        frames.forEachIndexed { index, frame ->
            if (frame.at[label] == null) {
                if (seen) missing++
                return@forEachIndexed
            }
            if (missing in 1..gap) found += "frame $index: $label came back after $missing frames off screen"
            seen = true
            missing = 0
        }
        return found
    }

    /**
     * Frames where [label], with no finger on it, at once changed speed by more than [jerk] px
     * per frame: what the eye sees as a jump.
     */
    fun jumps(label: String, jerk: Float, axis: (Rect) -> Float = { it.left }): List<String> {
        val found = mutableListOf<String>()
        var before: Float? = null
        var speed: Float? = null
        frames.forEachIndexed { index, frame ->
            val now = frame.at[label]?.let(axis)
            if (now == null) {
                before = null
                speed = null
                return@forEachIndexed
            }
            val was = before
            if (was != null) {
                val step = now - was
                val wasSpeed = speed
                if (frame.finger == null && wasSpeed != null && abs(step - wasSpeed) > jerk) {
                    found += "frame $index: $label went from ${wasSpeed.roundToInt()} to ${step.roundToInt()} px a frame"
                }
                speed = step
            }
            before = now
        }
        return found
    }

    fun save() {
        val storage = TestStorage()
        val json = JSONObject().put("name", name).put("frames", JSONArray(frames.map { frame ->
            JSONObject()
                .put("ms", frame.ms)
                .put("finger", frame.finger?.let { JSONArray(listOf(it.x.toDouble(), it.y.toDouble())) } ?: JSONObject.NULL)
                .put("seen", frame.seen ?: JSONObject.NULL)
                .put("at", JSONObject(frame.at.mapValues { (_, r) -> r?.let { JSONArray(listOf(it.left, it.top, it.right, it.bottom).map(Float::toDouble)) } ?: JSONObject.NULL }))
        }))
        storage.openOutputFile("films/$name.json").use { it.write(json.toString(1).toByteArray()) }
        if (pictures.isEmpty()) return
        val (w, h) = pictures[0].second.let { it.width to it.height }
        val columns = COLUMNS.coerceAtMost(pictures.size)
        val rows = (pictures.size + columns - 1) / columns
        val sheet = Bitmap.createBitmap(columns * (w + GAP), rows * (h + LABEL + GAP), Bitmap.Config.ARGB_8888)
        val canvas = Canvas(sheet)
        canvas.drawColor(android.graphics.Color.DKGRAY)
        val paint = Paint().apply {
            color = android.graphics.Color.WHITE
            textSize = LABEL * 0.8f
            isAntiAlias = true
        }
        val fingerPaint = Paint().apply {
            color = android.graphics.Color.RED
            alpha = 160
            isAntiAlias = true
        }
        pictures.forEachIndexed { i, (index, picture) ->
            val x = (i % columns) * (w + GAP).toFloat()
            val y = (i / columns) * (h + LABEL + GAP).toFloat()
            val frame = frames[index]
            canvas.drawText("#$index ${frame.ms - frames[0].ms}ms${if (frame.finger != null) " ●" else ""}", x + 4, y + LABEL * 0.8f, paint)
            canvas.drawBitmap(picture, x, y + LABEL, null)
            frame.finger?.let { canvas.drawCircle(x + it.x / SCALE, y + LABEL + it.y / SCALE, 6f, fingerPaint) }
        }
        storage.openOutputFile("films/$name.png").use { sheet.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    companion object {
        const val FRAME_MS = 16
        private const val SCALE = 4
        private const val COLUMNS = 8
        private const val LABEL = 28
        private const val GAP = 6
    }
}

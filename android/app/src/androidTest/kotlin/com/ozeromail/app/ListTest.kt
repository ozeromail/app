package com.ozeromail.app

import android.graphics.Bitmap
import android.graphics.Color.blue
import android.graphics.Color.green
import android.graphics.Color.red
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Gestures on the list as a whole: pull, tabs, choosing, opening a letter and coming back. */
@RunWith(AndroidJUnit4::class)
class ListTest : TestMail() {
    @Test
    fun pullDropsTheRowsAndTheyComeBack() {
        val (a) = letters()
        film("pull", every = 3) {
            track("a", text(a))
            frame()
            val row = last("a")!!
            drag(row.at(0.5f), Offset(screen.width * 0.5f, row.center.y + screen.height * 0.3f), ms = 500)
            frames(10)
            up()
            val back = until(400) { abs(last("a")!!.top - row.top) < 1f }
            frames(10)
            assertTrue("the rows did not come back up after the pull", back)
        }
    }

    @Test
    fun onlyAPullDropsADrop() {
        rule.onNode(hasContentDescription("Accounts")).performClick()
        // The folders may be there before the sheet is laid out: wait until it has come up.
        var at = -1f
        waitFor {
            val top = if (has(hasText("Empty"))) bounds(hasText("Empty")).top else 0f
            (top > 0f && top == at).also { at = top }
        }
        rule.onNode(hasText("Empty")).apply { runCatching { performScrollTo() } }.performClick()
        waitFor { has(hasText("Nothing here")) }
        film("drops", every = 4) {
            track("words", hasText("Nothing here"))
            frames(60)
            val dp = rule.density.density
            // The lake stands over its words; under its horizon, down to where a drop lands, only
            // a falling drop is the accent's blue: the water of a plain folder is grey.
            val lake = last("words")!!.top - (6.dp + LAKE_HEIGHT).value * dp
            look = { accentPixels(it, (lake + (HORIZON + 2.dp).value * dp).toInt(), (lake + (HORIZON + FALL + 2.dp).value * dp).toInt()) }
            val at = Offset(screen.width / 2, screen.height * 0.6f)
            fun tapsAndScrolls(): Int {
                val from = frames.size
                repeat(3) {
                    down(at)
                    frames(3)
                    up()
                    frames(40)
                }
                swipe(at, Offset(at.x, at.y - 300 * dp), ms = 200)
                frames(60)
                // A little way down, less than it takes to drop.
                swipe(at, Offset(at.x, at.y + 30 * dp), ms = 200)
                frames(60)
                return frames.drop(from).maxOf { it.seen ?: 0 }
            }
            val before = tapsAndScrolls()
            val from = frames.size
            swipe(at, Offset(at.x, at.y + 250 * dp), ms = 500)
            frames(90)
            val pulled = frames.drop(from).maxOf { it.seen ?: 0 }
            val after = tapsAndScrolls()
            assertTrue("a pull dropped no drop ($pulled px of blue)", pulled > DROP_PX)
            assertTrue("taps or scrolls dropped a drop ($before px of blue)", before < DROP_PX)
            assertTrue("after a pull, taps or scrolls dropped a drop ($after px of blue)", after < DROP_PX)
        }
    }

    @Test
    fun tabDragSwitchesTab() {
        val (a) = letters()
        val people = bounds(hasText("People"))
        val notifications = bounds(hasText("Notifications"))
        film("tab-drag") {
            track("a", text(a))
            track("notice", text("Notice 1:"))
            frame()
            swipe(people.center, notifications.center, ms = 250)
            val switched = until(120) { last("notice")?.let { abs(it.left) < 1f } == true }
            frames(20)
            assertTrue("Notifications did not come in", switched)
            // As the finger lets go the rows go on from where they were.
            assertEquals(emptyList<String>(), jumps("notice", jerk = 60f))
        }
    }

    @Test
    fun tabTapSwitchesTabAndBack() {
        val (a) = letters()
        film("tab-tap") {
            track("a", text(a))
            track("digest", text("Digest 1:"))
            frame()
            rule.onNode(hasText("Newsletters")).performClick()
            val there = until(120) { last("digest")?.let { abs(it.left) < 1f } == true && last("a") == null }
            frames(10)
            rule.onNode(hasText("People")).performClick()
            val back = until(120) { last("a")?.let { abs(it.left) < 1f } == true && last("digest") == null }
            frames(10)
            assertTrue("Newsletters did not come in", there)
            assertTrue("People did not come back", back)
        }
    }

    @Test
    fun holdAndDrawChoosesRows() {
        val (a, b, c) = letters()
        film("choose") {
            track("a", text(a))
            track("b", text(b))
            track("c", text(c))
            frame()
            val from = last("a")!!.at(0.5f)
            down(from)
            // Held still past the hold time, on the clock the gesture reads.
            frames(40)
            Thread.sleep(600)
            frames(4)
            val to = last("c")!!.at(0.5f)
            for (step in 1..15) moveTo(Offset(from.x, from.y + (to.y - from.y) * step / 15))
            frames(10)
            up()
            frames(30)
            assertTrue("three rows are not chosen", has(hasText("3 selected")))
            back()
            frames(40)
        }
    }

    @Test
    fun openALetterAndComeBack() {
        val (a) = letters()
        film("open-back") {
            track("a", text(a))
            track("back", hasContentDescription("Back"))
            frame()
            val row = last("a")!!
            down(row.at(0.5f))
            up()
            val open = until(120) { last("back")?.let { it.left < screen.width * 0.1f } == true }
            frames(20)
            back()
            val returned = until(120) { last("a")?.let { abs(it.left) < 1f } == true }
            frames(10)
            assertTrue("the letter did not open", open)
            assertTrue("the list did not come back", returned)
        }
    }

    /** Pixels of the accent's blue between [top] and [bottom]. */
    private fun accentPixels(shot: Bitmap, top: Int, bottom: Int): Int {
        val from = top.coerceIn(0, shot.height)
        val rows = bottom.coerceIn(from, shot.height) - from
        val accent = Accent.toArgb()
        val pixels = IntArray(shot.width * rows)
        shot.getPixels(pixels, 0, shot.width, 0, from, shot.width, rows)
        return pixels.count { c ->
            abs(red(c) - red(accent)) < 40 && abs(green(c) - green(accent)) < 40 && blue(c) > blue(accent) - 40
        }
    }

    companion object {
        /** More blue than this is a drop. */
        const val DROP_PX = 30
    }
}

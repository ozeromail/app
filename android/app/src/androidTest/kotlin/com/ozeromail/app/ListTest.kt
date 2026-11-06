package com.ozeromail.app

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
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
    fun pullChecksAndLeavesTheRowsInPlace() {
        val (a) = letters()
        film("pull", every = 3) {
            track("a", text(a))
            track("checking", hasText(CHECKING))
            frame()
            val row = last("a")!!
            drag(row.at(0.5f), Offset(screen.width * 0.5f, row.center.y + screen.height * 0.3f), ms = 500)
            frames(10)
            // The system's spinner comes down over the list; the rows stay where they are.
            val moved = frames.maxOf { abs((it.at["a"]?.top ?: row.top) - row.top) }
            up()
            val checking = until(120) { last("checking") != null }
            frames(10)
            assertTrue("the rows moved with the pull ($moved px)", moved < 1f)
            assertTrue("the pull did not say it checks", checking)
        }
    }

    @Test
    fun aPullOnAnEmptyFolderChecks() {
        rule.onNode(hasContentDescription("Accounts")).performClick()
        // The folders may be there before the sheet is laid out: wait until it has come up.
        var at = -1f
        waitFor {
            val top = if (has(hasText("Other folders"))) bounds(hasText("Other folders")).top else 0f
            (top > 0f && top == at).also { at = top }
        }
        // The test account has more folders of its own than show before any is pinned.
        rule.onNode(hasText("Other folders")).apply { runCatching { performScrollTo() } }.performClick()
        waitFor { has(hasText("Empty")) }
        rule.onNode(hasText("Empty")).apply { runCatching { performScrollTo() } }.performClick()
        waitFor { has(hasText("Nothing here")) }
        film("empty-pull", every = 4) {
            track("checking", hasText(CHECKING))
            frames(30)
            val dp = rule.density.density
            val from = Offset(screen.width / 2, screen.height * 0.4f)
            swipe(from, Offset(from.x, from.y + 250 * dp), ms = 500)
            assertTrue("a pull on an empty folder did not say it checks", until(120) { last("checking") != null })
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
            assertEquals(emptyList<String>(), blinks("a"))
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
            // The page left goes on from the list's rows in the same frame.
            assertEquals(emptyList<String>(), blinks("a") + blinks("digest"))
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

    companion object {
        /** What the line over the dock says while a pull's check runs. */
        const val CHECKING = "Checking for new mail…"
    }
}

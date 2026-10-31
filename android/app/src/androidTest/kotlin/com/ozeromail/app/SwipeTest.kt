package com.ozeromail.app

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Swipes on message rows, as set by default: Done on the left and, further, Delete; Read on the
 * right. And the dock, dragged up, raises the mailboxes.
 */
@RunWith(AndroidJUnit4::class)
class SwipeTest : TestMail() {
    @Test
    fun doneSwipeTakesTheRowAndRowsBelowCloseUp() {
        val (a, b, c) = letters()
        film("done-swipe") {
            track("a", text(a))
            track("b", text(b))
            track("c", text(c))
            frame()
            val row = last("a")!!
            swipe(row.at(0.85f), row.at(0.3f), ms = 200)
            val closed = until(150) { last("a") == null && abs(last("b")!!.top - row.top) < 1f }
            frames(10)
            assertTrue("the row below did not take the swiped row's place", closed)
            assertTrue("no toast", has(hasText("Undo")))
        }
    }

    @Test
    fun undoBringsTheRowBack() {
        val (a, b) = letters()
        film("done-undo") {
            track("a", text(a))
            track("b", text(b))
            frame()
            val row = last("a")!!
            // Quick, without pictures: Undo has a few seconds of real time.
            shooting = false
            swipe(row.at(0.85f), row.at(0.3f), ms = 200)
            until(60) { has(hasText("Undo") and hasClickAction()) }
            // Once the toast is in: while it rises it is not yet where it will be tapped.
            frames(30)
            rule.onNode(hasText("Undo") and hasClickAction()).performClick()
            shooting = true
            val back = until(150) { last("a")?.let { abs(it.top - row.top) < 1f && abs(it.left) < 1f } == true }
            frames(10)
            assertTrue("the row did not come back to its place", back)
        }
    }

    @Test
    fun shortSwipeSpringsBack() {
        val (a) = letters()
        film("short-swipe") {
            track("a", text(a))
            frame()
            val row = last("a")!!
            swipe(row.at(0.8f), row.at(0.7f), ms = 300)
            val settled = until(90) { abs(last("a")!!.left) < 1f }
            frames(10)
            assertTrue("the row did not settle back", settled)
            assertFalse("a short swipe acted", has(hasText("Undo")))
        }
    }

    @Test
    fun readSwipeLeavesTheRowInPlace() {
        val (a) = letters()
        film("read-swipe") {
            track("a", text(a))
            frame()
            val row = last("a")!!
            swipe(row.at(0.15f), row.at(0.7f), ms = 200)
            val back = until(150) { last("a")?.let { abs(it.left) < 1f && abs(it.top - row.top) < 1f } == true }
            frames(10)
            assertTrue("the row did not come back after Read", back)
            // It goes back from where it was let go, not off the screen and in again.
            assertTrue("the row went off the screen", frames.all { (it.at["a"]?.left ?: 0f) < screen.width * 0.9f })
        }
    }

    @Test
    fun dragPastTheEdgeAndBackDoesNothing() {
        val (a) = letters()
        film("drag-back") {
            track("a", text(a))
            frame()
            val row = last("a")!!
            drag(row.at(0.85f), row.at(0.3f), ms = 250)
            frames(6)
            // Back to where it began, slowly, so nothing is flung.
            val steps = 20
            val from = row.at(0.3f)
            val to = row.at(0.85f)
            for (step in 1..steps) moveTo(Offset(from.x + (to.x - from.x) * step / steps, from.y))
            frames(6)
            up()
            val settled = until(90) { abs(last("a")!!.left) < 1f }
            frames(10)
            assertTrue("the row did not settle back", settled)
            assertFalse("a cancelled swipe acted", has(hasText("Undo")))
        }
    }

    @Test
    fun scrollingIsNotASwipe() {
        val (a) = letters()
        film("scroll") {
            track("a", text(a))
            frame()
            val row = last("a")!!
            // Mostly up, a little to the side, as a thumb scrolls.
            swipe(row.at(0.6f), Offset(screen.width * 0.52f, row.center.y - screen.height * 0.3f), ms = 250)
            frames(40)
            val sideways = frames.mapNotNull { it.at["a"]?.left }.maxOf { abs(it) }
            assertTrue("the row moved ${sideways.toInt()} px sideways while scrolling", sideways < 1f)
            assertTrue("the list did not scroll", last("a")?.top?.let { it < row.top - 100f } ?: true)
        }
    }

    @Test
    fun quickFlickTakesTheRow() {
        val (a) = letters()
        film("flick") {
            track("a", text(a))
            frame()
            val row = last("a")!!
            swipe(row.at(0.75f), row.at(0.55f), ms = 48)
            frames(90)
            assertNull("a quick flick did not take the row", last("a"))
        }
    }

    @Test
    fun neighboursFollowAHeldRow() {
        val (a, b) = letters()
        film("held-row") {
            track("a", text(a))
            track("b", text(b))
            frame()
            val row = last("a")!!
            drag(row.at(0.85f), row.at(0.6f), ms = 300)
            frames(30)
            val held = last("a")!!.left
            val drawn = last("b")!!.left
            up()
            frames(60)
            assertTrue("the next row was not drawn along ($drawn px for $held px)", drawn < 0f && drawn > held * 0.5f)
            assertEquals("the next row did not settle", 0f, last("b")!!.left, 1f)
        }
    }

    @Test
    fun longSwipeDeletes() {
        val (a) = letters()
        film("long-swipe") {
            track("a", text(a))
            frame()
            val row = last("a")!!
            drag(row.at(0.95f), row.at(0.15f), ms = 300)
            frames(20)
            assertTrue("past the long mark the strip does not say Delete", has(hasText("Delete")))
            up()
            val gone = until(150) { last("a") == null }
            frames(10)
            assertTrue("a long swipe did not take the row", gone)
            assertTrue("a long swipe did not delete", has(hasText("Moved to Trash")))
        }
    }

    @Test
    fun longSwipeBackToShortIsDone() {
        val (a) = letters()
        film("long-back") {
            track("a", text(a))
            frame()
            val row = last("a")!!
            drag(row.at(0.95f), row.at(0.15f), ms = 300)
            frames(10)
            // Back, slowly, to between the two marks.
            val steps = 20
            val from = row.at(0.15f)
            val to = row.at(0.55f)
            for (step in 1..steps) moveTo(Offset(from.x + (to.x - from.x) * step / steps, from.y))
            frames(10)
            up()
            val gone = until(150) { last("a") == null }
            frames(10)
            assertTrue("the short swipe did not take the row", gone)
            assertTrue("back from the long mark it did not archive", has(hasText("Moved to Archive")))
        }
    }

    @Test
    fun doneSwipeInTheArchiveMovesBackToInbox() {
        val (a) = letters()
        film("archive-swipe") {
            track("a", text(a))
            frame()
            // Quick, without pictures, to the archive.
            shooting = false
            val row = last("a")!!
            swipe(row.at(0.85f), row.at(0.3f), ms = 200)
            until(150) { last("a") == null }
            val pill = bounds(hasText("Search mail"))
            drag(pill.center, Offset(pill.center.x, pill.center.y - screen.height * 0.4f), ms = 300)
            up()
            // One account: its own folders, not the ones of all accounts.
            val archive = hasText("Archive") and hasClickAction()
            until(120) { has(archive) }
            rule.onNode(archive).performClick()
            until(300) { last("a") != null }
            frames(30)
            shooting = true
            val there = last("a")!!
            swipe(there.at(0.85f), there.at(0.3f), ms = 200)
            val gone = until(150) { last("a") == null }
            val told = until(90) { has(hasText("Moved to Inbox")) }
            frames(10)
            assertTrue("the Done swipe did nothing in the archive", gone)
            assertTrue("the Done swipe in the archive did not move it to the inbox", told)
        }
    }

    @Test
    fun dockDraggedUpRaisesTheMailboxes() {
        film("dock-up") {
            track("sheet", hasText("Mailboxes"))
            frame()
            val pill = bounds(hasText("Search mail"))
            drag(pill.center, Offset(pill.center.x, pill.center.y - screen.height * 0.25f), ms = 300)
            frames(6)
            val rising = last("sheet")
            up()
            frames(60)
            val top = last("sheet")?.top
            frames(6)
            assertTrue("the sheet did not follow the finger", rising != null)
            assertTrue("the sheet did not settle open", top != null && abs(last("sheet")!!.top - top) < 1f)
            assertTrue("the sheet is not up", top!! < pill.top - screen.height * 0.2f)
        }
    }

    @Test
    fun dockDraggedALittleFallsBack() {
        film("dock-back") {
            track("sheet", hasText("Mailboxes"))
            frame()
            val pill = bounds(hasText("Search mail"))
            // Slowly, a short way: not enough to open.
            drag(pill.center, Offset(pill.center.x, pill.center.y - screen.height * 0.08f), ms = 600)
            frames(10)
            up()
            frames(60)
            assertFalse("a short slow drag left the mailboxes open", has(hasText("Mailboxes")))
        }
    }
}

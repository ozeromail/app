package com.ozeromail.app

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Tabs with mail and without: the lake and the rows pass between them without a blink. */
@RunWith(AndroidJUnit4::class)
class EmptyTabTest : TestMail() {
    override val user = "tabs@example.com"

    @Test
    fun tapsBetweenEmptyAndFullTabs() {
        val (a) = letters()
        film("empty-tab-tap", every = 1) {
            meter = ::water
            track("a", text(a))
            track("empty", hasText(EMPTY))
            track("digest", text("Digest 1:"))
            frames(4)
            rule.onNode(hasText("Notifications")).performClick()
            frames(70)
            rule.onNode(hasText("Newsletters")).performClick()
            frames(70)
            rule.onNode(hasText("Notifications")).performClick()
            frames(70)
            rule.onNode(hasText("People")).performClick()
            frames(70)
            assertEquals(emptyList<String>(), flickers())
        }
    }

    @Test
    fun dragsBetweenEmptyAndFullTabs() {
        film("empty-tab-drag", every = 1) {
            meter = ::water
            track("empty", hasText(EMPTY))
            track("digest", text("Digest 1:"))
            frames(4)
            val people = bounds(hasText("People"))
            val notifications = bounds(hasText("Notifications"))
            swipe(people.center, notifications.center, ms = 300)
            frames(70)
            swipe(notifications.center, people.center, ms = 300)
            frames(70)
            assertEquals(emptyList<String>(), flickers())
        }
    }

    /**
     * Frames where the water was brighter than the lake at rest, as two lakes over each other,
     * or lost much of it in one frame, as a lake cut off.
     */
    private fun Film.flickers(): List<String> {
        val water = frames.map { it.meter ?: 0f }
        // The value the lake shows longest.
        val still = water.filter { it > 0f }.groupBy { it }.maxByOrNull { it.value.size }?.key ?: 0f
        assertTrue("no lake was filmed", still > 0f)
        return water.indices.mapNotNull { i ->
            when {
                water[i] > still * 1.25f -> "frame $i: ${water[i].toInt()} water against ${still.toInt()} at rest"
                i > 0 && water[i - 1] - water[i] > still * 0.4f -> "frame $i: water fell from ${water[i - 1].toInt()} to ${water[i].toInt()}"
                else -> null
            }
        }
    }

    /** Pale blue water pixels across the middle of the screen. */
    private fun water(shot: Bitmap): Float {
        var n = 0
        val w = shot.width
        val h = shot.height
        for (y in (h * 0.12f).toInt() until (h * 0.7f).toInt() step 2) {
            for (x in (w * 0.15f).toInt() until (w * 0.85f).toInt() step 2) {
                val c = shot.getPixel(x, y)
                // Pale only: an unread dot passing by is the full accent.
                if (Color.blue(c) - Color.red(c) > 60 && Color.blue(c) > 140 && Color.red(c) > 110) n++
            }
        }
        return n.toFloat()
    }

    companion object {
        val EMPTY: String get() = InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.tab_empty_title)
    }
}

package com.ozeromail.app

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
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
     * Blue water pixels right of the middle of the screen, where the unread dots of rows
     * gliding in or away never are while they show.
     */
    private fun water(shot: Bitmap): Float {
        val left = (shot.width * 0.4f).toInt()
        val top = (shot.height * 0.12f).toInt()
        val w = (shot.width * 0.85f).toInt() - left
        val h = (shot.height * 0.7f).toInt() - top
        val pixels = IntArray(w * h).also { shot.getPixels(it, 0, w, left, top, w, h) }
        var n = 0
        for (y in 0 until h step 2) {
            for (x in 0 until w step 2) {
                val c = pixels[y * w + x]
                if (Color.blue(c) - Color.red(c) > 60 && Color.blue(c) > 140) n++
            }
        }
        return n.toFloat()
    }

    companion object {
        val EMPTY: String get() = InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.tab_empty_title)
    }
}

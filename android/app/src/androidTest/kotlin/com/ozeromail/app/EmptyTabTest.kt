package com.ozeromail.app

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlin.math.abs
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
        }
    }

    /** Blue water pixels across the middle of the screen, where the lake lies and no unread dot does. */
    private fun water(shot: Bitmap): Float {
        var n = 0
        val w = shot.width
        val h = shot.height
        for (y in (h * 0.12f).toInt() until (h * 0.7f).toInt() step 2) {
            for (x in (w * 0.15f).toInt() until (w * 0.85f).toInt() step 2) {
                val c = shot.getPixel(x, y)
                if (Color.blue(c) - Color.red(c) > 60 && Color.blue(c) > 140 && abs(Color.green(c) - Color.blue(c)) < 160) n++
            }
        }
        return n.toFloat()
    }

    companion object {
        val EMPTY: String get() = InstrumentationRegistry.getInstrumentation().targetContext.getString(R.string.tab_empty_title)
    }
}

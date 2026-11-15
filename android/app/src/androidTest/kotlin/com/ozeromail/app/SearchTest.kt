package com.ozeromail.app

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The filter chips over the search field. */
@RunWith(AndroidJUnit4::class)
class SearchTest : TestMail() {
    private val unread = hasText("Unread") and isToggleable()
    private val flagged = hasText("Flagged") and isToggleable()

    @Test
    fun aChipTurnedOnSlidesToTheFrontAndBack() {
        rule.onAllNodes(hasContentDescription("Search mail") or hasText("Search mail")).onFirst().performClick()
        // Still once the screen and the keyboard have come up.
        var at: Rect? = null
        waitFor { has(flagged) && bounds(flagged).let { now -> (now == at).also { at = now } } }
        toggle("on") { assertTrue("the chip turned on is not at the front", it.left < last("unread")!!.left) }
        toggle("off") { assertTrue("the chip turned off did not go back", it.left > last("unread")!!.left) }
    }

    /** Films a tap on the flagged chip, checks the motion, then where it ended up. */
    private fun toggle(step: String, ended: Film.(Rect) -> Unit) {
        film("search-chip-$step") {
            track("unread", unread)
            track("flagged", flagged)
            frame()
            down(last("flagged")!!.center)
            up()
            frames(40)
            check(step)
            ended(last("flagged")!!)
        }
    }

    /** Both chips stay in sight the whole way and move without a jump. */
    private fun Film.check(step: String) {
        for (label in listOf("unread", "flagged")) {
            val gone = frames.indexOfFirst { frame -> frame.at[label].let { it == null || it.left < 0f } }
            assertTrue("$step: $label left the screen on frame $gone", gone < 0)
            val jumps = jumps(label, jerk = 6 * rule.density.density)
            assertTrue("$step: $label jumped: $jumps", jumps.isEmpty())
        }
    }
}

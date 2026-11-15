package com.ozeromail.app

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
    @Test
    fun aChipTurnedOnSlidesToTheFrontAndBack() {
        rule.onAllNodes(hasContentDescription("Search mail") or hasText("Search mail")).onFirst().performClick()
        val unread = hasText("Unread") and isToggleable()
        val flagged = hasText("Flagged") and isToggleable()
        // Still once the screen and the keyboard have come up.
        var at = -1f
        waitFor { has(flagged) && bounds(flagged).let { it.left + it.top * 7 }.let { now -> (now == at).also { at = now } } }
        film("search-chip-on") {
            track("unread", unread)
            track("flagged", flagged)
            frame()
            val chip = last("flagged")!!
            down(chip.center)
            up()
            frames(40)
            check("on")
            assertTrue("the chip turned on is not at the front", last("flagged")!!.left < last("unread")!!.left)
        }
        film("search-chip-off") {
            track("unread", unread)
            track("flagged", flagged)
            frame()
            val chip = last("flagged")!!
            down(chip.center)
            up()
            frames(40)
            check("off")
            assertTrue("the chip turned off did not go back", last("flagged")!!.left > last("unread")!!.left)
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

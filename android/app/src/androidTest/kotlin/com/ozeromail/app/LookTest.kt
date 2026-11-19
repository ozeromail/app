package com.ozeromail.app

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Test
import org.junit.runner.RunWith

/** How the inbox and a message look by day and at night: full-size stills to compare by eye, with no checks. */
@RunWith(AndroidJUnit4::class)
class LookTest : TestMail() {
    @Test
    fun inboxByDayAndNight() {
        // The rows run under the dock, so its glass has something to show.
        film("look", every = Int.MAX_VALUE) { dayAndNight() }
    }

    @Test
    fun messageByDayAndNight() {
        val (a) = letters()
        rule.onNode(text(a)).performClick()
        waitFor { has(hasContentDescription("Back")) }
        film("look-message", every = Int.MAX_VALUE) { dayAndNight() }
    }

    private fun Film.dayAndNight() {
        settle()
        still("day")
        try {
            night(true)
            settle()
            still("night")
        } finally {
            night(false)
        }
    }

    private fun night(on: Boolean) {
        InstrumentationRegistry.getInstrumentation().uiAutomation
            .executeShellCommand("cmd uimode night ${if (on) "yes" else "no"}")
            .close()
    }

    /** A second of real time, frame by frame, for the theme and the blur to catch up. */
    private fun settle() {
        var left = 60
        waitFor { left-- <= 0 }
    }
}

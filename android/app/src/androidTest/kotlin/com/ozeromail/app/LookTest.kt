package com.ozeromail.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Test
import org.junit.runner.RunWith

/** How the inbox looks by day and at night: full-size stills to compare by eye, with no checks. */
@RunWith(AndroidJUnit4::class)
class LookTest : TestMail() {
    @Test
    fun inboxByDayAndNight() {
        film("look", every = Int.MAX_VALUE) {
            // The rows run under the dock, so its glass has something to show.
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

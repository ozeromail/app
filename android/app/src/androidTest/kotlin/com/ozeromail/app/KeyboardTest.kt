package com.ozeromail.app

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The keyboard leaves with the screen it types in, not after it. */
@RunWith(AndroidJUnit4::class)
class KeyboardTest : TestMail() {
    @Test
    fun closingANewMessageLetsGoOfTheKeyboardAtOnce() {
        // A field being typed in, which holds the keyboard; other nodes may hold focus too.
        val typing = isFocused() and hasSetTextAction()
        rule.onAllNodes(hasContentDescription("Write")).onFirst().performClick()
        // A new message starts in To, with the keyboard up.
        waitFor { has(typing) }
        rule.onAllNodes(hasContentDescription("Close")).onFirst().performClick()
        val send = hasContentDescription("Send")
        // Frame by frame until nothing holds the keyboard: the message must still be sliding away.
        var frames = 0
        while (has(typing) && frames < 60) {
            rule.mainClock.advanceTimeByFrame()
            frames++
        }
        assertFalse("the keyboard is still held after $frames frames", has(typing))
        assertTrue("the keyboard stayed until the message was gone ($frames frames)", has(send) && bounds(send).top < screen.bottom)
        waitFor { !has(send) }
    }
}

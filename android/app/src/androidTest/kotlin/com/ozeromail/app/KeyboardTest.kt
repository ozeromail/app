package com.ozeromail.app

import androidx.compose.ui.test.hasContentDescription
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
        rule.onAllNodes(hasContentDescription("Write")).onFirst().performClick()
        // A new message starts in To, with the keyboard up.
        waitFor { has(isFocused()) }
        rule.onAllNodes(hasContentDescription("Close")).onFirst().performClick()
        repeat(3) { rule.mainClock.advanceTimeByFrame() }
        // The message is still sliding away, and nothing in it holds the keyboard any more.
        assertTrue("the message closed at once", has(hasContentDescription("Send")))
        assertFalse("the keyboard stays until the message is gone", has(isFocused()))
    }
}

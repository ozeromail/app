package com.ozeromail.app

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.isFocused
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.printToLog
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
        val send = hasContentDescription("Send")
        rule.onAllNodes(hasContentDescription("Write")).onFirst().performClick()
        // A new message starts in To, with the keyboard up.
        waitFor { has(typing) && has(send) }
        // Risen all the way, so the close button stays where it is tapped.
        var at = -1f
        waitFor { bounds(send).top.let { top -> (top < screen.bottom && top == at).also { at = top } } }
        // The one on screen; another may be there with no size.
        fun close() = rule.onAllNodes(hasContentDescription("Close")).fetchSemanticsNodes()
            .map { it.boundsInRoot }
            .firstOrNull { it.width > 0f && it.top < screen.bottom }
        waitFor { close() != null }
        val close = requireNotNull(close())
        film("keyboard-close", every = 3) {
            track("send", send)
            track("typing", typing)
            down(close.center)
            up()
            // Frame by frame until nothing holds the keyboard: the message must not be gone yet.
            val let = until(60) { !has(typing) }
            if (!let) rule.onRoot().printToLog(TAG)
            assertFalse("the keyboard is still held after ${frames.size} frames", has(typing))
            // Still there, if already below the edge: the keyboard went down under it at once.
            assertTrue("the keyboard stayed until the message was gone (${frames.size} frames)", last("send") != null)
            frames(40)
        }
    }

    private companion object {
        const val TAG = "KeyboardTest"
    }
}

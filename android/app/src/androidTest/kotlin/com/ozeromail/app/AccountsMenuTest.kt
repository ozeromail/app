package com.ozeromail.app

import android.graphics.Bitmap
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.services.storage.TestStorage
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The accounts button, held: a menu of each account's inbox and all inboxes over it. */
@RunWith(AndroidJUnit4::class)
class AccountsMenuTest : TestMail() {
    override val others = listOf(TABS)

    private val button = hasContentDescription("Accounts") and hasClickAction()

    /** A line of the open menu: the sheet is closed, so a name shows only there. */
    private fun line(name: String) = hasText(name) and hasClickAction()

    @Test
    fun holdThenSlideToAnAccountOpensItsInbox() {
        film("accounts-hold") {
            frame()
            rule.onNode(button).performTouchInput { down(center) }
            // Past the long press, the menu is in.
            frames(40)
            assertTrue("no menu on a held accounts button", has(line(TABS)) && has(line("All inboxes")))
            screenshot("accounts-hold-menu")
            // The finger, still down, goes to the line and lifts there.
            val from = rule.onNode(button).fetchSemanticsNode()
            val to = rule.onNode(line(TABS)).fetchSemanticsNode()
            val target = to.positionOnScreen - from.positionOnScreen + Offset(to.size.width / 2f, to.size.height / 2f)
            rule.onNode(button).performTouchInput {
                val start = center
                for (step in 1..10) moveTo(start + (target - start) * (step / 10f))
            }
            frames(10)
            rule.onNode(button).performTouchInput { up() }
            frames(40)
            assertTrue("the menu stayed after a pick", !has(line("All inboxes")))
            assertTrue("the picked account's inbox did not open", has(hasText(TABS)))
        }
    }

    @Test
    fun holdLetGoLeavesTheMenuForATap() {
        film("accounts-hold-tap") {
            frame()
            rule.onNode(button).performTouchInput { longClick() }
            frames(20)
            assertTrue("the menu closed as the finger lifted on the button", has(line(TABS)))
            rule.onNode(line(TABS)).performClick()
            frames(40)
            assertTrue("the picked account's inbox did not open", has(hasText(TABS)) && !has(line("All inboxes")))
        }
    }

    /** The whole screen with the menu's own window, which the film's pictures leave out. */
    private fun screenshot(name: String) {
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
        TestStorage().openOutputFile("films/$name.png").use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
        shot.recycle()
    }
}

package com.ozeromail.app

import android.graphics.Bitmap
import android.util.Log
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isRoot
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.test.swipeDown
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.test.hasSetTextAction
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.services.storage.TestStorage
import kotlinx.coroutines.runBlocking
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.ozero_api.Endpoint

/** Shared bits of the UX tour: full-screen stills and the semantics tree beside each. */
private object Tour {
    var count = 0

    fun shell(command: String) {
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command).close()
    }

    /** The whole screen, dialogs and system bars too, as `tour-<group>-<n>-<label>.png`. */
    fun save(group: String, label: String, roots: List<SemanticsNode>) {
        val name = "tour-$group-${"%02d".format(++count)}-$label"
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        if (shot != null) {
            TestStorage().openOutputFile("films/$name.png").use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
            shot.recycle()
        }
        val tree = buildString { roots.forEach { dump(it, 0) } }
        TestStorage().openOutputFile("films/$name.txt").use { it.write(tree.toByteArray()) }
    }

    private fun StringBuilder.dump(node: SemanticsNode, depth: Int) {
        val c = node.config
        val text = c.getOrNull(SemanticsProperties.Text)?.joinToString("|") { it.text }
        val desc = c.getOrNull(SemanticsProperties.ContentDescription)?.joinToString("|")
        val b = node.boundsInRoot
        if (text != null || desc != null) {
            append("  ".repeat(depth.coerceAtMost(20)))
            append("[${b.left.toInt()},${b.top.toInt()} ${b.width.toInt()}x${b.height.toInt()}]")
            text?.let { append(" t=\"$it\"") }
            desc?.let { append(" d=\"$it\"") }
            append('\n')
        }
        node.children.forEach { dump(it, depth + 1) }
    }
}

/**
 * Walks every screen of the app on the tour's test mail, saving stills to look at: no checks.
 * Each step that fails is logged and saved as such, and the walk goes on.
 */
@RunWith(AndroidJUnit4::class)
class TourTest : TestMail() {
    private var group = "x"

    private fun roots() = rule.onAllNodes(isRoot(), useUnmergedTree = true).fetchSemanticsNodes()

    private fun settle(frames: Int = 50) {
        var left = frames
        waitFor { left-- <= 0 }
    }

    private fun shot(label: String, frames: Int = 50) {
        settle(frames)
        Tour.save(group, label, roots())
    }

    private fun step(label: String, block: () -> Unit) {
        try {
            block()
        } catch (e: Throwable) {
            Log.e(TAG, "step $group/$label failed", e)
            runCatching { shot("FAILED-$label", 5) }
        }
    }

    private fun visible(matcher: SemanticsMatcher) = rule.onAllNodes(matcher).fetchSemanticsNodes()
        .any { it.boundsInRoot.width > 0f && it.boundsInRoot.top < screen.bottom && it.boundsInRoot.bottom > 0f }

    private fun tap(matcher: SemanticsMatcher) {
        waitFor(8_000) { rule.onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty() }
        val nodes = rule.onAllNodes(matcher).fetchSemanticsNodes()
        val i = nodes.indexOfFirst { it.boundsInRoot.width > 0f && it.boundsInRoot.top < screen.bottom && it.boundsInRoot.bottom > 0f }
            .coerceAtLeast(0)
        rule.onAllNodes(matcher)[i].apply { runCatching { performScrollTo() } }.performClick()
        settle(30)
    }

    private fun tap(text: String) = tap(hasText(text, substring = true))
    private fun tapDesc(desc: String) = tap(hasContentDescription(desc))

    private fun hold(text: String) {
        rule.onAllNodes(hasText(text, substring = true)).onFirst().performTouchInput { longClick() }
        settle(30)
    }

    private fun scrollUp() {
        rule.onAllNodes(isRoot()).onFirst().performTouchInput { swipeUp(startY = height * 0.75f, endY = height * 0.3f) }
    }

    private fun type(text: String) {
        rule.onAllNodes(hasSetTextAction()).onFirst().performTextInput(text)
    }

    private fun backTo() {
        back()
        settle(40)
    }

    private fun panel() = tapDesc("Accounts")

    @Test
    fun a_inbox() {
        group = "inbox"
        step("people") { shot("people") }
        step("scrolled") { scrollUp(); shot("people-scrolled") }
        step("notifications") { tap("Notifications"); shot("notifications") }
        step("newsletters") { tap("Newsletters"); shot("newsletters") }
        step("back-people") { tap("People") }
        step("swipe-half-left") {
            val row = bounds(text("Trip to the lake"))
            rule.onAllNodes(isRoot()).onFirst().performTouchInput {
                down(row.center.copy(x = width * 0.8f))
                repeat(10) { moveBy(androidx.compose.ui.geometry.Offset(-width * 0.035f, 0f)) }
            }
            shot("swipe-short-left", 10)
            rule.onAllNodes(isRoot()).onFirst().performTouchInput {
                repeat(10) { moveBy(androidx.compose.ui.geometry.Offset(-width * 0.035f, 0f)) }
            }
            shot("swipe-long-left", 10)
            rule.onAllNodes(isRoot()).onFirst().performTouchInput { cancel() }
            settle(60)
        }
        step("swipe-right") {
            val row = bounds(text("Trip to the lake"))
            rule.onAllNodes(isRoot()).onFirst().performTouchInput {
                down(row.center.copy(x = width * 0.2f))
                repeat(10) { moveBy(androidx.compose.ui.geometry.Offset(width * 0.035f, 0f)) }
            }
            shot("swipe-short-right", 10)
            rule.onAllNodes(isRoot()).onFirst().performTouchInput { cancel() }
            settle(60)
        }
        step("choose") { hold("Trip to the lake"); shot("choosing"); tap("Select all"); shot("choosing-all"); backTo(); shot("after-choose") }
        step("done-toast") {
            val row = bounds(text("Keys for the cabin"))
            rule.onAllNodes(isRoot()).onFirst().performTouchInput { swipe(row.center.copy(x = width * 0.9f), row.center.copy(x = width * 0.45f), 250L) }
            shot("toast-after-swipe", 30)
            tap("Undo")
            shot("after-undo")
        }
        step("night") {
            Tour.shell("cmd uimode night yes")
            shot("night", 90)
            tap("Newsletters")
            shot("night-newsletters")
            Tour.shell("cmd uimode night no")
            settle(90)
        }
    }

    @Test
    fun b_panel() {
        group = "panel"
        step("open") { panel(); shot("open", 80) }
        step("scrolled") { scrollUp(); shot("scrolled") }
        step("other") { tap("Other folders"); shot("other-folders"); scrollUp(); shot("other-folders-scrolled") }
        step("edit") { tap("Edit"); shot("edit"); tap("Done") }
        step("close") { backTo(); shot("closed") }
        for (folder in listOf("Sent", "Drafts", "Archive", "Trash", "Spam", "Flagged", "Snoozed", "Unread", "With attachments", "Receipts", "Empty")) {
            step("folder-$folder") {
                panel()
                settle(40)
                if (!visible(hasText(folder))) runCatching { tap("Other folders") }
                tap(hasText(folder))
                shot("folder-${folder.replace(' ', '_')}", 80)
                backTo()
            }
        }
    }

    @Test
    fun c_letters() {
        group = "letter"
        step("thread") {
            tap("Trip to the lake")
            shot("thread", 90)
            scrollUp()
            shot("thread-scrolled")
            runCatching { tapDesc("More") }
            shot("thread-more")
            backTo()
            backTo()
        }
        step("attachments") {
            tap("Photos and the plan")
            shot("attachments", 90)
            scrollUp()
            shot("attachments-scrolled")
            tap("plan.pdf")
            shot("attachment-pdf", 90)
            backTo()
            runCatching { tap("lake.png"); shot("attachment-png", 90); backTo() }
            backTo()
        }
        step("invite") { tap("Invitation: Planning"); shot("invite", 90); backTo() }
        step("long") {
            tap("Письмо из поездки")
            shot("long-ru", 90)
            scrollUp(); scrollUp()
            shot("long-ru-scrolled")
            backTo()
        }
        step("html") {
            tap("Newsletters")
            tap("This week by the water")
            shot("html", 120)
            scrollUp()
            shot("html-scrolled", 60)
            runCatching { tap("Show"); shot("html-images", 120) }
            backTo()
            tap("People")
        }
        step("people-header") {
            tap("Keys for the cabin")
            settle(60)
            runCatching { tap("Eva Novak"); shot("sender-details") ; backTo() }
            shot("keys")
            backTo()
        }
    }

    @Test
    fun d_actions() {
        group = "actions"
        step("more") { tap("Keys for the cabin"); settle(60); tapDesc("More"); shot("more-menu") }
        step("snooze") { tap("Snooze"); shot("snooze-sheet"); runCatching { tap("Pick a date and time"); shot("snooze-pick") }; backTo(); backTo() }
        step("move") { runCatching { tapDesc("More") }; tap("Move to folder"); shot("move-sheet"); backTo() }
        step("block") { runCatching { tapDesc("More") }; tap("Block sender"); shot("block"); backTo() }
        step("delete") { runCatching { tapDesc("More") }; runCatching { tap("Delete message") }; shot("after-delete", 20); runCatching { tap("Undo") } }
        step("unsubscribe") {
            backTo()
            tap("Newsletters")
            tap("Digest 1:")
            settle(60)
            runCatching { tap("Unsubscribe") }.onFailure { tapDesc("More"); tap("Unsubscribe") }
            shot("unsubscribe")
            backTo()
        }
    }

    @Test
    fun e_compose() {
        group = "compose"
        step("new") { tapDesc("Write"); shot("new", 80); type("anna"); shot("to-suggestions"); backTo(); backTo() }
        step("reply") {
            tap("Trip to the lake")
            settle(60)
            runCatching { tap("Reply…") }.onFailure { tapDesc("Reply") }
            shot("reply-inline", 80)
            runCatching { tapDesc("Open full screen"); shot("reply-full", 80) }
            backTo(); backTo(); backTo()
        }
        step("forward") {
            tap("Photos and the plan")
            settle(60)
            tapDesc("More")
            shot("more-before-forward")
            tap("Forward")
            shot("forward", 80)
            backTo(); backTo()
        }
    }

    @Test
    fun f_search() {
        group = "search"
        step("open") { tap("Search mail"); shot("open", 80) }
        step("type") { type("lake"); shot("results", 120) }
        step("none") { rule.onAllNodes(hasSetTextAction()).onFirst().performTextInput("zzqq"); shot("no-results", 160) }
        step("person") { backTo(); tap("Search mail"); type("Anna"); shot("person", 120) }
    }

    @Test
    fun g_settings() {
        group = "settings"
        step("open") { panel(); settle(40); tap("Settings"); shot("main", 80) }
        step("scrolled") { scrollUp(); shot("main-scrolled"); scrollUp(); shot("main-bottom") }
        step("account") { rule.onAllNodes(isRoot()).onFirst().performTouchInput { swipeDown() }; settle(30); tap(USER); shot("account", 80); scrollUp(); shot("account-scrolled"); scrollUp(); shot("account-bottom"); backTo() }
        for (page in listOf("Swipes", "Notifications", "Reading", "Images in mail", "Default signature", "Blocked senders")) {
            step(page) {
                if (!visible(hasText(page))) scrollUp()
                tap(page)
                shot(page.replace(' ', '_'), 80)
                scrollUp()
                shot("${page.replace(' ', '_')}-scrolled")
                backTo()
            }
        }
        step("remove") {
            tap(USER)
            settle(40)
            repeat(3) { scrollUp() }
            tap("Remove account")
            shot("remove-dialog")
            backTo()
        }
    }

    @Test
    fun h_twoAccounts() {
        group = "two"
        step("sign-in-second") {
            val app = androidx.test.core.app.ApplicationProvider.getApplicationContext<OzeroApp>()
            runBlocking {
                app.mail.signIn("work@example.org", PASSWORD, Endpoint("localhost", IMAP_PORT), Endpoint("localhost", SMTP_PORT))
                app.mail.refresh()
            }
            settle(120)
            shot("inbox")
        }
        step("panel") { panel(); shot("panel", 80); scrollUp(); shot("panel-scrolled"); tap("Edit"); shot("panel-edit"); tap("Done"); backTo() }
        step("settings") { panel(); settle(40); tap("Settings"); shot("settings", 80); backTo() }
        step("add") { panel(); settle(40); runCatching { tap("Add account") }; shot("add-account", 80); backTo() }
    }

    @Test
    fun i_largeText() {
        group = "large"
        try {
            Tour.shell("settings put system font_scale 1.6")
            settle(120)
            step("inbox") { shot("inbox", 60) }
            step("letter") { tap("Trip to the lake"); shot("thread", 90); backTo() }
            step("panel") { panel(); shot("panel", 80); tap("Settings"); shot("settings", 80); backTo(); backTo() }
            step("search") { tap("Search mail"); shot("search", 80); backTo() }
        } finally {
            Tour.shell("settings put system font_scale 1.0")
        }
    }

    @Test
    fun j_russian() {
        group = "ru"
        try {
            Tour.shell("cmd locale set-app-locales com.ozeromail.app --locales ru-RU")
            settle(200)
            step("inbox") { shot("inbox", 60) }
            step("letter") { tap("Trip to the lake"); shot("thread", 90); runCatching { tapDesc("Ещё"); shot("more") ; backTo() }; backTo() }
            step("panel") { tapDesc("Аккаунты"); shot("panel", 80); tap("Настройки"); shot("settings", 80); scrollUp(); shot("settings-scrolled"); backTo(); backTo() }
            step("compose") { tapDesc("Написать"); shot("compose", 80); backTo(); backTo() }
        } finally {
            Tour.shell("cmd locale set-app-locales com.ozeromail.app --locales en-US")
        }
    }

    @Test
    fun k_landscape() {
        group = "land"
        try {
            Tour.shell("settings put system accelerometer_rotation 0")
            Tour.shell("settings put system user_rotation 1")
            settle(200)
            step("inbox") { shot("inbox", 60) }
            step("letter") { tap("Trip to the lake"); shot("thread", 90); backTo() }
            step("panel") { panel(); shot("panel", 80); backTo() }
        } finally {
            Tour.shell("settings put system user_rotation 0")
        }
    }

    @Test
    fun l_offlineAndZero() {
        group = "states"
        step("offline") {
            Tour.shell("svc wifi disable")
            Tour.shell("svc data disable")
            settle(120)
            rule.onAllNodes(isRoot()).onFirst().performTouchInput { swipeDown(startY = height * 0.3f, endY = height * 0.8f) }
            shot("offline-pull", 200)
            Tour.shell("svc wifi enable")
            Tour.shell("svc data enable")
            settle(200)
        }
        for (tab in listOf("People", "Notifications", "Newsletters")) {
            step("zero-$tab") {
                if (visible(hasText(tab))) tap(tab)
                val first = rule.onAllNodes(hasText(":", substring = true)).fetchSemanticsNodes()
                    .firstOrNull { it.boundsInRoot.top > screen.height * 0.15f }
                    ?.config?.getOrNull(SemanticsProperties.Text)?.firstOrNull()?.text ?: return@step
                hold(first)
                tap("Select all")
                runCatching { tap("Done") }.onFailure { tapDesc("Done") }
                shot("after-$tab", 120)
            }
        }
        step("zero") { shot("inbox-zero", 200) }
    }

    private companion object {
        const val TAG = "TourTest"
    }
}

/** The first launch, with no account: every way in, and a wrong password. */
@RunWith(AndroidJUnit4::class)
class TourSignInTest {
    @get:Rule
    val rule = createEmptyComposeRule()

    private fun settle(frames: Int = 50) {
        rule.mainClock.autoAdvance = false
        repeat(frames) {
            rule.mainClock.advanceTimeByFrame()
            Thread.sleep(Film.FRAME_MS.toLong())
        }
    }

    private fun shot(label: String, frames: Int = 50) {
        settle(frames)
        Tour.save("signin", label, rule.onAllNodes(isRoot(), useUnmergedTree = true).fetchSemanticsNodes())
    }

    private fun step(label: String, block: () -> Unit) {
        try {
            block()
        } catch (e: Throwable) {
            Log.e("TourTest", "step signin/$label failed", e)
            runCatching { shot("FAILED-$label", 5) }
        }
    }

    private fun tap(text: String) {
        rule.onAllNodes(hasText(text, substring = true)).onFirst().apply { runCatching { performScrollTo() } }.performClick()
        settle(40)
    }

    private fun field(i: Int) = rule.onAllNodes(hasSetTextAction())[i]

    @Test
    fun walk() {
        rule.mainClock.autoAdvance = false
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        try {
            step("welcome") { shot("welcome", 120) }
            step("typed") { field(0).performTextInput("me@yandex.ru"); shot("typed-yandex") }
            step("known") { tap("Continue"); shot("known-provider", 80); scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }; settle(60) }
            step("manual") { tap("Set up manually"); shot("manual", 80) }
            step("exchange") { tap("Exchange"); shot("exchange", 80); scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }; settle(60) }
            step("imap") { tap("IMAP"); shot("imap", 80); scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }; settle(60) }
            step("gmail") { runCatching { tap("Gmail with an app password"); shot("gmail-app-password", 80); scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }; settle(60) } }
            step("back-start") { scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }; settle(60); shot("back-at-start") }
            step("wrong-password") {
                // The test server, its servers typed by hand, with a wrong password.
                scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }
                settle(30)
                tap("Set up manually")
                tap("IMAP")
                val fields = rule.onAllNodes(hasSetTextAction()).fetchSemanticsNodes().size
                Log.i("TourTest", "IMAP fields: $fields")
                runCatching { field(0).performTextInput("test@example.com") }
                runCatching { field(1).performTextInput("wrong") }
                shot("imap-filled")
                runCatching { tap("Sign in") }
                var left = 600
                while (left-- > 0 && rule.onAllNodes(hasText("didn’t accept", substring = true)).fetchSemanticsNodes().isEmpty() &&
                    rule.onAllNodes(hasText("reach", substring = true)).fetchSemanticsNodes().isEmpty()
                ) {
                    rule.mainClock.advanceTimeByFrame()
                    Thread.sleep(16)
                }
                shot("imap-error", 30)
            }
        } finally {
            scenario.close()
        }
    }
}

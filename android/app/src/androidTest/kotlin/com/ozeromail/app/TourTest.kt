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
    private fun tapExact(text: String) = tap(hasText(text))
    private fun tapDesc(desc: String) = tap(hasContentDescription(desc))

    private fun hold(text: String) {
        rule.onAllNodes(hasText(text, substring = true)).onFirst().performTouchInput { longClick() }
        settle(30)
    }

    private fun root() = rule.onAllNodes(isRoot()).onFirst()

    private fun scrollUp() {
        root().performTouchInput { swipeUp(startY = height * 0.75f, endY = height * 0.3f) }
    }

    private fun type(text: String) {
        rule.onAllNodes(hasSetTextAction()).onFirst().performTextInput(text)
    }

    private fun backTo() {
        back()
        settle(40)
    }

    /** Back to the inbox with nothing over it, from wherever a step left off. */
    private fun home() {
        repeat(5) {
            if (visible(hasText("Search mail")) && !visible(hasText("Mailboxes")) && !visible(hasContentDescription("Back"))) return
            back()
            settle(40)
        }
    }

    private fun panel() {
        home()
        tapDesc("Accounts")
        settle(40)
    }

    private fun settings() {
        panel()
        tapExact("Settings")
        settle(40)
    }

    /** A sideways drag on a row held part of the way, then taken back before letting go. */
    private fun drag(row: String, from: Float, by: Float, label: String) {
        val r = bounds(text(row))
        val step = androidx.compose.ui.geometry.Offset(screen.width * by / 10, 0f)
        root().performTouchInput { down(r.center.copy(x = screen.width * from)); repeat(10) { moveBy(step) } }
        shot("$label-short", 10)
        root().performTouchInput { repeat(10) { moveBy(step) } }
        shot("$label-long", 10)
        root().performTouchInput { repeat(20) { moveBy(-step) }; up() }
        settle(60)
    }

    @Test
    fun a_inbox() {
        group = "inbox"
        step("people") { shot("people") }
        step("scrolled") { scrollUp(); shot("people-scrolled") }
        step("notifications") { tapExact("Notifications"); shot("notifications") }
        step("newsletters") { tapExact("Newsletters"); shot("newsletters") }
        step("back-people") { tapExact("People"); root().performTouchInput { swipeDown() }; settle(60) }
        step("swipe-left") { drag("People 24:", 0.85f, -0.035f, "swipe-left") }
        step("swipe-right") { drag("People 24:", 0.15f, 0.035f, "swipe-right") }
        step("choose") { hold("People 24:"); shot("choosing"); tap("Select all"); shot("choosing-all"); backTo(); shot("after-choose") }
        step("done-toast") {
            val row = bounds(text("People 23:"))
            root().performTouchInput { swipe(row.center.copy(x = width * 0.9f), row.center.copy(x = width * 0.45f), 250L) }
            shot("toast-after-swipe", 30)
            tap("Undo")
            shot("after-undo")
        }
        step("night") {
            Tour.shell("cmd uimode night yes")
            shot("night", 90)
            tapExact("Newsletters")
            shot("night-newsletters")
            panel()
            shot("night-panel")
            home()
            tap("Trip to the lake")
            shot("night-thread", 90)
            home()
        }
        Tour.shell("cmd uimode night no")
    }

    @Test
    fun b_panel() {
        group = "panel"
        step("open") { panel(); shot("open", 80) }
        step("scrolled") { scrollUp(); shot("scrolled") }
        step("other") { tap("Other folders"); shot("other-folders"); scrollUp(); shot("other-folders-scrolled") }
        step("pin") { hold("Receipts"); shot("hold-folder") }
        for (folder in listOf("Drafts", "Snoozed", "Sent", "Archive", "Spam", "Trash", "Receipts", "Empty")) {
            step("folder-$folder") {
                panel()
                if (!visible(hasText(folder))) runCatching { tap("Other folders") }
                tapExact(folder)
                shot("folder-$folder", 80)
            }
        }
        step("folder-trash-empty") {
            panel(); tapExact("Trash"); settle(60)
            runCatching { tap("Empty Trash") }
            shot("empty-trash-confirm")
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
            val mores = rule.onAllNodes(hasContentDescription("More")).fetchSemanticsNodes().size
            Log.i(TAG, "thread More buttons: $mores")
            tapDesc("More")
            shot("thread-more")
        }
        step("attachments") {
            home()
            tap("Photos and the plan")
            shot("attachments", 90)
            tap("plan.pdf")
            shot("attachment-pdf", 90)
            backTo()
            tap("lake.png")
            shot("attachment-png", 90)
        }
        step("invite") { home(); tap("Invitation: Planning"); shot("invite", 90); scrollUp(); shot("invite-scrolled") }
        step("long") {
            home()
            tap("Письмо из поездки")
            shot("long-ru", 90)
            scrollUp(); scrollUp()
            shot("long-ru-scrolled")
        }
        step("html") {
            home()
            tapExact("Newsletters")
            tap("This week by the water")
            shot("html", 120)
            scrollUp()
            shot("html-scrolled", 60)
            tapExact("Show")
            shot("html-images", 120)
        }
        step("sender") {
            home()
            tapExact("People")
            tap("Keys for the cabin")
            settle(60)
            tapExact("Eva Novak")
            shot("sender-details")
        }
        step("flagged") { home(); tap("Keys for the cabin"); shot("flagged-letter", 80) }
    }

    @Test
    fun d_actions() {
        group = "actions"
        fun more() { home(); tap("Keys for the cabin"); settle(60); tapExact("More") }
        step("more") { more(); shot("more-menu") }
        step("snooze") { home(); tap("Keys for the cabin"); settle(60); tapExact("Snooze"); shot("snooze-sheet"); tap("Pick a date and time"); shot("snooze-pick") }
        step("move") { more(); tap("Move to folder"); shot("move-sheet") }
        step("block") { more(); tap("Block sender"); shot("block") }
        step("unsubscribe") {
            home()
            tapExact("Newsletters")
            tap("Digest 1:")
            settle(60)
            if (visible(hasText("Unsubscribe"))) tap("Unsubscribe") else { tapExact("More"); tap("Unsubscribe") }
            shot("unsubscribe")
        }
        step("delete") {
            home(); tapExact("People"); tap("People 22:"); settle(60); tapExact("Delete")
            shot("after-delete", 20)
            tap("Undo")
            shot("after-undo")
        }
    }

    @Test
    fun e_compose() {
        group = "compose"
        step("new") { tapDesc("Write"); shot("new", 80); type("anna"); shot("to-suggestions") }
        step("reply") {
            home()
            tap("Trip to the lake")
            settle(60)
            if (visible(hasText("Reply…"))) tap("Reply…") else tapExact("Reply")
            shot("reply", 80)
            runCatching { tapDesc("Open full screen"); shot("reply-full", 80) }
        }
        step("forward") {
            home()
            tap("Photos and the plan")
            settle(60)
            tapExact("More")
            shot("more-before-forward")
            tap("Forward")
            shot("forward", 80)
        }
    }

    @Test
    fun f_search() {
        group = "search"
        step("open") { tap("Search mail"); shot("open", 80) }
        step("type") { type("lake"); shot("results", 120) }
        step("none") { type("zzqq"); shot("no-results", 160) }
        step("person") { home(); tap("Search mail"); type("Anna"); shot("person", 120) }
    }

    @Test
    fun g_settings() {
        group = "settings"
        step("open") { settings(); shot("main", 80) }
        step("scrolled") { scrollUp(); shot("main-bottom") }
        step("account") { settings(); tap(USER); shot("account", 80); scrollUp(); shot("account-scrolled"); scrollUp(); shot("account-bottom") }
        for (page in listOf("Notifications", "Swipes", "Default signature", "Blocked senders", "Text size", "Images in mail")) {
            step(page) {
                settings()
                if (!visible(hasText(page))) scrollUp()
                tapExact(page)
                shot(page.replace(' ', '_'), 80)
                scrollUp()
                shot("${page.replace(' ', '_')}-scrolled")
            }
        }
        step("remove") {
            settings()
            tap(USER)
            settle(40)
            repeat(3) { scrollUp() }
            tap("Remove account")
            shot("remove-dialog")
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
        step("panel") { panel(); shot("panel", 80); scrollUp(); shot("panel-scrolled") }
        step("settings") { settings(); shot("settings", 80) }
        step("add") { settings(); tap("Add account"); shot("add-account", 80) }
    }

    @Test
    fun i_largeText() {
        group = "large"
        try {
            Tour.shell("settings put system font_scale 1.6")
            settle(150)
            step("inbox") { shot("inbox", 60) }
            step("letter") { home(); tap("Trip to the lake"); shot("thread", 90) }
            step("panel") { panel(); shot("panel", 80) }
            step("settings") { settings(); shot("settings", 80) }
            step("search") { home(); tap("Search mail"); shot("search", 80) }
            step("compose") { home(); tapDesc("Write"); shot("compose", 80) }
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
            step("letter") { tap("Trip to the lake"); shot("thread", 90); tapExact("Ещё"); shot("more") }
            step("panel") { back(); settle(30); back(); settle(30); tapDesc("Аккаунты"); shot("panel", 80); tapExact("Настройки"); shot("settings", 80); scrollUp(); shot("settings-scrolled") }
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
            screen = rule.onAllNodes(isRoot()).fetchSemanticsNodes().first().boundsInRoot
            step("inbox") { shot("inbox", 60) }
            step("letter") { tap("Trip to the lake"); shot("thread", 90) }
            step("panel") { panel(); shot("panel", 80) }
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
            root().performTouchInput { swipeDown(startY = height * 0.3f, endY = height * 0.8f) }
            shot("offline-pull", 200)
            shot("offline-later", 600)
        }
        Tour.shell("svc wifi enable")
        Tour.shell("svc data enable")
        settle(200)
        for (tab in listOf("People", "Notifications", "Newsletters")) {
            step("zero-$tab") {
                tapExact(tab)
                settle(30)
                val first = rule.onAllNodes(hasText(":", substring = true)).fetchSemanticsNodes()
                    .firstOrNull { it.boundsInRoot.top > screen.height * 0.15f && it.boundsInRoot.top < screen.height * 0.7f }
                    ?.config?.getOrNull(SemanticsProperties.Text)?.firstOrNull()?.text ?: return@step
                hold(first)
                tap("Select all")
                shot("selected-$tab")
                tap("Done")
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
            step("known") { rule.onAllNodes(hasText("Continue")).onFirst().performClick(); settle(40); shot("known-provider", 80); scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }; settle(60) }
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

package com.ozeromail.app

import android.Manifest
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onRoot
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.rule.GrantPermissionRule
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Before
import org.junit.Rule
import uniffi.ozero_api.Category
import uniffi.ozero_api.Endpoint

/**
 * Starts the app signed in to the test server of tools/ui-test-mail.sh, which the emulator
 * reaches on its own localhost through `adb reverse`, and waits for the inbox. The animation
 * clock stands still from then on: each test moves it frame by frame through a [Film].
 */
abstract class TestMail {
    @get:Rule(order = 0)
    val notifications: GrantPermissionRule = GrantPermissionRule.grant(Manifest.permission.POST_NOTIFICATIONS)

    @get:Rule(order = 1)
    val rule = createEmptyComposeRule()

    private lateinit var scenario: ActivityScenario<MainActivity>

    /** The whole screen, for gestures placed by its size. */
    lateinit var screen: Rect

    @Before
    fun start() {
        val app = ApplicationProvider.getApplicationContext<OzeroApp>()
        runBlocking {
            if (!app.mail.hasAccounts()) {
                app.mail.signIn(USER, PASSWORD, Endpoint("localhost", IMAP_PORT), Endpoint("localhost", SMTP_PORT))
            }
            // The first sync, waited for here rather than through the screen: the list shows only
            // what is saved, and a slow or failed sync then fails with its own reason.
            withTimeout(SYNC_MS) { app.mail.refresh() }
            check(app.mail.inbox(Category.PEOPLE, 1).isNotEmpty()) { "The first sync saved no mail" }
        }
        Settings(app).swipeTipSeen = true
        rule.mainClock.autoAdvance = false
        scenario = ActivityScenario.launch(MainActivity::class.java)
        waitFor { has(text(LETTER)) }
        screen = rule.onRoot().fetchSemanticsNode().boundsInRoot
    }

    @After
    fun stop() {
        if (::scenario.isInitialized) scenario.close()
    }

    /**
     * Runs frames on real time until [done]: the core works on its own threads, while the
     * clock stands still.
     */
    fun waitFor(timeoutMs: Long = 20_000, done: () -> Boolean) {
        val until = System.currentTimeMillis() + timeoutMs
        while (!done()) {
            check(System.currentTimeMillis() < until) { "Timed out" }
            rule.mainClock.advanceTimeByFrame()
            Thread.sleep(Film.FRAME_MS.toLong())
        }
    }

    /** The system back, as the back gesture ends. */
    fun back() {
        scenario.onActivity { it.onBackPressedDispatcher.onBackPressed() }
    }

    fun has(matcher: SemanticsMatcher) = rule.onAllNodes(matcher).fetchSemanticsNodes().isNotEmpty()

    fun bounds(matcher: SemanticsMatcher): Rect = rule.onAllNodes(matcher).fetchSemanticsNodes().first().boundsInRoot

    /** The subjects of the letters on screen, top to bottom. */
    fun letters(): List<String> = rule.onAllNodes(text(LETTER)).fetchSemanticsNodes()
        .sortedBy { it.boundsInRoot.top }
        .mapNotNull { node -> node.config.getOrNull(SemanticsProperties.Text)?.map { it.text }?.firstOrNull { LETTER in it } }
        .distinct()

    /** Films [name], saving it whatever happens, so a failure can be looked at. */
    fun film(name: String, every: Int = 2, scene: Film.() -> Unit) {
        val film = Film(rule, name, every)
        try {
            film.scene()
        } finally {
            film.save()
        }
    }

    /** A point on this row, [x] of the way across the screen. */
    fun Rect.at(x: Float) = Offset(screen.width * x, center.y)

    companion object {
        const val USER = "test@example.com"
        const val PASSWORD = "pass"
        val IMAP_PORT: UShort = 31993u
        val SMTP_PORT: UShort = 31587u
        const val SYNC_MS = 90_000L

        /** What every letter in People says; tools/ui-test-mail.sh writes them. */
        const val LETTER = ": plans for the weekend"

        fun text(value: String) = hasText(value, substring = true)
    }
}

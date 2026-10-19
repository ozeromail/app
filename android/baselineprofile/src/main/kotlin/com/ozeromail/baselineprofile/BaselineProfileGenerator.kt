package com.ozeromail.baselineprofile

import androidx.benchmark.macro.junit4.BaselineProfileRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Records what runs from a cold start to the first screen: on the CI emulator, with no account,
 * that is sign-in. Run `./gradlew :app:generateReleaseBaselineProfile`; the profile lands in
 * `app/src/release/generated/baselineProfiles`.
 */
@RunWith(AndroidJUnit4::class)
class BaselineProfileGenerator {
    @get:Rule
    val rule = BaselineProfileRule()

    @Test
    fun start() = rule.collect(packageName = "com.ozeromail.app", includeInStartupProfile = true) {
        pressHome()
        startActivityAndWait()
    }
}

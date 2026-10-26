package com.ozeromail.app

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

class OzeroApp : Application() {
    val mail by lazy { Mail(this) }

    /** Started activities; above zero the person sees the app, so new mail needs no notification. */
    @Volatile
    private var started = 0
    val inForeground: Boolean get() = started > 0
    private val shown = MutableStateFlow(false)
    /** Whether the person sees the app, as it changes. */
    val visible: StateFlow<Boolean> = shown

    override fun onCreate() {
        super.onCreate()
        Crashes.install(this)
        try {
            CoreNative.init(this)
        } catch (e: RuntimeException) {
            // The core then refuses secure connections with a TLS error instead of crashing here.
            Log.e("Ozero", "certificate checks are not set up", e)
        }
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            override fun onActivityStarted(activity: Activity) {
                started++
                shown.value = true
            }
            override fun onActivityStopped(activity: Activity) {
                started--
                shown.value = inForeground
            }
            override fun onActivityCreated(activity: Activity, state: Bundle?) = Unit
            override fun onActivityResumed(activity: Activity) = Unit
            override fun onActivityPaused(activity: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) = Unit
            override fun onActivityDestroyed(activity: Activity) = Unit
        })
    }
}

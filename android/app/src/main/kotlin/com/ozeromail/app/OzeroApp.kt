package com.ozeromail.app

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow

class OzeroApp : Application() {
    val mail by lazy { Mail(this) }

    /** Started activities; above zero the person sees the app, so new mail needs no notification. */
    private var started = 0
    private val shown = MutableStateFlow(false)
    val inForeground: Boolean get() = shown.value

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
                shown.value = ++started > 0
            }
            override fun onActivityStopped(activity: Activity) {
                shown.value = --started > 0
            }
            override fun onActivityCreated(activity: Activity, state: Bundle?) = Unit
            override fun onActivityResumed(activity: Activity) = Unit
            override fun onActivityPaused(activity: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) = Unit
            override fun onActivityDestroyed(activity: Activity) = Unit
        })
    }
}

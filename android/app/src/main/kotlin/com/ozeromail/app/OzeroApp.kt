package com.ozeromail.app

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.util.Log

class OzeroApp : Application() {
    val mail by lazy { Mail(this) }

    /** Started activities; above zero the person sees the app, so new mail needs no notification. */
    @Volatile
    private var started = 0
    val inForeground: Boolean get() = started > 0

    override fun onCreate() {
        super.onCreate()
        try {
            CoreNative.init(this)
        } catch (e: RuntimeException) {
            // The core then refuses secure connections with a TLS error instead of crashing here.
            Log.e("Ozero", "certificate checks are not set up", e)
        }
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            override fun onActivityStarted(activity: Activity) { started++ }
            override fun onActivityStopped(activity: Activity) { started-- }
            override fun onActivityCreated(activity: Activity, state: Bundle?) = Unit
            override fun onActivityResumed(activity: Activity) = Unit
            override fun onActivityPaused(activity: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) = Unit
            override fun onActivityDestroyed(activity: Activity) = Unit
        })
    }
}

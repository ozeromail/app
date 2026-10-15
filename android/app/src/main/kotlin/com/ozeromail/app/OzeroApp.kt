package com.ozeromail.app

import android.app.Application
import android.util.Log

class OzeroApp : Application() {
    val mail by lazy { Mail(this) }

    override fun onCreate() {
        super.onCreate()
        try {
            CoreNative.init(this)
        } catch (e: RuntimeException) {
            // The core then refuses secure connections with a TLS error instead of crashing here.
            Log.e("Ozero", "certificate checks are not set up", e)
        }
    }
}

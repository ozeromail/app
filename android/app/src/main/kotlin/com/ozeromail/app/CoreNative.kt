package com.ozeromail.app

import android.content.Context

/** The one core call that goes through plain JNI instead of the generated bindings. */
object CoreNative {
    init {
        System.loadLibrary("ozero_api")
    }

    /** Lets the core check server certificates against the Android trust store. */
    @JvmStatic
    external fun init(context: Context)
}

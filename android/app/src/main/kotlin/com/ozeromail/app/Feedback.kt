package com.ozeromail.app

import android.content.Context
import android.content.pm.PackageManager.PackageInfoFlags
import android.os.Build
import java.io.File
import java.io.PrintWriter
import java.io.StringWriter
import uniffi.ozero_api.DraftItem

/** Where letters to the author go. */
const val AUTHOR = "hello@ozeromail.com"

/** A letter to the author, with what the app runs on below, and a crash report if there is one. */
fun authorDraft(context: Context, crash: String? = null): DraftItem {
    val version = context.packageManager.getPackageInfo(context.packageName, PackageInfoFlags.of(0)).versionName
    val subject = if (crash == null) R.string.feedback_subject else R.string.crash_subject
    val device = "Ozero Mail $version, Android ${Build.VERSION.RELEASE}, ${Build.MANUFACTURER} ${Build.MODEL}"
    val text = listOfNotNull("\n\n--", device, crash).joinToString("\n")
    return NEW_DRAFT.copy(to = AUTHOR, subject = context.getString(subject), text = text)
}

/**
 * The last crash, kept on the phone until the person sends it or lets it go. Nothing leaves
 * the phone on its own: the report becomes a letter the person reads before sending.
 */
object Crashes {
    private const val LIMIT = 16 * 1024

    fun install(context: Context) {
        val file = file(context)
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            try {
                val trace = StringWriter().also { error.printStackTrace(PrintWriter(it)) }.toString()
                file.writeText(trace.take(LIMIT))
            } catch (_: Exception) {
                // The report is lost; the crash goes on as it would.
            }
            previous?.uncaughtException(thread, error)
        }
    }

    fun last(context: Context): String? = file(context).takeIf { it.exists() }?.readText()

    fun forget(context: Context) {
        file(context).delete()
    }

    private fun file(context: Context) = File(context.filesDir, "crash.txt")
}

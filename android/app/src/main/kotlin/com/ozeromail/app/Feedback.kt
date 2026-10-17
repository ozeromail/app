package com.ozeromail.app

import android.content.Context
import android.content.pm.PackageManager.PackageInfoFlags
import android.os.Build
import java.io.File
import uniffi.ozero_api.DraftItem

/** Where letters to the author go: the same address as on the site. */
private const val AUTHOR = "khizrim@khizrim.ru"

/** A letter to the author, with what the app runs on below, and a crash report if there is one. */
fun authorDraft(context: Context, crash: String? = null): DraftItem {
    val version = context.packageManager.getPackageInfo(context.packageName, PackageInfoFlags.of(0)).versionName ?: "?"
    val subject = if (crash == null) R.string.feedback_subject else R.string.crash_subject
    val device = "Ozero Mail $version, Android ${Build.VERSION.RELEASE}, ${Build.MANUFACTURER} ${Build.MODEL}"
    val text = listOfNotNull("\n\n-- ", device, crash).joinToString("\n")
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
                file.writeText(report(error).take(LIMIT))
            } catch (_: Throwable) {
                // The report is lost; the crash goes on as it would.
            } finally {
                previous?.uncaughtException(thread, error)
            }
        }
    }

    /** Must not run on the main thread. */
    fun last(context: Context): String? = file(context).takeIf { it.exists() }?.readText()

    /** Must not run on the main thread. */
    fun forget(context: Context) {
        file(context).delete()
    }

    /**
     * Where the code failed, without the errors' messages: they can hold a server's reply,
     * an address or a subject.
     */
    private fun report(error: Throwable): String =
        generateSequence(error) { it.cause }.take(8).joinToString("\nCaused by ") { e ->
            e.javaClass.name + e.stackTrace.joinToString("") { "\n  at $it" }
        }

    private fun file(context: Context) = File(context.filesDir, "crash.txt")
}

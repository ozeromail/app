package com.ozeromail.app

import android.content.Intent
import android.net.MailTo
import android.net.Uri

/** A message another app asks for: from a mailto link, or something shared, with its files. */
data class Shared(
    val to: String = "",
    val cc: String = "",
    val subject: String = "",
    val text: String = "",
    val files: List<Uri> = emptyList(),
)

/** What [intent] asks to write, or null when it asks for no message. */
fun sharedOf(intent: Intent): Shared? {
    val mailto = intent.data?.toString()?.takeIf(MailTo::isMailTo)?.let(MailTo::parse)
    val files = when (intent.action) {
        Intent.ACTION_VIEW, Intent.ACTION_SENDTO -> if (mailto == null) return null else emptyList()
        Intent.ACTION_SEND -> listOfNotNull(intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java))
        Intent.ACTION_SEND_MULTIPLE -> intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        else -> return null
    }
    fun addresses(fromLink: String?, extra: String) =
        (listOfNotNull(fromLink) + intent.getStringArrayExtra(extra).orEmpty()).filter { it.isNotBlank() }.joinToString(", ")
    return Shared(
        to = addresses(mailto?.to, Intent.EXTRA_EMAIL),
        cc = addresses(mailto?.cc, Intent.EXTRA_CC),
        subject = intent.getStringExtra(Intent.EXTRA_SUBJECT) ?: mailto?.subject.orEmpty(),
        text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString() ?: mailto?.body.orEmpty(),
        files = files,
    )
}

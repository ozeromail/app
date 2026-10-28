package com.ozeromail.app

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.ContactsContract
import android.provider.ContactsContract.CommonDataKinds.Email
import uniffi.ozero_api.ContactItem

/** The phone's contacts: read for "To" and names, opened and added to in the phone's own app. */
object PhoneContacts {
    fun allowed(context: Context) =
        context.checkSelfPermission(Manifest.permission.READ_CONTACTS) == PackageManager.PERMISSION_GRANTED

    /** Every contact with an address, by its lookup key; none without access. */
    fun read(context: Context): List<ContactItem> {
        if (!allowed(context)) return emptyList()
        val found = linkedMapOf<String, Pair<String, MutableList<String>>>()
        context.contentResolver.query(
            Email.CONTENT_URI,
            arrayOf(Email.LOOKUP_KEY, Email.DISPLAY_NAME_PRIMARY, Email.ADDRESS),
            null,
            null,
            null,
        )?.use { rows ->
            while (rows.moveToNext()) {
                val key = rows.getString(0) ?: continue
                val name = rows.getString(1).orEmpty()
                val email = rows.getString(2)?.trim().orEmpty()
                if (email.isEmpty()) continue
                val emails = found.getOrPut(key) { name to mutableListOf() }.second
                if (emails.none { it.equals(email, ignoreCase = true) }) emails += email
            }
        }
        return found.map { (key, contact) -> ContactItem(key, contact.first, contact.second) }
    }

    /** The phone's contacts app on the contact with [key]. */
    fun open(context: Context, key: String) {
        val uri = Uri.withAppendedPath(ContactsContract.Contacts.CONTENT_LOOKUP_URI, key)
        start(context, Intent(Intent.ACTION_VIEW, uri))
    }

    /** The phone's contacts app adding [email] with [name], to a new contact or one there already. */
    fun add(context: Context, name: String, email: String) {
        val intent = Intent(Intent.ACTION_INSERT_OR_EDIT).apply {
            type = ContactsContract.Contacts.CONTENT_ITEM_TYPE
            putExtra(ContactsContract.Intents.Insert.EMAIL, email)
            if (name != email) putExtra(ContactsContract.Intents.Insert.NAME, name)
        }
        start(context, intent)
    }

    private fun start(context: Context, intent: Intent) {
        try {
            context.startActivity(intent)
        } catch (_: ActivityNotFoundException) {
            // A phone without a contacts app: nothing to open.
        }
    }
}

package com.ozeromail.app

import android.app.Activity
import android.content.Context
import android.security.KeyChain
import android.util.Log
import java.security.Signature
import uniffi.ozero_api.ClientCertificates
import uniffi.ozero_api.Signing

/**
 * Client certificates in Android's KeyChain, such as one a company installs for its mail. The
 * key stays there: the core only asks it to sign. Called off the main thread, as KeyChain needs.
 */
class KeyChainCertificates(private val context: Context) : ClientCertificates {
    override fun chain(name: String): List<ByteArray> = try {
        KeyChain.getCertificateChain(context, name)?.map { it.encoded }.orEmpty()
    } catch (e: Exception) {
        Log.w("Ozero", "no certificate chain for $name", e)
        emptyList()
    }

    override fun sign(name: String, signing: Signing, data: ByteArray): ByteArray? = try {
        KeyChain.getPrivateKey(context, name)?.let { key ->
            Signature.getInstance(signing.algorithm).run {
                initSign(key)
                update(data)
                sign()
            }
        }
    } catch (e: Exception) {
        Log.w("Ozero", "certificate $name did not sign", e)
        null
    }
}

private val Signing.algorithm
    get() = when (this) {
        Signing.RSA_PKCS1_SHA256 -> "SHA256withRSA"
        Signing.RSA_PKCS1_SHA384 -> "SHA384withRSA"
        Signing.RSA_PKCS1_SHA512 -> "SHA512withRSA"
        Signing.RSA_PSS_SHA256 -> "SHA256withRSA/PSS"
        Signing.RSA_PSS_SHA384 -> "SHA384withRSA/PSS"
        Signing.RSA_PSS_SHA512 -> "SHA512withRSA/PSS"
        Signing.ECDSA_SHA256 -> "SHA256withECDSA"
        Signing.ECDSA_SHA384 -> "SHA384withECDSA"
        Signing.ECDSA_SHA512 -> "SHA512withECDSA"
    }

/** Lets the person pick a certificate in the system's own dialog; [onPicked] gets its name, or null. */
fun chooseCertificate(activity: Activity, current: String?, onPicked: (String?) -> Unit) {
    KeyChain.choosePrivateKeyAlias(activity, { name -> activity.runOnUiThread { onPicked(name) } }, null, null, null, current)
}

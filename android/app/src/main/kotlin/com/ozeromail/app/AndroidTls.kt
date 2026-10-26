package com.ozeromail.app

import android.content.Context
import android.security.KeyChain
import android.util.Log
import java.net.InetSocketAddress
import java.net.Socket
import java.security.Principal
import java.security.PrivateKey
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import javax.net.ssl.SNIHostName
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLEngine
import javax.net.ssl.SSLSocket
import javax.net.ssl.X509ExtendedKeyManager
import javax.net.ssl.X509TrustManager
import uniffi.ozero_api.PlatformSocket
import uniffi.ozero_api.PlatformTls
import uniffi.ozero_api.PlatformTlsException
import uniffi.ozero_api.ServerChecks

/** For the connection and the handshake. Later reads wait as long as the core does: it times
 * them itself, long polls included, and closes the socket when it gives up. */
private const val TIMEOUT_MS = 30_000

/**
 * Android's own TLS, for Exchange with a client certificate: some servers ask for the certificate
 * by renegotiating after the request, which the core's TLS cannot do. The core still checks the
 * server's certificate, and the key stays in KeyChain. Called off the main thread.
 */
class AndroidTls(private val context: Context) : PlatformTls {
    override fun connect(host: String, port: UShort, certificate: String, checks: ServerChecks): PlatformSocket = failing {
        val tls = SSLContext.getInstance("TLS").apply {
            init(arrayOf(KeyChainKey(context, certificate)), arrayOf(CoreChecks(checks)), null)
        }
        val tcp = Socket().apply {
            connect(InetSocketAddress(host, port.toInt()), TIMEOUT_MS)
            soTimeout = TIMEOUT_MS
        }
        val socket = try {
            (tls.socketFactory.createSocket(tcp, host, port.toInt(), true) as SSLSocket).apply {
                sslParameters = sslParameters.apply { serverNames = listOf(SNIHostName(host)) }
                startHandshake()
                soTimeout = 0
            }
        } catch (e: Exception) {
            // Closing the TCP socket closes a TLS one on top of it too.
            tcp.close()
            throw e
        }
        AndroidSocket(socket)
    }
}

private class AndroidSocket(private val socket: SSLSocket) : PlatformSocket {
    private val input = socket.inputStream
    private val output = socket.outputStream
    /** Only one thread reads. */
    private var buffer = ByteArray(0)

    override fun read(max: UInt): ByteArray = failing {
        if (buffer.size < max.toInt()) buffer = ByteArray(max.toInt())
        val n = input.read(buffer, 0, max.toInt())
        if (n < 0) ByteArray(0) else buffer.copyOf(n)
    }

    override fun write(data: ByteArray) = failing {
        output.write(data)
        output.flush()
    }

    override fun close() {
        runCatching { socket.close() }
    }
}

/** Offers only the certificate the person picked, whatever the server names. */
private class KeyChainKey(private val context: Context, private val name: String) : X509ExtendedKeyManager() {
    override fun chooseClientAlias(keyType: Array<out String>?, issuers: Array<out Principal>?, socket: Socket?) = name

    override fun chooseEngineClientAlias(keyType: Array<out String>?, issuers: Array<out Principal>?, engine: SSLEngine?) = name

    override fun getClientAliases(keyType: String?, issuers: Array<out Principal>?) = arrayOf(name)

    override fun getCertificateChain(alias: String?): Array<X509Certificate>? = keyChain { KeyChain.getCertificateChain(context, name) }

    override fun getPrivateKey(alias: String?): PrivateKey? = keyChain { KeyChain.getPrivateKey(context, name) }

    /** Null if KeyChain fails: the handshake then goes on without a certificate. */
    private fun <T> keyChain(get: () -> T?): T? = try {
        get()
    } catch (e: Exception) {
        Log.w("Ozero", "no certificate $name", e)
        null
    }

    override fun chooseServerAlias(keyType: String?, issuers: Array<out Principal>?, socket: Socket?): String? = null

    override fun getServerAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? = null
}

/** Takes a server only if the core's checks pass its chain, at each handshake. */
private class CoreChecks(private val checks: ServerChecks) : X509TrustManager {
    override fun checkServerTrusted(chain: Array<X509Certificate>, authType: String) {
        if (!checks.check(chain.map { it.encoded })) {
            throw CertificateException("the server certificate did not pass the checks")
        }
    }

    override fun checkClientTrusted(chain: Array<X509Certificate>, authType: String) =
        throw CertificateException("not a server")

    override fun getAcceptedIssuers() = emptyArray<X509Certificate>()
}

private inline fun <T> failing(block: () -> T): T = try {
    block()
} catch (e: PlatformTlsException) {
    throw e
} catch (e: Exception) {
    throw PlatformTlsException.Failed(e.message ?: e.javaClass.simpleName)
}

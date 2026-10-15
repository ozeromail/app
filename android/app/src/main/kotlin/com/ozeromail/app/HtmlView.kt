package com.ozeromail.app

import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.Intent
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/**
 * Shows a message's HTML with no scripts, no file access and no network.
 * The page from the core also carries its own policy; links open in the browser.
 */
@Composable
fun HtmlView(html: String, modifier: Modifier = Modifier) {
    AndroidView(
        modifier = modifier,
        factory = { context ->
            WebView(context).apply {
                configure()
                loadDataWithBaseURL(null, html, "text/html", "utf-8", null)
            }
        },
        onRelease = { it.destroy() },
    )
}

@SuppressLint("SetJavaScriptEnabled")
private fun WebView.configure() {
    settings.apply {
        javaScriptEnabled = false
        allowFileAccess = false
        allowContentAccess = false
        blockNetworkLoads = true
        // Wide newsletters start zoomed out to fit, and can be pinched.
        loadWithOverviewMode = true
        useWideViewPort = true
        builtInZoomControls = true
        displayZoomControls = false
    }
    webViewClient = object : WebViewClient() {
        // Only a tap opens a link; a page cannot redirect on its own.
        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            if (request.hasGesture() && request.url.scheme in LINK_SCHEMES) {
                try {
                    view.context.startActivity(Intent(Intent.ACTION_VIEW, request.url))
                } catch (_: ActivityNotFoundException) {
                    // Nothing handles this link; ignore the tap.
                }
            }
            return true
        }
    }
}

private val LINK_SCHEMES = setOf("http", "https", "mailto")

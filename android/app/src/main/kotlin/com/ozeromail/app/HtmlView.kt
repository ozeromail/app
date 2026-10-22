package com.ozeromail.app

import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.Intent
import android.graphics.Color
import android.view.ViewGroup
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import java.io.ByteArrayInputStream
import uniffi.ozero_api.RemoteImage

/**
 * Shows a message's HTML with no scripts, no file access and no network. Given no height,
 * it is as tall as its page.
 * The page from the core also carries its own policy; links open in the browser.
 * The page asks for its loaded [images] by address, so it stays small. [onShown] comes once
 * the page is drawn: until then the view is blank.
 */
@Composable
fun HtmlView(html: String, images: List<RemoteImage?>, modifier: Modifier = Modifier, onShown: () -> Unit = {}) {
    val shown by rememberUpdatedState(onShown)
    AndroidView(
        modifier = modifier,
        factory = { context ->
            WebView(context).apply {
                layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                // A plain page is clear and takes the screen's colour; a newsletter paints its own.
                setBackgroundColor(Color.TRANSPARENT)
                configure(images) { shown() }
                loadDataWithBaseURL(null, html, "text/html", "utf-8", null)
            }
        },
        onRelease = { view ->
            // Its client holds the loaded images; a web view kept by something else lets them go.
            view.webViewClient = WebViewClient()
            // A web view is destroyed only once out of the window.
            (view.parent as? ViewGroup)?.removeView(view)
            view.destroy()
            expectGone(view, "WebView of a closed message")
        },
    )
}

private fun WebView.configure(images: List<RemoteImage?>, onShown: () -> Unit) {
    settings.apply {
        javaScriptEnabled = false
        allowFileAccess = false
        allowContentAccess = false
        blockNetworkLoads = true
        // A page wider than the screen opens zoomed out to fit it, down to the page's minimum scale, and can be pinched.
        loadWithOverviewMode = true
        useWideViewPort = true
        builtInZoomControls = true
        displayZoomControls = false
    }
    webViewClient = object : WebViewClient() {
        override fun onPageCommitVisible(view: WebView, url: String?) = onShown()

        // Should the page never say it is drawn, it still shows once loaded.
        override fun onPageFinished(view: WebView, url: String?) = onShown()

        // The page's own loaded images, from images.ozero.invalid/<message>/<n>, and the reading font.
        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
            when (request.url.host) {
                // The page's origin is opaque, so the font is a cross-origin load and needs CORS.
                FONTS_HOST -> if (request.url.path == "/literata.ttf") {
                    return WebResourceResponse("font/ttf", null, 200, "OK", mapOf("Access-Control-Allow-Origin" to "*"), view.literata())
                }
                IMAGES_HOST -> request.url.lastPathSegment?.toIntOrNull()?.let(images::getOrNull)?.let {
                    return WebResourceResponse(it.mime, null, ByteArrayInputStream(it.data))
                }
                else -> return null
            }
            return WebResourceResponse("text/plain", null, 404, "Not Found", null, ByteArrayInputStream(ByteArray(0)))
        }

        // Only a tap opens a link; a page cannot redirect on its own.
        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            if (request.hasGesture() && request.url.scheme?.lowercase() in LINK_SCHEMES) {
                try {
                    // Only apps that take links from a browser may open it.
                    val intent = Intent(Intent.ACTION_VIEW, request.url).addCategory(Intent.CATEGORY_BROWSABLE)
                    view.context.startActivity(intent)
                } catch (_: ActivityNotFoundException) {
                    // Nothing handles this link; ignore the tap.
                }
            }
            return true
        }
    }
}

/** The reading font, a plain file in res/font. */
@SuppressLint("ResourceType")
private fun WebView.literata() = resources.openRawResource(R.font.literata)

private const val IMAGES_HOST = "images.ozero.invalid"
private const val FONTS_HOST = "fonts.ozero.invalid"
private val LINK_SCHEMES = setOf("http", "https", "mailto")

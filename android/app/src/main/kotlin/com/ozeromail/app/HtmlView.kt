package com.ozeromail.app

import android.content.ActivityNotFoundException
import android.content.Intent
import android.graphics.Color
import android.view.ViewGroup
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import java.io.ByteArrayInputStream
import uniffi.ozero_api.RemoteImage

/**
 * Shows a message's HTML with no scripts, no file access and no network. Given no height,
 * it is as tall as its page.
 * The page from the core also carries its own policy; links open in the browser.
 * The page asks for its loaded [images] by address, so it stays small.
 */
@Composable
fun HtmlView(html: String, images: List<RemoteImage?>, modifier: Modifier = Modifier) {
    AndroidView(
        modifier = modifier,
        factory = { context ->
            WebView(context).apply {
                layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                // A plain page is clear and takes the screen's colour; a newsletter paints its own.
                setBackgroundColor(Color.TRANSPARENT)
                configure(images)
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

private fun WebView.configure(images: List<RemoteImage?>) {
    settings.apply {
        javaScriptEnabled = false
        allowFileAccess = false
        allowContentAccess = false
        blockNetworkLoads = true
        // The page sets its own viewport; wider newsletters can be pinched.
        loadWithOverviewMode = true
        useWideViewPort = true
        builtInZoomControls = true
        displayZoomControls = false
    }
    webViewClient = object : WebViewClient() {
        // The page's own loaded images, from images.ozero.invalid/<message>/<n>, and the reading font.
        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
            if (request.url.host == FONTS_HOST && request.url.path == "/literata.ttf") {
                return WebResourceResponse("font/ttf", null, view.resources.openRawResource(R.font.literata))
            }
            if (request.url.host != IMAGES_HOST) return null
            val image = request.url.lastPathSegment?.toIntOrNull()?.let(images::getOrNull)
                ?: return WebResourceResponse("text/plain", null, 404, "Not Found", null, ByteArrayInputStream(ByteArray(0)))
            return WebResourceResponse(image.mime, null, ByteArrayInputStream(image.data))
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

private const val IMAGES_HOST = "images.ozero.invalid"
private const val FONTS_HOST = "fonts.ozero.invalid"
private val LINK_SCHEMES = setOf("http", "https", "mailto")

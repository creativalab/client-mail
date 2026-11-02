package com.ferrowl.app.ui.message

import android.content.Intent
import android.net.Uri
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ferrowl.app.R
import com.ferrowl.core.Message

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MessageScreen(folderId: String, uid: UInt, onBack: () -> Unit, vm: MessageViewModel = hiltViewModel()) {
    LaunchedEffect(folderId, uid) { vm.load(folderId, uid) }
    val state by vm.state.collectAsStateWithLifecycle()
    Scaffold(
        topBar = {
            TopAppBar(
                title = {},
                navigationIcon = { TextButton(onClick = onBack) { Text("‹ " + stringResource(R.string.folder_inbox)) } },
            )
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            when (val s = state) {
                MessageState.Loading -> CircularProgressIndicator(Modifier.align(Alignment.Center))
                is MessageState.Failed -> Text(s.error, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(16.dp))
                is MessageState.Shown -> Body(s.message)
            }
        }
    }
}

@Composable
private fun Body(m: Message) {
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(m.subject, style = MaterialTheme.typography.titleLarge)
            Text("${m.from} <${m.fromAddress}>", style = MaterialTheme.typography.bodyMedium)
            if (m.to.isNotBlank()) {
                Text(
                    stringResource(R.string.to) + ": " + m.to,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        val html = m.html
        if (html != null) {
            HtmlView(html)
        } else {
            SelectionContainer {
                Text(m.text, modifier = Modifier.verticalScroll(rememberScrollState()).padding(16.dp))
            }
        }
    }
}

/**
 * HTML mail in a locked-down WebView: no JavaScript, no file or content
 * access, links open in the browser. Remote images load, as on desktop by
 * default; blocking them will come with the privacy setting.
 *
 * Fitting the screen: the page gets the phone's width (see [fitToScreen]);
 * layouts wider than that (fixed-width newsletters) are zoomed out to fit
 * by the overview mode, and can be pinched to zoom in.
 */
@Composable
private fun HtmlView(html: String) {
    AndroidView(
        factory = { context ->
            WebView(context).apply {
                settings.javaScriptEnabled = false
                settings.allowFileAccess = false
                settings.allowContentAccess = false
                settings.domStorageEnabled = false
                settings.useWideViewPort = true
                settings.loadWithOverviewMode = true
                settings.setSupportZoom(true)
                settings.builtInZoomControls = true
                settings.displayZoomControls = false
                webViewClient = object : WebViewClient() {
                    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                        val url = request.url
                        if (url.scheme in setOf("http", "https", "mailto")) {
                            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url.toString())))
                        }
                        return true
                    }
                }
            }
        },
        update = { view ->
            // Load only when the message changes, not on every recomposition.
            if (view.tag != html) {
                view.tag = html
                view.loadDataWithBaseURL(null, fitToScreen(html), "text/html", "UTF-8", null)
            }
        },
        modifier = Modifier.fillMaxSize(),
    )
}

/**
 * Adds what makes mail readable on a phone. The mail's own viewport is
 * replaced: many newsletters ask for "device width, scale 1" and then use a
 * fixed 600 px table, which would run off the screen. Ours keeps the device
 * width (so responsive layouts still adapt) without fixing the scale, so the
 * overview mode can zoom wider layouts out to fit. Images and media never
 * exceed the screen; long words and links wrap.
 */
internal fun fitToScreen(html: String): String {
    val head = "<meta name=\"viewport\" content=\"width=device-width\">" +
        "<style>" +
        "body{margin:12px;overflow-wrap:anywhere}" +
        "img,video{max-width:100%!important;height:auto!important}" +
        "pre{white-space:pre-wrap}" +
        "</style>"
    val clean = html.replace(Regex("<meta[^>]*name\\s*=\\s*[\"']?viewport[^>]*>", RegexOption.IGNORE_CASE), "")
    val at = Regex("<head[^>]*>", RegexOption.IGNORE_CASE).find(clean)
    return if (at != null) {
        clean.substring(0, at.range.last + 1) + head + clean.substring(at.range.last + 1)
    } else {
        "<!doctype html><html><head>$head</head><body>$clean</body></html>"
    }
}

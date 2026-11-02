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
                settings.loadWithOverviewMode = true
                settings.useWideViewPort = true
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
        update = { it.loadDataWithBaseURL(null, html, "text/html", "UTF-8", null) },
        modifier = Modifier.fillMaxSize(),
    )
}

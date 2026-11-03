package com.ferrowl.app.ui.setup

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.browser.customtabs.CustomTabsIntent
import androidx.core.net.toUri
import com.ferrowl.core.BrowserOpener

/**
 * Opens the provider's sign-in page in a Custom Tab (the user's browser,
 * shown over the app). The engine calls [open] from a background thread;
 * the tab must be started on the main thread.
 *
 * After signing in, the page tells the user to close the tab: the engine
 * has already received the answer on its local port.
 */
class CustomTabOpener(private val context: Context) : BrowserOpener {
    override fun open(url: String) {
        Handler(Looper.getMainLooper()).post {
            CustomTabsIntent.Builder()
                .setShowTitle(true)
                .build()
                .launchUrl(context, url.toUri())
        }
    }
}

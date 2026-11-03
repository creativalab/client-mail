package com.ferrowl.app

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.ferrowl.app.data.MicrosoftSignIn
import com.ferrowl.app.ui.navigation.FerrowlNavHost
import com.ferrowl.app.ui.theme.FerrowlTheme
import dagger.hilt.android.AndroidEntryPoint
import javax.inject.Inject

/**
 * The only visible Activity: every screen is a Compose destination inside
 * [FerrowlNavHost] (single-Activity architecture). It also receives the
 * answer of a Microsoft sign-in, forwarded by [OAuthRedirectActivity].
 */
@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    @Inject lateinit var microsoft: MicrosoftSignIn

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (savedInstanceState == null) handle(intent)
        setContent {
            FerrowlTheme {
                FerrowlNavHost()
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    private fun handle(intent: Intent?) {
        val uri = intent?.data ?: return
        if (microsoft.isAnswer(uri)) {
            microsoft.complete(uri)
            setIntent(Intent(intent).setData(null))
        }
    }
}

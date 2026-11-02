package com.ferrowl.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.ferrowl.app.ui.navigation.FerrowlNavHost
import com.ferrowl.app.ui.theme.FerrowlTheme
import dagger.hilt.android.AndroidEntryPoint

/**
 * The only Activity: every screen is a Compose destination inside
 * [FerrowlNavHost] (single-Activity architecture).
 */
@AndroidEntryPoint
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            FerrowlTheme {
                FerrowlNavHost()
            }
        }
    }
}

package com.ferrowl.app.ui.navigation

import androidx.compose.runtime.Composable
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import com.ferrowl.app.ui.inbox.InboxScreen
import com.ferrowl.app.ui.message.MessageScreen
import com.ferrowl.app.ui.setup.SetupScreen
import com.ferrowl.app.ui.start.StartViewModel
import kotlinx.serialization.Serializable

// Type-safe destinations (Navigation Compose): each screen is a class, and
// its arguments are its fields.
@Serializable object Start
@Serializable object Setup
@Serializable object Inbox
@Serializable data class Read(val folderId: String, val uid: Long)

@Composable
fun FerrowlNavHost() {
    val nav = rememberNavController()
    NavHost(navController = nav, startDestination = Start) {
        // Decides where to go: account setup the first time, inbox after.
        composable<Start> {
            val vm: StartViewModel = hiltViewModel()
            com.ferrowl.app.ui.start.StartScreen(vm) { hasAccount ->
                nav.navigate(if (hasAccount) Inbox else Setup) { popUpTo(Start) { inclusive = true } }
            }
        }
        composable<Setup> {
            SetupScreen(onDone = { nav.navigate(Inbox) { popUpTo(Setup) { inclusive = true } } })
        }
        composable<Inbox> {
            InboxScreen(onOpen = { folderId, uid -> nav.navigate(Read(folderId, uid.toLong())) })
        }
        composable<Read> { entry ->
            val route = entry.toRoute<Read>()
            MessageScreen(folderId = route.folderId, uid = route.uid.toUInt(), onBack = { nav.popBackStack() })
        }
    }
}

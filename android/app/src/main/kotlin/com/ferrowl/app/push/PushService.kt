package com.ferrowl.app.push

import com.ferrowl.app.data.MailRepository
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dagger.hilt.android.AndroidEntryPoint
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject

/**
 * Receives the server's "new mail" signal (a data-only Firebase message, so
 * the app decides what to show). It reads the inbox and notifies the new
 * unread messages; if that fails in time, a plain "New mail" is shown.
 */
@AndroidEntryPoint
class PushService : FirebaseMessagingService() {
    @Inject lateinit var repo: MailRepository

    @Inject lateinit var store: PushStore

    @Inject lateinit var registrar: PushRegistrar

    @Inject lateinit var notifier: MailNotifier

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onNewToken(token: String) {
        scope.launch { registrar.tokenChanged(token) }
    }

    // Called on a background thread; Android gives it about 20 seconds.
    override fun onMessageReceived(message: RemoteMessage) {
        if (message.data["type"] != "new_mail") return
        runBlocking {
            val shown = withTimeoutOrNull(TIMEOUT_MS) { notifyNewMessages() } ?: false
            if (!shown) notifier.showGeneric()
        }
    }

    /** True if it decided what to show (possibly nothing: already read). */
    private suspend fun notifyNewMessages(): Boolean = runCatching {
        repo.ensureConnected()
        val headers = repo.headers(repo.inboxId())
        val last = store.current().lastNotifiedUid
        val fresh = headers.filter { !it.seen && it.uid.toLong() > last }.sortedByDescending { it.uid }
        if (fresh.isNotEmpty()) {
            notifier.show(fresh)
            store.setLastNotified(fresh.first().uid.toLong())
        }
        true
    }.getOrDefault(false)

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private companion object {
        const val TIMEOUT_MS = 15_000L
    }
}

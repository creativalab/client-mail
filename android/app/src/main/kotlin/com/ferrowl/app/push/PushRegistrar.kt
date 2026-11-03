package com.ferrowl.app.push

import android.util.Base64
import com.ferrowl.app.data.AccountStore
import com.ferrowl.core.Protocol
import com.ferrowl.core.pushAccountJson
import com.ferrowl.core.translate
import com.google.firebase.messaging.FirebaseMessaging
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.tasks.await
import java.security.SecureRandom
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Keeps this phone registered on the push server while instant
 * notifications are on:
 *
 * 1. the phone registers with its Firebase token and gets a secret device
 *    token;
 * 2. it creates a random 256-bit key, kept only here (encrypted with the
 *    Android Keystore), and sends the account with it: the server stores
 *    the account sealed with that key and starts watching the inbox;
 * 3. at every app start it sends the key again (`unlock`), because the
 *    server keeps it in memory only and forgets it when it restarts.
 */
@Singleton
class PushRegistrar @Inject constructor(
    private val store: PushStore,
    private val client: PushClient,
    private val accounts: AccountStore,
) {
    private val lock = Mutex()

    /** Turns notifications on. Throws with a message to show if it fails. */
    suspend fun enable() {
        val account = accounts.account.first() ?: return
        if (account.protocol == Protocol.POP3) throw PushException(translate("push_pop3"), 0)
        store.setEnabled(true)
        try {
            sync()
        } catch (e: Exception) {
            store.setEnabled(false)
            throw e
        }
    }

    /** Turns notifications off and removes everything from the server. */
    suspend fun disable() = lock.withLock {
        val s = store.current()
        store.setEnabled(false)
        s.deviceToken?.let { runCatching { client.forget(it) } }
        store.clearRegistration()
    }

    /** Brings the registration up to date (app start, new account). */
    suspend fun sync(): Unit = lock.withLock {
        if (!store.current().enabled) return
        val account = accounts.account.first() ?: return
        if (account.protocol == Protocol.POP3) return
        val fcmToken = FirebaseMessaging.getInstance().token.await()
        var s = store.current()
        if (s.deviceToken != null) {
            try {
                client.updateToken(s.deviceToken!!, fcmToken)
            } catch (e: PushException) {
                // The server no longer knows this phone: register again.
                if (e.status != 401) throw e
                store.clearRegistration()
                s = store.current()
            }
        }
        val token = s.deviceToken ?: client.register(fcmToken).device_token.also { newToken ->
            store.saveDevice(newToken, newKey())
        }
        val key = store.current().key ?: return
        val accountId = store.current().accountId
        if (accountId == null) {
            store.saveAccount(client.addAccount(token, key, pushAccountJson(account)))
        } else {
            client.unlock(token, key)
        }
    }

    /** The account was replaced (new setup): watch the new one instead. */
    suspend fun accountChanged() {
        lock.withLock {
            val s = store.current()
            if (s.deviceToken != null && s.accountId != null) {
                runCatching { client.deleteAccount(s.deviceToken, s.accountId) }
            }
            store.clearAccount()
        }
        sync()
    }

    /** Firebase gave the phone a new token. */
    suspend fun tokenChanged(fcmToken: String) {
        val s = store.current()
        if (s.enabled && s.deviceToken != null) runCatching { client.updateToken(s.deviceToken, fcmToken) }
    }

    private fun newKey(): String {
        val bytes = ByteArray(32).also { SecureRandom().nextBytes(it) }
        return Base64.encodeToString(bytes, Base64.NO_WRAP)
    }
}

package com.ferrowl.app.push

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import javax.inject.Inject
import javax.inject.Singleton

/** A tapped new-mail notification: the account and the message to open (null uid = its inbox). */
data class OpenRequest(val email: String, val uid: Long?)

/**
 * Hands a tapped notification from [com.ferrowl.app.MainActivity] to the
 * mailbox, which switches to the account if needed and opens the message.
 */
@Singleton
class NotificationOpen @Inject constructor() {
    private val _pending = MutableStateFlow<OpenRequest?>(null)
    val pending: StateFlow<OpenRequest?> = _pending.asStateFlow()

    fun request(email: String, uid: Long?) {
        _pending.value = OpenRequest(email, uid)
    }

    fun done() {
        _pending.value = null
    }

    companion object {
        const val EXTRA_EMAIL = "com.ferrowl.app.open.email"
        const val EXTRA_UID = "com.ferrowl.app.open.uid"
    }
}

package com.ferrowl.app.data

import android.content.Context
import androidx.core.content.edit
import com.ferrowl.core.TokenVault
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Where the engine keeps sign-in tokens (Microsoft, Google): app-private
 * preferences, each value encrypted with the Keystore key of [SecretBox].
 * Excluded from backups like the rest of the app's data.
 *
 * The engine calls this from its own background threads, so the methods
 * are synchronous (`commit`, not `apply`: a token must be on disk before
 * the sign-in is reported as done).
 */
@Singleton
class KeystoreTokenVault @Inject constructor(
    @ApplicationContext context: Context,
    private val secrets: SecretBox,
) : TokenVault {
    private val prefs = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    override fun get(key: String): String? =
        prefs.getString(key, null)?.let { sealed -> runCatching { secrets.decrypt(sealed) }.getOrNull() }

    override fun set(key: String, secret: String): Boolean =
        runCatching {
            val sealed = secrets.encrypt(secret)
            prefs.edit(commit = true) { putString(key, sealed) }
        }.isSuccess

    override fun delete(key: String) {
        prefs.edit(commit = true) { remove(key) }
    }

    private companion object {
        const val FILE = "sign-in"
    }
}

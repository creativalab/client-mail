package com.ferrowl.app.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.ferrowl.core.Account
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import javax.inject.Inject
import javax.inject.Singleton

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "accounts")

/**
 * Saves the account settings on the phone (Jetpack DataStore). The password
 * and the OpenPGP passphrase are encrypted with [SecretBox] before saving.
 * One account for now; the format already allows a list.
 */
@Singleton
class AccountStore @Inject constructor(
    @ApplicationContext private val context: Context,
    private val secrets: SecretBox,
) {
    /** The saved account, or null before the first setup. */
    val account: Flow<Account?> = context.dataStore.data.map { prefs ->
        prefs[KEY]?.let { json -> runCatching { Json.decodeFromString<Stored>(json).toAccount(secrets) }.getOrNull() }
    }

    suspend fun save(account: Account) {
        context.dataStore.edit { it[KEY] = Json.encodeToString(Stored.from(account, secrets)) }
    }

    suspend fun clear() {
        context.dataStore.edit { it.remove(KEY) }
    }

    /** What is written to disk: the engine's Account with secrets encrypted. */
    @Serializable
    private data class Stored(
        val displayName: String,
        val email: String,
        val username: String,
        val passwordSealed: String,
        val incomingHost: String,
        val incomingPort: Int,
        val smtpHost: String,
        val smtpPort: Int,
        val pop3: Boolean,
        val pop3KeepDays: Int? = null,
        val pgpPassphraseSealed: String = "",
    ) {
        fun toAccount(secrets: SecretBox) = Account(
            displayName = displayName,
            email = email,
            username = username,
            password = secrets.decrypt(passwordSealed),
            incomingHost = incomingHost,
            incomingPort = incomingPort.toUShort(),
            smtpHost = smtpHost,
            smtpPort = smtpPort.toUShort(),
            protocol = if (pop3) com.ferrowl.core.Protocol.POP3 else com.ferrowl.core.Protocol.IMAP,
            pop3KeepDays = pop3KeepDays?.toUInt(),
            pgpPassphrase = if (pgpPassphraseSealed.isEmpty()) "" else secrets.decrypt(pgpPassphraseSealed),
        )

        companion object {
            fun from(a: Account, secrets: SecretBox) = Stored(
                displayName = a.displayName,
                email = a.email,
                username = a.username,
                passwordSealed = secrets.encrypt(a.password),
                incomingHost = a.incomingHost,
                incomingPort = a.incomingPort.toInt(),
                smtpHost = a.smtpHost,
                smtpPort = a.smtpPort.toInt(),
                pop3 = a.protocol == com.ferrowl.core.Protocol.POP3,
                pop3KeepDays = a.pop3KeepDays?.toInt(),
                pgpPassphraseSealed = if (a.pgpPassphrase.isEmpty()) "" else secrets.encrypt(a.pgpPassphrase),
            )
        }
    }

    private companion object {
        val KEY = stringPreferencesKey("account")
    }
}

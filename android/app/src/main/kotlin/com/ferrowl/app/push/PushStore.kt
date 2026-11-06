package com.ferrowl.app.push

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.ferrowl.app.data.SecretBox
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.serialization.json.Json
import javax.inject.Inject
import javax.inject.Singleton

private val Context.push: DataStore<Preferences> by preferencesDataStore(name = "push")

/** What the phone knows about its registration on the push server. */
data class PushState(
    val enabled: Boolean = false,
    /** Secret token identifying this phone to the server. */
    val deviceToken: String? = null,
    /** The phone's key (base64), which seals the account on the server. */
    val key: String? = null,
    /** Accounts watched by the server: address (lowercase) -> account id there. */
    val accounts: Map<String, String> = emptyMap(),
    /** Per address: highest inbox UID already notified (no repeated notifications). */
    val lastNotified: Map<String, Long> = emptyMap(),
) {
    /** The address of an account id the server sent. */
    fun emailOf(accountId: String): String? = accounts.entries.firstOrNull { it.value == accountId }?.key
}

/**
 * Saves the push registration (Jetpack DataStore). The device token and the
 * key are secrets: they are encrypted with [SecretBox] (Android Keystore)
 * before saving.
 */
@Singleton
class PushStore @Inject constructor(
    @ApplicationContext private val context: Context,
    private val secrets: SecretBox,
) {
    val state: Flow<PushState> = context.push.data.map { p ->
        PushState(
            enabled = p[ENABLED] ?: false,
            deviceToken = p[TOKEN]?.let(::open),
            key = p[KEY]?.let(::open),
            accounts = p[ACCOUNTS]?.let { runCatching { Json.decodeFromString<Map<String, String>>(it) }.getOrNull() }.orEmpty(),
            lastNotified = p[LAST_UIDS]?.let { runCatching { Json.decodeFromString<Map<String, Long>>(it) }.getOrNull() }.orEmpty(),
        )
    }

    suspend fun current(): PushState = state.first()

    suspend fun setEnabled(on: Boolean) = context.push.edit { it[ENABLED] = on }

    suspend fun saveDevice(deviceToken: String, key: String) = context.push.edit {
        it[TOKEN] = secrets.encrypt(deviceToken)
        it[KEY] = secrets.encrypt(key)
    }

    /** The server watches this account under `id`. */
    suspend fun saveAccount(email: String, id: String) = editAccounts { it + (email.lowercase() to id) }

    suspend fun removeAccount(email: String) = editAccounts { it - email.lowercase() }

    /** An older version registered one account, without its address: forgotten (registered again). */
    suspend fun legacyAccount(): String? = context.push.data.first()[ACCOUNT]

    suspend fun clearLegacyAccount() = context.push.edit { it.remove(ACCOUNT) }

    suspend fun setLastNotified(email: String, uid: Long) = context.push.edit { p ->
        val now = p[LAST_UIDS]?.let { runCatching { Json.decodeFromString<Map<String, Long>>(it) }.getOrNull() }.orEmpty()
        p[LAST_UIDS] = Json.encodeToString(now + (email.lowercase() to uid))
    }

    private suspend fun editAccounts(change: (Map<String, String>) -> Map<String, String>) = context.push.edit { p ->
        val now = p[ACCOUNTS]?.let { runCatching { Json.decodeFromString<Map<String, String>>(it) }.getOrNull() }.orEmpty()
        p[ACCOUNTS] = Json.encodeToString(change(now))
    }

    /** Forgets the registration (the switch stays as it is). */
    suspend fun clearRegistration() = context.push.edit {
        it.remove(TOKEN)
        it.remove(KEY)
        it.remove(ACCOUNT)
        it.remove(ACCOUNTS)
    }

    /** An unreadable secret (e.g. after a backup restore) counts as missing. */
    private fun open(sealed: String): String? = runCatching { secrets.decrypt(sealed) }.getOrNull()

    private companion object {
        val ENABLED = booleanPreferencesKey("enabled")
        val TOKEN = stringPreferencesKey("device_token")
        val KEY = stringPreferencesKey("key")
        /** Older versions: the only account watched. */
        val ACCOUNT = stringPreferencesKey("account_id")
        val ACCOUNTS = stringPreferencesKey("accounts")
        val LAST_UIDS = stringPreferencesKey("last_notified_uids")
    }
}

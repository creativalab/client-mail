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
    val accountId: String? = null,
    /** Highest inbox UID already notified (no repeated notifications). */
    val lastNotifiedUid: Long = 0,
)

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
            accountId = p[ACCOUNT],
            lastNotifiedUid = p[LAST_UID]?.toLongOrNull() ?: 0,
        )
    }

    suspend fun current(): PushState = state.first()

    suspend fun setEnabled(on: Boolean) = context.push.edit { it[ENABLED] = on }

    suspend fun saveDevice(deviceToken: String, key: String) = context.push.edit {
        it[TOKEN] = secrets.encrypt(deviceToken)
        it[KEY] = secrets.encrypt(key)
    }

    suspend fun saveAccount(id: String) = context.push.edit { it[ACCOUNT] = id }

    suspend fun setLastNotified(uid: Long) = context.push.edit { it[LAST_UID] = uid.toString() }

    suspend fun clearAccount() = context.push.edit { it.remove(ACCOUNT) }

    /** Forgets the registration (the switch stays as it is). */
    suspend fun clearRegistration() = context.push.edit {
        it.remove(TOKEN)
        it.remove(KEY)
        it.remove(ACCOUNT)
    }

    /** An unreadable secret (e.g. after a backup restore) counts as missing. */
    private fun open(sealed: String): String? = runCatching { secrets.decrypt(sealed) }.getOrNull()

    private companion object {
        val ENABLED = booleanPreferencesKey("enabled")
        val TOKEN = stringPreferencesKey("device_token")
        val KEY = stringPreferencesKey("key")
        val ACCOUNT = stringPreferencesKey("account_id")
        val LAST_UID = stringPreferencesKey("last_notified_uid")
    }
}

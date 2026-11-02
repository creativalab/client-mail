package com.ferrowl.app.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

private val Context.settings: DataStore<Preferences> by preferencesDataStore(name = "settings")

/** App preferences that are not about an account. */
@Singleton
class SettingsStore @Inject constructor(@ApplicationContext private val context: Context) {
    /** Lists grouped into conversations (true, the default) or one row per message. */
    val threaded: Flow<Boolean> = context.settings.data.map { it[THREADED] ?: true }

    suspend fun setThreaded(on: Boolean) {
        context.settings.edit { it[THREADED] = on }
    }

    private companion object {
        val THREADED = booleanPreferencesKey("threaded")
    }
}

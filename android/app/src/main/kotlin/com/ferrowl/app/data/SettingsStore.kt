package com.ferrowl.app.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.ferrowl.core.SortOrder
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

    /** Order of message lists (newest first by default). */
    val sort: Flow<SortOrder> = context.settings.data.map { prefs ->
        prefs[SORT]?.let { name -> SortOrder.entries.firstOrNull { it.name == name } } ?: SortOrder.NEWEST
    }

    suspend fun setSort(order: SortOrder) {
        context.settings.edit { it[SORT] = order.name }
    }

    private companion object {
        val THREADED = booleanPreferencesKey("threaded")
        val SORT = stringPreferencesKey("sort")
    }
}

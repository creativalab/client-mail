package com.ferrowl.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.SettingsStore
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class SettingsViewModel @Inject constructor(private val settings: SettingsStore) : ViewModel() {
    /** Conversations (true) or one row per message. */
    val threaded: StateFlow<Boolean> =
        settings.threaded.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), true)

    fun setThreaded(on: Boolean) {
        viewModelScope.launch { settings.setThreaded(on) }
    }
}

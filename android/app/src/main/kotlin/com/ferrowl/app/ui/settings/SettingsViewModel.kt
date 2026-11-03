package com.ferrowl.app.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.AccountStore
import com.ferrowl.app.data.SettingsStore
import com.ferrowl.app.push.PushRegistrar
import com.ferrowl.app.push.PushStore
import com.ferrowl.core.Protocol
import com.ferrowl.core.translate
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val settings: SettingsStore,
    private val push: PushRegistrar,
    pushStore: PushStore,
    accounts: AccountStore,
) : ViewModel() {
    /** Conversations (true) or one row per message. */
    val threaded: StateFlow<Boolean> =
        settings.threaded.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), true)

    /** Whether instant notifications are on (as saved). */
    val pushEnabled: StateFlow<Boolean> =
        pushStore.state.map { it.enabled }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    /** POP3 accounts have no IDLE: the server cannot watch them. */
    val pushAvailable: StateFlow<Boolean> =
        accounts.account.map { it?.protocol != Protocol.POP3 }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), true)

    private val _pushBusy = MutableStateFlow(false)
    val pushBusy: StateFlow<Boolean> = _pushBusy.asStateFlow()

    private val _pushError = MutableStateFlow<String?>(null)
    val pushError: StateFlow<String?> = _pushError.asStateFlow()

    fun setThreaded(on: Boolean) {
        viewModelScope.launch { settings.setThreaded(on) }
    }

    /** Turns instant notifications on (registers with the server) or off. */
    fun setPush(on: Boolean) {
        if (_pushBusy.value) return
        _pushBusy.value = true
        _pushError.value = null
        viewModelScope.launch {
            try {
                if (on) push.enable() else push.disable()
            } catch (e: Exception) {
                _pushError.value = translate("push_error").replace("{err}", e.message.orEmpty())
            } finally {
                _pushBusy.value = false
            }
        }
    }
}

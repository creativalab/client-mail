package com.ferrowl.app.ui.inbox

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.Header
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class InboxState(
    val folderId: String = "",
    val messages: List<Header> = emptyList(),
    val refreshing: Boolean = false,
    val error: String? = null,
)

@HiltViewModel
class InboxViewModel @Inject constructor(private val repo: MailRepository) : ViewModel() {
    private val _state = MutableStateFlow(InboxState())
    val state: StateFlow<InboxState> = _state.asStateFlow()

    init {
        refresh()
    }

    /**
     * Shows what is stored on the phone at once, then asks the server for
     * the newest messages.
     */
    fun refresh() {
        _state.update { it.copy(refreshing = true, error = null) }
        viewModelScope.launch {
            try {
                repo.ensureConnected()
                val folder = repo.inboxId()
                val cached = repo.cachedHeaders(folder)
                _state.update { it.copy(folderId = folder, messages = cached.ifEmpty { it.messages }) }
                val fresh = repo.headers(folder)
                _state.update { it.copy(messages = fresh, refreshing = false) }
            } catch (e: FerrowlException) {
                _state.update { it.copy(refreshing = false, error = e.message) }
            }
        }
    }

    /** Marks a message read in the list as soon as it is opened. */
    fun opened(uid: UInt) = _state.update { s ->
        s.copy(messages = s.messages.map { if (it.uid == uid) it.copy(seen = true) else it })
    }
}

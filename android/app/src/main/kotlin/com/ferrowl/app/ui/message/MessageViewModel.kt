package com.ferrowl.app.ui.message

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.Message
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

sealed interface MessageState {
    data object Loading : MessageState
    data class Shown(val message: Message) : MessageState
    data class Failed(val error: String) : MessageState
}

@HiltViewModel
class MessageViewModel @Inject constructor(private val repo: MailRepository) : ViewModel() {
    private val _state = MutableStateFlow<MessageState>(MessageState.Loading)
    val state: StateFlow<MessageState> = _state.asStateFlow()
    private var loaded: Pair<String, UInt>? = null

    fun load(folderId: String, uid: UInt) {
        if (loaded == folderId to uid) return
        loaded = folderId to uid
        viewModelScope.launch {
            _state.value = try {
                MessageState.Shown(repo.message(folderId, uid))
            } catch (e: FerrowlException) {
                MessageState.Failed(e.message ?: "")
            }
        }
    }
}

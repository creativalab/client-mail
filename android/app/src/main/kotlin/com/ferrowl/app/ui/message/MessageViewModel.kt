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
    /** Archived or deleted: the screen closes. */
    data object Gone : MessageState
}

@HiltViewModel
class MessageViewModel @Inject constructor(private val repo: MailRepository) : ViewModel() {
    private val _state = MutableStateFlow<MessageState>(MessageState.Loading)
    val state: StateFlow<MessageState> = _state.asStateFlow()
    private var folderId = ""
    private var uid = 0u

    fun load(folderId: String, uid: UInt) {
        if (this.folderId == folderId && this.uid == uid) return
        this.folderId = folderId
        this.uid = uid
        viewModelScope.launch {
            _state.value = try {
                MessageState.Shown(repo.message(folderId, uid))
            } catch (e: FerrowlException) {
                MessageState.Failed(e.message ?: "")
            }
        }
    }

    /** True when Delete removes for good (the message is in the Trash). */
    val deletesForever: Boolean get() = repo.deletesForever(folderId)

    fun archive() = act { repo.archive(folderId, uid) }

    fun delete() = act { repo.delete(folderId, uid) }

    fun markUnread() = act { repo.setSeen(folderId, uid, false) }

    private fun act(action: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                action()
                _state.value = MessageState.Gone
            } catch (e: FerrowlException) {
                _state.value = MessageState.Failed(e.message ?: "")
            }
        }
    }
}

package com.ferrowl.app.ui.conversation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import com.ferrowl.core.Header
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

/** The messages of one conversation, oldest first, from the local copy. */
@HiltViewModel
class ConversationViewModel @Inject constructor(private val repo: MailRepository) : ViewModel() {
    private val _messages = MutableStateFlow<List<Header>>(emptyList())
    val messages: StateFlow<List<Header>> = _messages.asStateFlow()
    private var folderId = ""
    private var uids: List<UInt> = emptyList()

    fun load(folderId: String, uids: List<UInt>) {
        this.folderId = folderId
        this.uids = uids
        reload()
    }

    init {
        // Opened, moved or deleted members update the list.
        viewModelScope.launch { repo.changes.collect { reload() } }
    }

    private fun reload() {
        if (folderId.isEmpty()) return
        viewModelScope.launch {
            val byUid = repo.cachedHeaders(folderId).associateBy { it.uid }
            _messages.value = uids.mapNotNull(byUid::get)
        }
    }
}

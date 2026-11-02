package com.ferrowl.app.ui.mailbox

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import com.ferrowl.app.data.SettingsStore
import com.ferrowl.core.Conversation
import com.ferrowl.core.groupConversations
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.Folder
import com.ferrowl.core.Header
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class MailboxState(
    val folders: List<Folder> = emptyList(),
    val unread: Map<String, UInt> = emptyMap(),
    val folderId: String = "",
    val messages: List<Header> = emptyList(),
    val refreshing: Boolean = false,
    val error: String? = null,
    /** Search text; when not blank the list shows [results]. */
    val query: String = "",
    val searching: Boolean = false,
    val results: List<Header> = emptyList(),
    /** Conversations (true) or one row per message. */
    val threaded: Boolean = true,
    /** Checked messages (uids); not empty = selection mode. */
    val selected: Set<UInt> = emptySet(),
    /** Asking before deleting the selection for good (in the Trash). */
    val confirmDelete: Boolean = false,
) {
    val selecting: Boolean get() = selected.isNotEmpty()
    val folder: Folder? get() = folders.firstOrNull { it.id == folderId }
    val shown: List<Header> get() = if (query.isBlank()) messages else results

    /** The list rows: whole conversations, or single messages as one-member ones. */
    val rows: List<Conversation>
        get() = if (threaded) {
            groupConversations(shown)
        } else {
            shown.map { Conversation(it, listOf(it.uid), if (it.seen) 0u else 1u) }
        }
}

@HiltViewModel
class MailboxViewModel @Inject constructor(
    private val repo: MailRepository,
    private val settings: SettingsStore,
) : ViewModel() {
    private val _state = MutableStateFlow(MailboxState())
    val state: StateFlow<MailboxState> = _state.asStateFlow()

    /** Confirmations from actions done anywhere ("Archived", "Sent"). */
    val notices: SharedFlow<String> = repo.notices

    private var searchJob: Job? = null

    init {
        viewModelScope.launch { settings.threaded.collect { on -> _state.update { it.copy(threaded = on) } } }
        viewModelScope.launch {
            try {
                repo.ensureConnected()
                _state.update { it.copy(folders = repo.folders.value, folderId = repo.inboxId()) }
                refresh()
            } catch (e: FerrowlException) {
                _state.update { it.copy(error = e.message) }
            }
        }
        // Something was moved, deleted or sent: reload from the local copy.
        viewModelScope.launch {
            repo.changes.collect {
                val id = _state.value.folderId
                if (id.isNotEmpty()) _state.update { it.copy(messages = repo.cachedHeaders(id)) }
                loadUnread()
            }
        }
    }

    fun openFolder(id: String) {
        if (id == _state.value.folderId) return
        _state.update { it.copy(folderId = id, messages = emptyList(), query = "", results = emptyList(), selected = emptySet()) }
        refresh()
    }

    /** The local copy at once, then the newest messages from the server. */
    fun refresh() {
        val id = _state.value.folderId.ifEmpty { return }
        _state.update { it.copy(refreshing = true, error = null) }
        viewModelScope.launch {
            try {
                val cached = repo.cachedHeaders(id)
                if (cached.isNotEmpty()) _state.update { if (it.folderId == id) it.copy(messages = cached) else it }
                val fresh = repo.headers(id)
                _state.update { if (it.folderId == id) it.copy(messages = fresh, refreshing = false) else it }
                loadUnread()
            } catch (e: FerrowlException) {
                _state.update { it.copy(refreshing = false, error = e.message) }
            }
        }
    }

    private suspend fun loadUnread() {
        runCatching { repo.unread() }.onSuccess { counts -> _state.update { it.copy(unread = counts) } }
    }

    fun setQuery(text: String) {
        _state.update { it.copy(query = text) }
        searchJob?.cancel()
        if (text.isBlank()) return
        searchJob = viewModelScope.launch {
            _state.update { it.copy(searching = true) }
            val found = repo.search(_state.value.folderId, text)
            _state.update { it.copy(results = found, searching = false) }
        }
    }

    /** Checks or unchecks a row (all messages of a conversation). */
    fun toggle(uids: List<UInt>) = _state.update { s ->
        val all = s.selected.containsAll(uids)
        s.copy(selected = if (all) s.selected - uids.toSet() else s.selected + uids)
    }

    enum class Pick { ALL, NONE, READ, UNREAD }

    fun select(pick: Pick) = _state.update { s ->
        s.copy(
            selected = s.shown.filter {
                when (pick) {
                    Pick.ALL -> true
                    Pick.NONE -> false
                    Pick.READ -> it.seen
                    Pick.UNREAD -> !it.seen
                }
            }.map { it.uid }.toSet(),
        )
    }

    fun clearSelection() = _state.update { it.copy(selected = emptySet(), confirmDelete = false) }

    fun markSelected(seen: Boolean) = bulk { folder, uids ->
        _state.update { s ->
            s.copy(messages = s.messages.map { if (it.uid in uids) it.copy(seen = seen) else it })
        }
        repo.setSeen(folder, uids, seen)
    }

    fun archiveSelected() = bulk { folder, uids -> repo.archive(folder, uids) }

    fun moveSelected(targetId: String) = bulk { folder, uids -> repo.move(folder, uids, targetId) }

    /** Trash, or (in the Trash) ask first and then delete for good. */
    fun deleteSelected(confirmed: Boolean = false) {
        val s = _state.value
        if (!confirmed && repo.deletesForever(s.folderId)) {
            _state.update { it.copy(confirmDelete = true) }
            return
        }
        bulk { folder, uids -> repo.delete(folder, uids) }
    }

    private fun bulk(action: suspend (folder: String, uids: List<UInt>) -> Unit) {
        val s = _state.value
        val uids = s.selected.toList()
        if (uids.isEmpty()) return
        _state.update { it.copy(selected = emptySet(), confirmDelete = false) }
        viewModelScope.launch {
            try {
                action(s.folderId, uids)
            } catch (e: FerrowlException) {
                _state.update { it.copy(error = e.message) }
            }
        }
    }

    fun setThreaded(on: Boolean) {
        viewModelScope.launch { settings.setThreaded(on) }
    }

    /** Marks a message read in the list as soon as it is opened. */
    fun opened(uid: UInt) = _state.update { s ->
        s.copy(
            messages = s.messages.map { if (it.uid == uid) it.copy(seen = true) else it },
            results = s.results.map { if (it.uid == uid) it.copy(seen = true) else it },
        )
    }
}

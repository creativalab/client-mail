package com.ferrowl.app.ui.mailbox

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import com.ferrowl.app.data.SettingsStore
import com.ferrowl.core.Conversation
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.Folder
import com.ferrowl.core.Header
import com.ferrowl.core.SortOrder
import com.ferrowl.core.groupConversations
import com.ferrowl.core.sortConversations
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class MailboxState(
    /** Addresses of every account on the phone, and the active one. */
    val accounts: List<String> = emptyList(),
    val active: String = "",
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
    val sort: SortOrder = SortOrder.NEWEST,
    /** Gmail inbox: the tabs are shown, and which one is open (0 = Primary). */
    val tabs: Boolean = false,
    val category: Int = 0,
    /** Checked messages (uids); not empty = selection mode. */
    val selected: Set<UInt> = emptySet(),
    /** Asking before deleting the selection for good (in the Trash). */
    val confirmDelete: Boolean = false,
) {
    val selecting: Boolean get() = selected.isNotEmpty()

    /** Gmail inbox: unread messages per tab (category index → count). */
    val unreadByCategory: Map<Int, Int>
        get() = if (!tabs) emptyMap() else messages.filter { !it.seen }.groupingBy { it.category.toInt() }.eachCount()
    val folder: Folder? get() = folders.firstOrNull { it.id == folderId }
    val shown: List<Header>
        get() = when {
            query.isNotBlank() -> results
            tabs -> messages.filter { it.category.toInt() == category }
            else -> messages
        }

    /** The list rows: whole conversations, or single messages as one-member ones. */
    val rows: List<Conversation>
        get() = sortConversations(
            if (threaded) {
                groupConversations(shown)
            } else {
                shown.map { Conversation(it, listOf(it.uid), if (it.seen) 0u else 1u) }
            },
            sort,
        )
}

@HiltViewModel
class MailboxViewModel @Inject constructor(
    private val repo: MailRepository,
    private val settings: SettingsStore,
) : ViewModel() {
    private val _state = MutableStateFlow(MailboxState())
    val state: StateFlow<MailboxState> = _state.asStateFlow()

    /** The error was shown to the user (as a notice at the top). */
    fun errorShown() = _state.update { it.copy(error = null) }

    private var searchJob: Job? = null

    init {
        viewModelScope.launch { settings.threaded.collect { on -> _state.update { it.copy(threaded = on) } } }
        viewModelScope.launch { settings.sort.collect { o -> _state.update { it.copy(sort = o) } } }
        viewModelScope.launch {
            // The saved inbox right away; connecting can take a while.
            _state.update { it.copy(refreshing = true) }
            val (early, gmail) = repo.cachedInbox()
            if (early.isNotEmpty()) {
                _state.update { if (it.folderId.isEmpty()) it.copy(folderId = "INBOX", messages = early, tabs = gmail) else it }
            }
            try {
                repo.ensureConnected()
                _state.update {
                    it.copy(folders = repo.folders.value, folderId = repo.inboxId(), tabs = repo.isGmail())
                }
                refresh()
            } catch (e: FerrowlException) {
                _state.update { it.copy(refreshing = false, error = e.message) }
            }
        }
        viewModelScope.launch {
            repo.accountList.collect { list -> _state.update { it.copy(accounts = list.map { a -> a.email }) } }
        }
        viewModelScope.launch { repo.activeEmail.collect { e -> _state.update { it.copy(active = e.orEmpty()) } } }
        // Another account became active: start over with its inbox.
        viewModelScope.launch {
            repo.switched.collect {
                _state.update {
                    it.copy(
                        folders = repo.folders.value,
                        folderId = repo.inboxId(),
                        tabs = repo.isGmail(),
                        unread = emptyMap(),
                        messages = emptyList(),
                        query = "",
                        results = emptyList(),
                        selected = emptySet(),
                        category = 0,
                        error = null,
                    )
                }
                refresh()
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

    /** Shows another account (connects it first). */
    fun switchAccount(email: String) {
        if (email.equals(_state.value.active, ignoreCase = true)) return
        _state.update { it.copy(refreshing = true, error = null) }
        viewModelScope.launch {
            try {
                repo.switchTo(email)
            } catch (e: FerrowlException) {
                _state.update { it.copy(refreshing = false, error = e.message) }
            }
        }
    }

    fun openFolder(id: String) {
        if (id == _state.value.folderId) return
        _state.update {
            it.copy(
                folderId = id,
                messages = emptyList(),
                query = "",
                results = emptyList(),
                selected = emptySet(),
                tabs = repo.isGmail() && id == repo.inboxId(),
                category = 0,
            )
        }
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
                var fresh = repo.headers(id)
                // Sender lists: blocked mail leaves the inbox, allowed mail leaves Spam.
                val moved = runCatching { repo.applySenderRules(id, fresh) }.getOrDefault(emptyList())
                if (moved.isNotEmpty()) fresh = fresh.filter { it.uid !in moved }
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

    fun starSelected(on: Boolean) = bulk { folder, uids -> star(folder, uids, on) }

    /** The star of a list row (all its messages). */
    fun toggleStar(uids: List<UInt>, on: Boolean) {
        val folder = _state.value.folderId
        viewModelScope.launch {
            try {
                star(folder, uids, on)
            } catch (e: FerrowlException) {
                star(folder, uids, !on, local = true)
                _state.update { it.copy(error = e.message) }
            }
        }
    }

    /** Updates the list right away, then the server (unless [local]). */
    private suspend fun star(folder: String, uids: List<UInt>, on: Boolean, local: Boolean = false) {
        _state.update { s ->
            s.copy(
                messages = s.messages.map { if (it.uid in uids) it.copy(flagged = on) else it },
                results = s.results.map { if (it.uid in uids) it.copy(flagged = on) else it },
            )
        }
        if (!local) repo.setFlagged(folder, uids, on)
    }

    /** Gmail: whether the folder's messages have labels. */
    val gmail: Boolean get() = repo.isGmail()

    fun labelChoices(): List<Folder> = repo.labelChoices(_state.value.folderId)

    /** Adds or removes a label on the checked messages (the selection stays). */
    fun toggleLabel(label: String, on: Boolean) {
        val s = _state.value
        val uids = s.selected.toList()
        if (uids.isEmpty()) return
        val apply = { add: Boolean ->
            _state.update { st ->
                fun fix(h: Header) = if (h.uid in uids) h.copy(labels = h.labels.filter { it != label } + if (add) listOf(label) else emptyList()) else h
                st.copy(messages = st.messages.map(::fix), results = st.results.map(::fix))
            }
        }
        apply(on)
        viewModelScope.launch {
            try {
                repo.setLabel(s.folderId, uids, label, on)
            } catch (e: FerrowlException) {
                apply(!on)
                _state.update { it.copy(error = e.message) }
            }
        }
    }

    /** Gmail inbox tab (0 Primary … 4 Forums). */
    fun setCategory(c: Int) = _state.update { it.copy(category = c, selected = emptySet()) }

    fun setSort(order: SortOrder) {
        viewModelScope.launch { settings.setSort(order) }
    }

    /** Whether the open folder is Spam (the action becomes "Not spam"). */
    val inSpam: Boolean get() = repo.isSpam(_state.value.folderId)

    /** Report spam, or in Spam "not spam" (which also allows the senders). */
    fun spamSelected() {
        val toSpam = !inSpam
        val s = _state.value
        if (!toSpam) {
            val senders = s.shown.filter { it.uid in s.selected && it.fromAddress.isNotBlank() }.map { it.fromAddress }.toSet()
            viewModelScope.launch { senders.forEach { runCatching { repo.setSenderRule(it, true) } } }
        }
        bulk { folder, uids -> repo.spam(folder, uids, toSpam) }
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

    /** Marks a message read in the list as soon as it is opened. */
    fun opened(uid: UInt) = _state.update { s ->
        s.copy(
            messages = s.messages.map { if (it.uid == uid) it.copy(seen = true) else it },
            results = s.results.map { if (it.uid == uid) it.copy(seen = true) else it },
        )
    }
}

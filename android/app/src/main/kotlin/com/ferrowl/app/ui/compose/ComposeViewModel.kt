package com.ferrowl.app.ui.compose

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import com.ferrowl.app.ui.message.ReplyKind
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.Outgoing
import com.ferrowl.core.forwardDraft
import com.ferrowl.core.replyDraft
import com.ferrowl.core.translate
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class ComposeState(
    val to: String = "",
    val cc: String = "",
    val subject: String = "",
    val body: String = "",
    /** Threading headers and flags kept from the draft (replies). */
    val base: Outgoing? = null,
    val loading: Boolean = false,
    val sending: Boolean = false,
    val error: String? = null,
    val sent: Boolean = false,
    /** The user typed something: leaving asks before discarding. */
    val edited: Boolean = false,
)

@HiltViewModel
class ComposeViewModel @Inject constructor(private val repo: MailRepository) : ViewModel() {
    private val _state = MutableStateFlow(ComposeState())
    val state: StateFlow<ComposeState> = _state.asStateFlow()
    private var started = false

    /** Prepares a reply or forward from the original message (once). */
    fun start(kind: ReplyKind?, folderId: String, uid: UInt) {
        if (started) return
        started = true
        if (kind == null) return
        _state.update { it.copy(loading = true) }
        viewModelScope.launch {
            try {
                val original = repo.message(folderId, uid)
                val own = repo.savedAccount()?.email.orEmpty()
                val draft = when (kind) {
                    ReplyKind.REPLY -> replyDraft(original, false, own)
                    ReplyKind.REPLY_ALL -> replyDraft(original, true, own)
                    ReplyKind.FORWARD -> forwardDraft(original)
                }
                _state.update {
                    it.copy(to = draft.to, cc = draft.cc, subject = draft.subject, body = draft.body, base = draft, loading = false)
                }
            } catch (e: FerrowlException) {
                _state.update { it.copy(loading = false, error = e.message) }
            }
        }
    }

    fun setTo(v: String) = _state.update { it.copy(to = v, edited = true, error = null) }
    fun setCc(v: String) = _state.update { it.copy(cc = v, edited = true) }
    fun setSubject(v: String) = _state.update { it.copy(subject = v, edited = true) }
    fun setBody(v: String) = _state.update { it.copy(body = v, edited = true) }

    fun send() {
        val s = _state.value
        if (s.to.isBlank()) {
            _state.update { it.copy(error = translate("err_no_recipient")) }
            return
        }
        val draft = (s.base ?: Outgoing("", "", "", "", "", null, null, emptyList(), emptyList(), false, false))
            .copy(to = s.to.trim(), cc = s.cc.trim(), subject = s.subject.trim(), body = s.body)
        _state.update { it.copy(sending = true, error = null) }
        viewModelScope.launch {
            try {
                repo.send(draft)
                _state.update { it.copy(sending = false, sent = true) }
            } catch (e: FerrowlException) {
                _state.update { it.copy(sending = false, error = e.message) }
            }
        }
    }
}

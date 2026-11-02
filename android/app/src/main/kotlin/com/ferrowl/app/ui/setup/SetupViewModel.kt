package com.ferrowl.app.ui.setup

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import com.ferrowl.core.Account
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.Login
import com.ferrowl.core.Protocol
import com.ferrowl.core.Provider
import com.ferrowl.core.knownProviders
import com.ferrowl.core.providerForEmail
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * Everything the setup screen shows, in one immutable value. The screen only
 * draws it and reports user actions back (unidirectional data flow).
 */
data class SetupState(
    val providers: List<Provider> = emptyList(),
    /** Index into [providers], or null for manual setup ("Other"). */
    val provider: Int? = null,
    val name: String = "",
    val email: String = "",
    val password: String = "",
    val pop3: Boolean = false,
    val username: String = "",
    val incomingHost: String = "",
    val incomingPort: String = "993",
    val smtpHost: String = "",
    val smtpPort: String = "465",
    val busy: Boolean = false,
    val error: String? = null,
    val done: Boolean = false,
) {
    val chosen: Provider? get() = provider?.let(providers::getOrNull)

    /** Microsoft accounts need OAuth sign-in, not available yet on Android. */
    val blocked: Boolean get() = chosen?.login == Login.MICROSOFT_SIGN_IN

    val canConnect: Boolean
        get() = !busy && !blocked && email.contains('@') && password.isNotEmpty() &&
            (chosen != null || (incomingHost.isNotBlank() && smtpHost.isNotBlank() &&
                incomingPort.toIntOrNull() != null && smtpPort.toIntOrNull() != null))

    fun toAccount(): Account {
        val p = chosen
        return Account(
            displayName = name.trim(),
            email = email.trim(),
            username = if (p != null || username.isBlank()) email.trim() else username.trim(),
            password = password,
            incomingHost = p?.imapHost ?: incomingHost.trim(),
            incomingPort = (p?.imapPort?.toInt() ?: incomingPort.toInt()).toUShort(),
            smtpHost = p?.smtpHost ?: smtpHost.trim(),
            smtpPort = (p?.smtpPort?.toInt() ?: smtpPort.toInt()).toUShort(),
            protocol = if (p == null && pop3) Protocol.POP3 else Protocol.IMAP,
            pop3KeepDays = null,
            pgpPassphrase = "",
        )
    }
}

@HiltViewModel
class SetupViewModel @Inject constructor(private val repo: MailRepository) : ViewModel() {
    private val _state = MutableStateFlow(SetupState(providers = knownProviders()))
    val state: StateFlow<SetupState> = _state.asStateFlow()

    fun pickProvider(index: Int?) = _state.update { it.copy(provider = index, error = null) }

    fun setName(v: String) = _state.update { it.copy(name = v) }

    fun setPassword(v: String) = _state.update { it.copy(password = v, error = null) }

    /** Typing the address selects the provider when the domain is known. */
    fun setEmail(v: String) = _state.update { s ->
        val detected = providerForEmail(v)?.toInt()
        s.copy(email = v, error = null, provider = detected ?: s.provider)
    }

    fun setPop3(on: Boolean) = _state.update { s ->
        s.copy(
            pop3 = on,
            incomingPort = when {
                on && s.incomingPort == "993" -> "995"
                !on && s.incomingPort == "995" -> "993"
                else -> s.incomingPort
            },
        )
    }

    fun setUsername(v: String) = _state.update { it.copy(username = v) }
    fun setIncomingHost(v: String) = _state.update { it.copy(incomingHost = v) }
    fun setIncomingPort(v: String) = _state.update { it.copy(incomingPort = v.filter(Char::isDigit)) }
    fun setSmtpHost(v: String) = _state.update { it.copy(smtpHost = v) }
    fun setSmtpPort(v: String) = _state.update { it.copy(smtpPort = v.filter(Char::isDigit)) }

    /** Connects; on success the account is saved and the screen moves on. */
    fun connect() {
        val account = _state.value.toAccount()
        _state.update { it.copy(busy = true, error = null) }
        viewModelScope.launch {
            try {
                repo.setUp(account)
                _state.update { it.copy(busy = false, done = true) }
            } catch (e: FerrowlException) {
                _state.update { it.copy(busy = false, error = e.message) }
            }
        }
    }
}

package com.ferrowl.app.ui.setup

import android.app.PendingIntent
import android.content.Intent
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.GoogleSignIn
import com.ferrowl.app.data.MailRepository
import com.ferrowl.app.data.MicrosoftSignIn
import com.ferrowl.app.data.SignInState
import com.ferrowl.core.Account
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.Login
import com.ferrowl.core.OAuthProvider
import com.ferrowl.core.Protocol
import com.ferrowl.core.Provider
import com.ferrowl.core.knownProviders
import com.ferrowl.core.oauthAccount
import com.ferrowl.core.oauthAvailable
import com.ferrowl.core.providerForEmail
import com.ferrowl.core.translate
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

    /** Microsoft accounts sign in on Microsoft's page instead of a password. */
    val signsIn: Boolean get() = chosen?.login == Login.MICROSOFT_SIGN_IN

    val canSignIn: Boolean get() = !busy && signsIn && oauthAvailable(OAuthProvider.MICROSOFT)

    /** Gmail: Sign in with Google (Google Identity Services), or an app password. */
    val isGoogle: Boolean get() = chosen?.imapHost == "imap.gmail.com"

    val canConnect: Boolean
        get() = !busy && !signsIn && email.contains('@') && password.isNotEmpty() &&
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
class SetupViewModel @Inject constructor(
    private val repo: MailRepository,
    private val microsoft: MicrosoftSignIn,
    private val google: GoogleSignIn,
) : ViewModel() {
    private val _state = MutableStateFlow(SetupState(providers = knownProviders()))
    val state: StateFlow<SetupState> = _state.asStateFlow()

    /** The error was shown to the user (as a notice at the top). */
    fun errorShown() = _state.update { it.copy(error = null) }

    init {
        // The Microsoft sign-in may finish while this screen is in the
        // background, or after Android restarted the app.
        viewModelScope.launch {
            microsoft.state.collect { st ->
                when (st) {
                    SignInState.Idle -> _state.update { it.copy(busy = false) }
                    SignInState.Waiting, SignInState.Working -> _state.update { it.copy(busy = true, error = null) }
                    SignInState.Done -> {
                        microsoft.acknowledge()
                        _state.update { it.copy(busy = false, done = true) }
                    }
                    is SignInState.Failed -> {
                        microsoft.acknowledge()
                        _state.update { it.copy(busy = false, error = st.message) }
                    }
                }
            }
        }
    }

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

    /**
     * Microsoft: prepares the sign-in and returns the page to open in the
     * browser. Microsoft's answer reopens the app, which finishes it
     * ([MicrosoftSignIn]); progress arrives through [state].
     */
    fun signIn(): String? =
        try {
            microsoft.start(_state.value.name.trim(), _state.value.email.trim())
        } catch (e: FerrowlException) {
            _state.update { it.copy(error = e.message) }
            null
        }

    /**
     * Google: asks for access. Returns the account picker / consent screen
     * to show, or null when Google answered at once (or failed).
     */
    suspend fun googleStart(): PendingIntent? {
        _state.update { it.copy(busy = true, error = null) }
        return try {
            when (val start = google.start()) {
                is GoogleSignIn.Start.Token -> {
                    finishGoogle(start.token)
                    null
                }
                is GoogleSignIn.Start.NeedsUser -> start.intent
            }
        } catch (e: Exception) {
            _state.update { it.copy(busy = false, error = translate("err_google_signin")) }
            null
        }
    }

    /** The answer of Google's screen (null data: the user went back). */
    fun googleResult(data: Intent?) {
        val token = google.tokenFrom(data)
        if (token == null) {
            _state.update { it.copy(busy = false) }
            return
        }
        finishGoogle(token)
    }

    private fun finishGoogle(token: String) {
        viewModelScope.launch {
            try {
                val email = google.emailOf(token)
                repo.setUp(oauthAccount(OAuthProvider.GOOGLE, email, _state.value.name.trim().ifEmpty { email }))
                _state.update { it.copy(busy = false, done = true) }
            } catch (e: FerrowlException) {
                _state.update { it.copy(busy = false, error = e.message) }
            } catch (e: java.io.IOException) {
                _state.update { it.copy(busy = false, error = translate("err_google_signin")) }
            }
        }
    }

    /** Back on the form with the sign-in page closed and no answer: allow a new try. */
    fun resumed() = microsoft.cancelIfWaiting()

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

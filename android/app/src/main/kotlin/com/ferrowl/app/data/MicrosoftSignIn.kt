package com.ferrowl.app.data

import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.util.Base64
import androidx.core.content.edit
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.OAuthProvider
import com.ferrowl.core.OAuthRequest
import com.ferrowl.core.oauthAccount
import com.ferrowl.core.oauthBegin
import com.ferrowl.core.oauthFinish
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.net.URLEncoder
import java.security.MessageDigest
import javax.inject.Inject
import javax.inject.Singleton

/** Where a Microsoft sign-in is. */
sealed interface SignInState {
    data object Idle : SignInState

    /** The sign-in page is open in the browser. */
    data object Waiting : SignInState

    /** Microsoft sent the user back: finishing and connecting. */
    data object Working : SignInState

    data object Done : SignInState

    data class Failed(val message: String) : SignInState
}

/**
 * Sign in with Microsoft on Android.
 *
 * The page opens in a Custom Tab; Microsoft then sends the browser to the
 * app's own address (`msauth://com.ferrowl.app/<signature hash>`, registered
 * on the Ferrowl app in Microsoft Entra), which reopens Ferrowl through
 * [com.ferrowl.app.OAuthRedirectActivity] even if Android stopped it in the
 * meantime. The request (PKCE verifier and state) is therefore kept on disk,
 * encrypted with [SecretBox], until the answer arrives.
 */
@Singleton
class MicrosoftSignIn @Inject constructor(
    @ApplicationContext private val context: Context,
    private val secrets: SecretBox,
    private val repo: MailRepository,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val prefs = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)
    private val _state = MutableStateFlow<SignInState>(SignInState.Idle)
    val state: StateFlow<SignInState> = _state.asStateFlow()

    /** The address Microsoft sends the browser back to; depends on the app's signing key. */
    val redirectUri: String by lazy {
        val hash = Base64.encodeToString(MessageDigest.getInstance("SHA-1").digest(signingCertificate()), Base64.NO_WRAP)
        "msauth://${context.packageName}/${URLEncoder.encode(hash, "UTF-8")}"
    }

    /** Prepares a sign-in and returns the page to open. */
    fun start(name: String, email: String): String {
        val request = oauthBegin(OAuthProvider.MICROSOFT, email, redirectUri)
        val pending = Pending(name, email, request.url, request.state, request.verifier, request.redirect)
        prefs.edit(commit = true) { putString(KEY, secrets.encrypt(Json.encodeToString(pending))) }
        _state.value = SignInState.Waiting
        return request.url
    }

    /** True for the address Microsoft sends back to. */
    fun isAnswer(uri: Uri): Boolean = uri.scheme == "msauth" && uri.host == context.packageName

    /** Finishes the sign-in with Microsoft's answer and sets the account up. */
    fun complete(answer: Uri) {
        val pending = load() ?: return
        _state.value = SignInState.Working
        scope.launch {
            try {
                val request = OAuthRequest(pending.url, pending.state, pending.verifier, pending.redirect)
                val email = oauthFinish(OAuthProvider.MICROSOFT, request, answer.toString(), pending.email)
                repo.setUp(oauthAccount(OAuthProvider.MICROSOFT, email, pending.name.ifBlank { email }))
                _state.value = SignInState.Done
            } catch (e: FerrowlException) {
                _state.value = SignInState.Failed(e.message.orEmpty())
            } finally {
                prefs.edit(commit = true) { remove(KEY) }
            }
        }
    }

    /** The user is back on the form without finishing (tab closed): allow a new try. */
    fun cancelIfWaiting() {
        if (_state.value == SignInState.Waiting) _state.value = SignInState.Idle
    }

    fun acknowledge() {
        _state.value = SignInState.Idle
    }

    private fun load(): Pending? =
        prefs.getString(KEY, null)?.let { sealed ->
            runCatching { Json.decodeFromString<Pending>(secrets.decrypt(sealed)) }.getOrNull()
        }

    @Suppress("DEPRECATION")
    private fun signingCertificate(): ByteArray {
        val pm = context.packageManager
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            pm.getPackageInfo(context.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
                .signingInfo!!.apkContentsSigners.first().toByteArray()
        } else {
            pm.getPackageInfo(context.packageName, PackageManager.GET_SIGNATURES).signatures!!.first().toByteArray()
        }
    }

    /** A sign-in waiting for Microsoft's answer (stored encrypted). */
    @Serializable
    private data class Pending(
        val name: String,
        val email: String,
        val url: String,
        val state: String,
        val verifier: String,
        val redirect: String,
    )

    private companion object {
        const val FILE = "sign-in-pending"
        const val KEY = "microsoft"
    }
}

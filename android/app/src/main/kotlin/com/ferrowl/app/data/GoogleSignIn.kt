package com.ferrowl.app.data

import android.accounts.Account
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import com.ferrowl.core.AccessTokenSource
import com.ferrowl.core.OAuthProvider
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.AuthorizationResult
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Tasks
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.tasks.await
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.TimeUnit
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Sign in with Google on Android, through Google Identity Services: the
 * phone's own account picker and consent screen, no browser and no
 * password. Google recognises the app from its package name and signing
 * certificate (the "Ferrowl Android" OAuth client of the Google Cloud
 * project), so no client ID is needed here.
 *
 * Google gives short-lived access tokens and no refresh token: when the
 * engine needs one ([accessToken]) the request is simply repeated, which
 * Google answers without asking the user again once access was granted.
 */
@Singleton
class GoogleSignIn @Inject constructor(
    @ApplicationContext private val context: Context,
) : AccessTokenSource {
    private val client get() = Identity.getAuthorizationClient(context)

    /** Outcome of starting a sign-in: a token at once, or a screen to show. */
    sealed interface Start {
        data class Token(val token: String) : Start

        data class NeedsUser(val intent: PendingIntent) : Start
    }

    /** First step: asks Google; the user usually has to pick an account and agree. */
    suspend fun start(): Start {
        val result = client.authorize(request(null)).await()
        val pending = result.pendingIntent
        return if (result.hasResolution() && pending != null) {
            Start.NeedsUser(pending)
        } else {
            Start.Token(result.accessToken ?: error("no access token"))
        }
    }

    /** The answer of the account picker / consent screen. */
    fun tokenFrom(data: Intent?): String? =
        runCatching { client.getAuthorizationResultFromIntent(data) }.getOrNull()?.accessToken

    /** Address of the Google account the token belongs to. */
    suspend fun emailOf(token: String): String = withContext(Dispatchers.IO) {
        val conn = URL(USERINFO).openConnection() as HttpURLConnection
        try {
            conn.setRequestProperty("Authorization", "Bearer $token")
            conn.connectTimeout = 15_000
            conn.readTimeout = 15_000
            val body = conn.inputStream.bufferedReader().use { it.readText() }
            json.decodeFromString<UserInfo>(body).email
        } finally {
            conn.disconnect()
        }
    }

    /** Called by the engine (background thread) whenever it needs a token for a Google account. */
    override fun accessToken(provider: OAuthProvider, email: String): String? {
        if (provider != OAuthProvider.GOOGLE) return null
        val result: AuthorizationResult = runCatching {
            Tasks.await(client.authorize(request(email)), 30, TimeUnit.SECONDS)
        }.getOrNull() ?: return null
        // Needing the user again (access removed): the engine reports it.
        return if (result.hasResolution()) null else result.accessToken
    }

    private fun request(email: String?): AuthorizationRequest =
        AuthorizationRequest.builder()
            .setRequestedScopes(SCOPES)
            .apply { if (email != null) setAccount(Account(email, "com.google")) }
            .build()

    @Serializable
    private data class UserInfo(val email: String)

    private companion object {
        /** Full Gmail access (IMAP/SMTP need it) and the address. */
        val SCOPES = listOf(Scope("https://mail.google.com/"), Scope("openid"), Scope("email"))
        const val USERINFO = "https://openidconnect.googleapis.com/v1/userinfo"
        val json = Json { ignoreUnknownKeys = true }
    }
}

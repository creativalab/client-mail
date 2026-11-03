package com.ferrowl.app.push

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import javax.inject.Inject
import javax.inject.Singleton

/** The push server answered with an error (its message, ready to show). */
class PushException(message: String, val status: Int) : IOException(message)

/**
 * Calls the push server's HTTP API (see server/src/api.rs). Plain
 * HttpURLConnection over HTTPS: four small JSON calls do not need an HTTP
 * library. Every call runs off the main thread.
 */
@Singleton
class PushClient @Inject constructor() {
    private val json = Json { ignoreUnknownKeys = true }

    @Serializable private data class FcmTokenBody(val fcm_token: String)

    @Serializable data class Registered(val device_id: String, val device_token: String)

    @Serializable private data class Unlock(val key: String)

    @Serializable private data class Added(val account_id: String)

    @Serializable private data class ErrorBody(val error: String = "")

    /** Registers the phone; returns its id and secret token. */
    suspend fun register(fcmToken: String): Registered =
        json.decodeFromString(call("POST", "/v1/devices", null, json.encodeToString(FcmTokenBody(fcmToken))))

    suspend fun updateToken(deviceToken: String, fcmToken: String) {
        call("PUT", "/v1/device", deviceToken, json.encodeToString(FcmTokenBody(fcmToken)))
    }

    /** Forgets the phone and its accounts on the server. */
    suspend fun forget(deviceToken: String) {
        call("DELETE", "/v1/device", deviceToken, null)
    }

    /**
     * Stores an account (`accountJson`: the engine's settings format) sealed
     * with `key`, and starts watching it. Returns the account's id.
     */
    suspend fun addAccount(deviceToken: String, key: String, accountJson: String): String {
        // The account is embedded as JSON as it is (it is already JSON).
        val body = """{"key":${json.encodeToString(key)},"account":$accountJson}"""
        return json.decodeFromString<Added>(call("POST", "/v1/accounts", deviceToken, body)).account_id
    }

    /** Stops watching an account and deletes it from the server. */
    suspend fun deleteAccount(deviceToken: String, accountId: String) {
        call("DELETE", "/v1/accounts/$accountId", deviceToken, null)
    }

    /** After a server restart, lets it read the accounts again. */
    suspend fun unlock(deviceToken: String, key: String) {
        call("POST", "/v1/unlock", deviceToken, json.encodeToString(Unlock(key)))
    }

    private suspend fun call(method: String, path: String, deviceToken: String?, body: String?): String =
        withContext(Dispatchers.IO) {
            val conn = URL(BASE + path).openConnection() as HttpURLConnection
            try {
                conn.requestMethod = method
                conn.connectTimeout = 15_000
                // Adding an account checks its login first: allow time.
                conn.readTimeout = 45_000
                conn.setRequestProperty("Accept", "application/json")
                deviceToken?.let { conn.setRequestProperty("Authorization", "Bearer $it") }
                if (body != null) {
                    conn.doOutput = true
                    conn.setRequestProperty("Content-Type", "application/json")
                    conn.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
                }
                val status = conn.responseCode
                val stream = if (status in 200..299) conn.inputStream else conn.errorStream
                val text = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
                if (status !in 200..299) {
                    val msg = runCatching { json.decodeFromString<ErrorBody>(text).error }.getOrNull()
                    throw PushException(msg?.ifBlank { null } ?: "HTTP $status", status)
                }
                text
            } finally {
                conn.disconnect()
            }
        }

    private companion object {
        const val BASE = "https://push.ferrowl.com"
    }
}

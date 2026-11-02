package com.ferrowl.app.data

import com.ferrowl.core.Account
import com.ferrowl.core.Engine
import com.ferrowl.core.Folder
import com.ferrowl.core.FolderKind
import com.ferrowl.core.Header
import com.ferrowl.core.Message
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The single entry point to mail for the UI: it wraps the Rust [Engine]
 * and the saved account. ViewModels talk to this class, never to the
 * engine directly, so the rest of the app does not depend on the bindings.
 *
 * Engine calls are `suspend` functions: they run off the main thread and
 * throw `FerrowlException` with a message ready to show.
 */
@Singleton
class MailRepository @Inject constructor(
    private val engine: Engine,
    private val accounts: AccountStore,
) {
    private val connectLock = Mutex()
    private val _folders = MutableStateFlow<List<Folder>>(emptyList())

    /** Folders of the connected account (empty until [ensureConnected]). */
    val folders: StateFlow<List<Folder>> = _folders.asStateFlow()

    suspend fun savedAccount(): Account? = accounts.account.first()

    /** Checks settings without saving them. Returns a confirmation text. */
    suspend fun test(account: Account): String = engine.test(account)

    /** Connects and, if it works, saves the account. */
    suspend fun setUp(account: Account) {
        connectLock.withLock {
            _folders.value = engine.connect(account)
            accounts.save(account)
        }
    }

    /** Connects the saved account once per process. */
    suspend fun ensureConnected(): Boolean = connectLock.withLock {
        if (_folders.value.isNotEmpty()) return true
        val account = accounts.account.first() ?: return false
        _folders.value = engine.connect(account)
        true
    }

    fun inboxId(): String = _folders.value.firstOrNull { it.kind == FolderKind.INBOX }?.id ?: "INBOX"

    /** Messages already on the phone (local database, off the main thread). */
    suspend fun cachedHeaders(folderId: String): List<Header> =
        withContext(Dispatchers.IO) { engine.cachedHeaders(folderId) }

    /** Newest messages from the server. */
    suspend fun headers(folderId: String): List<Header> = engine.headers(folderId).headers

    suspend fun message(folderId: String, uid: UInt): Message = engine.message(folderId, uid)

    suspend fun search(folderId: String, query: String): List<Header> =
        withContext(Dispatchers.IO) { engine.search(folderId, query) }
}

package com.ferrowl.app.data

import com.ferrowl.core.Account
import com.ferrowl.core.AddressSuggestion
import com.ferrowl.core.Contact
import com.ferrowl.core.Engine
import com.ferrowl.core.Folder
import com.ferrowl.core.FolderKind
import com.ferrowl.core.Header
import com.ferrowl.core.Message
import com.ferrowl.core.Outgoing
import com.ferrowl.core.markdownToHtml
import com.ferrowl.core.translate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
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
    private val _changes = MutableSharedFlow<Unit>(extraBufferCapacity = 8)
    private val _notices = MutableSharedFlow<String>(extraBufferCapacity = 8)

    /** Folders of the connected account (empty until [ensureConnected]). */
    val folders: StateFlow<List<Folder>> = _folders.asStateFlow()

    /** Emits after messages were moved, deleted or sent: lists reload. */
    val changes: SharedFlow<Unit> = _changes.asSharedFlow()

    /** Short confirmations to show in a snackbar ("Archived", "Sent"). */
    val notices: SharedFlow<String> = _notices.asSharedFlow()

    suspend fun savedAccount(): Account? = accounts.account.first()

    /** Checks settings without saving them. Returns a confirmation text. */
    suspend fun test(account: Account): String = engine.test(account)

    /** Connects and, if it works, saves the account. */
    suspend fun setUp(account: Account) {
        connectLock.withLock {
            _folders.value = engine.connect(account)
            accounts.save(account)
        }
        learnFromSent()
        syncContactsQuietly()
    }

    /** Connects the saved account once per process. */
    suspend fun ensureConnected(): Boolean {
        val connected = connectLock.withLock {
            if (_folders.value.isNotEmpty()) return true
            val account = accounts.account.first() ?: return false
            _folders.value = engine.connect(account)
            true
        }
        if (connected) {
            learnFromSent()
            syncContactsQuietly()
        }
        return connected
    }

    /**
     * The address book learns who we write to from the Sent folder: its
     * newest page is read once per connection. Failures don't matter here.
     */
    private suspend fun learnFromSent() {
        val sent = folderOf(FolderKind.SENT) ?: return
        runCatching { engine.headers(sent.id) }
    }

    /**
     * Contacts from the account's CardDAV server, once per connection.
     * Most mail accounts have no such server: failures are not shown here
     * (the contacts screen shows them when the user syncs by hand).
     */
    private suspend fun syncContactsQuietly() {
        runCatching { engine.syncContacts() }
    }

    /** Saved contacts matching [query] (all when empty), by name. */
    suspend fun contacts(query: String): List<Contact> =
        withContext(Dispatchers.IO) { engine.contacts(query) }

    /** Updates the contacts from the server; returns how many are saved. */
    suspend fun syncContacts(): Int = engine.syncContacts().toInt()

    /** Recipients matching what is being typed. */
    suspend fun suggest(query: String): List<AddressSuggestion> =
        withContext(Dispatchers.IO) { engine.suggestAddresses(query) }

    fun folder(id: String): Folder? = _folders.value.firstOrNull { it.id == id }

    private fun folderOf(kind: FolderKind): Folder? = _folders.value.firstOrNull { it.kind == kind }

    fun inboxId(): String = folderOf(FolderKind.INBOX)?.id ?: "INBOX"

    /** Messages already on the phone (local database, off the main thread). */
    suspend fun cachedHeaders(folderId: String): List<Header> =
        withContext(Dispatchers.IO) { engine.cachedHeaders(folderId) }

    /** Newest messages from the server. */
    suspend fun headers(folderId: String): List<Header> = engine.headers(folderId).headers

    suspend fun search(folderId: String, query: String): List<Header> =
        withContext(Dispatchers.IO) { engine.search(folderId, query) }

    /** Unread count of every folder. */
    suspend fun unread(): Map<String, UInt> =
        engine.unread(_folders.value.filter { it.selectable }.map { it.id })

    suspend fun message(folderId: String, uid: UInt): Message = engine.message(folderId, uid)

    suspend fun setSeen(folderId: String, uid: UInt, seen: Boolean) {
        engine.setSeen(folderId, uid, seen)
        _changes.emit(Unit)
    }

    suspend fun archive(folderId: String, uid: UInt) {
        engine.archive(folderId, uid)
        changed(translate("archived"))
    }

    /** Whether deleting from this folder is final (it is the Trash). */
    fun deletesForever(folderId: String): Boolean =
        folder(folderId)?.kind == FolderKind.TRASH || folderOf(FolderKind.TRASH) == null

    /** Moves to the Trash, or deletes for good when already there. */
    suspend fun delete(folderId: String, uid: UInt) {
        if (deletesForever(folderId)) {
            engine.deleteForever(folderId, uid)
            changed(translate("deleted_forever"))
        } else {
            engine.moveTo(folderId, uid, folderOf(FolderKind.TRASH)!!.id)
            changed(translate("moved_to_trash"))
        }
    }

    /** Marks several messages read or unread at once. */
    suspend fun setSeen(folderId: String, uids: List<UInt>, seen: Boolean) {
        engine.setSeenMany(folderId, uids, seen)
        _changes.emit(Unit)
    }

    suspend fun archive(folderId: String, uids: List<UInt>) {
        engine.archiveMany(folderId, uids)
        changed(translate("bulk_archived").replace("{n}", uids.size.toString()))
    }

    /** Moves several messages to another folder. */
    suspend fun move(folderId: String, uids: List<UInt>, targetId: String) {
        engine.moveMany(folderId, uids, targetId)
        changed(translate("bulk_moved").replace("{n}", uids.size.toString()))
    }

    /** Moves several messages to the Trash, or deletes them for good when already there. */
    suspend fun delete(folderId: String, uids: List<UInt>) {
        val n = uids.size.toString()
        if (deletesForever(folderId)) {
            engine.deleteForeverMany(folderId, uids)
            changed(translate("bulk_deleted_forever").replace("{n}", n))
        } else {
            engine.moveMany(folderId, uids, folderOf(FolderKind.TRASH)!!.id)
            changed(translate("bulk_trashed").replace("{n}", n))
        }
    }

    /**
     * Sends; Markdown formatting typed by the user also goes out as HTML,
     * as on desktop. A warning is shown when the copy in Sent failed.
     */
    suspend fun send(draft: Outgoing) {
        val warning = engine.send(draft.copy(html = markdownToHtml(draft.body)))
        changed(warning?.let { translate("sent_no_copy").replace("{err}", it) } ?: translate("sent_ok"))
    }

    private suspend fun changed(notice: String) {
        _changes.emit(Unit)
        _notices.emit(notice)
    }
}

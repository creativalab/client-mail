package com.ferrowl.app.ui.mailbox

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.combinedClickable
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.TextButton
import com.ferrowl.app.ui.fill
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ferrowl.app.R
import com.ferrowl.core.SortOrder
import com.ferrowl.core.Folder
import com.ferrowl.core.Header
import kotlinx.coroutines.launch

/**
 * The mail list of one folder, with the folder list in a side drawer,
 * search in the top bar and a button to write a new message.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MailboxScreen(
    /** Opens a message; `starred` lets the message screen show its star. */
    onOpen: (folderId: String, uid: UInt, starred: Boolean) -> Unit,
    onOpenConversation: (folderId: String, uids: List<UInt>) -> Unit,
    onCompose: () -> Unit,
    onSettings: () -> Unit,
    onContacts: () -> Unit,
    onCalendar: () -> Unit,
    vm: MailboxViewModel = hiltViewModel(),
) {
    val s by vm.state.collectAsStateWithLifecycle()
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    var searchOpen by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { vm.notices.collect { snackbar.showSnackbar(it) } }
    // With messages on screen, a failed refresh is reported in a snackbar
    // (an empty list shows the error in its place instead).
    LaunchedEffect(s.error) { s.error?.let { if (s.shown.isNotEmpty()) snackbar.showSnackbar(it) } }
    if (s.confirmDelete) ConfirmBulkDelete(vm)

    ModalNavigationDrawer(
        drawerState = drawer,
        drawerContent = {
            ModalDrawerSheet {
                // Scrollable list inside the sheet: some accounts have dozens of folders.
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    // Contacts and calendar on top: the folder list can be long.
                    Spacer(Modifier.height(12.dp))
                    NavigationDrawerItem(
                        label = { Text(stringResource(R.string.contacts)) },
                        selected = false,
                        onClick = {
                            scope.launch { drawer.close() }
                            onContacts()
                        },
                        modifier = Modifier.padding(horizontal = 12.dp),
                    )
                    NavigationDrawerItem(
                        label = { Text(stringResource(R.string.calendar)) },
                        selected = false,
                        onClick = {
                            scope.launch { drawer.close() }
                            onCalendar()
                        },
                        modifier = Modifier.padding(horizontal = 12.dp),
                    )
                    Text(
                        stringResource(R.string.section_folders),
                        style = MaterialTheme.typography.titleSmall,
                        modifier = Modifier.padding(start = 28.dp, top = 24.dp, bottom = 12.dp),
                    )
                    s.folders.filter { it.selectable }.forEach { f ->
                        val count = s.unread[f.id]?.toInt() ?: 0
                        // Subfolders are indented under their parent.
                        val depth = (f.path.size - 1).coerceAtLeast(0)
                        NavigationDrawerItem(
                            label = { Text(f.name, modifier = Modifier.padding(start = (16 * depth).dp)) },
                            badge = { if (count > 0) Text(count.toString()) },
                            selected = f.id == s.folderId,
                            onClick = {
                                vm.openFolder(f.id)
                                scope.launch { drawer.close() }
                            },
                            modifier = Modifier.padding(horizontal = 12.dp),
                        )
                    }
                    HorizontalDivider(Modifier.padding(vertical = 8.dp))
                    NavigationDrawerItem(
                        label = { Text(stringResource(R.string.settings)) },
                        selected = false,
                        onClick = {
                            scope.launch { drawer.close() }
                            onSettings()
                        },
                        modifier = Modifier.padding(horizontal = 12.dp),
                    )
                    // Keeps the last folder clear of the gesture/navigation bar.
                    Spacer(Modifier.navigationBarsPadding().height(12.dp))
                }
            }
        },
    ) {
        BackHandler(enabled = s.selecting) { vm.clearSelection() }
        Scaffold(
            topBar = {
                if (s.selecting) {
                    SelectionBar(s.selected.size, vm, s.folders, s.folderId)
                } else TopAppBar(
                    navigationIcon = {
                        IconButton(onClick = { scope.launch { drawer.open() } }) {
                            Icon(Icons.Filled.Menu, contentDescription = stringResource(R.string.section_folders))
                        }
                    },
                    title = {
                        if (searchOpen) {
                            TextField(
                                value = s.query,
                                onValueChange = vm::setQuery,
                                placeholder = { Text(stringResource(R.string.search_placeholder), maxLines = 1, overflow = TextOverflow.Ellipsis) },
                                singleLine = true,
                                colors = TextFieldDefaults.colors(
                                    focusedContainerColor = Color.Transparent,
                                    unfocusedContainerColor = Color.Transparent,
                                ),
                                modifier = Modifier.fillMaxWidth(),
                            )
                        } else {
                            Text(s.folder?.name ?: stringResource(R.string.folder_inbox))
                        }
                    },
                    actions = {
                        IconButton(onClick = {
                            if (searchOpen) vm.setQuery("")
                            searchOpen = !searchOpen
                        }) {
                            Icon(
                                if (searchOpen) Icons.Filled.Close else Icons.Filled.Search,
                                contentDescription = stringResource(if (searchOpen) R.string.close else R.string.search_placeholder),
                            )
                        }
                        if (!searchOpen) SortMenu(s.sort, vm::setSort)
                    },
                )
            },
            floatingActionButton = {
                if (!s.selecting) {
                    FloatingActionButton(onClick = onCompose) {
                        Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.compose))
                    }
                }
            },
            snackbarHost = { SnackbarHost(snackbar) },
        ) { padding ->
            Column(Modifier.fillMaxSize().padding(padding)) {
                PullToRefreshBox(
                    isRefreshing = s.refreshing,
                    onRefresh = vm::refresh,
                    modifier = Modifier.fillMaxSize(),
                ) {
                    val rows = s.rows
                    when {
                        s.error != null && rows.isEmpty() -> Centered(s.error!!, MaterialTheme.colorScheme.error)
                        rows.isEmpty() && !s.refreshing && !s.searching -> Centered(stringResource(R.string.list_empty))
                        else -> LazyColumn(Modifier.fillMaxSize()) {
                            items(rows, key = { it.latest.uid.toLong() }) { c ->
                                MessageRow(
                                    c.latest,
                                    count = c.uids.size,
                                    unread = c.unread > 0u,
                                    checked = if (s.selecting) s.selected.containsAll(c.uids) else null,
                                    onLongClick = { vm.toggle(c.uids) },
                                    starred = rowStarred(s, c.uids),
                                    onStar = { on -> vm.toggleStar(c.uids, on) },
                                ) {
                                    if (s.selecting) {
                                        vm.toggle(c.uids)
                                    } else if (c.uids.size > 1) {
                                        onOpenConversation(s.folderId, c.uids)
                                    } else {
                                        // Opening reads it: update the dot right away.
                                        vm.opened(c.latest.uid)
                                        onOpen(s.folderId, c.latest.uid, rowStarred(s, c.uids))
                                    }
                                }
                                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                            }
                            // Room for the compose button over the last row.
                            item { Spacer(Modifier.height(88.dp)) }
                        }
                    }
                }
            }
        }
    }
}

/** Top bar while messages are checked: count, select menu and bulk actions. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SelectionBar(count: Int, vm: MailboxViewModel, folders: List<Folder>, current: String) {
    var menu by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf(false) }
    if (moving) {
        MoveDialog(folders.filter { it.selectable && it.id != current }, onDismiss = { moving = false }) { target ->
            moving = false
            vm.moveSelected(target)
        }
    }
    TopAppBar(
        navigationIcon = {
            IconButton(onClick = vm::clearSelection) {
                Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.select_none))
            }
        },
        title = { Text(fill(stringResource(R.string.selected_count), "n" to count)) },
        actions = {
            IconButton(onClick = { vm.deleteSelected() }) {
                Icon(Icons.Filled.Delete, contentDescription = stringResource(R.string.delete))
            }
            IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, contentDescription = null) }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                val item = @Composable { label: Int, action: () -> Unit ->
                    DropdownMenuItem(text = { Text(stringResource(label)) }, onClick = { menu = false; action() })
                }
                // "Select" heading over the choices.
                DropdownMenuItem(text = { Text(stringResource(R.string.select)) }, onClick = {}, enabled = false)
                item(R.string.select_all) { vm.select(MailboxViewModel.Pick.ALL) }
                item(R.string.select_read) { vm.select(MailboxViewModel.Pick.READ) }
                item(R.string.select_unread) { vm.select(MailboxViewModel.Pick.UNREAD) }
                HorizontalDivider()
                item(R.string.mark_read) { vm.markSelected(true) }
                item(R.string.mark_unread) { vm.markSelected(false) }
                item(R.string.star_add) { vm.starSelected(true) }
                item(R.string.star_remove) { vm.starSelected(false) }
                item(R.string.archive) { vm.archiveSelected() }
                item(if (vm.inSpam) R.string.spam_not else R.string.spam_report) { vm.spamSelected() }
                item(R.string.move_to) { moving = true }
            }
        },
    )
}

/**
 * One list row: a message, or the newest message of a conversation of
 * [count]. A long press starts selection; [checked] is null outside it.
 */
@Composable
internal fun MessageRow(
    h: Header,
    count: Int = 1,
    unread: Boolean = !h.seen,
    checked: Boolean? = null,
    onLongClick: (() -> Unit)? = null,
    starred: Boolean = h.flagged,
    /** Shows a star that toggles (with the new state); null = no star. */
    onStar: ((Boolean) -> Unit)? = null,
    onClick: () -> Unit,
) {
    val weight = if (!unread) FontWeight.Normal else FontWeight.SemiBold
    Row(
        Modifier
            .fillMaxWidth()
            .background(if (checked == true) MaterialTheme.colorScheme.primaryContainer else Color.Transparent)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(start = if (checked != null) 4.dp else 0.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (checked != null) Checkbox(checked = checked, onCheckedChange = { onClick() })
        Box(Modifier.weight(1f)) { RowBody(h, count, unread, weight) }
        if (onStar != null) StarButton(starred) { onStar(!starred) }
    }
}

/** The star of a row: gold when on, faint when off. */
@Composable
fun StarButton(starred: Boolean, onClick: () -> Unit) {
    IconButton(onClick = onClick) {
        Icon(
            Icons.Filled.Star,
            contentDescription = stringResource(if (starred) R.string.star_remove else R.string.star_add),
            tint = if (starred) Color(0xFFE2A400) else MaterialTheme.colorScheme.outlineVariant,
        )
    }
}

/** "Sort by…": the orders, the current one checked. */
@Composable
private fun SortMenu(current: SortOrder, onPick: (SortOrder) -> Unit) {
    var open by remember { mutableStateOf(false) }
    IconButton(onClick = { open = true }) {
        Icon(Icons.AutoMirrored.Filled.List, contentDescription = stringResource(R.string.sort_by))
    }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        DropdownMenuItem(text = { Text(stringResource(R.string.sort_by)) }, onClick = {}, enabled = false)
        SortOrder.entries.forEach { order ->
            DropdownMenuItem(
                text = { Text(stringResource(sortLabel(order))) },
                trailingIcon = { if (order == current) Icon(Icons.Filled.Check, contentDescription = null) },
                onClick = {
                    open = false
                    onPick(order)
                },
            )
        }
    }
}

/** The phone-language label of a sort order. */
private fun sortLabel(order: SortOrder): Int = when (order) {
    SortOrder.NEWEST -> R.string.sort_newest
    SortOrder.OLDEST -> R.string.sort_oldest
    SortOrder.LARGEST -> R.string.sort_largest
    SortOrder.SMALLEST -> R.string.sort_smallest
    SortOrder.SENDER_AZ -> R.string.sort_sender_az
    SortOrder.SENDER_ZA -> R.string.sort_sender_za
    SortOrder.SUBJECT_AZ -> R.string.sort_subject_az
}

/** Whether any message of a row is starred. */
private fun rowStarred(s: MailboxState, uids: List<UInt>): Boolean = s.shown.any { it.flagged && it.uid in uids }

@Composable
private fun RowBody(h: Header, count: Int, unread: Boolean, weight: FontWeight) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (unread) {
                Box(Modifier.size(8.dp).clip(CircleShape).background(MaterialTheme.colorScheme.primary))
            }
            Text(h.from, fontWeight = weight, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            if (count > 1) {
                Text(
                    count.toString(),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.surfaceVariant)
                        .padding(horizontal = 7.dp, vertical = 1.dp),
                )
            }
            Text(
                DateUtils.getRelativeTimeSpanString(h.timestamp * 1000).toString(),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Text(h.subject, fontWeight = weight, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            h.preview,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** "Move to": the folders, as on the drawer. */
@Composable
private fun MoveDialog(folders: List<Folder>, onDismiss: () -> Unit, onPick: (String) -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.move_to)) },
        text = {
            LazyColumn {
                items(folders, key = { it.id }) { f ->
                    Text(
                        f.name,
                        modifier = Modifier
                            .fillMaxWidth()
                            .combinedClickable(onClick = { onPick(f.id) })
                            .padding(start = (16 * (f.path.size - 1).coerceAtLeast(0)).dp, top = 12.dp, bottom = 12.dp),
                    )
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

@Composable
private fun ConfirmBulkDelete(vm: MailboxViewModel) {
    AlertDialog(
        onDismissRequest = vm::clearSelection,
        title = { Text(stringResource(R.string.confirm_delete_title)) },
        text = { Text(stringResource(R.string.cannot_undo)) },
        confirmButton = { TextButton(onClick = { vm.deleteSelected(confirmed = true) }) { Text(stringResource(R.string.delete_forever)) } },
        dismissButton = { TextButton(onClick = vm::clearSelection) { Text(stringResource(R.string.cancel)) } },
    )
}

@Composable
private fun Centered(text: String, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) { Text(text, color = color) }
}

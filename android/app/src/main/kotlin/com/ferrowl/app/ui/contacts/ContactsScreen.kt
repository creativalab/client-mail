package com.ferrowl.app.ui.contacts

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Email
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ferrowl.app.R
import com.ferrowl.core.Contact
import com.ferrowl.core.initials

/**
 * The account's contacts (from its CardDAV server), with search. Tapping a
 * contact opens a sheet with its addresses and phone numbers.
 *
 * @param onWrite opens a new message to the given "Name <address>".
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ContactsScreen(onBack: () -> Unit, onWrite: (String) -> Unit, vm: ContactsViewModel = hiltViewModel()) {
    val s by vm.state.collectAsStateWithLifecycle()
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.contacts)) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back_to_mail))
                    }
                },
                actions = {
                    IconButton(onClick = vm::sync, enabled = !s.syncing) {
                        Icon(Icons.Filled.Refresh, contentDescription = stringResource(R.string.contacts_sync))
                    }
                },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (s.syncing) LinearProgressIndicator(Modifier.fillMaxWidth())
            OutlinedTextField(
                value = s.query,
                onValueChange = vm::setQuery,
                placeholder = { Text(stringResource(R.string.contacts_search)) },
                leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            )
            s.error?.let {
                Text(
                    it,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                )
            }
            if (s.contacts.isEmpty()) {
                Text(
                    stringResource(R.string.contacts_empty),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(16.dp),
                )
            } else {
                LazyColumn(Modifier.fillMaxSize()) {
                    items(s.contacts) { c ->
                        ListItem(
                            headlineContent = { Text(c.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                            supportingContent = {
                                val detail = c.emails.firstOrNull() ?: c.phones.firstOrNull().orEmpty()
                                Text(detail, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            },
                            leadingContent = { Avatar(c.name, 40.dp) },
                            modifier = Modifier.clickable { vm.open(c) },
                        )
                    }
                }
            }
        }
    }

    s.open?.let { c ->
        ModalBottomSheet(onDismissRequest = { vm.open(null) }) {
            ContactSheet(c, onWrite = { address ->
                vm.open(null)
                onWrite(if (c.name.isBlank() || c.name == address) address else "${c.name} <$address>")
            })
        }
    }
}

/** Name, addresses (each with "Write"), phone numbers (tap to dial), note. */
@Composable
private fun ContactSheet(c: Contact, onWrite: (String) -> Unit) {
    val uri = LocalUriHandler.current
    // Rows take the sheet's color instead of the default surface.
    val rowColors = ListItemDefaults.colors(containerColor = Color.Transparent)
    Column(
        Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = 16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row(
            Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Avatar(c.name, 56.dp)
            Column {
                Text(c.name, style = MaterialTheme.typography.titleLarge)
                if (c.organization.isNotBlank() && c.organization != c.name) {
                    Text(c.organization, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
        c.emails.forEach { address ->
            ListItem(
                headlineContent = { Text(address) },
                leadingContent = { Icon(Icons.Filled.Email, contentDescription = null) },
                colors = rowColors,
                trailingContent = {
                    TextButton(onClick = { onWrite(address) }) { Text(stringResource(R.string.contact_write)) }
                },
            )
        }
        c.phones.forEach { tel ->
            ListItem(
                headlineContent = { Text(tel) },
                leadingContent = { Icon(Icons.Filled.Phone, contentDescription = null) },
                colors = rowColors,
                // Opens the dialer with the number (no call permission needed).
                modifier = Modifier.clickable { runCatching { uri.openUri("tel:" + tel.filter { it.isDigit() || it == '+' }) } },
            )
        }
        if (c.note.isNotBlank()) {
            Text(
                c.note,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
            )
        }
    }
}

/** Circle with the contact's initials. */
@Composable
private fun Avatar(name: String, size: Dp) {
    Box(
        Modifier.size(size).clip(CircleShape).background(MaterialTheme.colorScheme.primaryContainer),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            initials(name),
            color = MaterialTheme.colorScheme.onPrimaryContainer,
            fontWeight = FontWeight.SemiBold,
            style = if (size > 48.dp) MaterialTheme.typography.titleMedium else MaterialTheme.typography.labelLarge,
        )
    }
}

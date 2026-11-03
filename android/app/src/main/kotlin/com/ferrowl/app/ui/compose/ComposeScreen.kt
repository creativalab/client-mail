package com.ferrowl.app.ui.compose

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ferrowl.app.R
import com.ferrowl.app.ui.message.ReplyKind
import com.ferrowl.app.ui.notice.ReportError
import com.ferrowl.core.AddressSuggestion

/**
 * Writing a message: new, reply, reply all or forward. Markdown typed in the
 * body (bold, lists, links) is also sent as HTML, as on desktop.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ComposeScreen(
    kind: ReplyKind?,
    folderId: String,
    uid: UInt,
    to: String,
    onClose: () -> Unit,
    vm: ComposeViewModel = hiltViewModel(),
) {
    LaunchedEffect(Unit) { vm.start(kind, folderId, uid, to) }
    val s by vm.state.collectAsStateWithLifecycle()
    ReportError(s.error, vm::errorShown)
    LaunchedEffect(s.sent) { if (s.sent) onClose() }
    var askDiscard by remember { mutableStateOf(false) }
    val leave = { if (s.edited) askDiscard = true else onClose() }
    BackHandler(onBack = leave)

    val title = when (kind) {
        null -> R.string.compose_new
        ReplyKind.FORWARD -> R.string.compose_forward
        else -> R.string.compose_reply
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(title)) },
                navigationIcon = {
                    IconButton(onClick = leave) { Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.discard)) }
                },
                actions = {
                    if (s.sending) {
                        CircularProgressIndicator(Modifier.padding(12.dp).size(24.dp), strokeWidth = 2.dp)
                    } else {
                        IconButton(onClick = vm::send, enabled = !s.loading) {
                            Icon(Icons.AutoMirrored.Filled.Send, contentDescription = stringResource(R.string.send))
                        }
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .imePadding()
                .verticalScroll(rememberScrollState()),
        ) {
            Line(stringResource(R.string.to), s.to, vm::setTo, KeyboardType.Email)
            if (s.suggestFor == true) Suggestions(s.suggestions, vm::pick)
            Line(stringResource(R.string.cc), s.cc, vm::setCc, KeyboardType.Email)
            if (s.suggestFor == false) Suggestions(s.suggestions, vm::pick)
            Line(stringResource(R.string.subject), s.subject, vm::setSubject)
            TextField(
                value = s.body,
                onValueChange = vm::setBody,
                placeholder = { Text(stringResource(R.string.body_placeholder)) },
                colors = plain(),
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }

    if (askDiscard) {
        AlertDialog(
            onDismissRequest = { askDiscard = false },
            title = { Text(stringResource(R.string.discard)) },
            confirmButton = { TextButton(onClick = { askDiscard = false; onClose() }) { Text(stringResource(R.string.discard)) } },
            dismissButton = { TextButton(onClick = { askDiscard = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun Line(label: String, value: String, onChange: (String) -> Unit, keyboard: KeyboardType = KeyboardType.Text) {
    TextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = keyboard),
        colors = plain(),
        modifier = Modifier.fillMaxWidth(),
    )
    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
}

/** Addresses matching the word being typed; tap one to insert it. */
@Composable
private fun Suggestions(list: List<AddressSuggestion>, onPick: (AddressSuggestion) -> Unit) {
    Column(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceContainer)) {
        list.forEach { s ->
            Column(
                Modifier
                    .fillMaxWidth()
                    .clickable { onPick(s) }
                    .padding(horizontal = 16.dp, vertical = 8.dp),
            ) {
                if (s.name.isNotBlank()) Text(s.name, style = MaterialTheme.typography.bodyMedium)
                Text(s.address, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

/** Fields without the filled background, like a mail form. */
@Composable
private fun plain() = TextFieldDefaults.colors(
    focusedContainerColor = Color.Transparent,
    unfocusedContainerColor = Color.Transparent,
    disabledContainerColor = Color.Transparent,
)

package com.ferrowl.app.ui.setup

import android.content.Context
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ferrowl.app.R
import com.ferrowl.app.ui.fill
import com.ferrowl.app.ui.notice.ReportError
import com.ferrowl.core.Login
import kotlinx.coroutines.launch

/** First screen: pick the provider, type address and password, connect. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SetupScreen(onDone: () -> Unit, vm: SetupViewModel = hiltViewModel()) {
    val s by vm.state.collectAsStateWithLifecycle()
    LaunchedEffect(s.done) { if (s.done) onDone() }
    ReportError(s.error, vm::errorShown)
    // Coming back without an answer (sign-in page closed): the button works again.
    LifecycleResumeEffect(Unit) {
        vm.resumed()
        onPauseOrDispose {}
    }
    val uri = LocalUriHandler.current

    Column(
        Modifier
            .fillMaxSize()
            .safeDrawingPadding()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(stringResource(R.string.setup_title), style = MaterialTheme.typography.headlineMedium)
        Text(stringResource(R.string.setup_desc), color = MaterialTheme.colorScheme.onSurfaceVariant)

        Text(stringResource(R.string.provider_label), style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            s.providers.forEachIndexed { i, p ->
                FilterChip(selected = s.provider == i, onClick = { vm.pickProvider(i) }, label = { Text(p.name) })
            }
            FilterChip(
                selected = s.provider == null,
                onClick = { vm.pickProvider(null) },
                label = { Text(stringResource(R.string.provider_other)) },
            )
        }

        Field(stringResource(R.string.display_name), s.name, vm::setName, Modifier.fillMaxWidth())
        Field(stringResource(R.string.email), s.email, vm::setEmail, modifier = Modifier.fillMaxWidth(), keyboard = KeyboardType.Email)

        val p = s.chosen
        if (s.isGoogle) {
            val scope = rememberCoroutineScope()
            val picker = rememberLauncherForActivityResult(ActivityResultContracts.StartIntentSenderForResult()) { r ->
                vm.googleResult(r.data)
            }
            Button(
                onClick = {
                    scope.launch {
                        vm.googleStart()?.let { picker.launch(IntentSenderRequest.Builder(it.intentSender).build()) }
                    }
                },
                enabled = !s.busy,
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.oauth_google)) }
            Text(stringResource(R.string.oauth_or_app_password), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (s.signsIn) {
            Text(stringResource(R.string.ms_signin_hint), color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            Field(stringResource(R.string.password), s.password, vm::setPassword, modifier = Modifier.fillMaxWidth(), keyboard = KeyboardType.Password, secret = true)
            if (p?.login == Login.APP_PASSWORD) {
                Text(
                    fill(stringResource(R.string.app_password_hint), "provider" to p.name),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                p.guideUrl?.let { url ->
                    TextButton(onClick = { uri.openUri(url) }) { Text(stringResource(R.string.app_password_howto)) }
                }
            }
        }

        if (p == null) ManualServers(s, vm)


        if (s.signsIn) {
            val context = LocalContext.current
            Button(
                onClick = { vm.signIn()?.let { url -> openSignInPage(context, url) } },
                enabled = s.canSignIn,
                modifier = Modifier.fillMaxWidth(),
            ) {
                if (s.busy) {
                    CircularProgressIndicator(Modifier.width(20.dp), strokeWidth = 2.dp)
                } else {
                    Text(stringResource(R.string.oauth_microsoft))
                }
            }
            if (s.busy) {
                Text(stringResource(R.string.oauth_waiting), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        } else {
            Button(onClick = vm::connect, enabled = s.canConnect, modifier = Modifier.fillMaxWidth()) {
                if (s.busy) {
                    CircularProgressIndicator(Modifier.width(20.dp), strokeWidth = 2.dp)
                } else {
                    Text(stringResource(R.string.save))
                }
            }
        }
    }
}

/** "Other": every server setting, as in the desktop form. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ManualServers(s: SetupState, vm: SetupViewModel) {
    Text(stringResource(R.string.account_type), style = MaterialTheme.typography.labelLarge)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(selected = !s.pop3, onClick = { vm.setPop3(false) }, label = { Text("IMAP") })
        FilterChip(selected = s.pop3, onClick = { vm.setPop3(true) }, label = { Text("POP3") })
    }
    Field(stringResource(R.string.username), s.username, vm::setUsername, Modifier.fillMaxWidth())
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Field(
            stringResource(if (s.pop3) R.string.pop3_server else R.string.imap_server),
            s.incomingHost,
            vm::setIncomingHost,
            modifier = Modifier.weight(1f),
            keyboard = KeyboardType.Uri,
        )
        Field(stringResource(R.string.port), s.incomingPort, vm::setIncomingPort, modifier = Modifier.width(96.dp), keyboard = KeyboardType.Number)
    }
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Field(stringResource(R.string.smtp_server), s.smtpHost, vm::setSmtpHost, modifier = Modifier.weight(1f), keyboard = KeyboardType.Uri)
        Field(stringResource(R.string.port), s.smtpPort, vm::setSmtpPort, modifier = Modifier.width(96.dp), keyboard = KeyboardType.Number)
    }
    Text(
        stringResource(if (s.pop3) R.string.server_hint_pop3 else R.string.server_hint),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun Field(
    label: String,
    value: String,
    onChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    keyboard: KeyboardType = KeyboardType.Text,
    secret: Boolean = false,
) {
    // Secret fields get an eye to show what is being typed.
    var shown by rememberSaveable { mutableStateOf(false) }
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = keyboard),
        visualTransformation = if (secret && !shown) PasswordVisualTransformation() else VisualTransformation.None,
        trailingIcon = if (secret) {
            {
                IconButton(onClick = { shown = !shown }) {
                    Icon(
                        painterResource(if (shown) R.drawable.ic_visibility_off else R.drawable.ic_visibility),
                        contentDescription = stringResource(if (shown) R.string.hide_password else R.string.show_password),
                    )
                }
            }
        } else {
            null
        },
        modifier = modifier,
    )
}

/** The sign-in page in a Custom Tab: the user's browser, shown over the app. */
private fun openSignInPage(context: Context, url: String) {
    CustomTabsIntent.Builder().setShowTitle(true).build().launchUrl(context, url.toUri())
}

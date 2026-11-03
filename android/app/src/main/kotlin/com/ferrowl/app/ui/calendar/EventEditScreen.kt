package com.ferrowl.app.ui.calendar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ferrowl.app.R
import com.ferrowl.app.ui.notice.ReportError
import com.ferrowl.core.Repeat
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** Which date or time picker is open. */
private enum class Picking { START_DATE, START_TIME, END_DATE, END_TIME }

/**
 * New event, or changes to a stored one: title, all day, start and end
 * (Material date and time pickers), repeat, calendar, place and notes.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EventEditScreen(onClose: () -> Unit, vm: EventEditViewModel = hiltViewModel()) {
    val s by vm.state.collectAsStateWithLifecycle()
    LaunchedEffect(s.done) { if (s.done) onClose() }
    ReportError(s.error, vm::errorShown)
    var picking by remember { mutableStateOf<Picking?>(null) }
    val locale = LocalConfiguration.current.locales[0]
    val dateFmt = remember(locale) { DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM).withLocale(locale) }
    val timeFmt = remember(locale) { DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(if (vm.url == null) R.string.new_event else R.string.edit_event)) },
                navigationIcon = {
                    IconButton(onClick = onClose) {
                        Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.cancel))
                    }
                },
                actions = {
                    TextButton(onClick = vm::save, enabled = !s.loading && !s.saving && s.summary.isNotBlank()) {
                        Text(stringResource(R.string.event_save))
                    }
                },
            )
        },
    ) { padding ->
        if (s.loading) return@Scaffold
        Column(
            Modifier.fillMaxSize().padding(padding).imePadding().verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            OutlinedTextField(
                value = s.summary,
                onValueChange = { v -> vm.update { it.copy(summary = v) } },
                label = { Text(stringResource(R.string.event_title)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.all_day), modifier = Modifier.weight(1f))
                Switch(checked = s.allDay, onCheckedChange = { on -> vm.update { it.copy(allDay = on) } })
            }
            WhenRow(
                label = stringResource(R.string.event_starts),
                date = s.start.toLocalDate().format(dateFmt),
                time = if (s.allDay) null else s.start.toLocalTime().format(timeFmt),
                onDate = { picking = Picking.START_DATE },
                onTime = { picking = Picking.START_TIME },
            )
            WhenRow(
                label = stringResource(R.string.event_ends),
                date = s.end.toLocalDate().format(dateFmt),
                time = if (s.allDay) null else s.end.toLocalTime().format(timeFmt),
                onDate = { picking = Picking.END_DATE },
                onTime = { picking = Picking.END_TIME },
            )
            RepeatMenu(s.repeat) { r -> vm.update { it.copy(repeat = r) } }
            if (s.calendars.size > 1) {
                Choice(
                    label = stringResource(R.string.event_calendar),
                    selected = s.calendars.firstOrNull { it.url == s.calendarUrl }?.name.orEmpty(),
                    options = s.calendars.map { it.name to it.url },
                ) { url -> vm.update { it.copy(calendarUrl = url) } }
            }
            OutlinedTextField(
                value = s.location,
                onValueChange = { v -> vm.update { it.copy(location = v) } },
                label = { Text(stringResource(R.string.event_location)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = s.notes,
                onValueChange = { v -> vm.update { it.copy(notes = v) } },
                label = { Text(stringResource(R.string.event_notes)) },
                minLines = 3,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }

    when (val p = picking) {
        Picking.START_DATE, Picking.END_DATE -> {
            val current = if (p == Picking.START_DATE) s.start else s.end
            DateDialog(current.toLocalDate(), onDismiss = { picking = null }) { d ->
                picking = null
                if (p == Picking.START_DATE) {
                    vm.setStart(LocalDateTime.of(d, s.start.toLocalTime()))
                } else {
                    vm.update { it.copy(end = LocalDateTime.of(d, it.end.toLocalTime())) }
                }
            }
        }
        Picking.START_TIME, Picking.END_TIME -> {
            val current = if (p == Picking.START_TIME) s.start else s.end
            TimeDialog(current.toLocalTime(), onDismiss = { picking = null }) { t ->
                picking = null
                if (p == Picking.START_TIME) {
                    vm.setStart(LocalDateTime.of(s.start.toLocalDate(), t))
                } else {
                    vm.update { it.copy(end = LocalDateTime.of(it.end.toLocalDate(), t)) }
                }
            }
        }
        null -> {}
    }
}

/** "Starts: [12 Nov 2026] [15:00]" (no time for all-day events). */
@Composable
private fun WhenRow(label: String, date: String, time: String?, onDate: () -> Unit, onTime: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onDate) { Text(date) }
            if (time != null) OutlinedButton(onClick = onTime) { Text(time) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DateDialog(initial: LocalDate, onDismiss: () -> Unit, onPick: (LocalDate) -> Unit) {
    // The date picker works in UTC milliseconds at midnight.
    val state = rememberDatePickerState(initialSelectedDateMillis = initial.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli())
    DatePickerDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(onClick = {
                state.selectedDateMillis?.let { onPick(Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate()) } ?: onDismiss()
            }) { Text(stringResource(android.R.string.ok)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    ) { DatePicker(state) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TimeDialog(initial: LocalTime, onDismiss: () -> Unit, onPick: (LocalTime) -> Unit) {
    val state = rememberTimePickerState(initialHour = initial.hour, initialMinute = initial.minute)
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(onClick = { onPick(LocalTime.of(state.hour, state.minute)) }) { Text(stringResource(android.R.string.ok)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
        text = { TimePicker(state) },
    )
}

@Composable
private fun RepeatMenu(current: Repeat, onPick: (Repeat) -> Unit) {
    val labels = listOf(
        Repeat.NEVER to stringResource(R.string.repeat_never),
        Repeat.DAILY to stringResource(R.string.repeat_daily),
        Repeat.WEEKLY to stringResource(R.string.repeat_weekly),
        Repeat.MONTHLY to stringResource(R.string.repeat_monthly),
        Repeat.YEARLY to stringResource(R.string.repeat_yearly),
    ) + if (current == Repeat.CUSTOM) listOf(Repeat.CUSTOM to stringResource(R.string.repeat_custom)) else emptyList()
    Choice(
        label = stringResource(R.string.repeat),
        selected = labels.first { it.first == current }.second,
        options = labels.map { it.second to it.first },
        onPick = onPick,
    )
}

/** A read-only field that opens a menu of choices (Material exposed dropdown). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun <T> Choice(label: String, selected: String, options: List<Pair<String, T>>, onPick: (T) -> Unit) {
    var open by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded = open, onExpandedChange = { open = it }) {
        OutlinedTextField(
            value = selected,
            onValueChange = {},
            readOnly = true,
            label = { Text(label) },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = open) },
            modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable),
        )
        ExposedDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            options.forEach { (text, value) ->
                DropdownMenuItem(text = { Text(text) }, onClick = {
                    open = false
                    onPick(value)
                })
            }
        }
    }
}

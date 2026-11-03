package com.ferrowl.app.ui.calendar

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material.icons.filled.LocationOn
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ferrowl.app.R
import com.ferrowl.core.CalendarEvent
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale

/** Date and time formats in the phone's language (24 h or AM/PM as set). */
private class Formats(val locale: Locale) {
    val day: DateTimeFormatter = DateTimeFormatter.ofPattern("EEEE d MMMM", locale)
    val month: DateTimeFormatter = DateTimeFormatter.ofPattern("LLLL yyyy", locale)
    val time: DateTimeFormatter = DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale)

    fun day(d: LocalDate) = d.format(day).replaceFirstChar { it.titlecase(locale) }
}

/** Formats for the current configuration (rebuilt if the language changes). */
@Composable
private fun rememberFormats(): Formats {
    val locale = LocalConfiguration.current.locales[0]
    return remember(locale) { Formats(locale) }
}

/**
 * The account's events (from its CalDAV server), one month at a time, as
 * an agenda grouped by day. Tapping an event opens its details in a sheet.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CalendarScreen(onBack: () -> Unit, vm: CalendarViewModel = hiltViewModel()) {
    val s by vm.state.collectAsStateWithLifecycle()
    val f = rememberFormats()
    val today = LocalDate.now()
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(s.month.format(f.month).replaceFirstChar { it.titlecase(f.locale) }) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back_to_mail))
                    }
                },
                actions = {
                    IconButton(onClick = { vm.moveMonth(-1) }) {
                        Icon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, contentDescription = stringResource(R.string.previous_month))
                    }
                    TextButton(onClick = { vm.moveMonth(0) }) { Text(stringResource(R.string.today)) }
                    IconButton(onClick = { vm.moveMonth(1) }) {
                        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = stringResource(R.string.next_month))
                    }
                    IconButton(onClick = vm::sync, enabled = !s.syncing) {
                        Icon(Icons.Filled.Refresh, contentDescription = stringResource(R.string.calendar_sync))
                    }
                },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            if (s.syncing) LinearProgressIndicator(Modifier.fillMaxWidth())
            s.error?.let {
                Text(
                    it,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
            }
            if (s.days.isEmpty()) {
                Text(
                    stringResource(R.string.calendar_empty),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(16.dp),
                )
            } else {
                LazyColumn(Modifier.fillMaxSize()) {
                    s.days.forEach { day ->
                        item(key = day.date.toString()) { DayHeading(day.date, day.date == today) }
                        items(day.events, key = { "${day.date}/${it.uid}/${it.start}" }) { e ->
                            EventRow(e, onClick = { vm.open(e) })
                        }
                    }
                }
            }
        }
    }

    s.open?.let { e ->
        ModalBottomSheet(onDismissRequest = { vm.open(null) }) { EventSheet(e) }
    }
}

@Composable
private fun DayHeading(date: LocalDate, isToday: Boolean) {
    Row(
        Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            rememberFormats().day(date),
            style = MaterialTheme.typography.titleSmall,
            color = if (isToday) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (isToday) {
            Text(
                stringResource(R.string.today),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onPrimary,
                modifier = Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(MaterialTheme.colorScheme.primary)
                    .padding(horizontal = 8.dp, vertical = 1.dp),
            )
        }
    }
}

@Composable
private fun EventRow(e: CalendarEvent, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(e.summary, maxLines = 1, overflow = TextOverflow.Ellipsis, fontWeight = FontWeight.SemiBold) },
        supportingContent = {
            val meta = listOf(timeRange(e, rememberFormats()), e.location).filter { it.isNotBlank() }.joinToString(" · ")
            Text(meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
        },
        // A bar in the calendar's color.
        leadingContent = {
            Box(Modifier.width(4.dp).height(36.dp).clip(RoundedCornerShape(2.dp)).background(calendarColor(e)))
        },
        modifier = Modifier.clickable(onClick = onClick),
    )
}

/** Title, when, place, calendar, organizer and description. */
@Composable
private fun EventSheet(e: CalendarEvent) {
    val rowColors = ListItemDefaults.colors(containerColor = Color.Transparent)
    val f = rememberFormats()
    Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = 16.dp)) {
        Text(
            e.summary,
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
        )
        val day = when {
            e.allDay && e.firstDay != null -> LocalDate.parse(e.firstDay)
            else -> Instant.ofEpochSecond(e.start).atZone(ZoneId.systemDefault()).toLocalDate()
        }
        var whenText = f.day(day)
        val last = e.lastDay?.let(LocalDate::parse)
        if (last != null && last != day) whenText += " – " + f.day(last)
        whenText += "\n" + timeRange(e, f)
        if (e.recurring) whenText += "\n" + stringResource(R.string.event_repeats)
        ListItem(
            headlineContent = { Text(whenText) },
            leadingContent = { Icon(Icons.Filled.DateRange, contentDescription = null, tint = calendarColor(e)) },
            supportingContent = { if (e.calendar.isNotBlank()) Text(e.calendar) },
            colors = rowColors,
        )
        if (e.location.isNotBlank()) {
            ListItem(
                headlineContent = { Text(e.location) },
                leadingContent = { Icon(Icons.Filled.LocationOn, contentDescription = null) },
                colors = rowColors,
            )
        }
        if (e.organizer.isNotBlank()) {
            ListItem(
                headlineContent = { Text(e.organizer) },
                overlineContent = { Text(stringResource(R.string.organizer)) },
                leadingContent = { Icon(Icons.Filled.Person, contentDescription = null) },
                colors = rowColors,
            )
        }
        if (e.description.isNotBlank()) {
            Text(
                e.description,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
            )
        }
    }
}

/** "09:00 – 10:30" in the phone's format, or "All day". */
@Composable
private fun timeRange(e: CalendarEvent, f: Formats): String {
    if (e.allDay) return stringResource(R.string.all_day)
    val zone = ZoneId.systemDefault()
    val start = Instant.ofEpochSecond(e.start).atZone(zone)
    val end = Instant.ofEpochSecond(e.end).atZone(zone)
    return if (e.end <= e.start) {
        start.format(f.time)
    } else {
        "${start.format(f.time)} – ${end.format(f.time)}"
    }
}

/** The calendar's color from the server ("#RRGGBB[AA]"), else the theme's. */
@Composable
private fun calendarColor(e: CalendarEvent): Color {
    val hex = e.color.trim()
    if (hex.length >= 7 && hex.startsWith("#")) {
        hex.substring(1, 7).toLongOrNull(16)?.let { return Color(0xFF000000 or it) }
    }
    return MaterialTheme.colorScheme.primary
}

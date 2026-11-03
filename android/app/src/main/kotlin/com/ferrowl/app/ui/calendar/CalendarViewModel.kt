package com.ferrowl.app.ui.calendar

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ferrowl.app.data.MailRepository
import com.ferrowl.core.CalendarEvent
import com.ferrowl.core.FerrowlException
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.LocalDate
import java.time.YearMonth
import java.time.ZoneId
import javax.inject.Inject

/** One day of the agenda and its events. */
data class AgendaDay(val date: LocalDate, val events: List<CalendarEvent>)

data class CalendarState(
    val month: YearMonth = YearMonth.now(),
    val days: List<AgendaDay> = emptyList(),
    val syncing: Boolean = false,
    /** Why the last sync failed (shown above the list). */
    val error: String? = null,
    /** The event shown in the bottom sheet. */
    val open: CalendarEvent? = null,
)

@HiltViewModel
class CalendarViewModel @Inject constructor(private val repo: MailRepository) : ViewModel() {
    private val _state = MutableStateFlow(CalendarState())
    val state: StateFlow<CalendarState> = _state.asStateFlow()
    private val _notices = MutableSharedFlow<String>(extraBufferCapacity = 4)

    /** Results to show in a snackbar ("Event deleted."). */
    val notices: SharedFlow<String> = _notices.asSharedFlow()

    init {
        // The saved copy right away; the app already synced after connecting.
        reload()
    }

    /** Moves by months; 0 goes back to the current month. */
    fun moveMonth(delta: Long) {
        _state.update { it.copy(month = if (delta == 0L) YearMonth.now() else it.month.plusMonths(delta)) }
        reload()
    }

    fun open(event: CalendarEvent?) = _state.update { it.copy(open = event) }

    /** Reads the month again (after an event was saved or deleted). */
    fun refresh() = reload()

    /** Deletes the open event (a repeating one with all its occurrences). */
    fun delete(event: CalendarEvent) {
        _state.update { it.copy(open = null) }
        viewModelScope.launch {
            val notice = try {
                repo.deleteEvent(event.url)
            } catch (e: FerrowlException) {
                e.message.orEmpty()
            }
            _notices.emit(notice)
            reload()
        }
    }

    /** Asks the server for changes (the sync button). */
    fun sync() {
        if (_state.value.syncing) return
        _state.update { it.copy(syncing = true, error = null) }
        viewModelScope.launch {
            try {
                repo.ensureConnected()
                repo.syncCalendar()
                _state.update { it.copy(syncing = false) }
            } catch (e: FerrowlException) {
                _state.update { it.copy(syncing = false, error = e.message) }
            }
            reload()
        }
    }

    private fun reload() {
        val month = _state.value.month
        val zone = ZoneId.systemDefault()
        val from = month.atDay(1).atStartOfDay(zone).toEpochSecond()
        val to = month.plusMonths(1).atDay(1).atStartOfDay(zone).toEpochSecond()
        viewModelScope.launch {
            val events = repo.agenda(from, to)
            _state.update { it.copy(days = groupByDay(events, month, zone)) }
        }
    }

    /** Lists each event on every day of [month] it covers. */
    private fun groupByDay(events: List<CalendarEvent>, month: YearMonth, zone: ZoneId): List<AgendaDay> {
        val byDay = sortedMapOf<LocalDate, MutableList<CalendarEvent>>()
        for (e in events) {
            val first: LocalDate
            val last: LocalDate
            if (e.allDay && e.firstDay != null && e.lastDay != null) {
                first = LocalDate.parse(e.firstDay)
                last = LocalDate.parse(e.lastDay)
            } else {
                first = Instant.ofEpochSecond(e.start).atZone(zone).toLocalDate()
                // The day of the event's last second (an event ending at
                // midnight does not show on the next day).
                last = Instant.ofEpochSecond(maxOf(e.end - 1, e.start)).atZone(zone).toLocalDate()
            }
            var d = maxOf(first, month.atDay(1))
            val end = minOf(last, month.atEndOfMonth())
            while (!d.isAfter(end)) {
                byDay.getOrPut(d) { mutableListOf() }.add(e)
                d = d.plusDays(1)
            }
        }
        return byDay.map { (date, list) -> AgendaDay(date, list) }
    }
}

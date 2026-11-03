package com.ferrowl.app.ui.calendar

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.navigation.toRoute
import com.ferrowl.app.data.MailRepository
import com.ferrowl.app.ui.navigation.EditEvent
import com.ferrowl.core.CalendarInfo
import com.ferrowl.core.EventDraft
import com.ferrowl.core.FerrowlException
import com.ferrowl.core.Repeat
import com.ferrowl.core.newEventDraft
import com.ferrowl.core.translate
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import javax.inject.Inject

/** The event form; times are wall-clock times on the phone. */
data class EventFormState(
    val loading: Boolean = true,
    val summary: String = "",
    val location: String = "",
    val notes: String = "",
    val allDay: Boolean = false,
    val start: LocalDateTime = LocalDateTime.now(),
    val end: LocalDateTime = LocalDateTime.now(),
    val repeat: Repeat = Repeat.NEVER,
    /** Calendars to choose from (new events only). */
    val calendars: List<CalendarInfo> = emptyList(),
    val calendarUrl: String = "",
    val saving: Boolean = false,
    val error: String? = null,
    /** Saved: the screen closes (the calendar shows the change). */
    val done: Boolean = false,
) {
    /** A title, and an end not before the start. */
    val valid: Boolean
        get() = summary.isNotBlank() && if (allDay) !end.toLocalDate().isBefore(start.toLocalDate()) else !end.isBefore(start)
}

@HiltViewModel
class EventEditViewModel @Inject constructor(
    private val repo: MailRepository,
    saved: SavedStateHandle,
) : ViewModel() {
    private val route = saved.toRoute<EditEvent>()

    /** The event being changed, or null for a new one. */
    val url: String? = route.url

    private val _state = MutableStateFlow(EventFormState())
    val state: StateFlow<EventFormState> = _state.asStateFlow()

    /** The error was shown to the user (as a notice at the top). */
    fun errorShown() = _state.update { it.copy(error = null) }

    init {
        viewModelScope.launch {
            val draft = url?.let { repo.eventDraft(it) } ?: newEventDraft(route.day)
            val calendars = if (url == null) repo.calendars() else emptyList()
            _state.value = EventFormState(
                loading = false,
                summary = draft.summary,
                location = draft.location,
                notes = draft.description,
                allDay = draft.allDay,
                start = LocalDateTime.parse(draft.start),
                end = LocalDateTime.parse(draft.end),
                repeat = draft.repeat,
                calendars = calendars,
                calendarUrl = calendars.firstOrNull()?.url.orEmpty(),
                error = if (url == null && calendars.isEmpty()) translate("err_no_calendar") else null,
            )
        }
    }

    fun update(change: (EventFormState) -> EventFormState) = _state.update { change(it).copy(error = null) }

    /** Moves the start, keeping the event's length (as calendar apps do). */
    fun setStart(start: LocalDateTime) = _state.update {
        val length = java.time.Duration.between(it.start, it.end)
        it.copy(start = start, end = start.plus(length), error = null)
    }

    fun save() {
        val s = _state.value
        if (s.saving) return
        if (!s.valid) {
            _state.update { it.copy(error = translate("event_bad_time")) }
            return
        }
        _state.update { it.copy(saving = true) }
        val fmt = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm")
        val draft = EventDraft(
            summary = s.summary,
            location = s.location,
            description = s.notes,
            allDay = s.allDay,
            start = s.start.format(fmt),
            end = s.end.format(fmt),
            repeat = s.repeat,
        )
        viewModelScope.launch {
            try {
                repo.saveEvent(s.calendarUrl, url, draft)
                _state.update { it.copy(saving = false, done = true) }
            } catch (e: FerrowlException) {
                _state.update { it.copy(saving = false, error = e.message) }
            }
        }
    }
}

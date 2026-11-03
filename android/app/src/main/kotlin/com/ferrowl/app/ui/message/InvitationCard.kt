package com.ferrowl.app.ui.message

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material.icons.filled.LocationOn
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ferrowl.app.R
import com.ferrowl.core.Invitation
import com.ferrowl.core.InviteReply
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/**
 * A meeting invitation found in the message: what, when, where, who, and
 * Accept / Maybe / Decline. For a cancellation it offers to remove the
 * event from the calendar; for a reply to our invitation it says who
 * answered and how.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun InvitationCard(
    inv: Invitation,
    state: InviteState,
    onRespond: (InviteReply) -> Unit,
    onRemove: () -> Unit,
) {
    OutlinedCard(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            val title = when (inv.method) {
                "CANCEL" -> R.string.invite_cancelled_title
                "REPLY" -> R.string.invite_reply_title
                else -> R.string.invite_title
            }
            Text(stringResource(title), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
            Text(inv.summary, style = MaterialTheme.typography.titleMedium)
            var whenText = whenText(inv)
            if (inv.recurring) whenText += " · " + stringResource(R.string.event_repeats)
            Line(Icons.Filled.DateRange, whenText)
            if (inv.location.isNotBlank()) Line(Icons.Filled.LocationOn, inv.location)
            if (inv.organizer.isNotBlank() && inv.method != "REPLY") {
                val who = if (inv.organizerName.isBlank()) inv.organizer else "${inv.organizerName} <${inv.organizer}>"
                Line(Icons.Filled.Person, stringResource(R.string.organizer) + ": " + who)
            }
            when (inv.method) {
                "REQUEST" -> FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                    itemVerticalAlignment = Alignment.CenterVertically,
                ) {
                    listOf(
                        InviteReply.ACCEPTED to R.string.invite_accept,
                        InviteReply.TENTATIVE to R.string.invite_tentative,
                        InviteReply.DECLINED to R.string.invite_decline,
                    ).forEach { (reply, label) ->
                        // The chosen answer is filled, the others outlined.
                        if (state.reply == reply) {
                            Button(onClick = { onRespond(reply) }, enabled = !state.busy) { Text(stringResource(label)) }
                        } else {
                            OutlinedButton(onClick = { onRespond(reply) }, enabled = !state.busy) { Text(stringResource(label)) }
                        }
                    }
                }
                "CANCEL" -> if (state.inCalendar) {
                    OutlinedButton(onClick = onRemove, enabled = !state.busy) { Text(stringResource(R.string.invite_remove)) }
                }
                "REPLY" -> inv.answers.forEach { a ->
                    val key = when (a.status) {
                        "ACCEPTED" -> R.string.invite_reply_accepted
                        "TENTATIVE" -> R.string.invite_reply_tentative
                        "DECLINED" -> R.string.invite_reply_declined
                        else -> null
                    }
                    if (key != null) {
                        Text(stringResource(key).replace("{who}", a.name.ifBlank { a.email }))
                    }
                }
            }
        }
    }
}

@Composable
private fun Line(icon: ImageVector, text: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.Top) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(text, style = MaterialTheme.typography.bodyMedium)
    }
}

/** "Thursday 12 November · 15:00 – 16:00" in the phone's language and format. */
@Composable
private fun whenText(inv: Invitation): String {
    val locale = LocalConfiguration.current.locales[0]
    val day = remember(locale) { DateTimeFormatter.ofPattern("EEEE d MMMM", locale) }
    val time = remember(locale) { DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale) }
    val zone = ZoneId.systemDefault()
    val start = Instant.ofEpochSecond(inv.start).atZone(zone)
    val first = inv.firstDay?.let(LocalDate::parse) ?: start.toLocalDate()
    var text = first.format(day).replaceFirstChar { it.titlecase(locale) }
    val last = inv.lastDay?.let(LocalDate::parse)
    if (last != null && last != first) text += " – " + last.format(day)
    text += if (inv.allDay) {
        " · " + stringResource(R.string.all_day)
    } else {
        " · " + start.format(time) + " – " + Instant.ofEpochSecond(inv.end).atZone(zone).format(time)
    }
    return text
}

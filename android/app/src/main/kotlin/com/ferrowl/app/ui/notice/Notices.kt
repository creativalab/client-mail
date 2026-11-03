package com.ferrowl.app.ui.notice

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import javax.inject.Inject
import javax.inject.Singleton

/**
 * A short message at the top of the screen: a confirmation or a problem,
 * optionally with one action (e.g. "Undo").
 */
data class Notice(
    val text: String,
    val error: Boolean,
    val action: String? = null,
    val onAction: (() -> Unit)? = null,
)

/**
 * The app's notices, shown at the top of every screen by [TopNoticeHost]:
 * confirmations ("Archived", "Sent") and errors, already in words for the
 * user (the engine translates technical errors).
 */
@Singleton
class Notices @Inject constructor() {
    private val _flow = MutableSharedFlow<Notice>(extraBufferCapacity = 8)
    val flow: SharedFlow<Notice> = _flow.asSharedFlow()

    fun info(text: String) {
        if (text.isNotBlank()) _flow.tryEmit(Notice(text, error = false))
    }

    /** A confirmation with an action button (shown a little longer). */
    fun withAction(text: String, action: String, onAction: () -> Unit) {
        _flow.tryEmit(Notice(text, error = false, action = action, onAction = onAction))
    }

    fun error(text: String) {
        if (text.isNotBlank()) _flow.tryEmit(Notice(text, error = true))
    }
}

/** Google Play Billing for composables (provided once, in MainActivity). */
val LocalBilling = staticCompositionLocalOf<com.ferrowl.app.data.Billing> { error("Billing not provided") }

/** The notices for composables (provided once, in MainActivity). */
val LocalNotices = staticCompositionLocalOf<Notices> { error("Notices not provided") }

/**
 * Shows each notice at the top, under the status bar, for a few seconds
 * (errors a little longer); tapping it closes it. Screen readers announce it.
 */
@Composable
fun TopNoticeHost(notices: Notices, modifier: Modifier = Modifier) {
    var current by remember { mutableStateOf<Notice?>(null) }
    var visible by remember { mutableStateOf(false) }
    LaunchedEffect(notices) {
        // A new notice replaces the one on screen at once (its action
        // belongs to the newest thing that happened).
        notices.flow.collectLatest { n ->
            current = n
            visible = true
            delay(if (n.error || n.action != null) 6_000 else 3_500)
            visible = false
            delay(300)
        }
    }
    Box(modifier.fillMaxWidth().statusBarsPadding().padding(horizontal = 12.dp, vertical = 8.dp), contentAlignment = Alignment.TopCenter) {
        AnimatedVisibility(
            visible = visible && current != null,
            enter = slideInVertically { -it } + fadeIn(),
            exit = slideOutVertically { -it } + fadeOut(),
        ) {
            val n = current ?: return@AnimatedVisibility
            val colors = MaterialTheme.colorScheme
            Surface(
                color = if (n.error) colors.errorContainer else colors.inverseSurface,
                contentColor = if (n.error) colors.onErrorContainer else colors.inverseOnSurface,
                shape = RoundedCornerShape(14.dp),
                shadowElevation = 6.dp,
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { visible = false }
                    .semantics { liveRegion = LiveRegionMode.Polite },
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        n.text,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.weight(1f).padding(start = 16.dp, top = 12.dp, bottom = 12.dp, end = 8.dp),
                    )
                    if (n.action != null) {
                        TextButton(onClick = {
                            n.onAction?.invoke()
                            visible = false
                        }) { Text(n.action, color = if (n.error) colors.onErrorContainer else colors.inversePrimary) }
                    }
                }
            }
        }
    }
}

/**
 * Shows a screen's error as a notice at the top, then tells the screen it
 * was shown ([shown] clears it), so the same error can be shown again later.
 */
@Composable
fun ReportError(error: String?, shown: () -> Unit) {
    val notices = LocalNotices.current
    LaunchedEffect(error) {
        if (error != null) {
            notices.error(error)
            shown()
        }
    }
}
